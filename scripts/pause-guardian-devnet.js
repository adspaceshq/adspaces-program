const fs = require("node:fs");
const path = require("node:path");
const anchor = require("@coral-xyz/anchor");
const bs58 = require("bs58");

// Sets and uses the devnet pause guardian, a key that can pause the protocol but never unpause it:
//   status            shows the config authority, pause state and current guardian.
//   message <pubkey>  prints a base58 message for the Squads vault (the config authority) to
//                     initialize the guardian, or update it once it exists; import it in the
//                     Squads app (Transactions > Import Transaction), approve 2 of 3, execute.
//                     11111111111111111111111111111111 disables an existing guardian.
//                     The message is simulated first and the result printed.
//   pause [--confirm] the guardian (ADSPACES_GUARDIAN_WALLET) pauses the protocol. Unpausing
//                     stays with the vault through pause_protocol.
const PROGRAM_ID = new anchor.web3.PublicKey("8Vg24teSaNrY4AmksQ8hZw2z2RRPuobeCtir7jVTYGw5");
const RPC_URL = process.env.ADSPACES_DEVNET_RPC || "https://api.devnet.solana.com";
const multisigState = path.join(__dirname, "..", ".anchor", "devnet-multisig.json");

function loadKeypair(walletPath) {
  return anchor.web3.Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(walletPath, "utf8"))));
}

async function main() {
  const [command, guardianArg] = process.argv.slice(2);
  if (!["status", "message", "pause"].includes(command)) {
    throw new Error("Usage: node scripts/pause-guardian-devnet.js status | message <guardian> | pause [--confirm]");
  }
  const connection = new anchor.web3.Connection(RPC_URL, "confirmed");
  // Read-only commands need no real wallet; pause signs with the guardian key.
  let signer = anchor.web3.Keypair.generate();
  if (command === "pause") {
    const guardianWallet = process.env.ADSPACES_GUARDIAN_WALLET;
    if (!guardianWallet) throw new Error("Set ADSPACES_GUARDIAN_WALLET to the guardian keypair file");
    signer = loadKeypair(guardianWallet);
  }
  const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(signer), { commitment: "confirmed" });
  const idl = JSON.parse(fs.readFileSync(path.join(__dirname, "..", "target", "idl", "adspaces.json"), "utf8"));
  const program = new anchor.Program(idl, provider);

  const [config] = anchor.web3.PublicKey.findProgramAddressSync([Buffer.from("config")], PROGRAM_ID);
  const [pauseGuardian] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("pause_guardian"), config.toBuffer()],
    PROGRAM_ID,
  );
  const stored = await program.account.config.fetch(config);
  const guardian = await program.account.pauseGuardian.fetchNullable(pauseGuardian);
  const disabled = guardian && guardian.guardian.equals(anchor.web3.PublicKey.default);
  console.log(`Config: ${config}`);
  console.log(`Authority: ${stored.authority}`);
  console.log(`Paused: ${stored.paused}`);
  console.log(`Pause guardian PDA: ${pauseGuardian}`);
  console.log(`Guardian: ${guardian ? (disabled ? "disabled" : guardian.guardian.toBase58()) : "not initialized"}`);
  if (command === "status") return;

  if (command === "pause") {
    if (!guardian || disabled) throw new Error("No active guardian; set one with the message command first");
    if (!guardian.guardian.equals(signer.publicKey)) {
      throw new Error(`Guardian is ${guardian.guardian}, not ${signer.publicKey}`);
    }
    if (stored.paused) console.log("Protocol is already paused; guardian_pause will succeed without change.");
    if (!process.argv.includes("--confirm")) {
      console.log("Dry run. Re-run with --confirm to pause the protocol.");
      return;
    }
    const signature = await program.methods
      .guardianPause()
      .accountsStrict({ guardian: signer.publicKey, config, pauseGuardian })
      .rpc();
    console.log(`Paused: ${signature}`);
    return;
  }

  if (!guardianArg) throw new Error("Usage: node scripts/pause-guardian-devnet.js message <guardian>");
  const newGuardian = new anchor.web3.PublicKey(guardianArg);
  const { vault: vaultAddress } = JSON.parse(fs.readFileSync(multisigState, "utf8"));
  const vault = new anchor.web3.PublicKey(vaultAddress);
  console.log(`Squads vault: ${vault}`);
  if (!stored.authority.equals(vault)) throw new Error(`Authority is ${stored.authority}, not the vault ${vault}`);

  let instruction;
  if (guardian) {
    if (guardian.guardian.equals(newGuardian)) throw new Error(`Guardian is already ${newGuardian}`);
    console.log(`Action: update_pause_guardian -> ${newGuardian}`);
    instruction = await program.methods
      .updatePauseGuardian(newGuardian)
      .accountsStrict({ authority: vault, config, pauseGuardian })
      .instruction();
  } else {
    if (newGuardian.equals(anchor.web3.PublicKey.default)) throw new Error("The first guardian cannot be the default key");
    // The vault pays the PDA's rent, so it needs a little SOL before this executes.
    console.log(`Action: initialize_pause_guardian -> ${newGuardian}`);
    instruction = await program.methods
      .initializePauseGuardian(newGuardian)
      .accountsStrict({
        authority: vault,
        config,
        pauseGuardian,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .instruction();
  }
  const message = anchor.web3.Message.compile({
    payerKey: vault,
    instructions: [instruction],
    recentBlockhash: (await connection.getLatestBlockhash()).blockhash,
  });

  const simulation = await connection.simulateTransaction(new anchor.web3.VersionedTransaction(message), {
    sigVerify: false,
    replaceRecentBlockhash: true,
  });
  console.log(`\nSimulation: ${simulation.value.err ? `failed ${JSON.stringify(simulation.value.err)}` : "ok"}`);
  for (const log of simulation.value.logs ?? []) console.log(`  ${log}`);

  console.log("\nImport this in Squads (Transactions > Import Transaction):\n");
  console.log((bs58.default ?? bs58).encode(message.serialize()));
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
