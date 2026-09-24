mod common;

use anchor_lang::{
    solana_program::{rent, system_program},
    Event, InstructionData, ToAccountMetas,
};
use base64::Engine;
use solana_keypair::Keypair;
use solana_signer::Signer;
use spl_token_interface::state;

use common::*;

const PRICE: u64 = 150_000_000; // 150 USDC
const FEE_BPS: u128 = 250;
const PROOF_URI: &str = "ipfs://adspaces-proof-test";
const DISPUTE_URI: &str = "ipfs://adspaces-dispute-test";
const DECISION_URI: &str = "ipfs://adspaces-decision-test";

fn assert_event<E: Event>(meta: &litesvm::types::TransactionMetadata, event: &E) {
    let encoded = base64::engine::general_purpose::STANDARD.encode(event.data());
    assert!(meta
        .logs
        .iter()
        .any(|log| log == &format!("Program data: {encoded}")));
}

fn create_fixed_listing(
    env: &mut Env,
    seller: &Keypair,
    slug_hash: [u8; 16],
    deadline: i64,
) -> Pubkey {
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let ix = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/api/listings/settle".to_string(),
            mode: adspaces::constants::MODE_FIXED,
            deadline,
            artwork_close: deadline,
            delivery_deadline: deadline + 7 * 86_400,
            min_increment: 0,
            double_up: false,
        }
        .data(),
        adspaces::accounts::CreateListing {
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
        adspaces::instruction::AddSpot { price }.data(),
        adspaces::accounts::AddSpot {
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
        adspaces::instruction::ClaimSpot {}.data(),
        adspaces::accounts::ClaimSpot {
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
        adspaces::instruction::WithdrawSeller {}.data(),
        adspaces::accounts::WithdrawSeller {
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

#[allow(clippy::result_large_err, clippy::too_many_arguments)]
fn refund(
    env: &mut Env,
    caller: &Keypair,
    seller: &Pubkey,
    listing: &Pubkey,
    spot: &Pubkey,
    vault: &Pubkey,
    buyer_ata: &Pubkey,
) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        adspaces::instruction::RefundSpot {}.data(),
        adspaces::accounts::RefundSpot {
            caller: caller.pubkey(),
            listing: *listing,
            seller: *seller,
            spot: *spot,
            vault: *vault,
            buyer_ata: *buyer_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], caller, &[])
}

#[allow(clippy::result_large_err)]
fn try_submit_proof_with_uri(
    env: &mut Env,
    seller: &Keypair,
    listing: &Pubkey,
    spot: &Pubkey,
    proof_uri: &str,
) -> litesvm::types::TransactionResult {
    let (proof, _) = proof_pda(spot);
    let ix = new_ix(
        program_id(),
        adspaces::instruction::SubmitProof {
            proof_hash: [7; 32],
            proof_uri: proof_uri.to_string(),
        }
        .data(),
        adspaces::accounts::SubmitProof {
            seller: seller.pubkey(),
            listing: *listing,
            spot: *spot,
            proof,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[])
}

#[allow(clippy::result_large_err)]
fn try_submit_proof(
    env: &mut Env,
    seller: &Keypair,
    listing: &Pubkey,
    spot: &Pubkey,
) -> litesvm::types::TransactionResult {
    try_submit_proof_with_uri(env, seller, listing, spot, PROOF_URI)
}

fn submit_proof(env: &mut Env, seller: &Keypair, listing: &Pubkey, spot: &Pubkey) {
    try_submit_proof(env, seller, listing, spot).unwrap();
}

fn settle_balance(price: u64, donation: u64, slug_hash: [u8; 16]) -> (u64, u64) {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, slug_hash, now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, price);
    let (buyer, buyer_ata) = env.new_user(price);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);

    if donation > 0 {
        let (donor, donor_ata) = env.new_user(donation);
        let transfer = spl_token::instruction::transfer(
            &token_program_id(),
            &donor_ata,
            &vault,
            &donor.pubkey(),
            &[],
            donation,
        )
        .unwrap();
        send(&mut env.svm, &[transfer], &donor, &[]).unwrap();
    }

    env.warp_to_timestamp(now + 86_400);
    submit_proof(&mut env, &seller, &listing, &spot);
    env.warp_to_timestamp(now + 8 * 86_400 + adspaces::constants::REVIEW_WINDOW_SECONDS);
    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let treasury = env.treasury;
    let treasury_ata = empty_token_account(&mut env, &seller, &treasury);
    withdraw(
        &mut env,
        &seller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &seller_ata,
        &treasury_ata,
    )
    .unwrap();
    (
        token_balance(&env.svm, &seller_ata),
        token_balance(&env.svm, &treasury_ata),
    )
}

#[allow(clippy::result_large_err)]
fn dispute(
    env: &mut Env,
    buyer: &Keypair,
    listing: &Pubkey,
    spot: &Pubkey,
) -> litesvm::types::TransactionResult {
    dispute_with_evidence(env, buyer, listing, spot, [9; 32], DISPUTE_URI)
}

#[allow(clippy::result_large_err)]
fn dispute_with_evidence(
    env: &mut Env,
    buyer: &Keypair,
    listing: &Pubkey,
    spot: &Pubkey,
    evidence_hash: [u8; 32],
    evidence_uri: &str,
) -> litesvm::types::TransactionResult {
    let (dispute, _) = dispute_pda(spot);
    let ix = new_ix(
        program_id(),
        adspaces::instruction::RaiseDispute {
            evidence_hash,
            evidence_uri: evidence_uri.to_string(),
        }
        .data(),
        adspaces::accounts::RaiseDispute {
            buyer: buyer.pubkey(),
            listing: *listing,
            spot: *spot,
            dispute,
            system_program: system_program::ID,
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
    let proof_meta = try_submit_proof(&mut env, &seller, &listing, &spot).unwrap();
    let (proof, _) = proof_pda(&spot);
    let stored_proof = read_proof(&env.svm, &proof);
    assert_eq!(stored_proof.spot, spot);
    assert_eq!(stored_proof.proof_hash, [7; 32]);
    assert_eq!(stored_proof.proof_uri, PROOF_URI);
    assert_eq!(
        stored_proof.submitted_at,
        read_spot(&env.svm, &spot).proof_submitted_at
    );
    assert_event(
        &proof_meta,
        &adspaces::events::ProofSubmitted {
            listing,
            index: 0,
            proof,
            seller: seller.pubkey(),
            proof_hash: [7; 32],
            submitted_at: read_spot(&env.svm, &spot).proof_submitted_at,
        },
    );
    env.warp_to_timestamp(now + 37 * 86_400 + adspaces::constants::REVIEW_WINDOW_SECONDS);

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
        adspaces::constants::SPOT_FINALIZED
    );
    assert!(env.svm.get_account(&vault).is_none());
}

#[test]
fn settlement_handles_fee_rounding_and_maximum_balance() {
    assert_eq!(settle_balance(1, 0, [32; 16]), (1, 0));

    let maximum_fee = (u64::MAX as u128 * FEE_BPS / 10_000) as u64;
    assert_eq!(
        settle_balance(u64::MAX, 0, [33; 16]),
        (u64::MAX - maximum_fee, maximum_fee)
    );
}

#[test]
fn settlement_accounts_for_tokens_sent_directly_to_vault() {
    let donation = 7;
    let balance = PRICE + donation;
    let fee = (balance as u128 * FEE_BPS / 10_000) as u64;
    assert_eq!(
        settle_balance(PRICE, donation, [34; 16]),
        (balance - fee, fee)
    );
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
        adspaces::instruction::CloseSpot {}.data(),
        adspaces::accounts::CloseSpot {
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
        adspaces::constants::SPOT_CANCELLED
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
        adspaces::instruction::CloseSpot {}.data(),
        adspaces::accounts::CloseSpot {
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
fn seller_cancels_funded_spot_with_full_buyer_refund() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [28; 16], now + 30 * 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    let stranger = Keypair::new();
    env.svm.airdrop(&stranger.pubkey(), 10_000_000_000).unwrap();
    assert!(refund(
        &mut env,
        &stranger,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &buyer_ata
    )
    .is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);

    let meta = refund(
        &mut env,
        &seller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &buyer_ata,
    )
    .unwrap();
    assert_event(
        &meta,
        &adspaces::events::SpotRefunded {
            listing,
            index: 0,
            buyer: buyer.pubkey(),
            amount: PRICE,
            seller_initiated: true,
        },
    );
    assert_eq!(token_balance(&env.svm, &buyer_ata), PRICE);
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        adspaces::constants::SPOT_CANCELLED
    );
    assert!(env.svm.get_account(&vault).is_none());
}

#[test]
fn anyone_can_refund_missing_proof_after_delivery_deadline() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [29; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    let (caller, wrong_ata) = env.new_user(0);

    env.warp_to_timestamp(now + 8 * 86_400 - 1);
    assert!(refund(
        &mut env,
        &caller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &buyer_ata
    )
    .is_err());
    env.svm.expire_blockhash();
    env.warp_to_timestamp(now + 8 * 86_400);
    assert!(refund(
        &mut env,
        &caller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &wrong_ata
    )
    .is_err());
    let meta = refund(
        &mut env,
        &caller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &buyer_ata,
    )
    .unwrap();
    assert_event(
        &meta,
        &adspaces::events::SpotRefunded {
            listing,
            index: 0,
            buyer: buyer.pubkey(),
            amount: PRICE,
            seller_initiated: false,
        },
    );
    assert_eq!(token_balance(&env.svm, &buyer_ata), PRICE);
    assert_eq!(token_balance(&env.svm, &wrong_ata), 0);
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        adspaces::constants::SPOT_CANCELLED
    );
    assert!(env.svm.get_account(&vault).is_none());
}

#[test]
fn third_party_cannot_refund_when_proof_exists() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [30; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 86_400);
    submit_proof(&mut env, &seller, &listing, &spot);
    env.warp_to_timestamp(now + 8 * 86_400);
    let caller = Keypair::new();
    env.svm.airdrop(&caller.pubkey(), 10_000_000_000).unwrap();
    assert!(refund(
        &mut env,
        &caller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &buyer_ata
    )
    .is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        adspaces::constants::SPOT_FUNDED
    );
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
    env.warp_to_timestamp(now + 86_400 + adspaces::constants::REVIEW_WINDOW_SECONDS);
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
    let later_caller = Keypair::new();
    env.svm
        .airdrop(&later_caller.pubkey(), 10_000_000_000)
        .unwrap();
    env.warp_to_timestamp(now + 8 * 86_400 + adspaces::constants::REVIEW_WINDOW_SECONDS - 1);
    assert!(withdraw(
        &mut env,
        &later_caller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &seller_ata,
        &treasury_ata
    )
    .is_err());
    env.warp_to_timestamp(now + 8 * 86_400 + adspaces::constants::REVIEW_WINDOW_SECONDS);
    assert!(dispute(&mut env, &buyer, &listing, &spot).is_err());
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
    assert!(dispute(&mut env, &buyer, &listing, &spot).is_err());
    env.svm.expire_blockhash();
    let stranger = Keypair::new();
    env.svm.airdrop(&stranger.pubkey(), 10_000_000_000).unwrap();
    assert!(dispute(&mut env, &stranger, &listing, &spot).is_err());
    env.warp_to_timestamp(now + 8 * 86_400 + adspaces::constants::REVIEW_WINDOW_SECONDS / 2);
    for (hash, uri) in [
        ([0; 32], "".to_string()),
        ([9; 32], "".to_string()),
        ([0; 32], DISPUTE_URI.to_string()),
        (
            [9; 32],
            "x".repeat(adspaces::constants::DISPUTE_URI_MAX_LEN + 1),
        ),
    ] {
        assert!(dispute_with_evidence(&mut env, &buyer, &listing, &spot, hash, &uri).is_err());
    }
    let (dispute_record, _) = dispute_pda(&spot);
    assert!(env.svm.get_account(&dispute_record).is_none());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
    let maximum_evidence_uri = "e".repeat(adspaces::constants::DISPUTE_URI_MAX_LEN);
    let dispute_meta = dispute_with_evidence(
        &mut env,
        &buyer,
        &listing,
        &spot,
        [9; 32],
        &maximum_evidence_uri,
    )
    .unwrap();
    let stored_dispute = read_dispute(&env.svm, &dispute_record);
    assert_eq!(stored_dispute.spot, spot);
    assert_eq!(stored_dispute.buyer, buyer.pubkey());
    assert!(stored_dispute.proof_submitted);
    assert_eq!(stored_dispute.evidence_hash, [9; 32]);
    assert_eq!(stored_dispute.evidence_uri, maximum_evidence_uri);
    assert_event(
        &dispute_meta,
        &adspaces::events::DisputeRaised {
            listing,
            index: 0,
            dispute: dispute_record,
            buyer: buyer.pubkey(),
            proof_submitted: true,
            evidence_hash: [9; 32],
            raised_at: env.clock().unix_timestamp,
        },
    );
    env.warp_to_timestamp(now + 8 * 86_400 + adspaces::constants::REVIEW_WINDOW_SECONDS);
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
        adspaces::constants::SPOT_DISPUTED
    );
    assert_eq!(token_balance(&env.svm, &vault), PRICE);

    env.warp_to_timestamp(now + 365 * 86_400);
    assert!(refund(
        &mut env,
        &stranger,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &buyer_ata,
    )
    .is_err());
    assert!(withdraw(
        &mut env,
        &seller,
        &seller.pubkey(),
        &listing,
        &spot,
        &vault,
        &seller_ata,
        &treasury_ata,
    )
    .is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);

    let resolver = env.mint_authority.pubkey();
    let (dispute_record, _) = dispute_pda(&spot);
    let maximum_decision_uri = "d".repeat(adspaces::constants::DISPUTE_URI_MAX_LEN);
    let resolve = new_ix(
        program_id(),
        adspaces::instruction::ResolveDispute {
            seller_share_bps: 5_000,
            decision_hash: [7; 32],
            decision_uri: maximum_decision_uri.clone(),
        }
        .data(),
        adspaces::accounts::ResolveDispute {
            resolver,
            config: env.config,
            listing,
            spot,
            dispute: dispute_record,
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
    let attacker_ata = empty_token_account(&mut env, &seller, &stranger.pubkey());
    for destination in [seller_ata, buyer_ata, treasury_ata] {
        let mut redirected = resolve.clone();
        let account_index = redirected
            .accounts
            .iter()
            .position(|account| account.pubkey == destination)
            .unwrap();
        redirected.accounts[account_index].pubkey = attacker_ata;
        assert!(send(&mut env.svm, &[redirected], &env.mint_authority, &[]).is_err());
    }
    assert_eq!(token_balance(&env.svm, &attacker_ata), 0);
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
    let mut invalid_decision = resolve.clone();
    invalid_decision.data = adspaces::instruction::ResolveDispute {
        seller_share_bps: 5_000,
        decision_hash: [0; 32],
        decision_uri: String::new(),
    }
    .data();
    assert!(send(&mut env.svm, &[invalid_decision], &env.mint_authority, &[]).is_err());
    for (hash, uri) in [
        ([7; 32], "".to_string()),
        ([0; 32], DECISION_URI.to_string()),
        (
            [7; 32],
            "x".repeat(adspaces::constants::DISPUTE_URI_MAX_LEN + 1),
        ),
    ] {
        let mut invalid_decision = resolve.clone();
        invalid_decision.data = adspaces::instruction::ResolveDispute {
            seller_share_bps: 5_000,
            decision_hash: hash,
            decision_uri: uri,
        }
        .data();
        assert!(send(&mut env.svm, &[invalid_decision], &env.mint_authority, &[]).is_err());
    }
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
    let resolve_meta = send(&mut env.svm, &[resolve.clone()], &env.mint_authority, &[]).unwrap();
    let half = PRICE / 2;
    let fee = half as u128 * FEE_BPS / 10_000;
    let stored_dispute = read_dispute(&env.svm, &dispute_record);
    assert_eq!(stored_dispute.decision_hash, [7; 32]);
    assert_eq!(stored_dispute.decision_uri, maximum_decision_uri);
    assert_eq!(stored_dispute.decided_at, env.clock().unix_timestamp);
    assert_eq!(stored_dispute.seller_share_bps, 5_000);
    assert_eq!(stored_dispute.buyer_refund, half);
    assert_eq!(stored_dispute.seller_amount, half - fee as u64);
    assert_eq!(stored_dispute.fee, fee as u64);
    assert_event(
        &resolve_meta,
        &adspaces::events::DisputeResolved {
            listing,
            index: 0,
            dispute: dispute_record,
            resolver,
            decision_hash: [7; 32],
            seller_share_bps: 5_000,
            buyer_refund: half,
            seller_amount: half - fee as u64,
            fee: fee as u64,
        },
    );
    assert_eq!(token_balance(&env.svm, &buyer_ata), half);
    assert_eq!(token_balance(&env.svm, &seller_ata), half - fee as u64);
    assert_eq!(token_balance(&env.svm, &treasury_ata), fee as u64);
    assert!(env.svm.get_account(&vault).is_none());
    env.svm.expire_blockhash();
    assert!(send(&mut env.svm, &[resolve], &env.mint_authority, &[]).is_err());
}

#[test]
fn missing_proof_dispute_waits_for_delivery_deadline() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [26; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 86_400);
    assert!(dispute(&mut env, &buyer, &listing, &spot).is_err());
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        adspaces::constants::SPOT_FUNDED
    );
    env.svm.expire_blockhash();
    env.warp_to_timestamp(now + 8 * 86_400);
    let dispute_meta = dispute(&mut env, &buyer, &listing, &spot).unwrap();
    let (dispute_record, _) = dispute_pda(&spot);
    assert!(!read_dispute(&env.svm, &dispute_record).proof_submitted);
    assert_event(
        &dispute_meta,
        &adspaces::events::DisputeRaised {
            listing,
            index: 0,
            dispute: dispute_record,
            buyer: buyer.pubkey(),
            proof_submitted: false,
            evidence_hash: [9; 32],
            raised_at: env.clock().unix_timestamp,
        },
    );
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        adspaces::constants::SPOT_DISPUTED
    );
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}

#[test]
fn late_proof_is_rejected() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [27; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 8 * 86_400);
    assert!(try_submit_proof(&mut env, &seller, &listing, &spot).is_err());
    assert_eq!(read_spot(&env.svm, &spot).proof_submitted_at, 0);
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}

#[test]
fn proof_uri_must_be_nonempty_and_bounded() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [31; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 86_400);

    assert!(try_submit_proof_with_uri(&mut env, &seller, &listing, &spot, "").is_err());
    assert!(
        try_submit_proof_with_uri(&mut env, &seller, &listing, &spot, &"a".repeat(201)).is_err()
    );
    let maximum_uri = "a".repeat(adspaces::constants::PROOF_URI_MAX_LEN);
    try_submit_proof_with_uri(&mut env, &seller, &listing, &spot, &maximum_uri).unwrap();
    let (proof, _) = proof_pda(&spot);
    assert_eq!(read_proof(&env.svm, &proof).proof_uri, maximum_uri);
    assert!(read_spot(&env.svm, &spot).proof_submitted_at > 0);
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}
