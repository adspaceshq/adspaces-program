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
const PRICE = 1_000_000n;

function readKeypair(variable) {
  const file = process.env[variable];
  if (!file) throw new Error(`${variable} is required`);
  return anchor.web3.Keypair.fromSecretKey(
    Uint8Array.from(JSON.parse(fs.readFileSync(file, "utf8"))),
  );
}

async function main() {
  if (process.argv[2] !== "--confirm") {
    throw new Error("Usage: node scripts/smoke-devnet.js --confirm");
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
  if (!storedConfig.paymentMint.equals(PAYMENT_MINT)) {
    throw new Error("Devnet config payment mint is unexpected");
  }

  const buyerAta = await getAssociatedTokenAddress(PAYMENT_MINT, buyer.publicKey);
  const startingBalance = (await getAccount(connection, buyerAta)).amount;
  if (startingBalance < PRICE) throw new Error("Buyer needs at least 1 devnet USDC");

  const slugHash = Array.from(
    crypto.createHash("sha256").update(`devnet-smoke-${Date.now()}`).digest().subarray(0, 16),
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

  const listingSignature = await program.methods
    .createListing(
      slugHash,
      "https://adspaces.fun/devnet-smoke",
      0,
      new anchor.BN(now + 3600),
      new anchor.BN(now + 7200),
      new anchor.BN(now + 10_800),
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

  const spotSignature = await program.methods
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

  const claimSignature = await program.methods
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

  const fundedVault = await getAccount(connection, vault);
  if (fundedVault.amount !== PRICE) throw new Error("Vault balance is incorrect after claim");

  const refundSignature = await program.methods
    .refundSpot()
    .accountsStrict({
      caller: seller.publicKey,
      listing,
      seller: seller.publicKey,
      spot,
      vault,
      buyerAta,
      usdcMint: PAYMENT_MINT,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .rpc();

  const storedSpot = await program.account.spot.fetch(spot);
  const endingBalance = (await getAccount(connection, buyerAta)).amount;
  if (storedSpot.status !== 3 || endingBalance !== startingBalance) {
    throw new Error("Refund did not restore the buyer balance and cancel the spot");
  }
  if (await connection.getAccountInfo(vault)) throw new Error("Vault was not closed after refund");

  console.log(`Listing: ${listing}`);
  console.log(`Spot: ${spot}`);
  console.log(`Create listing: ${listingSignature}`);
  console.log(`Add spot: ${spotSignature}`);
  console.log(`Claim: ${claimSignature}`);
  console.log(`Refund: ${refundSignature}`);
  console.log("Devnet smoke test passed");
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
