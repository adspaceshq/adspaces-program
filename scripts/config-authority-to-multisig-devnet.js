const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const anchor = require("@coral-xyz/anchor");
const bs58 = require("bs58");

// Moves the devnet config authority (fees, treasury, reviewer, pause, dispute resolution) from Ao
// to the Squads vault in two steps:
//   propose  Ao proposes the vault as the new authority (sent by this script).
//   message  prints a base58 message for the vault's accept_config_authority; import it in the
//            Squads app (Transactions > Import Transaction), approve 2 of 3, execute.
//   status   shows the current authority and any pending transfer.
const PROGRAM_ID = new anchor.web3.PublicKey("8Vg24teSaNrY4AmksQ8hZw2z2RRPuobeCtir7jVTYGw5");
const RPC_URL = process.env.ADSPACES_DEVNET_RPC || "https://api.devnet.solana.com";
const multisigState = path.join(__dirname, "..", ".anchor", "devnet-multisig.json");

async function main() {
  const [command] = process.argv.slice(2);
  if (!["propose", "message", "status"].includes(command)) {
    throw new Error("Usage: node scripts/config-authority-to-multisig-devnet.js propose|message|status [--confirm]");
  }
  const { vault: vaultAddress } = JSON.parse(fs.readFileSync(multisigState, "utf8"));
  const vault = new anchor.web3.PublicKey(vaultAddress);
  const walletPath = process.env.ADSPACES_DEPLOY_WALLET || path.join(os.homedir(), ".config/solana/adspaces-devnet.json");
  const payer = anchor.web3.Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(walletPath, "utf8"))));
  const connection = new anchor.web3.Connection(RPC_URL, "confirmed");
  const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(payer), { commitment: "confirmed" });
  const idl = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "target", "idl", "adspaces.json"), "utf8"));
  const program = new anchor.Program(idl, provider);

  const [config] = anchor.web3.PublicKey.findProgramAddressSync([Buffer.from("config")], PROGRAM_ID);
  const [transfer] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("authority_transfer"), config.toBuffer()],
    PROGRAM_ID,
  );
  const stored = await program.account.config.fetch(config);
  const pending = await program.account.authorityTransfer.fetchNullable(transfer);
  console.log(`Config: ${config}`);
  console.log(`Authority: ${stored.authority}`);
  console.log(`Treasury: ${stored.treasury}`);
  console.log(`Pending transfer: ${pending ? `${pending.currentAuthority} -> ${pending.proposedAuthority}` : "none"}`);
  console.log(`Squads vault: ${vault}`);
  if (command === "status") return;

  if (command === "propose") {
    if (!stored.authority.equals(payer.publicKey)) throw new Error(`Authority is ${stored.authority}, not ${payer.publicKey}`);
    if (pending) throw new Error("A transfer is already pending; cancel it first");
    if (!process.argv.includes("--confirm")) {
      console.log("Dry run. Re-run with --confirm to propose the vault.");
      return;
    }
    const signature = await program.methods
      .proposeConfigAuthority(vault)
      .accountsStrict({ authority: payer.publicKey, config, transfer, systemProgram: anchor.web3.SystemProgram.programId })
      .rpc();
    console.log(`Proposed: ${signature}`);
    return;
  }

  if (!pending || !pending.proposedAuthority.equals(vault)) throw new Error("No pending transfer to the vault; run propose first");
  const accept = await program.methods
    .acceptConfigAuthority()
    .accountsStrict({ proposedAuthority: vault, config, transfer, previousAuthority: stored.authority })
    .instruction();
  const message = anchor.web3.Message.compile({
    payerKey: vault,
    instructions: [accept],
    recentBlockhash: (await connection.getLatestBlockhash()).blockhash,
  });
  console.log("\nImport this in Squads (Transactions > Import Transaction):\n");
  console.log((bs58.default ?? bs58).encode(message.serialize()));
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
