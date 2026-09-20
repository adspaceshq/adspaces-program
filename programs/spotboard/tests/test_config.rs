mod common;

use anchor_lang::{InstructionData, ToAccountMetas};
use solana_keypair::Keypair;
use solana_signer::Signer;

use common::*;

#[allow(clippy::result_large_err)]
fn update_config(
    env: &mut Env,
    authority: &Keypair,
    treasury: Pubkey,
    fee_bps: u16,
) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        spotboard::instruction::UpdateConfig { treasury, fee_bps }.data(),
        spotboard::accounts::UpdateConfig {
            authority: authority.pubkey(),
            config: env.config,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], authority, &[])
}

fn copy_keypair(keypair: &Keypair) -> Keypair {
    let bytes = keypair.to_bytes();
    Keypair::new_from_array(bytes[..32].try_into().unwrap())
}

#[test]
fn initializes_protocol_config() {
    let env = Env::new();
    let config = read_config(&env.svm, &env.config);

    assert_eq!(config.authority, env.mint_authority.pubkey());
    assert_eq!(config.payment_mint, env.usdc);
    assert_eq!(config.treasury, env.treasury);
    assert_eq!(config.fee_bps, spotboard::constants::PLATFORM_FEE_BPS);
}

#[test]
fn authority_updates_treasury_and_fee() {
    let mut env = Env::new();
    let treasury = Pubkey::new_unique();
    let authority = copy_keypair(&env.mint_authority);

    update_config(&mut env, &authority, treasury, 300).unwrap();

    let config = read_config(&env.svm, &env.config);
    assert_eq!(config.treasury, treasury);
    assert_eq!(config.fee_bps, 300);
}

#[test]
fn rejects_unauthorized_or_invalid_updates() {
    let mut env = Env::new();
    let unauthorized = Keypair::new();
    env.svm
        .airdrop(&unauthorized.pubkey(), 10_000_000_000)
        .unwrap();
    let treasury = Pubkey::new_unique();

    assert!(update_config(&mut env, &unauthorized, treasury, 300).is_err());

    let authority = copy_keypair(&env.mint_authority);
    assert!(update_config(&mut env, &authority, treasury, 10_001).is_err());

    let config = read_config(&env.svm, &env.config);
    assert_eq!(config.treasury, env.treasury);
    assert_eq!(config.fee_bps, spotboard::constants::PLATFORM_FEE_BPS);
}
