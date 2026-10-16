import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  createAssociatedTokenAccount,
  getAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { expect } from "chai";

import { Adspaces } from "../target/types/adspaces";
import { ensureConfig } from "./helpers/setup";

describe("funded spot refunds", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.adspaces as Program<Adspaces>;
  const wallet = provider.wallet as anchor.Wallet;
  const price = 150_000_000n;
  const [config] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    program.programId,
  );
  const [reviewAuthority] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("review_authority"), config.toBuffer()],
    program.programId,
  );

  // scripts/test-program.sh runs this suite on the devnet-short-review build, whose minimum proof
  // window (delivery_deadline - artwork_close) is 60 s instead of 1 h.
  const minProofWindowSeconds = 60;

  async function setReviewAuthority(authority: anchor.web3.PublicKey) {
    await program.methods
      .updateReviewAuthority(authority)
      .accountsStrict({ authority: wallet.publicKey, config, reviewAuthority })
      .rpc();
  }

  async function validatorTime(): Promise<number> {
    const clock = await provider.connection.getAccountInfo(anchor.web3.SYSVAR_CLOCK_PUBKEY);
    if (!clock) throw new Error("Validator clock is unavailable");
    return Number(clock.data.readBigInt64LE(32));
  }

  async function fundedSpot(
    deadlineOffset: number,
    deliveryOffset: number,
    rentPayer: anchor.web3.Keypair = wallet.payer,
  ) {
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
    const users = rentPayer === wallet.payer ? [buyer, caller] : [buyer, caller, rentPayer];
    for (const user of users) {
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
    // The rent payer must be the review authority key. For a separate rent payer, make it the
    // reviewer for the listing and its spot, then hand the role back to the provider wallet.
    const separatePayer = rentPayer !== wallet.payer;
    if (separatePayer) await setReviewAuthority(rentPayer.publicKey);
    try {
      await program.methods
        .createListing(
          slugHash,
          "ipfs://adspaces-validator-refund",
          0,
          new anchor.BN(deadline),
          new anchor.BN(deadline),
          new anchor.BN(deliveryDeadline),
          new anchor.BN(48 * 60 * 60),
          new anchor.BN(0),
          false,
          250,
          0,
          250,
        )
        .accountsStrict({
          seller: wallet.publicKey,
          reviewer: rentPayer.publicKey,
          rentPayer: rentPayer.publicKey,
          listing,
          config,
          reviewAuthority,
          usdcMint: paymentMint,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([rentPayer])
        .rpc();
      await program.methods
        .addSpot(new anchor.BN(Number(price)))
        .accountsStrict({
          seller: wallet.publicKey,
          rentPayer: rentPayer.publicKey,
          reviewer: rentPayer.publicKey,
          reviewAuthority,
          config,
          listing,
          spot,
          vault,
          usdcMint: paymentMint,
          systemProgram: anchor.web3.SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([rentPayer])
        .rpc();
    } finally {
      if (separatePayer) await setReviewAuthority(wallet.publicKey);
    }
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
    return { buyerAta, caller, deliveryDeadline, listing, paymentMint, rentPayer, spot, vault };
  }

  it("lets the seller cancel a funded spot with a full buyer refund", async () => {
    const { buyerAta, listing, paymentMint, spot, vault } = await fundedSpot(3600, 7200);
    expect((await getAccount(provider.connection, buyerAta)).amount).to.equal(0n);
    await program.methods
      .refundSpot()
      .accountsStrict({
        caller: wallet.publicKey,
        listing,
        rentPayer: wallet.publicKey,
        spot,
        vault,
        buyerAta,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        proof: null,
      })
      .rpc();
    expect((await getAccount(provider.connection, buyerAta)).amount).to.equal(price);
    expect(await provider.connection.getAccountInfo(spot)).to.equal(null);
    expect(await provider.connection.getAccountInfo(vault)).to.equal(null);

    // Every spot is closed, so the rent payer can close the listing and take its rent back.
    await program.methods
      .closeListing()
      .accountsStrict({ rentPayer: wallet.publicKey, listing })
      .rpc();
    expect(await provider.connection.getAccountInfo(listing)).to.equal(null);
  });

  it("lets anyone refund a missing proof only after the delivery deadline", async () => {
    // A rent payer apart from the seller shows the rent goes to it, not the seller. It is also exact:
    // the provider wallet is the local validator's mint, too large for exact lamport math in a number.
    const { buyerAta, caller, deliveryDeadline, listing, paymentMint, rentPayer, spot, vault } =
      await fundedSpot(8, 8 + minProofWindowSeconds, anchor.web3.Keypair.generate());
    const refund = () =>
      program.methods
        .refundSpot()
        .accountsStrict({
          caller: caller.publicKey,
          listing,
          rentPayer: rentPayer.publicKey,
          spot,
          vault,
          buyerAta,
          usdcMint: paymentMint,
          tokenProgram: TOKEN_PROGRAM_ID,
          proof: null,
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
    const rent =
      (await provider.connection.getBalance(spot)) + (await provider.connection.getBalance(vault));
    const rentPayerBefore = await provider.connection.getBalance(rentPayer.publicKey);
    await refund();
    expect((await provider.connection.getBalance(rentPayer.publicKey)) - rentPayerBefore).to.equal(
      rent,
    );
    expect((await getAccount(provider.connection, buyerAta)).amount).to.equal(price);
    expect(await provider.connection.getAccountInfo(spot)).to.equal(null);
    expect(await provider.connection.getAccountInfo(vault)).to.equal(null);
  });
});
