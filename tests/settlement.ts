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

describe("spot settlement", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.adspaces as Program<Adspaces>;
  const wallet = provider.wallet as anchor.Wallet;
  const buyer = anchor.web3.Keypair.generate();
  const caller = anchor.web3.Keypair.generate();
  const slugHash = Array.from({ length: 16 }, (_, index) => 50 + index);
  const price = new anchor.BN(150_000_000);
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
  let paymentMint: anchor.web3.PublicKey;
  let treasury: anchor.web3.PublicKey;
  let buyerAta: anchor.web3.PublicKey;
  let sellerAta: anchor.web3.PublicKey;
  let treasuryAta: anchor.web3.PublicKey;
  let sellerInitial: bigint;
  let treasuryInitial: bigint;

  before(async () => {
    ({ paymentMint, treasury } = await ensureConfig(program, provider));

    for (const user of [buyer, caller]) {
      const signature = await provider.connection.requestAirdrop(
        user.publicKey,
        anchor.web3.LAMPORTS_PER_SOL,
      );
      await provider.connection.confirmTransaction(signature, "confirmed");
    }

    buyerAta = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      paymentMint,
      buyer.publicKey,
    );
    sellerAta = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        wallet.payer,
        paymentMint,
        wallet.publicKey,
      )
    ).address;
    treasuryAta = (
      await getOrCreateAssociatedTokenAccount(
        provider.connection,
        wallet.payer,
        paymentMint,
        treasury,
      )
    ).address;
    sellerInitial = (await getAccount(provider.connection, sellerAta)).amount;
    treasuryInitial = (await getAccount(provider.connection, treasuryAta)).amount;
    await mintTo(
      provider.connection,
      wallet.payer,
      paymentMint,
      buyerAta,
      wallet.payer,
      1_000_000_000,
    );

    const now = await validatorTime();
    await program.methods
      .createListing(
        slugHash,
        "https://adspaces.test/listings/anchor-settlement",
        0,
        new anchor.BN(now + 3600),
        new anchor.BN(now + 3600),
        new anchor.BN(now + 86400),
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
      .addSpot(price)
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
  });

  it("rejects settlement before the deadline", async () => {
    let rejected = false;
    try {
      await settle();
    } catch {
      rejected = true;
    }
    expect(rejected).to.equal(true);
    expect((await getAccount(provider.connection, vault)).amount).to.equal(150_000_000n);
    expect((await getAccount(provider.connection, sellerAta)).amount).to.equal(sellerInitial);
    expect((await getAccount(provider.connection, treasuryAta)).amount).to.equal(treasuryInitial);
    expect((await program.account.spot.fetch(spot)).status).to.equal(1);
  });

  async function settle() {
    await program.methods
      .withdrawSeller()
      .accountsStrict({
        caller: caller.publicKey,
        listing,
        seller: wallet.publicKey,
        spot,
        vault,
        sellerAta,
        treasuryAta,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([caller])
      .rpc();
  }

  async function validatorTime(): Promise<number> {
    const clock = await provider.connection.getAccountInfo(anchor.web3.SYSVAR_CLOCK_PUBKEY);
    if (!clock) throw new Error("Validator clock is unavailable");
    return Number(clock.data.readBigInt64LE(32));
  }
});
