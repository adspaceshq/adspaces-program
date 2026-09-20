import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { createMint } from "@solana/spl-token";
import { expect } from "chai";

import { Spotboard } from "../target/types/spotboard";

describe("protocol config", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.spotboard as Program<Spotboard>;
  const wallet = provider.wallet as anchor.Wallet;
  const treasury = anchor.web3.Keypair.generate();
  const [config] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("config")],
    program.programId,
  );
  let paymentMint: anchor.web3.PublicKey;

  before(async () => {
    paymentMint = await createMint(
      provider.connection,
      wallet.payer,
      wallet.publicKey,
      null,
      6,
    );
  });

  it("initializes the protocol config", async () => {
    await program.methods
      .initializeConfig(treasury.publicKey, 250)
      .accountsStrict({
        authority: wallet.publicKey,
        config,
        paymentMint,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const stored = await program.account.config.fetch(config);
    expect(stored.authority.equals(wallet.publicKey)).to.equal(true);
    expect(stored.paymentMint.equals(paymentMint)).to.equal(true);
    expect(stored.treasury.equals(treasury.publicKey)).to.equal(true);
    expect(stored.feeBps).to.equal(250);
  });

  it("rejects unauthorized updates", async () => {
    const attacker = anchor.web3.Keypair.generate();
    const signature = await provider.connection.requestAirdrop(
      attacker.publicKey,
      anchor.web3.LAMPORTS_PER_SOL,
    );
    await provider.connection.confirmTransaction(signature, "confirmed");

    let rejected = false;
    try {
      await program.methods
        .updateConfig(anchor.web3.Keypair.generate().publicKey, 300)
        .accountsStrict({ authority: attacker.publicKey, config })
        .signers([attacker])
        .rpc();
    } catch {
      rejected = true;
    }

    expect(rejected).to.equal(true);
  });

  it("lets the authority update treasury and fee", async () => {
    const nextTreasury = anchor.web3.Keypair.generate().publicKey;

    await program.methods
      .updateConfig(nextTreasury, 300)
      .accountsStrict({ authority: wallet.publicKey, config })
      .rpc();

    const stored = await program.account.config.fetch(config);
    expect(stored.treasury.equals(nextTreasury)).to.equal(true);
    expect(stored.feeBps).to.equal(300);
  });
});
