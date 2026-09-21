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

describe("fixed-price spots", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.spotboard as Program<Spotboard>;
  const wallet = provider.wallet as anchor.Wallet;
  const buyer = anchor.web3.Keypair.generate();
  const slugHash = Array.from({ length: 16 }, (_, index) => index + 1);
  const price = new anchor.BN(150_000_000);
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
  let buyerAta: anchor.web3.PublicKey;

  before(async () => {
    ({ paymentMint } = await ensureConfig(program, provider));

    const airdrop = await provider.connection.requestAirdrop(
      buyer.publicKey,
      anchor.web3.LAMPORTS_PER_SOL,
    );
    await provider.connection.confirmTransaction(airdrop, "confirmed");

    buyerAta = await createAssociatedTokenAccount(
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
      1_000_000_000,
    );

    const now = Math.floor(Date.now() / 1000);
    await program.methods
      .createListing(
        slugHash,
        "https://spotboard.test/listings/anchor-fixed",
        0,
        new anchor.BN(now + 3600),
        new anchor.BN(now + 7200),
        new anchor.BN(now + 86400),
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
  });

  it("claims a spot and persists the buyer", async () => {
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

    const stored = await program.account.spot.fetch(spot);
    const vaultAccount = await getAccount(provider.connection, vault);
    expect(stored.status).to.equal(1);
    expect(stored.buyer.equals(buyer.publicKey)).to.equal(true);
    expect(vaultAccount.amount).to.equal(150_000_000n);
  });

  it("rejects a second claim", async () => {
    let rejected = false;
    try {
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
    } catch {
      rejected = true;
    }

    expect(rejected).to.equal(true);
  });
});
