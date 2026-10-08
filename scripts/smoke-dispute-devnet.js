const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const anchor = require("@coral-xyz/anchor");
const {
  createAccount,
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
const PRICE = 1_000_000n;

function readKeypair(variable) {
  const file = process.env[variable];
  if (!file) throw new Error(`${variable} is required`);
  return anchor.web3.Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(fs.readFileSync(file, "utf8"))),
  );
}

async function validatorTime(connection) {
  const account = await connection.getAccountInfo(anchor.web3.SYSVAR_CLOCK_PUBKEY);
  if (!account) throw new Error("Devnet clock is unavailable");
  return Number(account.data.readBigInt64LE(32));
}

async function waitUntil(connection, timestamp) {
  const timeout = Date.now() + 90_000;
  while ((await validatorTime(connection)) < timestamp) {
    if (Date.now() > timeout) throw new Error("Timed out waiting for devnet clock");
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }
}

async function main() {
  if (process.argv[2] !== "--confirm") {
    throw new Error("Usage: node scripts/smoke-dispute-devnet.js --confirm");
  }

  const seller = readKeypair("ADSPACES_DEPLOY_WALLET");
  const buyer = readKeypair("ADSPACES_BUYER_WALLET");
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
  if (!storedConfig.authority.equals(seller.publicKey)) {
    throw new Error("Seller wallet is not the devnet dispute resolver");
  }

  const buyerAta = await getAssociatedTokenAddress(PAYMENT_MINT, buyer.publicKey);
  const sellerAta = await getAssociatedTokenAddress(PAYMENT_MINT, seller.publicKey);
  let treasuryAta = await getAssociatedTokenAddress(PAYMENT_MINT, storedConfig.treasury);
  if (treasuryAta.equals(sellerAta)) {
    treasuryAta = await createAccount(
      connection,
      seller,
      PAYMENT_MINT,
      storedConfig.treasury,
      anchor.web3.Keypair.generate(),
    );
  }
  const buyerBefore = (await getAccount(connection, buyerAta)).amount;
  const sellerBefore = (await getAccount(connection, sellerAta)).amount;
  const treasuryBefore = (await getAccount(connection, treasuryAta)).amount;
  if (buyerBefore < PRICE) throw new Error("Buyer needs at least 1 devnet USDC");

  const pendingDisputes = await program.account.disputeRecord.all();
  for (const pending of pendingDisputes) {
    if (!pending.account.buyer.equals(buyer.publicKey) || pending.account.decidedAt.toNumber() !== 0) {
      continue;
    }
    const pendingSpot = await program.account.spot.fetch(pending.account.spot);
    const pendingListing = await program.account.listing.fetch(pendingSpot.listing);
    if (!pendingListing.seller.equals(seller.publicKey) || pendingSpot.status !== 4) continue;

    const [pendingProof] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("proof"), pending.account.spot.toBuffer()],
      PROGRAM_ID,
    );
    const signature = await program.methods
      .resolveDispute(5_000, Array(32).fill(3), "https://adspaces.fun/devnet-decision")
      .accountsStrict({
        resolver: seller.publicKey,
        config,
        listing: pendingSpot.listing,
        spot: pending.account.spot,
        proof: pending.account.proofSubmitted ? pendingProof : null,
        dispute: pending.publicKey,
        vault: pendingSpot.vault,
        sellerAta,
        seller: seller.publicKey,
        buyerAta,
        treasuryAta,
        usdcMint: PAYMENT_MINT,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
    console.log(`Recovered dispute: ${pending.publicKey}`);
    console.log(`Resolution: ${signature}`);
    console.log("Pending devnet dispute resolved");
    return;
  }

  const slugHash = Array.from(
    crypto.createHash("sha256").update(`devnet-dispute-${Date.now()}`).digest().subarray(0, 16),
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
  const [proof] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("proof"), spot.toBuffer()],
    PROGRAM_ID,
  );
  const [dispute] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("dispute"), spot.toBuffer()],
    PROGRAM_ID,
  );
  const now = await validatorTime(connection);
  const listingDeadline = now + 15;
  const deliveryDeadline = now + 30;

  await program.methods
    .createListing(
      slugHash,
      "https://adspaces.fun/devnet-dispute-smoke",
      0,
      new anchor.BN(listingDeadline),
      new anchor.BN(listingDeadline),
      new anchor.BN(deliveryDeadline),
      new anchor.BN(48 * 60 * 60),
      new anchor.BN(0),
      false,
      250,
      0,
      250,
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
    .addSpot(new anchor.BN(PRICE.toString()))
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
      usdcMint: PAYMENT_MINT,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .signers([buyer])
    .rpc();

  await waitUntil(connection, listingDeadline);
  const proofSignature = await program.methods
    .submitProof(Array(32).fill(1), "https://adspaces.fun/devnet-proof")
    .accountsStrict({
      seller: seller.publicKey,
      listing,
      spot,
      proof,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .rpc();

  await waitUntil(connection, deliveryDeadline);
  const disputeSignature = await program.methods
    .raiseDispute(Array(32).fill(2), "https://adspaces.fun/devnet-evidence")
    .accountsStrict({
      buyer: buyer.publicKey,
      listing,
      spot,
      dispute,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .signers([buyer])
    .rpc();
  const resolutionSignature = await program.methods
    .resolveDispute(5_000, Array(32).fill(3), "https://adspaces.fun/devnet-decision")
    .accountsStrict({
      resolver: seller.publicKey,
      config,
      listing,
      spot,
      proof,
      dispute,
      vault,
      sellerAta,
      seller: seller.publicKey,
      buyerAta,
      treasuryAta,
      usdcMint: PAYMENT_MINT,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .rpc();

  const buyerAfter = (await getAccount(connection, buyerAta)).amount;
  const sellerAfter = (await getAccount(connection, sellerAta)).amount;
  const treasuryAfter = (await getAccount(connection, treasuryAta)).amount;
  if (
    (await connection.getAccountInfo(dispute)) ||
    (await connection.getAccountInfo(proof)) ||
    buyerAfter !== buyerBefore - 500_000n ||
    sellerAfter !== sellerBefore + 487_500n ||
    treasuryAfter !== treasuryBefore + 12_500n ||
    (await connection.getAccountInfo(spot)) ||
    (await connection.getAccountInfo(vault))
  ) {
    throw new Error("Resolved dispute balances or account state are incorrect");
  }

  console.log(`Listing: ${listing}`);
  console.log(`Dispute: ${dispute}`);
  console.log(`Proof: ${proofSignature}`);
  console.log(`Dispute raised: ${disputeSignature}`);
  console.log(`Resolution: ${resolutionSignature}`);
  console.log("Devnet dispute smoke test passed");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
