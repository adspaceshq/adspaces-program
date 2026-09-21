mod common;

use anchor_lang::{
    solana_program::{rent, system_program},
    InstructionData, ToAccountMetas,
};
use solana_keypair::Keypair;
use solana_signer::Signer;
use spl_token_interface::state;

use common::*;

const PRICE: u64 = 150_000_000; // 150 USDC
const FEE_BPS: u128 = 250;

fn create_fixed_listing(
    env: &mut Env,
    seller: &Keypair,
    slug_hash: [u8; 16],
    deadline: i64,
) -> Pubkey {
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let ix = new_ix(
        program_id(),
        spotboard::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://spotboard.test/api/listings/settle".to_string(),
            mode: spotboard::constants::MODE_FIXED,
            deadline,
            artwork_close: deadline,
            delivery_deadline: deadline + 7 * 86_400,
            min_increment: 0,
            double_up: false,
        }
        .data(),
        spotboard::accounts::CreateListing {
            seller: seller.pubkey(),
            listing,
            config: env.config,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[]).unwrap();
    listing
}

fn add_spot_to(env: &mut Env, seller: &Keypair, listing: &Pubkey, price: u64) -> (Pubkey, Pubkey) {
    let index = read_listing(&env.svm, listing).spot_count;
    let (spot, _) = spot_pda(listing, index);
    let (vault, _) = vault_pda(&spot);
    let ix = new_ix(
        program_id(),
        spotboard::instruction::AddSpot { price }.data(),
        spotboard::accounts::AddSpot {
            seller: seller.pubkey(),
            listing: *listing,
            spot,
            vault,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
            token_program: token_program_id(),
            rent: rent::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[]).unwrap();
    (spot, vault)
}

fn empty_token_account(env: &mut Env, funder: &Keypair, owner: &Pubkey) -> Pubkey {
    let account = Keypair::new();
    let create = anchor_lang::solana_program::system_instruction::create_account(
        &funder.pubkey(),
        &account.pubkey(),
        RENT_FUNDING,
        state::Account::LEN as u64,
        &token_program_id(),
    );
    send(&mut env.svm, &[create], funder, &[&account]).unwrap();
    let init = spl_token::instruction::initialize_account(
        &token_program_id(),
        &account.pubkey(),
        &env.usdc,
        owner,
    )
    .unwrap();
    send(&mut env.svm, &[init], funder, &[]).unwrap();
    account.pubkey()
}

fn claim(
    env: &mut Env,
    buyer: &Keypair,
    buyer_ata: &Pubkey,
    listing: &Pubkey,
    spot: &Pubkey,
    vault: &Pubkey,
) {
    let ix = new_ix(
        program_id(),
        spotboard::instruction::ClaimSpot {}.data(),
        spotboard::accounts::ClaimSpot {
            buyer: buyer.pubkey(),
            listing: *listing,
            spot: *spot,
            vault: *vault,
            buyer_ata: *buyer_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], buyer, &[]).unwrap();
}

#[allow(clippy::result_large_err, clippy::too_many_arguments)]
fn withdraw(
    env: &mut Env,
    caller: &Keypair,
    seller: &Pubkey,
    listing: &Pubkey,
    spot: &Pubkey,
    vault: &Pubkey,
    seller_ata: &Pubkey,
    treasury_ata: &Pubkey,
) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        spotboard::instruction::WithdrawSeller {}.data(),
        spotboard::accounts::WithdrawSeller {
            caller: caller.pubkey(),
            listing: *listing,
            seller: *seller,
            spot: *spot,
            vault: *vault,
            seller_ata: *seller_ata,
            treasury_ata: *treasury_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], caller, &[])
}

fn submit_proof(env: &mut Env, seller: &Keypair, listing: &Pubkey, spot: &Pubkey) {
    let ix = new_ix(
        program_id(),
        spotboard::instruction::SubmitProof {
            proof_hash: [7; 32],
        }
        .data(),
        spotboard::accounts::SubmitProof {
            seller: seller.pubkey(),
            listing: *listing,
            spot: *spot,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[]).unwrap();
}

#[allow(clippy::result_large_err)]
fn dispute(
    env: &mut Env,
    buyer: &Keypair,
    listing: &Pubkey,
    spot: &Pubkey,
) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        spotboard::instruction::RaiseDispute {}.data(),
        spotboard::accounts::RaiseDispute {
            buyer: buyer.pubkey(),
            listing: *listing,
            spot: *spot,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], buyer, &[])
}

#[test]
fn withdraw_before_deadline_fails() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [20u8; 16], now + 30 * 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);

    let (buyer, buyer_ata) = env.new_user(1_000_000_000);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);

    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let treasury = env.treasury;
    let treasury_ata = empty_token_account(&mut env, &seller, &treasury);
    assert!(withdraw(
        &mut env,
        &seller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &seller_ata,
        &treasury_ata
    )
    .is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}

#[test]
fn withdraw_pays_seller_minus_fee_and_closes_vault() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [21u8; 16], now + 30 * 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);

    let (buyer, buyer_ata) = env.new_user(1_000_000_000);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);

    env.warp_to_timestamp(now + 31 * 86_400);
    submit_proof(&mut env, &seller, &listing, &spot);
    env.warp_to_timestamp(now + 31 * 86_400 + spotboard::constants::REVIEW_WINDOW_SECONDS);

    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let treasury = env.treasury;
    let treasury_ata = empty_token_account(&mut env, &seller, &treasury);
    let caller = Keypair::new();
    env.svm.airdrop(&caller.pubkey(), 10_000_000_000).unwrap();
    withdraw(
        &mut env,
        &caller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &seller_ata,
        &treasury_ata,
    )
    .unwrap();

    let expected_fee = PRICE as u128 * FEE_BPS / 10_000;
    assert_eq!(
        token_balance(&env.svm, &seller_ata),
        PRICE - expected_fee as u64
    );
    assert_eq!(token_balance(&env.svm, &treasury_ata), expected_fee as u64);
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        spotboard::constants::SPOT_FINALIZED
    );
    assert!(env.svm.get_account(&vault).is_none());
}

#[test]
fn close_spot_reclaims_unfunded_vault() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [22u8; 16], now + 30 * 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);

    let close = new_ix(
        program_id(),
        spotboard::instruction::CloseSpot {}.data(),
        spotboard::accounts::CloseSpot {
            seller: seller.pubkey(),
            listing,
            spot,
            vault,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[close], &seller, &[]).unwrap();
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        spotboard::constants::SPOT_CANCELLED
    );
    assert!(env.svm.get_account(&vault).is_none());
}

#[test]
fn close_funded_spot_fails() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [23u8; 16], now + 30 * 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);

    let (buyer, buyer_ata) = env.new_user(1_000_000_000);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);

    let close = new_ix(
        program_id(),
        spotboard::instruction::CloseSpot {}.data(),
        spotboard::accounts::CloseSpot {
            seller: seller.pubkey(),
            listing,
            spot,
            vault,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    assert!(send(&mut env.svm, &[close], &seller, &[]).is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}

#[test]
fn proof_and_review_are_required_before_payout() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [24; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let treasury = env.treasury;
    let treasury_ata = empty_token_account(&mut env, &seller, &treasury);
    env.warp_to_timestamp(now + 86_400);
    assert!(withdraw(
        &mut env,
        &seller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &seller_ata,
        &treasury_ata
    )
    .is_err());
    submit_proof(&mut env, &seller, &listing, &spot);
    let caller = Keypair::new();
    env.svm.airdrop(&caller.pubkey(), 10_000_000_000).unwrap();
    assert!(withdraw(
        &mut env,
        &caller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &seller_ata,
        &treasury_ata
    )
    .is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}

#[test]
fn only_winner_can_dispute_and_block_payout() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [25; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 86_400);
    submit_proof(&mut env, &seller, &listing, &spot);
    let stranger = Keypair::new();
    env.svm.airdrop(&stranger.pubkey(), 10_000_000_000).unwrap();
    assert!(dispute(&mut env, &stranger, &listing, &spot).is_err());
    dispute(&mut env, &buyer, &listing, &spot).unwrap();
    env.warp_to_timestamp(now + 86_400 + spotboard::constants::REVIEW_WINDOW_SECONDS);
    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let treasury = env.treasury;
    let treasury_ata = empty_token_account(&mut env, &seller, &treasury);
    assert!(withdraw(
        &mut env,
        &seller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &seller_ata,
        &treasury_ata
    )
    .is_err());
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        spotboard::constants::SPOT_DISPUTED
    );
    assert_eq!(token_balance(&env.svm, &vault), PRICE);

    let resolver = env.mint_authority.pubkey();
    let resolve = new_ix(
        program_id(),
        spotboard::instruction::ResolveDispute {
            seller_share_bps: 5_000,
        }
        .data(),
        spotboard::accounts::ResolveDispute {
            resolver,
            config: env.config,
            listing,
            spot,
            vault,
            seller_ata,
            seller: seller.pubkey(),
            buyer_ata,
            treasury_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    let mut unauthorized = resolve.clone();
    unauthorized.accounts[0].pubkey = stranger.pubkey();
    assert!(send(&mut env.svm, &[unauthorized], &stranger, &[]).is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
    send(&mut env.svm, &[resolve], &env.mint_authority, &[]).unwrap();
    let half = PRICE / 2;
    let fee = half as u128 * FEE_BPS / 10_000;
    assert_eq!(token_balance(&env.svm, &buyer_ata), half);
    assert_eq!(token_balance(&env.svm, &seller_ata), half - fee as u64);
    assert_eq!(token_balance(&env.svm, &treasury_ata), fee as u64);
    assert!(env.svm.get_account(&vault).is_none());
}

#[test]
fn winner_can_dispute_missing_proof_after_deadline() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [26; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 86_400);
    dispute(&mut env, &buyer, &listing, &spot).unwrap();
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        spotboard::constants::SPOT_DISPUTED
    );
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}
