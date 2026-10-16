const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const anchor = require("@coral-xyz/anchor");
const multisig = require("@sqds/multisig");
const bs58 = require("bs58");

// Builds base58 Squads "Import Transaction" messages for the multisig-only admin actions. Each
// message is a legacy Message with the Squads vault (index 0) as payer and signer; it is simulated
// as the vault first and the result printed. Nothing is ever sent from here.
//   status                      config, roles, program upgrade state, multisig and latest proposal
//   unpause | pause             pause_protocol(false | true)
//   reviewer <pubkey>           update_review_authority
//   timeout <seconds>           set_dispute_timeout (bounds from the IDL constants)
//   fees <treasury> <fee_bps>   update_config
//   guardian <pubkey|none>      initialize_pause_guardian, or update_pause_guardian (none disables)
//   upgrade <buffer>            BPF upgradeable loader Upgrade, spill = Ao
// Import the printed message in the Squads app (Transactions > Import Transaction) with Vault 0,
// approve 2 of 3, execute.
const PROGRAM_ID = new anchor.web3.PublicKey("8Vg24teSaNrY4AmksQ8hZw2z2RRPuobeCtir7jVTYGw5");
const LOADER_ID = new anchor.web3.PublicKey("BPFLoaderUpgradeab1e11111111111111111111111");
const RPC_URL = process.env.ADSPACES_DEVNET_RPC || "https://api.devnet.solana.com";
const repoRoot = path.join(__dirname, "..");
const multisigState = path.join(repoRoot, ".anchor", "devnet-multisig.json");
const localBinary = path.join(repoRoot, "target", "deploy", "adspaces.so");
// UpgradeableLoaderState headers: Buffer = tag u32 + Option<Pubkey>; ProgramData = tag u32 + slot
// u64 + Option<Pubkey>.
const BUFFER_HEADER = 37;
const PROGRAM_DATA_HEADER = 45;
const USAGE = [
  "Usage: node scripts/vault-message-devnet.js <command>",
  "  status",
  "  unpause | pause",
  "  reviewer <pubkey>",
  "  timeout <seconds>",
  "  fees <treasury> <fee_bps>",
  "  guardian <pubkey|none>",
  "  upgrade <buffer>",
].join("\n");

const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const optionalKey = (data, offset) => (data[offset] === 1 ? new anchor.web3.PublicKey(data.subarray(offset + 1, offset + 33)) : null);

function parsePubkey(value, label) {
  if (!value) throw new Error(`Missing ${label}\n${USAGE}`);
  try {
    return new anchor.web3.PublicKey(value);
  } catch {
    throw new Error(`${label} is not a valid public key: ${value}`);
  }
}

function idlConstant(idl, name, fallback) {
  const found = (idl.constants ?? []).find((constant) => constant.name === name);
  return found ? Number(found.value) : fallback;
}

// Ao is read for its address only (the upgrade spill account); nothing is signed with it.
function aoAddress() {
  const walletPath = process.env.ADSPACES_DEPLOY_WALLET || path.join(os.homedir(), ".config/solana/adspaces-devnet.json");
  const secret = Uint8Array.from(JSON.parse(fs.readFileSync(walletPath, "utf8")));
  return anchor.web3.Keypair.fromSecretKey(secret).publicKey;
}

async function readProgram(connection) {
  const [programData] = anchor.web3.PublicKey.findProgramAddressSync([PROGRAM_ID.toBuffer()], LOADER_ID);
  const account = await connection.getAccountInfo(programData);
  if (!account) throw new Error(`ProgramData ${programData} not found`);
  return {
    programData,
    slot: Number(account.data.readBigUInt64LE(4)),
    authority: optionalKey(account.data, 12),
    capacity: account.data.length - PROGRAM_DATA_HEADER,
    lamports: account.lamports,
  };
}

async function readMultisig(connection, multisigPda) {
  const account = await multisig.accounts.Multisig.fromAccountAddress(connection, multisigPda);
  const index = BigInt(account.transactionIndex.toString());
  let latest = null;
  if (index > 0n) {
    const [proposalPda] = multisig.getProposalPda({ multisigPda, transactionIndex: index });
    const info = await connection.getAccountInfo(proposalPda);
    const proposal = info ? multisig.accounts.Proposal.fromAccountInfo(info)[0] : null;
    latest = { index, proposalPda, proposal };
  }
  return { account, latest };
}

function permissionNames(mask) {
  const names = [];
  if (mask & multisig.types.Permission.Initiate) names.push("Initiate");
  if (mask & multisig.types.Permission.Vote) names.push("Vote");
  if (mask & multisig.types.Permission.Execute) names.push("Execute");
  return names.join(", ") || "none";
}

async function printStatus(connection, ctx) {
  const { config, stored, reviewer, guardian, vault, multisigPda } = ctx;
  const [program, vaultBalance, ms] = await Promise.all([
    readProgram(connection),
    connection.getBalance(vault),
    readMultisig(connection, multisigPda),
  ]);
  const mark = (key) => (key && key.equals(vault) ? `${key} (vault)` : `${key}`);
  console.log(`RPC: ${RPC_URL}`);
  console.log(`\nProgram: ${PROGRAM_ID}`);
  console.log(`ProgramData: ${program.programData}`);
  console.log(`Upgrade authority: ${program.authority ? mark(program.authority) : "none (immutable)"}`);
  console.log(`Program data capacity: ${program.capacity} bytes (last deployed slot ${program.slot})`);
  if (fs.existsSync(localBinary)) {
    const size = fs.statSync(localBinary).size;
    console.log(`Local target/deploy/adspaces.so: ${size} bytes${size > program.capacity ? " (needs extend before upgrade)" : ""}`);
  }
  console.log(`\nConfig: ${config}`);
  console.log(`Config authority: ${mark(stored.authority)}`);
  console.log(`Treasury: ${mark(stored.treasury)}`);
  console.log(`Fee floor: ${stored.feeBps} bps`);
  console.log(`Paused: ${stored.paused}`);
  console.log(`Dispute timeout: ${stored.disputeTimeoutSeconds} s`);
  console.log(`Review authority: ${reviewer ? reviewer.authority : "not initialized"}`);
  const guardianText = !guardian
    ? "not initialized"
    : guardian.guardian.equals(anchor.web3.PublicKey.default)
      ? "disabled"
      : guardian.guardian.toBase58();
  console.log(`Pause guardian: ${guardianText}`);
  console.log(`\nMultisig: ${multisigPda}`);
  console.log(`Vault (index 0): ${vault}, ${vaultBalance / anchor.web3.LAMPORTS_PER_SOL} SOL`);
  const voters = ms.account.members.filter((member) => member.permissions.mask & multisig.types.Permission.Vote).length;
  console.log(`Threshold: ${ms.account.threshold} of ${voters} voters (${ms.account.members.length} members), time lock ${ms.account.timeLock} s`);
  console.log(`Config authority (Squads): ${ms.account.configAuthority.equals(anchor.web3.PublicKey.default) ? "none" : ms.account.configAuthority}`);
  for (const member of ms.account.members) {
    console.log(`  ${member.key} ${permissionNames(member.permissions.mask)}`);
  }
  console.log(`Latest transaction index: ${ms.account.transactionIndex} (stale below ${ms.account.staleTransactionIndex})`);
  if (ms.latest) {
    const proposal = ms.latest.proposal;
    if (!proposal) {
      console.log(`Latest proposal: none for #${ms.latest.index} (${ms.latest.proposalPda})`);
    } else {
      console.log(
        `Latest proposal #${ms.latest.index}: ${proposal.status.__kind}, ` +
          `${proposal.approved.length} approved, ${proposal.rejected.length} rejected, ${proposal.cancelled.length} cancelled`,
      );
    }
  }
}

async function buildUpgrade(connection, vault, bufferArg) {
  const buffer = parsePubkey(bufferArg, "buffer");
  const spill = aoAddress();
  const [program, bufferAccount] = await Promise.all([readProgram(connection), connection.getAccountInfo(buffer)]);
  if (!bufferAccount) throw new Error(`Buffer ${buffer} not found`);
  if (!bufferAccount.owner.equals(LOADER_ID) || bufferAccount.data.readUInt32LE(0) !== 1) {
    throw new Error(`${buffer} is not an upgradeable loader buffer`);
  }
  if (!program.authority || !program.authority.equals(vault)) {
    throw new Error(`Program upgrade authority is ${program.authority ?? "none"}, not the vault ${vault}`);
  }
  const bufferAuthority = optionalKey(bufferAccount.data, 4);
  console.log(`Buffer: ${buffer}`);
  console.log(`Buffer authority: ${bufferAuthority ?? "none"}`);
  if (!bufferAuthority || !bufferAuthority.equals(vault)) {
    throw new Error(
      `Buffer authority must be the vault. Run: solana program set-buffer-authority ${buffer} --new-buffer-authority ${vault} --url devnet`,
    );
  }
  const bufferData = bufferAccount.data.subarray(BUFFER_HEADER);
  console.log(`Buffer data: ${bufferData.length} bytes; program data capacity: ${program.capacity} bytes`);
  console.log(`Buffer sha256: ${sha256(bufferData)}`);
  if (fs.existsSync(localBinary)) {
    const local = fs.readFileSync(localBinary);
    const localHash = sha256(local);
    console.log(`Local  sha256: ${localHash} (target/deploy/adspaces.so, ${local.length} bytes)`);
    console.log(localHash === sha256(bufferData) ? "Buffer matches the local build." : "WARNING: buffer does NOT match the local build.");
  } else {
    console.log("Local target/deploy/adspaces.so not found; cannot compare hashes.");
  }
  if (bufferData.length > program.capacity) {
    const extra = bufferData.length - program.capacity;
    // The loader rejects extends under 10240 bytes (unless reaching the max size). On devnet the
    // unchecked ExtendProgram is still active, so any payer (Ao) can extend without the vault.
    const extend = Math.max(extra, 10240);
    console.log(`\nThe buffer is ${extra} bytes larger than the program data account. Extend it first (Ao pays the rent):`);
    console.log(`  solana program extend ${PROGRAM_ID} ${extend} --url devnet -k ~/.config/solana/adspaces-devnet.json`);
    throw new Error("Program data too small for this buffer; extend, then re-run upgrade");
  }
  const key = (pubkey, isWritable, isSigner = false) => ({ pubkey, isSigner, isWritable });
  return {
    description: `Upgrade ${PROGRAM_ID} from buffer ${buffer} (spill ${spill}); the buffer is closed into the spill.`,
    instruction: new anchor.web3.TransactionInstruction({
      programId: LOADER_ID,
      data: Buffer.from([3, 0, 0, 0]),
      keys: [
        key(program.programData, true),
        key(PROGRAM_ID, true),
        key(buffer, true),
        key(spill, true),
        key(anchor.web3.SYSVAR_RENT_PUBKEY, false),
        key(anchor.web3.SYSVAR_CLOCK_PUBKEY, false),
        key(vault, false, true),
      ],
    }),
  };
}

async function buildAction(command, args, ctx, connection, program, idl) {
  const { config, stored, reviewer, guardian, vault, reviewAuthority, pauseGuardian } = ctx;
  const methods = program.methods;
  switch (command) {
    case "unpause":
    case "pause": {
      const paused = command === "pause";
      if (stored.paused === paused) console.log(`Note: protocol is already ${paused ? "paused" : "unpaused"}; this changes nothing.`);
      return {
        description: `pause_protocol(${paused}): ${paused ? "pause" : "resume"} the protocol.`,
        instruction: await methods.pauseProtocol(paused).accountsStrict({ authority: vault, config }).instruction(),
      };
    }
    case "reviewer": {
      const next = parsePubkey(args[0], "reviewer");
      if (next.equals(anchor.web3.PublicKey.default)) throw new Error("The reviewer cannot be the default key");
      if (!reviewer) throw new Error("Review authority is not initialized");
      if (reviewer.authority.equals(next)) console.log(`Note: reviewer is already ${next}; the program rejects an unchanged reviewer.`);
      return {
        // The reviewer key is also the only accepted rent payer for new listings; existing listings keep
        // theirs, so the old key must stay funded and online until they close.
        description: `update_review_authority: reviewer ${reviewer.authority} -> ${next} (also the rent payer for new listings; the old key keeps paying and receiving rent for existing ones).`,
        instruction: await methods
          .updateReviewAuthority(next)
          .accountsStrict({ authority: vault, config, reviewAuthority })
          .instruction(),
      };
    }
    case "timeout": {
      const min = idlConstant(idl, "MIN_DISPUTE_TIMEOUT_SECONDS", 3600);
      const max = idlConstant(idl, "MAX_DISPUTE_TIMEOUT_SECONDS", 2592000);
      if (!/^\d+$/.test(args[0] ?? "")) throw new Error(`timeout needs whole seconds\n${USAGE}`);
      const seconds = Number(args[0]);
      if (seconds < min || seconds > max) throw new Error(`timeout must be ${min} to ${max} seconds`);
      if (Number(stored.disputeTimeoutSeconds) === seconds) console.log(`Note: dispute timeout is already ${seconds} s.`);
      return {
        description: `set_dispute_timeout: ${stored.disputeTimeoutSeconds} s -> ${seconds} s (disputes raised afterwards only).`,
        instruction: await methods
          .setDisputeTimeout(new anchor.BN(seconds))
          .accountsStrict({ authority: vault, config })
          .instruction(),
      };
    }
    case "fees": {
      const treasury = parsePubkey(args[0], "treasury");
      if (treasury.equals(anchor.web3.PublicKey.default)) throw new Error("The treasury cannot be the default key");
      if (!/^\d+$/.test(args[1] ?? "")) throw new Error(`fees needs fee_bps as a whole number\n${USAGE}`);
      const feeBps = Number(args[1]);
      if (feeBps > 10_000) throw new Error("fee_bps must be 0 to 10000");
      if (!treasury.equals(vault)) console.log(`Note: treasury ${treasury} is not the Squads vault.`);
      return {
        description: `update_config: treasury ${stored.treasury} -> ${treasury}, fee floor ${stored.feeBps} -> ${feeBps} bps.`,
        instruction: await methods
          .updateConfig(treasury, feeBps)
          .accountsStrict({ authority: vault, config })
          .instruction(),
      };
    }
    case "guardian": {
      const next = args[0] === "none" ? anchor.web3.PublicKey.default : parsePubkey(args[0], "guardian");
      if (guardian) {
        if (guardian.guardian.equals(next)) console.log("Note: guardian is unchanged; the update succeeds without change.");
        return {
          description: `update_pause_guardian: ${guardian.guardian} -> ${next}${args[0] === "none" ? " (disabled)" : ""}.`,
          instruction: await methods
            .updatePauseGuardian(next)
            .accountsStrict({ authority: vault, config, pauseGuardian })
            .instruction(),
        };
      }
      if (next.equals(anchor.web3.PublicKey.default)) throw new Error("No guardian exists yet; the first guardian cannot be none");
      return {
        description: `initialize_pause_guardian: ${next} (the vault pays the PDA rent).`,
        instruction: await methods
          .initializePauseGuardian(next)
          .accountsStrict({ authority: vault, config, pauseGuardian, systemProgram: anchor.web3.SystemProgram.programId })
          .instruction(),
      };
    }
    case "upgrade":
      return buildUpgrade(connection, vault, args[0]);
    default:
      throw new Error(USAGE);
  }
}

async function main() {
  const [command, ...args] = process.argv.slice(2);
  const commands = ["status", "unpause", "pause", "reviewer", "timeout", "fees", "guardian", "upgrade"];
  if (!commands.includes(command)) throw new Error(USAGE);

  const { multisig: multisigAddress, vault: vaultAddress } = JSON.parse(fs.readFileSync(multisigState, "utf8"));
  const multisigPda = new anchor.web3.PublicKey(multisigAddress);
  const vault = new anchor.web3.PublicKey(vaultAddress);
  const [derivedVault] = multisig.getVaultPda({ multisigPda, index: 0 });
  if (!derivedVault.equals(vault)) throw new Error(`Vault ${vault} is not vault 0 of ${multisigPda}`);

  const connection = new anchor.web3.Connection(RPC_URL, "confirmed");
  // Read-only: a throwaway wallet satisfies the provider; nothing is signed or sent.
  const provider = new anchor.AnchorProvider(connection, new anchor.Wallet(anchor.web3.Keypair.generate()), {
    commitment: "confirmed",
  });
  const idl = JSON.parse(fs.readFileSync(path.join(repoRoot, "target", "idl", "adspaces.json"), "utf8"));
  const program = new anchor.Program(idl, provider);

  const [config] = anchor.web3.PublicKey.findProgramAddressSync([Buffer.from("config")], PROGRAM_ID);
  const [reviewAuthority] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("review_authority"), config.toBuffer()],
    PROGRAM_ID,
  );
  const [pauseGuardian] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("pause_guardian"), config.toBuffer()],
    PROGRAM_ID,
  );
  const [stored, reviewer, guardian] = await Promise.all([
    program.account.config.fetch(config),
    program.account.reviewAuthority.fetchNullable(reviewAuthority),
    program.account.pauseGuardian.fetchNullable(pauseGuardian),
  ]);
  const ctx = { config, stored, reviewer, guardian, vault, multisigPda, reviewAuthority, pauseGuardian };

  if (command === "status") {
    await printStatus(connection, ctx);
    return;
  }
  if (command !== "upgrade" && !stored.authority.equals(vault)) {
    throw new Error(`Config authority is ${stored.authority}, not the vault ${vault}`);
  }

  const { description, instruction } = await buildAction(command, args, ctx, connection, program, idl);
  const message = anchor.web3.Message.compile({
    payerKey: vault,
    instructions: [instruction],
    recentBlockhash: (await connection.getLatestBlockhash()).blockhash,
  });
  console.log(`\nAction: ${description}`);

  const simulation = await connection.simulateTransaction(new anchor.web3.VersionedTransaction(message), {
    sigVerify: false,
    replaceRecentBlockhash: true,
  });
  console.log(`Simulation as the vault: ${simulation.value.err ? `FAILED ${JSON.stringify(simulation.value.err)}` : "ok"}`);
  for (const log of simulation.value.logs ?? []) console.log(`  ${log}`);

  console.log("\nBase58 message:\n");
  console.log((bs58.default ?? bs58).encode(message.serialize()));
  console.log("\nImport in Squads (Transactions > Import Transaction) with Vault 0, approve 2 of 3, execute.");
  if (simulation.value.err) console.log("The simulation failed: do not import this message until the cause is fixed.");
}

main().catch((error) => {
  console.error(error.message ?? error);
  process.exit(1);
});
