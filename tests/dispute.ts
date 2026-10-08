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

import { Adspaces } from "../target/types/adspaces";
import { ensureConfig } from "./helpers/setup";

describe("dispute lifecycle", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.adspaces as Program<Adspaces>;
  const wallet = provider.wallet as anchor.Wallet;
  const buyer = anchor.web3.Keypair.generate();
  const slugHash = Array.from(anchor.web3.Keypair.generate().publicKey.toBytes().slice(0, 16));
  const price = 150_000_000n;
  const [config] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    program.programId,
  );
  const [reviewAuthority] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("review_authority"), config.toBuffer()],
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
    const deadline = Date.now() + 30_000;
    while ((await validatorTime()) < timestamp) {
      if (Date.now() > deadline) throw new Error("Validator clock did not advance");
      await provider.sendAndConfirm(
        new anchor.web3.Transaction().add(
          anchor.web3.SystemProgram.transfer({
            fromPubkey: wallet.publicKey,
            toPubkey: wallet.publicKey,
            lamports: 0,
          }),
        ),
      );
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  }

  async function expectRejected(action: () => Promise<unknown>): Promise<void> {
    let rejected = false;
    try {
      await action();
    } catch {
      rejected = true;
    }
    expect(rejected).to.equal(true);
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
    const auctionEnd = now + 2;
    const deliveryEnd = now + 4;
    await program.methods
      .createListing(
        slugHash,
        "ipfs://adspaces-validator-dispute-listing",
        0,
        new anchor.BN(auctionEnd),
        new anchor.BN(auctionEnd),
        new anchor.BN(deliveryEnd),
        new anchor.BN(48 * 60 * 60),
        new anchor.BN(0),
        false,
        250,
        0,
        250,
      )
      .accountsStrict({
        seller: wallet.publicKey,
        reviewer: wallet.publicKey,
        listing,
        config,
        reviewAuthority,
        usdcMint: paymentMint,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    await program.methods
      .addSpot(new anchor.BN(Number(price)))
      .accountsStrict({
        seller: wallet.publicKey,
        config,
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
        config,
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
      .submitProof(Array(32).fill(1), "ipfs://adspaces-validator-proof")
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
    expect(storedProof.proofUri).to.equal("ipfs://adspaces-validator-proof");

    const raiseDispute = (hash: number[], uri: string) =>
      program.methods
        .raiseDispute(hash, uri)
        .accountsStrict({
          buyer: buyer.publicKey,
          listing,
          spot,
          dispute,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([buyer])
        .rpc();
    await expectRejected(() =>
      raiseDispute(Array(32).fill(2), "ipfs://adspaces-validator-evidence"),
    );
    await waitUntil(deliveryEnd);
    await expectRejected(() => raiseDispute(Array(32).fill(0), ""));
    expect(await provider.connection.getAccountInfo(dispute)).to.equal(null);
    await raiseDispute(Array(32).fill(2), "ipfs://adspaces-validator-evidence");
    await expectRejected(() =>
      raiseDispute(Array(32).fill(2), "ipfs://adspaces-validator-evidence"),
    );
    const openedDispute = await program.account.disputeRecord.fetch(dispute);
    expect(openedDispute.buyer.equals(buyer.publicKey)).to.equal(true);
    expect(openedDispute.evidenceUri).to.equal("ipfs://adspaces-validator-evidence");
    expect(openedDispute.proofSubmitted).to.equal(true);
    expect((await program.account.spot.fetch(spot)).status).to.equal(4);

    const sellerBefore = (await getAccount(provider.connection, sellerAta)).amount;
    const treasuryBefore = (await getAccount(provider.connection, treasuryAta)).amount;
    const resolveDispute = (hash: number[], uri: string) =>
      program.methods
        .resolveDispute(5_000, hash, uri)
        .accountsStrict({
          resolver: wallet.publicKey,
          config,
          listing,
          spot,
          proof,
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
    await expectRejected(() => resolveDispute(Array(32).fill(0), ""));
    expect((await getAccount(provider.connection, vault)).amount).to.equal(price);
    await resolveDispute(Array(32).fill(3), "ipfs://adspaces-validator-decision");

    const feeBps = (await program.account.listing.fetch(listing)).feeBps;
    const sellerGross = price / 2n;
    const fee = (sellerGross * BigInt(feeBps)) / 10_000n;
    expect(await provider.connection.getAccountInfo(dispute)).to.equal(null);
    expect(await provider.connection.getAccountInfo(proof)).to.equal(null);
    expect((await getAccount(provider.connection, buyerAta)).amount).to.equal(sellerGross);
    expect((await getAccount(provider.connection, sellerAta)).amount - sellerBefore).to.equal(
      sellerGross - fee,
    );
    expect((await getAccount(provider.connection, treasuryAta)).amount - treasuryBefore).to.equal(
      fee,
    );
    expect(await provider.connection.getAccountInfo(spot)).to.equal(null);
    expect(await provider.connection.getAccountInfo(vault)).to.equal(null);
    await expectRejected(() =>
      resolveDispute(Array(32).fill(3), "ipfs://adspaces-validator-decision"),
    );
  });

  it("refunds only the winning auction bidder after a missing-proof dispute", async () => {
    const { paymentMint, treasury } = await ensureConfig(program, provider);
    const alice = anchor.web3.Keypair.generate();
    const bob = anchor.web3.Keypair.generate();
    const auctionSlug = Array.from(anchor.web3.Keypair.generate().publicKey.toBytes().slice(0, 16));
    const [auctionListing] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("listing"), wallet.publicKey.toBuffer(), Buffer.from(auctionSlug)],
      program.programId,
    );
    const [auctionSpot] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("spot"), auctionListing.toBuffer(), Buffer.from([0])],
      program.programId,
    );
    const [auctionVault] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), auctionSpot.toBuffer()],
      program.programId,
    );
    const [auctionDispute] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("dispute"), auctionSpot.toBuffer()],
      program.programId,
    );
    for (const bidder of [alice, bob]) {
      const signature = await provider.connection.requestAirdrop(
        bidder.publicKey,
        anchor.web3.LAMPORTS_PER_SOL,
      );
      await provider.connection.confirmTransaction(signature, "confirmed");
    }
    const aliceAta = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      paymentMint,
      alice.publicKey,
    );
    const bobAta = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      paymentMint,
      bob.publicKey,
    );
    for (const account of [aliceAta, bobAta]) {
      await mintTo(
        provider.connection,
        wallet.payer,
        paymentMint,
        account,
        wallet.payer,
        500_000_000,
      );
    }
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

    const now = await validatorTime();
    const deliveryEnd = now + 4;
    await program.methods
      .createListing(
        auctionSlug,
        "ipfs://adspaces-validator-auction-dispute",
        1,
        new anchor.BN(now + 2),
        new anchor.BN(now + 2),
        new anchor.BN(deliveryEnd),
        new anchor.BN(48 * 60 * 60),
        new anchor.BN(10_000_000),
        false,
        250,
        1_000,
        750,
      )
      .accountsStrict({
        seller: wallet.publicKey,
        reviewer: wallet.publicKey,
        listing: auctionListing,
        config,
        reviewAuthority,
        usdcMint: paymentMint,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    await program.methods
      .addSpot(new anchor.BN(100_000_000))
      .accountsStrict({
        seller: wallet.publicKey,
        config,
        listing: auctionListing,
        spot: auctionSpot,
        vault: auctionVault,
        usdcMint: paymentMint,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();
    await program.methods
      .placeBid(new anchor.BN(100_000_000))
      .accountsStrict({
        bidder: alice.publicKey,
        config,
        listing: auctionListing,
        spot: auctionSpot,
        vault: auctionVault,
        bidderAta: aliceAta,
        prevBidderAta: null,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([alice])
      .rpc();
    await program.methods
      .placeBid(new anchor.BN(110_000_000))
      .accountsStrict({
        bidder: bob.publicKey,
        config,
        listing: auctionListing,
        spot: auctionSpot,
        vault: auctionVault,
        bidderAta: bobAta,
        prevBidderAta: aliceAta,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([bob])
      .rpc();
    expect((await getAccount(provider.connection, aliceAta)).amount).to.equal(500_000_000n);
    expect((await getAccount(provider.connection, auctionVault)).amount).to.equal(110_000_000n);

    await waitUntil(deliveryEnd);
    let loserRejected = false;
    try {
      await program.methods
        .raiseDispute(Array(32).fill(0), "")
        .accountsStrict({
          buyer: alice.publicKey,
          listing: auctionListing,
          spot: auctionSpot,
          dispute: auctionDispute,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([alice])
        .rpc();
    } catch {
      loserRejected = true;
    }
    expect(loserRejected).to.equal(true);
    await program.methods
      .raiseDispute(Array(32).fill(0), "")
      .accountsStrict({
        buyer: bob.publicKey,
        listing: auctionListing,
        spot: auctionSpot,
        dispute: auctionDispute,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([bob])
      .rpc();
    const openedDispute = await program.account.disputeRecord.fetch(auctionDispute);
    expect(openedDispute.buyer.equals(bob.publicKey)).to.equal(true);
    expect(openedDispute.proofSubmitted).to.equal(false);
    expect(openedDispute.evidenceUri).to.equal("");

    const sellerBefore = (await getAccount(provider.connection, sellerAta)).amount;
    const treasuryBefore = (await getAccount(provider.connection, treasuryAta)).amount;
    await program.methods
      .resolveDispute(0, Array(32).fill(4), "ipfs://adspaces-validator-auction-refund")
      .accountsStrict({
        resolver: wallet.publicKey,
        config,
        listing: auctionListing,
        spot: auctionSpot,
        proof: null,
        dispute: auctionDispute,
        vault: auctionVault,
        sellerAta,
        seller: wallet.publicKey,
        buyerAta: bobAta,
        treasuryAta,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
    expect(await provider.connection.getAccountInfo(auctionDispute)).to.equal(null);
    expect((await getAccount(provider.connection, bobAta)).amount).to.equal(500_000_000n);
    expect((await getAccount(provider.connection, sellerAta)).amount).to.equal(sellerBefore);
    expect((await getAccount(provider.connection, treasuryAta)).amount).to.equal(treasuryBefore);
    expect(await provider.connection.getAccountInfo(auctionSpot)).to.equal(null);
    expect(await provider.connection.getAccountInfo(auctionVault)).to.equal(null);
  });
});
