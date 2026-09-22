import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  createAssociatedTokenAccount,
  getAccount,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { expect } from "chai";

import { Spotboard } from "../target/types/spotboard";
import { ensureConfig } from "./helpers/setup";

describe("dispute lifecycle", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.spotboard as Program<Spotboard>;
  const wallet = provider.wallet as anchor.Wallet;
  const buyer = anchor.web3.Keypair.generate();
  const slugHash = Array.from(anchor.web3.Keypair.generate().publicKey.toBytes().slice(0, 16));
  const price = 150_000_000n;
  const [config] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    program.programId,
  );
  const [listing] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("listing"), wallet.publicKey.toBuffer(), Buffer.from(slugHash)],
    program.programId,
  );
  const [spot] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("spot"), listing.toBuffer(), Buffer.from([0])],
    program.programId,
  );
  const [vault] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("vault"), spot.toBuffer()],
    program.programId,
  );
  const [proof] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("proof"), spot.toBuffer()],
    program.programId,
  );
  const [dispute] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("dispute"), spot.toBuffer()],
    program.programId,
  );

  async function validatorTime(): Promise<number> {
    const clock = await provider.connection.getAccountInfo(anchor.web3.SYSVAR_CLOCK_PUBKEY);
    if (!clock) throw new Error("Validator clock is unavailable");
    return Number(clock.data.readBigInt64LE(32));
  }

  async function waitUntil(timestamp: number): Promise<void> {
    const deadline = Date.now() + 120_000;
    while ((await validatorTime()) < timestamp) {
      if (Date.now() > deadline) throw new Error("Validator clock did not advance");
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  }

  it("stores proof and evidence, then splits escrow by the resolver decision", async () => {
    const { paymentMint, treasury } = await ensureConfig(program, provider);
    const airdrop = await provider.connection.requestAirdrop(
      buyer.publicKey,
      anchor.web3.LAMPORTS_PER_SOL,
    );
    await provider.connection.confirmTransaction(airdrop, "confirmed");

    const buyerAta = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      paymentMint,
      buyer.publicKey,
    );
    const sellerAta = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        wallet.payer,
        paymentMint,
        wallet.publicKey,
      )
    ).address;
    const treasuryAta = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        wallet.payer,
        paymentMint,
        treasury,
      )
    ).address;
    await mintTo(
      provider.connection,
      wallet.payer,
      paymentMint,
      buyerAta,
      wallet.payer,
      Number(price),
    );

    const now = await validatorTime();
    const auctionEnd = now + 12;
    const deliveryEnd = now + 30;
    await program.methods
      .createListing(
        slugHash,
        "ipfs://spotboard-validator-dispute-listing",
        0,
        new anchor.BN(auctionEnd),
        new anchor.BN(auctionEnd),
        new anchor.BN(deliveryEnd),
        new anchor.BN(0),
        false,
      )
      .accountsStrict({
        seller: wallet.publicKey,
        listing,
        config,
        usdcMint: paymentMint,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    await program.methods
      .addSpot(new anchor.BN(Number(price)))
      .accountsStrict({
        seller: wallet.publicKey,
        listing,
        spot,
        vault,
        usdcMint: paymentMint,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();
    await program.methods
      .claimSpot()
      .accountsStrict({
        buyer: buyer.publicKey,
        listing,
        spot,
        vault,
        buyerAta,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([buyer])
      .rpc();
    expect((await getAccount(provider.connection, vault)).amount).to.equal(price);

    await waitUntil(auctionEnd);
    await program.methods
      .submitProof(Array(32).fill(1), "ipfs://spotboard-validator-proof")
      .accountsStrict({
        seller: wallet.publicKey,
        listing,
        spot,
        proof,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    const storedProof = await program.account.proofRecord.fetch(proof);
    expect(storedProof.spot.equals(spot)).to.equal(true);
    expect(storedProof.proofUri).to.equal("ipfs://spotboard-validator-proof");

    await waitUntil(deliveryEnd);
    await program.methods
      .raiseDispute(Array(32).fill(2), "ipfs://spotboard-validator-evidence")
      .accountsStrict({
        buyer: buyer.publicKey,
        listing,
        spot,
        dispute,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([buyer])
      .rpc();
    const openedDispute = await program.account.disputeRecord.fetch(dispute);
    expect(openedDispute.buyer.equals(buyer.publicKey)).to.equal(true);
    expect(openedDispute.evidenceUri).to.equal("ipfs://spotboard-validator-evidence");
    expect(openedDispute.proofSubmitted).to.equal(true);
    expect((await program.account.spot.fetch(spot)).status).to.equal(4);

    const sellerBefore = (await getAccount(provider.connection, sellerAta)).amount;
    const treasuryBefore = (await getAccount(provider.connection, treasuryAta)).amount;
    await program.methods
      .resolveDispute(5_000, Array(32).fill(3), "ipfs://spotboard-validator-decision")
      .accountsStrict({
        resolver: wallet.publicKey,
        config,
        listing,
        spot,
        dispute,
        vault,
        sellerAta,
        seller: wallet.publicKey,
        buyerAta,
        treasuryAta,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    const feeBps = (await program.account.listing.fetch(listing)).feeBps;
    const sellerGross = price / 2n;
    const fee = (sellerGross * BigInt(feeBps)) / 10_000n;
    const decision = await program.account.disputeRecord.fetch(dispute);
    expect(decision.decisionUri).to.equal("ipfs://spotboard-validator-decision");
    expect(decision.sellerShareBps).to.equal(5_000);
    expect(decision.buyerRefund.toString()).to.equal(sellerGross.toString());
    expect(decision.sellerAmount.toString()).to.equal((sellerGross - fee).toString());
    expect(decision.fee.toString()).to.equal(fee.toString());
    expect((await getAccount(provider.connection, buyerAta)).amount).to.equal(sellerGross);
    expect((await getAccount(provider.connection, sellerAta)).amount - sellerBefore).to.equal(
      sellerGross - fee,
    );
    expect((await getAccount(provider.connection, treasuryAta)).amount - treasuryBefore).to.equal(
      fee,
    );
    expect((await program.account.spot.fetch(spot)).status).to.equal(2);
    expect(await provider.connection.getAccountInfo(vault)).to.equal(null);
  });
});
