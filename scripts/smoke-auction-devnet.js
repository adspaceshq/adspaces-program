const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const anchor = require("@coral-xyz/anchor");
const {
  getAccount,
  getAssociatedTokenAddress,
  TOKEN_PROGRAM_ID,
} = require("@solana/spl-token");

const PROGRAM_ID = new anchor.web3.PublicKey(
  "6ft49ExLRbHUcY6crReW7qibZLrS5rdCudKWQRp2LMSW",
);
const PAYMENT_MINT = new anchor.web3.PublicKey(
  "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
);
const RPC_URL = process.env.ADSPACES_DEVNET_RPC || "https://api.devnet.solana.com";
const OPENING_BID = 1_000_000n;
const TAKEOVER_BID = 1_100_000n;

function readKeypair(variable) {
  const file = process.env[variable];
  if (!file) throw new Error(`${variable} is required`);
  return anchor.web3.Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(fs.readFileSync(file, "utf8"))),
  );
}

async function main() {
  if (process.argv[2] !== "--confirm") {
    throw new Error("Usage: node scripts/smoke-auction-devnet.js --confirm");
  }

  const seller = readKeypair("ADSPACES_DEPLOY_WALLET");
  const alice = readKeypair("ADSPACES_BUYER_WALLET");
  const bob = readKeypair("ADSPACES_BIDDER_WALLET");
  const connection = new anchor.web3.Connection(RPC_URL, "confirmed");
  const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(seller), {
    commitment: "confirmed",
  });
  const idlPath = path.join(__dirname, "..", "target", "idl", "adspaces.json");
  const program = new anchor.Program(JSON.parse(fs.readFileSync(idlPath, "utf8")), provider);
  const [config] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    PROGRAM_ID,
  );
  const [reviewAuthority] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("review_authority"), config.toBuffer()],
    PROGRAM_ID,
  );
  const storedConfig = await program.account.config.fetch(config);
  if (!storedConfig.paymentMint.equals(PAYMENT_MINT)) {
    throw new Error("Devnet config payment mint is unexpected");
  }

  const aliceAta = await getAssociatedTokenAddress(PAYMENT_MINT, alice.publicKey);
  const bobAta = await getAssociatedTokenAddress(PAYMENT_MINT, bob.publicKey);
  const aliceBefore = (await getAccount(connection, aliceAta)).amount;
  const bobBefore = (await getAccount(connection, bobAta)).amount;
  if (aliceBefore < OPENING_BID || bobBefore < TAKEOVER_BID) {
    throw new Error("Each bidder needs enough devnet USDC");
  }

  const slugHash = Array.from(
    crypto.createHash("sha256").update(`devnet-auction-${Date.now()}`).digest().subarray(0, 16),
  );
  const [listing] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("listing"), seller.publicKey.toBuffer(), Buffer.from(slugHash)],
    PROGRAM_ID,
  );
  const [spot] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("spot"), listing.toBuffer(), Buffer.from([0])],
    PROGRAM_ID,
  );
  const [vault] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("vault"), spot.toBuffer()],
    PROGRAM_ID,
  );
  const now = Math.floor(Date.now() / 1000);

  await program.methods
    .createListing(
      slugHash,
      "https://adspaces.fun/devnet-auction-smoke",
      1,
      new anchor.BN(now + 3600),
      new anchor.BN(now + 7200),
      new anchor.BN(now + 10_800),
      new anchor.BN(48 * 60 * 60),
      new anchor.BN(100_000),
      false,
      250,
      1_000,
      750,
    )
    .accountsStrict({
      seller: seller.publicKey,
      reviewer: seller.publicKey,
      listing,
      config,
      reviewAuthority,
      usdcMint: PAYMENT_MINT,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .rpc();
  await program.methods
    .addSpot(new anchor.BN(OPENING_BID.toString()))
    .accountsStrict({
      seller: seller.publicKey,
      config,
      listing,
      spot,
      vault,
      usdcMint: PAYMENT_MINT,
      systemProgram: anchor.web3.SystemProgram.programId,
      tokenProgram: TOKEN_PROGRAM_ID,
      rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      reviewer: seller.publicKey,
      reviewAuthority,
    })
    .rpc();

  const openingSignature = await program.methods
    .placeBid(new anchor.BN(OPENING_BID.toString()))
    .accountsStrict({
      bidder: alice.publicKey,
      config,
      listing,
      spot,
      vault,
      bidderAta: aliceAta,
      prevBidderAta: null,
      usdcMint: PAYMENT_MINT,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .signers([alice])
    .rpc();
  if ((await getAccount(connection, aliceAta)).amount !== aliceBefore - OPENING_BID) {
    throw new Error("Opening bidder was not charged correctly");
  }

  const takeoverSignature = await program.methods
    .placeBid(new anchor.BN(TAKEOVER_BID.toString()))
    .accountsStrict({
      bidder: bob.publicKey,
      config,
      listing,
      spot,
      vault,
      bidderAta: bobAta,
      prevBidderAta: aliceAta,
      usdcMint: PAYMENT_MINT,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .signers([bob])
    .rpc();
  const storedSpot = await program.account.spot.fetch(spot);
  if (
    (await getAccount(connection, aliceAta)).amount !== aliceBefore ||
    (await getAccount(connection, bobAta)).amount !== bobBefore - TAKEOVER_BID ||
    (await getAccount(connection, vault)).amount !== TAKEOVER_BID ||
    !storedSpot.currentBidder.equals(bob.publicKey)
  ) {
    throw new Error("Auction takeover or previous-bidder refund is incorrect");
  }

  const refundSignature = await program.methods
    .refundSpot()
    .accountsStrict({
      caller: seller.publicKey,
      listing,
      seller: seller.publicKey,
      spot,
      vault,
      buyerAta: bobAta,
      usdcMint: PAYMENT_MINT,
      tokenProgram: TOKEN_PROGRAM_ID,
      proof: null,
    })
    .rpc();
  if (
    (await getAccount(connection, bobAta)).amount !== bobBefore ||
    (await connection.getAccountInfo(spot)) ||
    (await connection.getAccountInfo(vault))
  ) {
    throw new Error("Auction winner refund or vault closure is incorrect");
  }

  console.log(`Listing: ${listing}`);
  console.log(`Opening bid: ${openingSignature}`);
  console.log(`Takeover bid: ${takeoverSignature}`);
  console.log(`Refund: ${refundSignature}`);
  console.log("Devnet auction smoke test passed");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
