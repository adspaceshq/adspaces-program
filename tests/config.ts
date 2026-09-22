import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { expect } from "chai";

import { Spotboard } from "../target/types/spotboard";
import { ensureConfig } from "./helpers/setup";

describe("protocol config", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.spotboard as Program<Spotboard>;
  const wallet = provider.wallet as anchor.Wallet;
  let config: anchor.web3.PublicKey;
  let paymentMint: anchor.web3.PublicKey;
  let treasury: anchor.web3.PublicKey;

  before(async () => {
    ({ config, paymentMint, treasury } = await ensureConfig(program, provider));
  });

  it("initializes the protocol config", async () => {
    const stored = await program.account.config.fetch(config);
    expect(stored.authority.equals(wallet.publicKey)).to.equal(true);
    expect(stored.paymentMint.equals(paymentMint)).to.equal(true);
    expect(stored.treasury.equals(treasury)).to.equal(true);
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

  it("requires the proposed authority to accept a handoff", async () => {
    const replacement = anchor.web3.Keypair.generate();
    const signature = await provider.connection.requestAirdrop(
      replacement.publicKey,
      anchor.web3.LAMPORTS_PER_SOL,
    );
    await provider.connection.confirmTransaction(signature, "confirmed");
    const [transfer] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("authority_transfer"), config.toBuffer()],
      program.programId,
    );

    await program.methods
      .proposeConfigAuthority(replacement.publicKey)
      .accountsStrict({
        authority: wallet.publicKey,
        config,
        transfer,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
    const pending = await program.account.authorityTransfer.fetch(transfer);
    expect(pending.currentAuthority.equals(wallet.publicKey)).to.equal(true);
    expect(pending.proposedAuthority.equals(replacement.publicKey)).to.equal(true);
    expect((await program.account.config.fetch(config)).authority.equals(wallet.publicKey)).to.equal(
      true,
    );

    let rejected = false;
    try {
      await program.methods
        .acceptConfigAuthority()
        .accountsStrict({
          proposedAuthority: wallet.publicKey,
          config,
          transfer,
          previousAuthority: wallet.publicKey,
        })
        .rpc();
    } catch {
      rejected = true;
    }
    expect(rejected).to.equal(true);

    await program.methods
      .acceptConfigAuthority()
      .accountsStrict({
        proposedAuthority: replacement.publicKey,
        config,
        transfer,
        previousAuthority: wallet.publicKey,
      })
      .signers([replacement])
      .rpc();
    expect((await program.account.config.fetch(config)).authority.equals(replacement.publicKey)).to
      .equal(true);
    expect(await provider.connection.getAccountInfo(transfer)).to.equal(null);

    await program.methods
      .proposeConfigAuthority(wallet.publicKey)
      .accountsStrict({
        authority: replacement.publicKey,
        config,
        transfer,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([replacement])
      .rpc();
    await program.methods
      .acceptConfigAuthority()
      .accountsStrict({
        proposedAuthority: wallet.publicKey,
        config,
        transfer,
        previousAuthority: replacement.publicKey,
      })
      .rpc();
    expect((await program.account.config.fetch(config)).authority.equals(wallet.publicKey)).to.equal(
      true,
    );
  });
});
