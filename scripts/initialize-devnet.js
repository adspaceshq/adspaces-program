const fs = require("node:fs");
const path = require("node:path");
const anchor = require("@coral-xyz/anchor");

const PROGRAM_ID = new anchor.web3.PublicKey(
  "6ft49ExLRbHUcY6crReW7qibZLrS5rdCudKWQRp2LMSW",
);
const PAYMENT_MINT = new anchor.web3.PublicKey(
  "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU",
);
const RPC_URL = process.env.ADSPACES_DEVNET_RPC || "https://api.devnet.solana.com";

async function main() {
  if (process.argv[2] !== "--confirm") {
    throw new Error("Usage: node scripts/initialize-devnet.js --confirm");
  }

  const walletPath = process.env.ADSPACES_DEPLOY_WALLET;
  if (!walletPath) throw new Error("ADSPACES_DEPLOY_WALLET is required");

  const secret = Uint8Array.from(JSON.parse(fs.readFileSync(walletPath, "utf8")));
  const payer = anchor.web3.Keypair.fromSecretKey(secret);
  const wallet = new anchor.Wallet(payer);
  const connection = new anchor.web3.Connection(RPC_URL, "confirmed");
  const provider = new anchor.AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });
  const idlPath = path.join(__dirname, "..", "target", "idl", "adspaces.json");
  const program = new anchor.Program(JSON.parse(fs.readFileSync(idlPath, "utf8")), provider);
  const treasury = new anchor.web3.PublicKey(
    process.env.ADSPACES_TREASURY_OWNER || wallet.publicKey,
  );
  const feeBps = Number(process.env.ADSPACES_FEE_BPS || "250");
  if (!Number.isInteger(feeBps) || feeBps < 0 || feeBps > 10_000) {
    throw new Error("ADSPACES_FEE_BPS must be an integer from 0 to 10000");
  }

  const [config] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    PROGRAM_ID,
  );
  const [reviewAuthority] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("review_authority"), config.toBuffer()],
    PROGRAM_ID,
  );
  const existing = await connection.getAccountInfo(config);
  if (existing) {
    const stored = await program.account.config.fetch(config);
    if (
      !stored.authority.equals(wallet.publicKey) ||
      !stored.paymentMint.equals(PAYMENT_MINT) ||
      !stored.treasury.equals(treasury) ||
      stored.feeBps !== feeBps
    ) {
      throw new Error("Existing devnet config does not match the requested settings");
    }
    console.log(`Config already initialized: ${config}`);
    console.log(`Authority: ${stored.authority}`);
    console.log(`Payment mint: ${stored.paymentMint}`);
    console.log(`Treasury owner: ${stored.treasury}`);
    console.log(`Fee: ${stored.feeBps} bps`);
  } else {
    const programAccount = await connection.getAccountInfo(PROGRAM_ID);
    if (!programAccount || !programAccount.executable) {
      throw new Error("AdSpaces program is not deployed on devnet");
    }
    const programData = new anchor.web3.PublicKey(programAccount.data.subarray(4, 36));
    const signature = await program.methods
      .initializeConfig(treasury, feeBps)
      .accountsStrict({
        authority: wallet.publicKey,
        config,
        program: PROGRAM_ID,
        programData,
        paymentMint: PAYMENT_MINT,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    console.log(`Config signature: ${signature}`);
  }

  const existingReviewer = await connection.getAccountInfo(reviewAuthority);
  if (existingReviewer) {
    const stored = await program.account.reviewAuthority.fetch(reviewAuthority);
    if (!stored.authority.equals(wallet.publicKey)) {
      throw new Error("Existing review authority does not match the deploy wallet");
    }
    console.log(`Review authority already initialized: ${reviewAuthority}`);
  } else {
    const signature = await program.methods
      .initializeReviewAuthority(wallet.publicKey)
      .accountsStrict({
        authority: wallet.publicKey,
        config,
        reviewAuthority,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    console.log(`Review authority signature: ${signature}`);
    console.log(`Review authority: ${reviewAuthority}`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
