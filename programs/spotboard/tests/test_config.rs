mod common;

use anchor_lang::solana_program::system_program;
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

#[allow(clippy::result_large_err)]
fn initialize_config(env: &mut Env, authority: &Keypair) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        spotboard::instruction::InitializeConfig {
            treasury: env.treasury,
            fee_bps: spotboard::constants::PLATFORM_FEE_BPS,
        }
        .data(),
        spotboard::accounts::InitializeConfig {
            authority: authority.pubkey(),
            config: env.config,
            program: program_id(),
            program_data: env.program_data,
            payment_mint: env.usdc,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], authority, &[])
}

#[allow(clippy::result_large_err)]
fn propose_authority(
    env: &mut Env,
    authority: &Keypair,
    proposed_authority: Pubkey,
) -> litesvm::types::TransactionResult {
    let (transfer, _) = authority_transfer_pda(&env.config);
    let ix = new_ix(
        program_id(),
        spotboard::instruction::ProposeConfigAuthority { proposed_authority }.data(),
        spotboard::accounts::ProposeConfigAuthority {
            authority: authority.pubkey(),
            config: env.config,
            transfer,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], authority, &[])
}

#[allow(clippy::result_large_err)]
fn accept_authority(
    env: &mut Env,
    proposed_authority: &Keypair,
    previous_authority: Pubkey,
) -> litesvm::types::TransactionResult {
    let (transfer, _) = authority_transfer_pda(&env.config);
    let ix = new_ix(
        program_id(),
        spotboard::instruction::AcceptConfigAuthority {}.data(),
        spotboard::accounts::AcceptConfigAuthority {
            proposed_authority: proposed_authority.pubkey(),
            config: env.config,
            transfer,
            previous_authority,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], proposed_authority, &[])
}

#[allow(clippy::result_large_err)]
fn cancel_authority(env: &mut Env, authority: &Keypair) -> litesvm::types::TransactionResult {
    let (transfer, _) = authority_transfer_pda(&env.config);
    let ix = new_ix(
        program_id(),
        spotboard::instruction::CancelConfigAuthority {}.data(),
        spotboard::accounts::CancelConfigAuthority {
            authority: authority.pubkey(),
            config: env.config,
            transfer,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], authority, &[])
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
fn only_upgrade_authority_can_initialize_config() {
    let mut env = Env::uninitialized();
    let attacker = Keypair::new();
    env.svm.airdrop(&attacker.pubkey(), 10_000_000_000).unwrap();

    assert!(initialize_config(&mut env, &attacker).is_err());
    assert!(env.svm.get_account(&env.config).is_none());
    let authority = copy_keypair(&env.mint_authority);
    initialize_config(&mut env, &authority).unwrap();
    assert_eq!(
        read_config(&env.svm, &env.config).authority,
        authority.pubkey()
    );
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

#[test]
fn authority_transfer_requires_acceptance() {
    let mut env = Env::new();
    let current = copy_keypair(&env.mint_authority);
    let proposed = Keypair::new();
    let wrong = Keypair::new();
    for key in [&proposed, &wrong] {
        env.svm.airdrop(&key.pubkey(), 10_000_000_000).unwrap();
    }
    let (transfer, _) = authority_transfer_pda(&env.config);

    assert!(propose_authority(&mut env, &wrong, proposed.pubkey()).is_err());
    assert!(propose_authority(&mut env, &current, Pubkey::default()).is_err());
    assert!(propose_authority(&mut env, &current, current.pubkey()).is_err());
    assert!(env.svm.get_account(&transfer).is_none());

    propose_authority(&mut env, &current, proposed.pubkey()).unwrap();
    let pending = read_authority_transfer(&env.svm, &transfer);
    assert_eq!(pending.current_authority, current.pubkey());
    assert_eq!(pending.proposed_authority, proposed.pubkey());
    assert_eq!(
        read_config(&env.svm, &env.config).authority,
        current.pubkey()
    );
    assert!(accept_authority(&mut env, &wrong, current.pubkey()).is_err());
    assert!(accept_authority(&mut env, &proposed, wrong.pubkey()).is_err());
    assert_eq!(
        read_config(&env.svm, &env.config).authority,
        current.pubkey()
    );

    accept_authority(&mut env, &proposed, current.pubkey()).unwrap();
    assert!(env.svm.get_account(&transfer).is_none());
    assert_eq!(
        read_config(&env.svm, &env.config).authority,
        proposed.pubkey()
    );
    assert!(update_config(&mut env, &current, Pubkey::new_unique(), 300).is_err());
    update_config(&mut env, &proposed, Pubkey::new_unique(), 300).unwrap();
}

#[test]
fn authority_can_cancel_and_replace_a_pending_transfer() {
    let mut env = Env::new();
    let current = copy_keypair(&env.mint_authority);
    let first = Keypair::new();
    let second = Keypair::new();
    env.svm.airdrop(&first.pubkey(), 10_000_000_000).unwrap();
    env.svm.airdrop(&second.pubkey(), 10_000_000_000).unwrap();
    let (transfer, _) = authority_transfer_pda(&env.config);

    propose_authority(&mut env, &current, first.pubkey()).unwrap();
    assert!(propose_authority(&mut env, &current, second.pubkey()).is_err());
    assert!(cancel_authority(&mut env, &first).is_err());
    cancel_authority(&mut env, &current).unwrap();
    assert!(env.svm.get_account(&transfer).is_none());
    assert_eq!(
        read_config(&env.svm, &env.config).authority,
        current.pubkey()
    );

    env.svm.expire_blockhash();
    propose_authority(&mut env, &current, second.pubkey()).unwrap();
    accept_authority(&mut env, &second, current.pubkey()).unwrap();
    assert_eq!(
        read_config(&env.svm, &env.config).authority,
        second.pubkey()
    );
}
