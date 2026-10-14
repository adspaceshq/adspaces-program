const fs = require("node:fs");
const path = require("node:path");
const anchor = require("@coral-xyz/anchor");

const PROGRAM_ID = new anchor.web3.PublicKey(
  "8Vg24teSaNrY4AmksQ8hZw2z2RRPuobeCtir7jVTYGw5",
);
const RPC_URL = process.env.ADSPACES_DEVNET_RPC || "https://api.devnet.solana.com";

async function main() {
  if (process.argv[2] !== "--confirm") {
    throw new Error("Usage: node scripts/migrate-config-devnet.js --confirm");
  }
  const walletPath = process.env.ADSPACES_DEPLOY_WALLET;
  if (!walletPath) throw new Error("ADSPACES_DEPLOY_WALLET is required");

  const secret = Uint8Array.from(JSON.parse(fs.readFileSync(walletPath, "utf8")));
  const wallet = new anchor.Wallet(anchor.web3.Keypair.fromSecretKey(secret));
  const connection = new anchor.web3.Connection(RPC_URL, "confirmed");
  const provider = new anchor.AnchorProvider(connection, wallet, { commitment: "confirmed" });
  const idlPath = path.join(__dirname, "..", "target", "idl", "adspaces.json");
  const program = new anchor.Program(JSON.parse(fs.readFileSync(idlPath, "utf8")), provider);

  const [config] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    PROGRAM_ID,
  );
  const before = await connection.getAccountInfo(config);
  if (!before) throw new Error("Config is not initialized on devnet");
  console.log(`Config size before: ${before.data.length} bytes`);

  const signature = await program.methods
    .migrateConfig()
    .accountsStrict({
      authority: wallet.publicKey,
      config,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .rpc();
  console.log(`Migrate signature: ${signature}`);

  const stored = await program.account.config.fetch(config);
  const after = await connection.getAccountInfo(config);
  console.log(`Config size after: ${after.data.length} bytes`);
  console.log(`Authority: ${stored.authority}`);
  console.log(`Treasury: ${stored.treasury}`);
  console.log(`Fee: ${stored.feeBps} bps`);
  console.log(`Paused: ${stored.paused}`);
  console.log(`Dispute timeout: ${stored.disputeTimeoutSeconds} seconds`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
