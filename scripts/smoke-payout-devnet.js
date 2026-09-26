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
const DEFAULT_REVIEW_WINDOW_SECONDS = 48 * 60 * 60;
const STATE_PATH = path.join(__dirname, "..", ".anchor", "devnet-payout.json");

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
  const timeout = Date.now() + 240_000;
  while ((await validatorTime(connection)) < timestamp) {
    if (Date.now() > timeout) throw new Error("Timed out waiting for devnet clock");
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }
}

function writeState(state) {
  fs.mkdirSync(path.dirname(STATE_PATH), { recursive: true });
  fs.writeFileSync(STATE_PATH, `${JSON.stringify(state, null, 2)}\n`);
}

function client() {
  const seller = readKeypair("ADSPACES_DEPLOY_WALLET");
  const buyer = readKeypair("ADSPACES_BUYER_WALLET");
  const connection = new anchor.web3.Connection(RPC_URL, "confirmed");
  const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(seller), {
    commitment: "confirmed",
  });
  const idlPath = path.join(__dirname, "..", "target", "idl", "adspaces.json");
  const program = new anchor.Program(JSON.parse(fs.readFileSync(idlPath, "utf8")), provider);
  return { buyer, connection, program, seller };
}

async function start() {
  if (fs.existsSync(STATE_PATH)) {
    throw new Error(`Pending payout state already exists at ${STATE_PATH}`);
  }
  const { buyer, connection, program, seller } = client();
  const [config] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    PROGRAM_ID,
  );
  const [reviewAuthority] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("review_authority"), config.toBuffer()],
    PROGRAM_ID,
  );
  const storedConfig = await program.account.config.fetch(config);
  const buyerAta = await getAssociatedTokenAddress(PAYMENT_MINT, buyer.publicKey);
  const sellerAta = await getAssociatedTokenAddress(PAYMENT_MINT, seller.publicKey);
  const buyerBefore = (await getAccount(connection, buyerAta)).amount;
  if (buyerBefore < PRICE) throw new Error("Buyer needs at least 1 devnet USDC");
  const treasuryAccount = await createAccount(
    connection,
    seller,
    PAYMENT_MINT,
    storedConfig.treasury,
    anchor.web3.Keypair.generate(),
  );

  const fundedSpots = await program.account.spot.all();
  for (const candidate of fundedSpots) {
    if (candidate.account.status !== 1 || !candidate.account.buyer.equals(buyer.publicKey)) continue;
    const candidateListing = await program.account.listing.fetch(candidate.account.listing);
    if (!candidateListing.seller.equals(seller.publicKey)) continue;
    const currentTime = await validatorTime(connection);
    if (candidate.account.proofSubmittedAt.toNumber() === 0) {
      if (currentTime >= candidateListing.deliveryDeadline.toNumber()) {
        await program.methods
          .refundSpot()
          .accountsStrict({
            caller: seller.publicKey,
            listing: candidate.account.listing,
            seller: seller.publicKey,
            spot: candidate.publicKey,
            vault: candidate.account.vault,
            buyerAta,
            usdcMint: PAYMENT_MINT,
            tokenProgram: TOKEN_PROGRAM_ID,
          })
          .rpc();
        console.log(`Refunded interrupted spot: ${candidate.publicKey}`);
        continue;
      }
      await waitUntil(connection, candidateListing.deadline.toNumber());
      const [candidateProof] = anchor.web3.PublicKey.findProgramAddressSync(
        [Buffer.from("proof"), candidate.publicKey.toBuffer()],
        PROGRAM_ID,
      );
      const proofSignature = await program.methods
        .submitProof(Array(32).fill(7), "https://adspaces.fun/devnet-payout-proof")
        .accountsStrict({
          seller: seller.publicKey,
          listing: candidate.account.listing,
          spot: candidate.publicKey,
          proof: candidateProof,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .rpc();
      writeState({
        buyerAta: buyerAta.toBase58(),
        eligibleAt:
          candidateListing.deliveryDeadline.toNumber() +
          candidateListing.reviewWindowSeconds.toNumber(),
        listing: candidate.account.listing.toBase58(),
        proof: candidateProof.toBase58(),
        sellerAta: sellerAta.toBase58(),
        spot: candidate.publicKey.toBase58(),
        treasuryAccount: treasuryAccount.toBase58(),
        vault: candidate.account.vault.toBase58(),
      });
      console.log(`Recovered funded spot: ${candidate.publicKey}`);
      console.log(`Proof: ${proofSignature}`);
      console.log(
        `Withdrawal eligible: ${new Date(
          (candidateListing.deliveryDeadline.toNumber() +
            candidateListing.reviewWindowSeconds.toNumber()) *
            1000,
        ).toISOString()}`,
      );
      console.log("Devnet payout smoke test staged");
      return;
    }
  }

  const slugHash = Array.from(
    crypto.createHash("sha256").update(`devnet-payout-${Date.now()}`).digest().subarray(0, 16),
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
  const now = await validatorTime(connection);
  const listingDeadline = now + 120;
  const deliveryDeadline = now + 180;

  await program.methods
    .createListing(
      slugHash,
      "https://adspaces.fun/devnet-payout-smoke",
      0,
      new anchor.BN(listingDeadline),
      new anchor.BN(listingDeadline),
      new anchor.BN(deliveryDeadline),
      new anchor.BN(DEFAULT_REVIEW_WINDOW_SECONDS),
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
    .submitProof(Array(32).fill(7), "https://adspaces.fun/devnet-payout-proof")
    .accountsStrict({
      seller: seller.publicKey,
      listing,
      spot,
      proof,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .rpc();

  const eligibleAt = deliveryDeadline + DEFAULT_REVIEW_WINDOW_SECONDS;
  writeState({
    buyerAta: buyerAta.toBase58(),
    eligibleAt,
    listing: listing.toBase58(),
    proof: proof.toBase58(),
    sellerAta: sellerAta.toBase58(),
    spot: spot.toBase58(),
    treasuryAccount: treasuryAccount.toBase58(),
    vault: vault.toBase58(),
  });
  console.log(`Listing: ${listing}`);
  console.log(`Proof: ${proofSignature}`);
  console.log(`Withdrawal eligible: ${new Date(eligibleAt * 1000).toISOString()}`);
  console.log(`State: ${STATE_PATH}`);
  console.log("Devnet payout smoke test staged");
}

async function finish() {
  if (!fs.existsSync(STATE_PATH)) throw new Error("No pending payout smoke test was found");
  const state = JSON.parse(fs.readFileSync(STATE_PATH, "utf8"));
  const { connection, program, seller } = client();
  const now = await validatorTime(connection);
  if (now < state.eligibleAt) {
    throw new Error(`Withdrawal becomes eligible at ${new Date(state.eligibleAt * 1000).toISOString()}`);
  }

  const sellerAta = new anchor.web3.PublicKey(state.sellerAta);
  const treasuryAta = new anchor.web3.PublicKey(state.treasuryAccount);
  const sellerBefore = (await getAccount(connection, sellerAta)).amount;
  const treasuryBefore = (await getAccount(connection, treasuryAta)).amount;
  const signature = await program.methods
    .withdrawSeller()
    .accountsStrict({
      caller: seller.publicKey,
      listing: new anchor.web3.PublicKey(state.listing),
      seller: seller.publicKey,
      spot: new anchor.web3.PublicKey(state.spot),
      vault: new anchor.web3.PublicKey(state.vault),
      sellerAta,
      treasuryAta,
      usdcMint: PAYMENT_MINT,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .rpc();
  const sellerAfter = (await getAccount(connection, sellerAta)).amount;
  const treasuryAfter = (await getAccount(connection, treasuryAta)).amount;
  if (
    sellerAfter !== sellerBefore + 975_000n ||
    treasuryAfter !== treasuryBefore + 25_000n ||
    (await program.account.spot.fetch(new anchor.web3.PublicKey(state.spot))).status !== 2 ||
    (await connection.getAccountInfo(new anchor.web3.PublicKey(state.vault)))
  ) {
    throw new Error("Seller payout, fee, or finalized account state is incorrect");
  }
  fs.unlinkSync(STATE_PATH);
  console.log(`Withdrawal: ${signature}`);
  console.log("Devnet payout smoke test passed");
}

async function main() {
  const action = process.argv[2];
  if (!["start", "finish"].includes(action) || process.argv[3] !== "--confirm") {
    throw new Error("Usage: node scripts/smoke-payout-devnet.js <start|finish> --confirm");
  }
  await (action === "start" ? start() : finish());
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
