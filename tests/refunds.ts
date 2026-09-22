import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  createAssociatedTokenAccount,
  getAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { expect } from "chai";

import { Spotboard } from "../target/types/spotboard";
import { ensureConfig } from "./helpers/setup";

describe("funded spot refunds", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.spotboard as Program<Spotboard>;
  const wallet = provider.wallet as anchor.Wallet;
  const price = 150_000_000n;
  const [config] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    program.programId,
  );

  async function validatorTime(): Promise<number> {
    const clock = await provider.connection.getAccountInfo(anchor.web3.SYSVAR_CLOCK_PUBKEY);
    if (!clock) throw new Error("Validator clock is unavailable");
    return Number(clock.data.readBigInt64LE(32));
  }

  async function fundedSpot(deadlineOffset: number, deliveryOffset: number) {
    const { paymentMint } = await ensureConfig(program, provider);
    const buyer = anchor.web3.Keypair.generate();
    const caller = anchor.web3.Keypair.generate();
    const slugHash = Array.from(anchor.web3.Keypair.generate().publicKey.toBytes().slice(0, 16));
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
    for (const user of [buyer, caller]) {
      const signature = await provider.connection.requestAirdrop(
        user.publicKey,
        anchor.web3.LAMPORTS_PER_SOL,
      );
      await provider.connection.confirmTransaction(signature, "confirmed");
    }
    const buyerAta = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      paymentMint,
      buyer.publicKey,
    );
    await mintTo(
      provider.connection,
      wallet.payer,
      paymentMint,
      buyerAta,
      wallet.payer,
      Number(price),
    );
    const now = await validatorTime();
    const deadline = now + deadlineOffset;
    const deliveryDeadline = now + deliveryOffset;
    await program.methods
      .createListing(
        slugHash,
        "ipfs://spotboard-validator-refund",
        0,
        new anchor.BN(deadline),
        new anchor.BN(deadline),
        new anchor.BN(deliveryDeadline),
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
    return { buyerAta, caller, deliveryDeadline, listing, paymentMint, spot, vault };
  }

  it("lets the seller cancel a funded spot with a full buyer refund", async () => {
    const { buyerAta, listing, paymentMint, spot, vault } = await fundedSpot(3600, 7200);
    expect((await getAccount(provider.connection, buyerAta)).amount).to.equal(0n);
    await program.methods
      .refundSpot()
      .accountsStrict({
        caller: wallet.publicKey,
        listing,
        seller: wallet.publicKey,
        spot,
        vault,
        buyerAta,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
    expect((await getAccount(provider.connection, buyerAta)).amount).to.equal(price);
    expect((await program.account.spot.fetch(spot)).status).to.equal(3);
    expect(await provider.connection.getAccountInfo(vault)).to.equal(null);
  });

  it("lets anyone refund a missing proof only after the delivery deadline", async () => {
    const { buyerAta, caller, deliveryDeadline, listing, paymentMint, spot, vault } =
      await fundedSpot(10, 20);
    const refund = () =>
      program.methods
        .refundSpot()
        .accountsStrict({
          caller: caller.publicKey,
          listing,
          seller: wallet.publicKey,
          spot,
          vault,
          buyerAta,
          usdcMint: paymentMint,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([caller])
        .rpc();
    let earlyRejected = false;
    try {
      await refund();
    } catch {
      earlyRejected = true;
    }
    expect(earlyRejected).to.equal(true);
    expect((await getAccount(provider.connection, vault)).amount).to.equal(price);

    const timeout = Date.now() + 120_000;
    while ((await validatorTime()) < deliveryDeadline) {
      if (Date.now() > timeout) throw new Error("Validator clock did not advance");
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
    await refund();
    expect((await getAccount(provider.connection, buyerAta)).amount).to.equal(price);
    expect((await program.account.spot.fetch(spot)).status).to.equal(3);
    expect(await provider.connection.getAccountInfo(vault)).to.equal(null);
  });
});
