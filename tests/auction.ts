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

describe("auction spots", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.spotboard as Program<Spotboard>;
  const wallet = provider.wallet as anchor.Wallet;
  const alice = anchor.web3.Keypair.generate();
  const bob = anchor.web3.Keypair.generate();
  const slugHash = Array.from({ length: 16 }, (_, index) => 100 + index);
  const startPrice = new anchor.BN(100_000_000);
  const increment = new anchor.BN(10_000_000);
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
  let paymentMint: anchor.web3.PublicKey;
  let aliceAta: anchor.web3.PublicKey;
  let bobAta: anchor.web3.PublicKey;

  before(async () => {
    ({ paymentMint } = await ensureConfig(program, provider));

    for (const bidder of [alice, bob]) {
      const signature = await provider.connection.requestAirdrop(
        bidder.publicKey,
        anchor.web3.LAMPORTS_PER_SOL,
      );
      await provider.connection.confirmTransaction(signature, "confirmed");
    }

    aliceAta = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      paymentMint,
      alice.publicKey,
    );
    bobAta = await createAssociatedTokenAccount(
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
        1_000_000_000,
      );
    }

    const now = Math.floor(Date.now() / 1000);
    await program.methods
      .createListing(
        slugHash,
        "https://spotboard.test/listings/anchor-auction",
        1,
        new anchor.BN(now + 3600),
        new anchor.BN(now + 7200),
        new anchor.BN(now + 86400),
        increment,
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
      .addSpot(startPrice)
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
  });

  it("accepts an opening bid at the starting price", async () => {
    await program.methods
      .placeBid(startPrice)
      .accountsStrict({
        bidder: alice.publicKey,
        listing,
        spot,
        vault,
        bidderAta: aliceAta,
        prevBidderAta: null,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([alice])
      .rpc();

    const stored = await program.account.spot.fetch(spot);
    expect(stored.currentBid.eq(startPrice)).to.equal(true);
    expect(stored.currentBidder.equals(alice.publicKey)).to.equal(true);
  });

  it("enforces the increment and refunds the previous bidder", async () => {
    let rejected = false;
    try {
      await program.methods
        .placeBid(startPrice.add(increment).subn(1))
        .accountsStrict({
          bidder: bob.publicKey,
          listing,
          spot,
          vault,
          bidderAta: bobAta,
          prevBidderAta: aliceAta,
          usdcMint: paymentMint,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([bob])
        .rpc();
    } catch {
      rejected = true;
    }
    expect(rejected).to.equal(true);

    const nextBid = startPrice.add(increment);
    await program.methods
      .placeBid(nextBid)
      .accountsStrict({
        bidder: bob.publicKey,
        listing,
        spot,
        vault,
        bidderAta: bobAta,
        prevBidderAta: aliceAta,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([bob])
      .rpc();

    const stored = await program.account.spot.fetch(spot);
    const aliceAccount = await getAccount(provider.connection, aliceAta);
    const vaultAccount = await getAccount(provider.connection, vault);
    expect(stored.currentBid.eq(nextBid)).to.equal(true);
    expect(stored.currentBidder.equals(bob.publicKey)).to.equal(true);
    expect(aliceAccount.amount).to.equal(1_000_000_000n);
    expect(vaultAccount.amount).to.equal(110_000_000n);
  });

  it("requires double the current bid in double-up mode", async () => {
    const first = anchor.web3.Keypair.generate();
    const second = anchor.web3.Keypair.generate();
    const doubleSlug = Array.from({ length: 16 }, (_, index) => 200 + index);
    const [doubleListing] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("listing"), wallet.publicKey.toBuffer(), Buffer.from(doubleSlug)],
      program.programId,
    );
    const [doubleSpot] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("spot"), doubleListing.toBuffer(), Buffer.from([0])],
      program.programId,
    );
    const [doubleVault] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), doubleSpot.toBuffer()],
      program.programId,
    );

    for (const bidder of [first, second]) {
      const signature = await provider.connection.requestAirdrop(
        bidder.publicKey,
        anchor.web3.LAMPORTS_PER_SOL,
      );
      await provider.connection.confirmTransaction(signature, "confirmed");
    }
    const firstAta = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      paymentMint,
      first.publicKey,
    );
    const secondAta = await createAssociatedTokenAccount(
      provider.connection,
      wallet.payer,
      paymentMint,
      second.publicKey,
    );
    for (const account of [firstAta, secondAta]) {
      await mintTo(
        provider.connection,
        wallet.payer,
        paymentMint,
        account,
        wallet.payer,
        1_000_000_000,
      );
    }

    const now = Math.floor(Date.now() / 1000);
    await program.methods
      .createListing(
        doubleSlug,
        "https://spotboard.test/listings/anchor-double-up",
        1,
        new anchor.BN(now + 3600),
        new anchor.BN(now + 7200),
        new anchor.BN(now + 86400),
        new anchor.BN(0),
        true,
      )
      .accountsStrict({
        seller: wallet.publicKey,
        listing: doubleListing,
        config,
        usdcMint: paymentMint,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    await program.methods
      .addSpot(startPrice)
      .accountsStrict({
        seller: wallet.publicKey,
        listing: doubleListing,
        spot: doubleSpot,
        vault: doubleVault,
        usdcMint: paymentMint,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();
    await program.methods
      .placeBid(startPrice)
      .accountsStrict({
        bidder: first.publicKey,
        listing: doubleListing,
        spot: doubleSpot,
        vault: doubleVault,
        bidderAta: firstAta,
        prevBidderAta: null,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([first])
      .rpc();

    let rejected = false;
    try {
      await program.methods
        .placeBid(new anchor.BN(150_000_000))
        .accountsStrict({
          bidder: second.publicKey,
          listing: doubleListing,
          spot: doubleSpot,
          vault: doubleVault,
          bidderAta: secondAta,
          prevBidderAta: firstAta,
          usdcMint: paymentMint,
          tokenProgram: TOKEN_PROGRAM_ID,
        })
        .signers([second])
        .rpc();
    } catch {
      rejected = true;
    }
    expect(rejected).to.equal(true);

    await program.methods
      .placeBid(new anchor.BN(200_000_000))
      .accountsStrict({
        bidder: second.publicKey,
        listing: doubleListing,
        spot: doubleSpot,
        vault: doubleVault,
        bidderAta: secondAta,
        prevBidderAta: firstAta,
        usdcMint: paymentMint,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([second])
      .rpc();

    const stored = await program.account.spot.fetch(doubleSpot);
    const refunded = await getAccount(provider.connection, firstAta);
    expect(stored.currentBid.eq(new anchor.BN(200_000_000))).to.equal(true);
    expect(refunded.amount).to.equal(1_000_000_000n);
  });
});
