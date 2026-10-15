const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const multisig = require("@sqds/multisig");
const { Connection, Keypair, PublicKey } = require("@solana/web3.js");

// Creates the 2-of-3 Squads v4 multisig that will hold the program upgrade authority and the
// config authority. The multisig has no config authority of its own, so changing members or the
// threshold also needs 2 of 3 votes. Authorities are handed to the multisig's vault (index 0).
const RPC_URL = process.env.ADSPACES_DEVNET_RPC || "https://api.devnet.solana.com";
const THRESHOLD = 2;
const MEMBERS = [
  ["meet", "F8y1d9t12YsADDHuYL2tN5wzpQ6emud5q5d29d3JYjf9"],
  ["sid", "9FojkGWy5eMruo4JUWHLgmgQotW4wShpR4qAPyMD88za"],
  ["paper backup", "A5gnSvMANxfsNadxBd5XaWCUnxTk2pTAjFQNxSSJXHFJ"],
];
const statePath = path.join(__dirname, "..", ".anchor", "devnet-multisig.json");

async function main() {
  const confirm = process.argv.includes("--confirm");
  // Ao (the program and config authority) pays for every devnet admin action.
  const walletPath = process.env.ADSPACES_DEPLOY_WALLET || path.join(os.homedir(), ".config/solana/adspaces-devnet.json");
  if (fs.existsSync(statePath)) {
    const saved = JSON.parse(fs.readFileSync(statePath, "utf8"));
    throw new Error(`Multisig already created: ${saved.multisig} (vault ${saved.vault})`);
  }

  const payer = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(walletPath, "utf8"))));
  const connection = new Connection(RPC_URL, "confirmed");
  const createKey = Keypair.generate();
  const [multisigPda] = multisig.getMultisigPda({ createKey: createKey.publicKey });
  const [vaultPda] = multisig.getVaultPda({ multisigPda, index: 0 });
  const members = MEMBERS.map(([, address]) => ({
    key: new PublicKey(address),
    permissions: multisig.types.Permissions.all(),
  }));

  console.log(`Payer: ${payer.publicKey}`);
  for (const [name, address] of MEMBERS) console.log(`Member ${name}: ${address}`);
  console.log(`Threshold: ${THRESHOLD} of ${MEMBERS.length}`);
  console.log(`Multisig: ${multisigPda}`);
  console.log(`Vault (the authority address): ${vaultPda}`);
  if (!confirm) {
    console.log("Dry run. Re-run with --confirm to create it (addresses change on each run).");
    return;
  }

  const [programConfigPda] = multisig.getProgramConfigPda({});
  const programConfig = await multisig.accounts.ProgramConfig.fromAccountAddress(connection, programConfigPda);
  const signature = await multisig.rpc.multisigCreateV2({
    connection,
    treasury: programConfig.treasury,
    createKey,
    creator: payer,
    multisigPda,
    configAuthority: null,
    threshold: THRESHOLD,
    members,
    timeLock: 0,
    rentCollector: null,
    sendOptions: { skipPreflight: false },
  });
  await connection.confirmTransaction(signature, "confirmed");

  const created = await multisig.accounts.Multisig.fromAccountAddress(connection, multisigPda);
  if (created.threshold !== THRESHOLD || created.members.length !== MEMBERS.length) {
    throw new Error("Created multisig does not match the requested members and threshold");
  }
  fs.mkdirSync(path.dirname(statePath), { recursive: true });
  fs.writeFileSync(
    statePath,
    `${JSON.stringify({ multisig: multisigPda.toBase58(), vault: vaultPda.toBase58(), threshold: THRESHOLD, members: MEMBERS, signature }, null, 2)}\n`,
  );
  console.log(`Created: ${signature}`);
  console.log(`Saved: ${statePath}`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
