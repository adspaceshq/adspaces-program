import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { createMint } from "@solana/spl-token";

import { Spotboard } from "../../target/types/spotboard";

export async function ensureConfig(
  program: Program<Spotboard>,
  provider: anchor.AnchorProvider,
) {
  const [config] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    program.programId,
  );

  try {
    const stored = await program.account.config.fetch(config);
    return {
      config,
      paymentMint: stored.paymentMint,
      treasury: stored.treasury,
    };
  } catch {
    const wallet = provider.wallet as anchor.Wallet;
    const paymentMint = await createMint(
      provider.connection,
      wallet.payer,
      wallet.publicKey,
      null,
      6,
    );
    const treasury = anchor.web3.Keypair.generate().publicKey;

    await program.methods
      .initializeConfig(treasury, 250)
      .accountsStrict({
        authority: wallet.publicKey,
        config,
        paymentMint,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    return { config, paymentMint, treasury };
  }
}
