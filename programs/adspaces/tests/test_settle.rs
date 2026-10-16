mod common;

use anchor_lang::{
    solana_program::{instruction::Instruction, rent, system_program},
    InstructionData, ToAccountMetas,
};
use solana_keypair::Keypair;
use solana_signer::Signer;
use spl_token_interface::state;

use common::*;

const PRICE: u64 = 150_000_000; // 150 USDC
const FEE_BPS: u128 = 250;
const PROOF_URI: &str = "ipfs://adspaces-proof-test";
const DISPUTE_URI: &str = "ipfs://adspaces-dispute-test";
const DECISION_URI: &str = "ipfs://adspaces-decision-test";

fn create_fixed_listing(
    env: &mut Env,
    seller: &Keypair,
    slug_hash: [u8; 16],
    deadline: i64,
) -> Pubkey {
    create_fixed_listing_with_review(
        env,
        seller,
        slug_hash,
        deadline,
        adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
    )
}

fn create_fixed_listing_with_review(
    env: &mut Env,
    seller: &Keypair,
    slug_hash: [u8; 16],
    deadline: i64,
    review_window_seconds: i64,
) -> Pubkey {
    create_fixed_listing_with_times(
        env,
        seller,
        slug_hash,
        deadline,
        deadline,
        review_window_seconds,
    )
}

fn create_fixed_listing_with_times(
    env: &mut Env,
    seller: &Keypair,
    slug_hash: [u8; 16],
    deadline: i64,
    artwork_close: i64,
    review_window_seconds: i64,
) -> Pubkey {
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let ix = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/api/listings/settle".to_string(),
            mode: adspaces::constants::MODE_FIXED,
            deadline,
            artwork_close,
            delivery_deadline: artwork_close + 7 * 86_400,
            review_window_seconds,
            min_increment: 0,
            double_up: false,
            fee_bps: 250,
            uplift_fee_bps: 0,
            max_fee_bps: 250,
        }
        .data(),
        adspaces::accounts::CreateListing {
            seller: seller.pubkey(),
            reviewer: env.mint_authority.pubkey(),
            rent_payer: env.rent_payer(),
            listing,
            config: env.config,
            review_authority: env.review_authority,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[&env.mint_authority]).unwrap();
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
            config: env.config,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
            token_program: token_program_id(),
            rent: rent::ID,
            reviewer: env.mint_authority.pubkey(),
            rent_payer: env.rent_payer(),
            review_authority: env.review_authority,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[&env.mint_authority]).unwrap();
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
            config: env.config,
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
            rent_payer: env.rent_payer(),
            spot: *spot,
            vault: *vault,
            seller_ata: *seller_ata,
            treasury_ata: *treasury_ata,
            proof: proof_pda(spot).0,
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
            rent_payer: env.rent_payer(),
            spot: *spot,
            vault: *vault,
            buyer_ata: *buyer_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
            proof: env
                .svm
                .get_account(&proof_pda(spot).0)
                .map(|_| proof_pda(spot).0),
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
            rent_payer: env.rent_payer(),
            listing: *listing,
            spot: *spot,
            proof,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[&env.mint_authority])
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
    env.warp_to_timestamp(now + 8 * 86_400 + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS);
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
            rent_payer: env.mint_authority.pubkey(),
            config: env.config,
            listing: *listing,
            spot: *spot,
            dispute,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], buyer, &[&env.mint_authority])
}

#[allow(clippy::result_large_err)]
fn withdraw_dispute(
    env: &mut Env,
    buyer: &Keypair,
    listing: &Pubkey,
    spot: &Pubkey,
) -> litesvm::types::TransactionResult {
    let (dispute, _) = dispute_pda(spot);
    let ix = new_ix(
        program_id(),
        adspaces::instruction::WithdrawDispute {}.data(),
        adspaces::accounts::WithdrawDispute {
            buyer: buyer.pubkey(),
            rent_payer: dispute_rent_payer(env, spot),
            listing: *listing,
            spot: *spot,
            dispute,
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
    env.warp_to_timestamp(now + 37 * 86_400 + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS);

    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let treasury = env.treasury;
    let treasury_ata = empty_token_account(&mut env, &seller, &treasury);
    let caller = Keypair::new();
    env.svm.airdrop(&caller.pubkey(), 10_000_000_000).unwrap();
    let meta = withdraw(
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
    assert_event(
        &meta,
        &adspaces::events::SellerWithdrawn {
            listing,
            index: 0,
            seller: seller.pubkey(),
            amount: PRICE - expected_fee as u64,
            fee: expected_fee as u64,
            treasury_ata,
        },
    );
    assert_eq!(
        token_balance(&env.svm, &seller_ata),
        PRICE - expected_fee as u64
    );
    assert_eq!(token_balance(&env.svm, &treasury_ata), expected_fee as u64);
    assert!(env.svm.get_account(&spot).is_none());
    assert!(env.svm.get_account(&proof).is_none());
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
fn close_spot_returns_unfunded_spot_rent_to_rent_payer() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [22u8; 16], now + 30 * 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let stranger = Keypair::new();
    env.svm.airdrop(&stranger.pubkey(), 10_000_000_000).unwrap();
    let rent_payer = env.mint_authority.insecure_clone();

    // The seller can no longer close spots; nor can anyone but the listing's rent payer.
    for intruder in [&seller, &stranger] {
        assert_program_error(
            close_spot(
                &mut env,
                intruder,
                intruder,
                &listing,
                &spot,
                &vault,
                &seller_ata,
            ),
            adspaces::error::AdspacesError::Unauthorized,
        );
    }

    let rent = lamports(&env.svm, &spot) + lamports(&env.svm, &vault);
    let payer_lamports = lamports(&env.svm, &rent_payer.pubkey());
    let seller_lamports = lamports(&env.svm, &seller.pubkey());
    close_spot(
        &mut env,
        &stranger,
        &rent_payer,
        &listing,
        &spot,
        &vault,
        &seller_ata,
    )
    .unwrap();
    assert!(env.svm.get_account(&spot).is_none());
    assert!(env.svm.get_account(&vault).is_none());
    assert_eq!(
        lamports(&env.svm, &rent_payer.pubkey()),
        payer_lamports + rent
    );
    assert_eq!(lamports(&env.svm, &seller.pubkey()), seller_lamports);
    assert_eq!(read_listing(&env.svm, &listing).closed_spot_count, 1);
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

    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let rent_payer = env.mint_authority.insecure_clone();
    assert_program_error(
        close_spot(
            &mut env,
            &rent_payer,
            &rent_payer,
            &listing,
            &spot,
            &vault,
            &seller_ata,
        ),
        adspaces::error::AdspacesError::SpotAlreadyFunded,
    );
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
    assert_eq!(read_listing(&env.svm, &listing).closed_spot_count, 0);
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
    assert!(refund(&mut env, &stranger, &listing, &spot, &vault, &buyer_ata).is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);

    let meta = refund(&mut env, &seller, &listing, &spot, &vault, &buyer_ata).unwrap();
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
    assert!(env.svm.get_account(&spot).is_none());
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
    assert!(refund(&mut env, &caller, &listing, &spot, &vault, &buyer_ata).is_err());
    env.svm.expire_blockhash();
    env.warp_to_timestamp(now + 8 * 86_400);
    assert!(refund(&mut env, &caller, &listing, &spot, &vault, &wrong_ata).is_err());
    let meta = refund(&mut env, &caller, &listing, &spot, &vault, &buyer_ata).unwrap();
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
    assert!(env.svm.get_account(&spot).is_none());
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
    assert!(refund(&mut env, &caller, &listing, &spot, &vault, &buyer_ata).is_err());
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
    let review_window_seconds = 3 * 86_400;
    let listing = create_fixed_listing_with_review(
        &mut env,
        &seller,
        [24; 16],
        now + 86_400,
        review_window_seconds,
    );
    assert_eq!(
        read_listing(&env.svm, &listing).review_window_seconds,
        review_window_seconds
    );
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
    env.warp_to_timestamp(now + 86_400 + review_window_seconds);
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
    env.warp_to_timestamp(now + 8 * 86_400 + review_window_seconds - 1);
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
    env.warp_to_timestamp(now + 8 * 86_400 + review_window_seconds);
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
    env.warp_to_timestamp(
        now + 8 * 86_400 + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS / 2,
    );
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
    env.warp_to_timestamp(now + 8 * 86_400 + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS);
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

    // Up to the last second before the timeout, only the resolver can move the escrow.
    let opened_at = read_dispute(&env.svm, &dispute_pda(&spot).0).opened_at;
    env.warp_to_timestamp(opened_at + adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS - 1);
    assert!(refund(&mut env, &stranger, &listing, &spot, &vault, &buyer_ata,).is_err());
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
            rent_payer: read_dispute(&env.svm, &dispute_record).rent_payer,
            dispute_rent_payer: read_dispute(&env.svm, &dispute_record).rent_payer,
            vault,
            seller_ata,
            seller: seller.pubkey(),
            buyer_ata,
            treasury_ata,
            proof: Some(proof_pda(&spot).0),
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
    assert!(env.svm.get_account(&dispute_record).is_none());
    assert!(env.svm.get_account(&spot).is_none());
    assert!(env.svm.get_account(&proof_pda(&spot).0).is_none());
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
            treasury_ata,
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
fn buyer_can_withdraw_dispute_and_restore_funded_spot() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [31; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 86_400);
    submit_proof(&mut env, &seller, &listing, &spot);
    env.warp_to_timestamp(
        now + 8 * 86_400 + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS / 2,
    );
    dispute(&mut env, &buyer, &listing, &spot).unwrap();
    let (dispute_record, _) = dispute_pda(&spot);
    assert!(env.svm.get_account(&dispute_record).is_some());
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        adspaces::constants::SPOT_DISPUTED
    );
    assert_eq!(token_balance(&env.svm, &vault), PRICE);

    let meta = withdraw_dispute(&mut env, &buyer, &listing, &spot).unwrap();
    assert_event(
        &meta,
        &adspaces::events::DisputeWithdrawn {
            listing,
            index: 0,
            dispute: dispute_record,
            buyer: buyer.pubkey(),
        },
    );
    assert!(env.svm.get_account(&dispute_record).is_none());
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        adspaces::constants::SPOT_FUNDED
    );
    assert_eq!(token_balance(&env.svm, &vault), PRICE);

    env.warp_to_timestamp(now + 8 * 86_400 + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS);
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
}

#[test]
fn dispute_withdrawal_rejects_non_buyer_and_resolved_disputes() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [32; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 86_400);
    submit_proof(&mut env, &seller, &listing, &spot);
    env.warp_to_timestamp(
        now + 8 * 86_400 + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS / 2,
    );
    dispute(&mut env, &buyer, &listing, &spot).unwrap();
    let stranger = Keypair::new();
    env.svm.airdrop(&stranger.pubkey(), 10_000_000_000).unwrap();
    assert!(withdraw_dispute(&mut env, &stranger, &listing, &spot).is_err());
    assert!(withdraw_dispute(&mut env, &seller, &listing, &spot).is_err());
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        adspaces::constants::SPOT_DISPUTED
    );

    let (dispute_record, _) = dispute_pda(&spot);
    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let treasury = env.treasury;
    let treasury_ata = empty_token_account(&mut env, &seller, &treasury);
    let resolve = new_ix(
        program_id(),
        adspaces::instruction::ResolveDispute {
            seller_share_bps: 10_000,
            decision_hash: [8; 32],
            decision_uri: DECISION_URI.to_string(),
        }
        .data(),
        adspaces::accounts::ResolveDispute {
            resolver: env.mint_authority.pubkey(),
            config: env.config,
            listing,
            spot,
            dispute: dispute_record,
            rent_payer: read_dispute(&env.svm, &dispute_record).rent_payer,
            dispute_rent_payer: read_dispute(&env.svm, &dispute_record).rent_payer,
            vault,
            seller_ata,
            seller: seller.pubkey(),
            buyer_ata,
            treasury_ata,
            proof: Some(proof_pda(&spot).0),
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[resolve], &env.mint_authority, &[]).unwrap();
    assert!(withdraw_dispute(&mut env, &buyer, &listing, &spot).is_err());
    assert!(env.svm.get_account(&spot).is_none());
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

#[test]
fn seller_refund_after_proof_returns_proof_rent_to_rent_payer() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [29; 16], now + 30 * 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 30 * 86_400 + 1);
    submit_proof(&mut env, &seller, &listing, &spot);
    let (proof, _) = proof_pda(&spot);
    assert!(env.svm.get_account(&proof).is_some());

    let rent = [spot, vault, proof]
        .iter()
        .map(|account| lamports(&env.svm, account))
        .sum::<u64>();
    let payer_lamports = lamports(&env.svm, &env.rent_payer());
    let seller_lamports = lamports(&env.svm, &seller.pubkey());
    let meta = refund(&mut env, &seller, &listing, &spot, &vault, &buyer_ata).unwrap();

    assert_eq!(token_balance(&env.svm, &buyer_ata), PRICE);
    assert!(env.svm.get_account(&spot).is_none());
    assert!(
        env.svm.get_account(&proof).is_none(),
        "proof rent must not be stranded"
    );
    // The seller only paid the transaction fee; all of the rent went to the rent payer.
    assert_eq!(
        lamports(&env.svm, &seller.pubkey()),
        seller_lamports - meta.fee
    );
    assert_eq!(lamports(&env.svm, &env.rent_payer()), payer_lamports + rent);
    assert_eq!(read_listing(&env.svm, &listing).closed_spot_count, 1);
}

#[test]
fn donated_tokens_cannot_block_closing_an_open_spot() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [30u8; 16], now + 30 * 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);

    // Someone sends 1 base unit straight to the open spot's vault.
    let (donor, donor_ata) = env.new_user(1_000);
    let transfer = spl_token::instruction::transfer(
        &token_program_id(),
        &donor_ata,
        &vault,
        &donor.pubkey(),
        &[],
        1,
    )
    .unwrap();
    send(&mut env.svm, &[transfer], &donor, &[]).unwrap();

    let rent_payer = env.mint_authority.insecure_clone();
    // The rent payer closes the spot, but the swept tokens can only go to the seller.
    assert_program_error(
        close_spot(
            &mut env,
            &rent_payer,
            &rent_payer,
            &listing,
            &spot,
            &vault,
            &donor_ata,
        ),
        adspaces::error::AdspacesError::Unauthorized,
    );
    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    close_spot(
        &mut env,
        &rent_payer,
        &rent_payer,
        &listing,
        &spot,
        &vault,
        &seller_ata,
    )
    .unwrap();
    assert_eq!(token_balance(&env.svm, &seller_ata), 1);
    assert!(env.svm.get_account(&spot).is_none());
    assert!(env.svm.get_account(&vault).is_none());
}

// ---- rent payer ---------------------------------------------------------------------------------

fn funded_keypair(env: &mut Env) -> Keypair {
    let keypair = Keypair::new();
    env.svm.airdrop(&keypair.pubkey(), 10_000_000_000).unwrap();
    keypair
}

/// Whoever paid the dispute's rent; the listing's rent payer when there is no dispute record yet.
fn dispute_rent_payer(env: &Env, spot: &Pubkey) -> Pubkey {
    let (dispute, _) = dispute_pda(spot);
    match env.svm.get_account(&dispute) {
        Some(_) => read_dispute(&env.svm, &dispute).rent_payer,
        None => env.rent_payer(),
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_ix(
    env: &Env,
    seller: &Pubkey,
    listing: &Pubkey,
    spot: &Pubkey,
    vault: &Pubkey,
    seller_ata: &Pubkey,
    buyer_ata: &Pubkey,
    treasury_ata: &Pubkey,
) -> Instruction {
    let (dispute, _) = dispute_pda(spot);
    let (proof, _) = proof_pda(spot);
    new_ix(
        program_id(),
        adspaces::instruction::ResolveDispute {
            seller_share_bps: 5_000,
            decision_hash: [8; 32],
            decision_uri: DECISION_URI.to_string(),
        }
        .data(),
        adspaces::accounts::ResolveDispute {
            resolver: env.mint_authority.pubkey(),
            config: env.config,
            listing: *listing,
            spot: *spot,
            dispute,
            rent_payer: env.rent_payer(),
            dispute_rent_payer: dispute_rent_payer(env, spot),
            vault: *vault,
            seller_ata: *seller_ata,
            seller: *seller,
            buyer_ata: *buyer_ata,
            treasury_ata: *treasury_ata,
            proof: env.svm.get_account(&proof).map(|_| proof),
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    )
}

fn timeout_ix(
    env: &Env,
    caller: &Pubkey,
    listing: &Pubkey,
    spot: &Pubkey,
    vault: &Pubkey,
    buyer_ata: &Pubkey,
) -> Instruction {
    let (dispute, _) = dispute_pda(spot);
    let (proof, _) = proof_pda(spot);
    new_ix(
        program_id(),
        adspaces::instruction::TimeoutDispute {}.data(),
        adspaces::accounts::TimeoutDispute {
            caller: *caller,
            config: env.config,
            listing: *listing,
            spot: *spot,
            dispute,
            rent_payer: env.rent_payer(),
            dispute_rent_payer: dispute_rent_payer(env, spot),
            proof: env.svm.get_account(&proof).map(|_| proof),
            vault: *vault,
            buyer_ata: *buyer_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    )
}

/// The same instruction with `impostor` passed as the (non-signing) rent payer account.
fn with_rent_payer(env: &Env, ix: &Instruction, impostor: &Pubkey) -> Instruction {
    let mut ix = ix.clone();
    for account in ix.accounts.iter_mut() {
        if account.pubkey == env.rent_payer() && !account.is_signer {
            account.pubkey = *impostor;
        }
    }
    ix
}

/// Runs a close and checks that all the rent held by `closed` went to the rent payer and that the
/// seller's SOL balance did not move. `close` must not use the seller or rent payer as fee payer.
fn assert_rent_returned(
    env: &mut Env,
    seller: &Pubkey,
    closed: &[Pubkey],
    close: impl FnOnce(&mut Env),
) {
    assert!(closed.iter().all(|a| lamports(&env.svm, a) > 0));
    let rent: u64 = closed.iter().map(|a| lamports(&env.svm, a)).sum();
    let payer_lamports = lamports(&env.svm, &env.rent_payer());
    let seller_lamports = lamports(&env.svm, seller);
    close(env);
    for account in closed {
        assert!(env.svm.get_account(account).is_none());
    }
    assert_eq!(lamports(&env.svm, &env.rent_payer()), payer_lamports + rent);
    assert_eq!(lamports(&env.svm, seller), seller_lamports);
}

#[test]
fn no_rent_leaks_to_seller() {
    let mut env = Env::new();
    let seller = funded_keypair(&mut env);
    let fee_payer = funded_keypair(&mut env);
    let rent_payer = env.mint_authority.insecure_clone();
    let treasury = env.treasury;
    let seller_ata = empty_token_account(&mut env, &fee_payer, &seller.pubkey());
    let treasury_ata = empty_token_account(&mut env, &fee_payer, &treasury);
    // Minting test USDC is the one place the rent payer pays a fee, so do it before measuring.
    let (buyer, buyer_ata) = env.new_user(PRICE);
    let now = env.clock().unix_timestamp;

    // The rent payer only co-signs (never pays fees), so its balance moves by rent alone.
    let payer_start = lamports(&env.svm, &rent_payer.pubkey());
    let listing = create_fixed_listing(&mut env, &seller, [50; 16], now + 86_400);
    let (sold, sold_vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (unsold, unsold_vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let rent: u64 = [listing, sold, sold_vault, unsold, unsold_vault]
        .iter()
        .map(|a| lamports(&env.svm, a))
        .sum();
    assert_eq!(lamports(&env.svm, &rent_payer.pubkey()), payer_start - rent);

    claim(&mut env, &buyer, &buyer_ata, &listing, &sold, &sold_vault);
    env.warp_to_timestamp(now + 86_400);
    let before_proof = lamports(&env.svm, &rent_payer.pubkey());
    submit_proof(&mut env, &seller, &listing, &sold);
    let (proof, _) = proof_pda(&sold);
    assert_eq!(
        lamports(&env.svm, &rent_payer.pubkey()),
        before_proof - lamports(&env.svm, &proof)
    );

    env.warp_to_timestamp(now + 8 * 86_400 + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS);
    // The payout can't send the rent anywhere but the listing's rent payer.
    let impostor_payout = new_ix(
        program_id(),
        adspaces::instruction::WithdrawSeller {}.data(),
        adspaces::accounts::WithdrawSeller {
            caller: fee_payer.pubkey(),
            listing,
            seller: seller.pubkey(),
            rent_payer: seller.pubkey(),
            spot: sold,
            vault: sold_vault,
            seller_ata,
            treasury_ata,
            proof,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    assert_program_error(
        send(&mut env.svm, &[impostor_payout], &fee_payer, &[]),
        adspaces::error::AdspacesError::Unauthorized,
    );
    assert_rent_returned(
        &mut env,
        &seller.pubkey(),
        &[sold, sold_vault, proof],
        |env| {
            withdraw(
                env,
                &fee_payer,
                &seller.pubkey(),
                &listing,
                &sold,
                &sold_vault,
                &seller_ata,
                &treasury_ata,
            )
            .unwrap();
        },
    );
    assert_eq!(read_listing(&env.svm, &listing).closed_spot_count, 1);

    assert_rent_returned(&mut env, &seller.pubkey(), &[unsold, unsold_vault], |env| {
        close_spot(
            env,
            &fee_payer,
            &rent_payer,
            &listing,
            &unsold,
            &unsold_vault,
            &seller_ata,
        )
        .unwrap();
    });
    assert_eq!(read_listing(&env.svm, &listing).closed_spot_count, 2);

    assert_rent_returned(&mut env, &seller.pubkey(), &[listing], |env| {
        close_listing(env, &fee_payer, &rent_payer, &listing).unwrap();
    });
    // Everything the rent payer put in came back, to the lamport.
    assert_eq!(lamports(&env.svm, &rent_payer.pubkey()), payer_start);
    assert_eq!(
        token_balance(&env.svm, &seller_ata),
        PRICE - (PRICE as u128 * FEE_BPS / 10_000) as u64
    );
}

#[test]
fn every_close_path_returns_rent_and_counts_toward_close_listing() {
    use adspaces::error::AdspacesError;

    let mut env = Env::new();
    let seller = funded_keypair(&mut env);
    let fee_payer = funded_keypair(&mut env);
    let rent_payer = env.mint_authority.insecure_clone();
    let treasury = env.treasury;
    let seller_ata = empty_token_account(&mut env, &fee_payer, &seller.pubkey());
    let treasury_ata = empty_token_account(&mut env, &fee_payer, &treasury);
    let now = env.clock().unix_timestamp;
    let deadline = now + 86_400;
    let delivery = deadline + 7 * 86_400;
    // Minting test USDC is the one place the rent payer pays a fee, so do it before measuring.
    let (buyer, buyer_ata) = env.new_user(4 * PRICE);

    let payer_start = lamports(&env.svm, &rent_payer.pubkey());
    let listing = create_fixed_listing(&mut env, &seller, [51; 16], deadline);
    let spots: Vec<(Pubkey, Pubkey)> = (0..5)
        .map(|_| add_spot_to(&mut env, &seller, &listing, PRICE))
        .collect();
    for (spot, vault) in &spots[1..] {
        claim(&mut env, &buyer, &buyer_ata, &listing, spot, vault);
    }
    let closed_count = |env: &Env| read_listing(&env.svm, &listing).closed_spot_count;
    let listing_blocked = |env: &mut Env| {
        assert_program_error(
            close_listing(env, &fee_payer, &rent_payer, &listing),
            AdspacesError::ListingHasActiveSpots,
        );
    };
    listing_blocked(&mut env);
    assert_eq!(closed_count(&env), 0);

    // 1. close_spot: the unsold spot, before the deadline.
    let (open, open_vault) = spots[0];
    assert_rent_returned(&mut env, &seller.pubkey(), &[open, open_vault], |env| {
        close_spot(
            env,
            &fee_payer,
            &rent_payer,
            &listing,
            &open,
            &open_vault,
            &seller_ata,
        )
        .unwrap();
    });
    assert_eq!(closed_count(&env), 1);
    listing_blocked(&mut env);

    env.warp_to_timestamp(deadline);
    let (paid, paid_vault) = spots[1];
    let (resolved, resolved_vault) = spots[3];
    submit_proof(&mut env, &seller, &listing, &paid);
    submit_proof(&mut env, &seller, &listing, &resolved);

    // 2. refund_spot: a third party refunds a spot with no proof once delivery is due.
    env.warp_to_timestamp(delivery);
    let (refunded, refunded_vault) = spots[2];
    let refund_ix = new_ix(
        program_id(),
        adspaces::instruction::RefundSpot {}.data(),
        adspaces::accounts::RefundSpot {
            caller: fee_payer.pubkey(),
            listing,
            rent_payer: env.rent_payer(),
            spot: refunded,
            vault: refunded_vault,
            buyer_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
            proof: None,
        }
        .to_account_metas(None),
    );
    let misdirected = with_rent_payer(&env, &refund_ix, &seller.pubkey());
    assert_program_error(
        send(&mut env.svm, &[misdirected], &fee_payer, &[]),
        AdspacesError::Unauthorized,
    );
    assert_rent_returned(
        &mut env,
        &seller.pubkey(),
        &[refunded, refunded_vault],
        |env| {
            send(&mut env.svm, &[refund_ix], &fee_payer, &[]).unwrap();
        },
    );
    assert_eq!(closed_count(&env), 2);
    listing_blocked(&mut env);

    // 3. resolve_dispute: a disputed spot with proof, split by the resolver.
    dispute(&mut env, &buyer, &listing, &resolved).unwrap();
    let (timed_out, timed_out_vault) = spots[4];
    dispute(&mut env, &buyer, &listing, &timed_out).unwrap();
    let opened_at = env.clock().unix_timestamp;
    let resolve = resolve_ix(
        &env,
        &seller.pubkey(),
        &listing,
        &resolved,
        &resolved_vault,
        &seller_ata,
        &buyer_ata,
        &treasury_ata,
    );
    let misdirected = with_rent_payer(&env, &resolve, &seller.pubkey());
    assert_program_error(
        send(
            &mut env.svm,
            &[misdirected],
            &fee_payer,
            &[&env.mint_authority],
        ),
        AdspacesError::Unauthorized,
    );
    let resolved_closed = [
        resolved,
        resolved_vault,
        proof_pda(&resolved).0,
        dispute_pda(&resolved).0,
    ];
    assert_rent_returned(&mut env, &seller.pubkey(), &resolved_closed, |env| {
        send(&mut env.svm, &[resolve], &fee_payer, &[&env.mint_authority]).unwrap();
    });
    assert_eq!(closed_count(&env), 3);
    listing_blocked(&mut env);

    // 4. withdraw_seller: the payout after the review window.
    env.warp_to_timestamp(delivery + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS);
    let paid_closed = [paid, paid_vault, proof_pda(&paid).0];
    assert_rent_returned(&mut env, &seller.pubkey(), &paid_closed, |env| {
        withdraw(
            env,
            &fee_payer,
            &seller.pubkey(),
            &listing,
            &paid,
            &paid_vault,
            &seller_ata,
            &treasury_ata,
        )
        .unwrap();
    });
    assert_eq!(closed_count(&env), 4);
    listing_blocked(&mut env);

    // 5. timeout_dispute: the unresolved dispute (no proof) times out.
    env.warp_to_timestamp(opened_at + adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS);
    let timeout = timeout_ix(
        &env,
        &fee_payer.pubkey(),
        &listing,
        &timed_out,
        &timed_out_vault,
        &buyer_ata,
    );
    let misdirected = with_rent_payer(&env, &timeout, &seller.pubkey());
    assert_program_error(
        send(&mut env.svm, &[misdirected], &fee_payer, &[]),
        AdspacesError::Unauthorized,
    );
    let timed_out_closed = [timed_out, timed_out_vault, dispute_pda(&timed_out).0];
    assert_rent_returned(&mut env, &seller.pubkey(), &timed_out_closed, |env| {
        send(&mut env.svm, &[timeout], &fee_payer, &[]).unwrap();
    });
    assert_eq!(closed_count(&env), 5);

    // Every spot is closed: the listing closes and the rent payer is made whole.
    assert_rent_returned(&mut env, &seller.pubkey(), &[listing], |env| {
        close_listing(env, &fee_payer, &rent_payer, &listing).unwrap();
    });
    assert_eq!(lamports(&env.svm, &rent_payer.pubkey()), payer_start);
}

#[test]
fn instructions_reject_a_rent_payer_other_than_the_listings() {
    use adspaces::error::AdspacesError;

    let mut env = Env::new();
    let seller = funded_keypair(&mut env);
    let impostor = funded_keypair(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [52; 16], now + 86_400);
    assert_eq!(
        read_listing(&env.svm, &listing).rent_payer,
        env.rent_payer()
    );

    // add_spot
    let (spot, _) = spot_pda(&listing, 0);
    let (vault, _) = vault_pda(&spot);
    let add = new_ix(
        program_id(),
        adspaces::instruction::AddSpot { price: PRICE }.data(),
        adspaces::accounts::AddSpot {
            seller: seller.pubkey(),
            rent_payer: impostor.pubkey(),
            listing,
            spot,
            vault,
            config: env.config,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
            token_program: token_program_id(),
            rent: rent::ID,
            reviewer: env.mint_authority.pubkey(),
            review_authority: env.review_authority,
        }
        .to_account_metas(None),
    );
    assert_program_error(
        send(
            &mut env.svm,
            &[add],
            &seller,
            &[&impostor, &env.mint_authority],
        ),
        AdspacesError::Unauthorized,
    );
    assert!(env.svm.get_account(&spot).is_none());

    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (open, open_vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);

    // submit_proof
    env.warp_to_timestamp(now + 86_400);
    let (proof, _) = proof_pda(&spot);
    let submit = new_ix(
        program_id(),
        adspaces::instruction::SubmitProof {
            proof_hash: [7; 32],
            proof_uri: PROOF_URI.to_string(),
        }
        .data(),
        adspaces::accounts::SubmitProof {
            seller: seller.pubkey(),
            rent_payer: impostor.pubkey(),
            listing,
            spot,
            proof,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert_program_error(
        send(&mut env.svm, &[submit], &seller, &[&impostor]),
        AdspacesError::Unauthorized,
    );
    assert!(env.svm.get_account(&proof).is_none());

    // raise_dispute
    env.warp_to_timestamp(now + 8 * 86_400);
    let (dispute, _) = dispute_pda(&spot);
    let raise = new_ix(
        program_id(),
        adspaces::instruction::RaiseDispute {
            evidence_hash: [9; 32],
            evidence_uri: DISPUTE_URI.to_string(),
        }
        .data(),
        adspaces::accounts::RaiseDispute {
            buyer: buyer.pubkey(),
            config: env.config,
            listing,
            spot,
            rent_payer: impostor.pubkey(),
            dispute,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert_program_error(
        send(&mut env.svm, &[raise], &buyer, &[&impostor]),
        AdspacesError::Unauthorized,
    );
    assert!(env.svm.get_account(&dispute).is_none());

    // close_spot and close_listing
    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    assert_program_error(
        close_spot(
            &mut env,
            &impostor,
            &impostor,
            &listing,
            &open,
            &open_vault,
            &seller_ata,
        ),
        AdspacesError::Unauthorized,
    );
    assert!(env.svm.get_account(&open).is_some());
    assert_program_error(
        close_listing(&mut env, &impostor, &impostor, &listing),
        AdspacesError::Unauthorized,
    );
    assert!(env.svm.get_account(&listing).is_some());
}

/// The same instruction with the optional proof account left out (Anchor's program-id placeholder).
fn without_proof(ix: &Instruction, proof: &Pubkey) -> Instruction {
    let mut ix = ix.clone();
    for account in ix.accounts.iter_mut() {
        if account.pubkey == *proof {
            account.pubkey = program_id();
            account.is_writable = false;
        }
    }
    ix
}

#[test]
fn closing_a_spot_with_proof_requires_its_proof_record() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [41; 16], now + 30 * 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 30 * 86_400 + 1);
    submit_proof(&mut env, &seller, &listing, &spot);
    let (proof, _) = proof_pda(&spot);

    // A seller refund that leaves the proof record out is refused.
    let refund_ix = new_ix(
        program_id(),
        adspaces::instruction::RefundSpot {}.data(),
        adspaces::accounts::RefundSpot {
            caller: seller.pubkey(),
            listing,
            rent_payer: env.rent_payer(),
            spot,
            vault,
            buyer_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
            proof: None,
        }
        .to_account_metas(None),
    );
    assert_program_error(
        send(&mut env.svm, &[refund_ix], &seller, &[]),
        adspaces::error::AdspacesError::ProofAccountRequired,
    );

    // Neither can anyone time out the dispute without it; with it, every account closes.
    env.warp_to_timestamp(read_listing(&env.svm, &listing).delivery_deadline);
    dispute(&mut env, &buyer, &listing, &spot).unwrap();
    let opened_at = read_dispute(&env.svm, &dispute_pda(&spot).0).opened_at;
    env.warp_to_timestamp(opened_at + adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS + 1);
    let caller = funded_keypair(&mut env);
    let timeout = timeout_ix(&env, &caller.pubkey(), &listing, &spot, &vault, &buyer_ata);
    assert_program_error(
        send(
            &mut env.svm,
            &[without_proof(&timeout, &proof)],
            &caller,
            &[],
        ),
        adspaces::error::AdspacesError::ProofAccountRequired,
    );
    send(&mut env.svm, &[timeout], &caller, &[]).unwrap();
    assert!(env.svm.get_account(&proof).is_none());
    assert!(env.svm.get_account(&spot).is_none());
    assert_eq!(token_balance(&env.svm, &buyer_ata), PRICE);
}

// ---- CertiK fixes: proof window, buyer-paid disputes, timeout priority, seller account ---------

#[test]
fn proof_before_artwork_close_is_rejected() {
    let mut env = Env::new();
    let seller = funded_keypair(&mut env);
    let now = env.clock().unix_timestamp;
    let deadline = now + 86_400;
    let artwork_close = deadline + 3 * 86_400;
    let listing = create_fixed_listing_with_times(
        &mut env,
        &seller,
        [80; 16],
        deadline,
        artwork_close,
        adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
    );
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);

    // Between the bidding deadline and artwork close the buyer can still send artwork, so no proof.
    for at in [deadline, artwork_close - 1] {
        env.warp_to_timestamp(at);
        env.svm.expire_blockhash();
        assert_program_error(
            try_submit_proof(&mut env, &seller, &listing, &spot),
            adspaces::error::AdspacesError::ArtworkWindowOpen,
        );
    }
    assert_eq!(read_spot(&env.svm, &spot).proof_submitted_at, 0);
    assert!(env.svm.get_account(&proof_pda(&spot).0).is_none());

    env.warp_to_timestamp(artwork_close);
    env.svm.expire_blockhash();
    submit_proof(&mut env, &seller, &listing, &spot);
    assert_eq!(read_spot(&env.svm, &spot).proof_submitted_at, artwork_close);
}

/// Raises a dispute with the buyer as its own rent payer: no platform co-signature. `fee_payer`
/// pays the transaction fee, so the buyer's SOL moves by the dispute rent alone.
#[allow(clippy::result_large_err)]
fn buyer_paid_dispute(
    env: &mut Env,
    fee_payer: &Keypair,
    buyer: &Keypair,
    listing: &Pubkey,
    spot: &Pubkey,
) -> litesvm::types::TransactionResult {
    let (dispute, _) = dispute_pda(spot);
    let ix = new_ix(
        program_id(),
        adspaces::instruction::RaiseDispute {
            evidence_hash: [9; 32],
            evidence_uri: DISPUTE_URI.to_string(),
        }
        .data(),
        adspaces::accounts::RaiseDispute {
            buyer: buyer.pubkey(),
            rent_payer: buyer.pubkey(),
            config: env.config,
            listing: *listing,
            spot: *spot,
            dispute,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    env.svm.expire_blockhash();
    send(&mut env.svm, &[ix], fee_payer, &[buyer])
}

struct Disputed {
    seller: Keypair,
    buyer: Keypair,
    buyer_ata: Pubkey,
    fee_payer: Keypair,
    listing: Pubkey,
    spot: Pubkey,
    vault: Pubkey,
    seller_ata: Pubkey,
    treasury_ata: Pubkey,
}

/// A claimed spot with proof, at the middle of its review window, ready for a dispute.
fn proof_spot_in_review(env: &mut Env, slug: u8) -> Disputed {
    let seller = funded_keypair(env);
    let fee_payer = funded_keypair(env);
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(env, &seller, [slug; 16], now + 86_400);
    let (spot, vault) = add_spot_to(env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 86_400);
    submit_proof(env, &seller, &listing, &spot);
    env.warp_to_timestamp(
        now + 8 * 86_400 + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS / 2,
    );
    let treasury = env.treasury;
    let seller_ata = empty_token_account(env, &fee_payer, &seller.pubkey());
    let treasury_ata = empty_token_account(env, &fee_payer, &treasury);
    Disputed {
        seller,
        buyer,
        buyer_ata,
        fee_payer,
        listing,
        spot,
        vault,
        seller_ata,
        treasury_ata,
    }
}

#[test]
fn buyer_can_raise_a_proof_dispute_without_the_rent_payer() {
    let mut env = Env::new();
    let d = proof_spot_in_review(&mut env, 81);
    let (dispute, _) = dispute_pda(&d.spot);
    let platform_lamports = lamports(&env.svm, &env.rent_payer());
    let buyer_lamports = lamports(&env.svm, &d.buyer.pubkey());

    // Nobody but the listing's rent payer or the buyer itself can fund the dispute.
    let stranger = funded_keypair(&mut env);
    let mut ix = new_ix(
        program_id(),
        adspaces::instruction::RaiseDispute {
            evidence_hash: [9; 32],
            evidence_uri: DISPUTE_URI.to_string(),
        }
        .data(),
        adspaces::accounts::RaiseDispute {
            buyer: d.buyer.pubkey(),
            rent_payer: stranger.pubkey(),
            config: env.config,
            listing: d.listing,
            spot: d.spot,
            dispute,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert_program_error(
        send(
            &mut env.svm,
            &[ix.clone()],
            &d.fee_payer,
            &[&d.buyer, &stranger],
        ),
        adspaces::error::AdspacesError::Unauthorized,
    );
    // Another buyer cannot pay for (and so raise) a dispute on this spot either.
    let (other, _) = env.new_user(0);
    ix.accounts[0].pubkey = other.pubkey();
    ix.accounts[4].pubkey = other.pubkey();
    assert_program_error(
        send(&mut env.svm, &[ix], &d.fee_payer, &[&other]),
        adspaces::error::AdspacesError::NotWinningBuyer,
    );

    buyer_paid_dispute(&mut env, &d.fee_payer, &d.buyer, &d.listing, &d.spot).unwrap();
    let stored = read_dispute(&env.svm, &dispute);
    assert_eq!(stored.rent_payer, d.buyer.pubkey());
    assert_eq!(stored.buyer, d.buyer.pubkey());
    assert!(stored.proof_submitted);
    let rent = lamports(&env.svm, &dispute);
    assert_eq!(lamports(&env.svm, &d.buyer.pubkey()), buyer_lamports - rent);
    assert_eq!(lamports(&env.svm, &env.rent_payer()), platform_lamports);
    assert_eq!(
        read_spot(&env.svm, &d.spot).status,
        adspaces::constants::SPOT_DISPUTED
    );
}

#[test]
fn buyer_paid_proofless_dispute_is_rejected() {
    let mut env = Env::new();
    let seller = funded_keypair(&mut env);
    let fee_payer = funded_keypair(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [82; 16], now + 86_400);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);
    env.warp_to_timestamp(now + 8 * 86_400);

    // Without proof the buyer has the public refund; a buyer-paid dispute would only let it keep the
    // spot open through raise and withdraw cycles the platform never approved.
    assert_program_error(
        buyer_paid_dispute(&mut env, &fee_payer, &buyer, &listing, &spot),
        adspaces::error::AdspacesError::ProofRequired,
    );
    assert!(env.svm.get_account(&dispute_pda(&spot).0).is_none());
    // The platform-paid path is unchanged.
    dispute(&mut env, &buyer, &listing, &spot).unwrap();
    assert_eq!(
        read_dispute(&env.svm, &dispute_pda(&spot).0).rent_payer,
        env.rent_payer()
    );
}

#[test]
fn buyer_paid_dispute_rent_returns_to_buyer_on_withdraw_resolve_timeout() {
    for ending in ["withdraw", "resolve", "timeout"] {
        let mut env = Env::new();
        let d = proof_spot_in_review(&mut env, 83);
        let (dispute, _) = dispute_pda(&d.spot);
        let (proof, _) = proof_pda(&d.spot);
        buyer_paid_dispute(&mut env, &d.fee_payer, &d.buyer, &d.listing, &d.spot).unwrap();
        let opened_at = read_dispute(&env.svm, &dispute).opened_at;

        let dispute_rent = lamports(&env.svm, &dispute);
        let spot_rent: u64 = [d.spot, d.vault, proof]
            .iter()
            .map(|a| lamports(&env.svm, a))
            .sum();
        let buyer_lamports = lamports(&env.svm, &d.buyer.pubkey());
        let platform_lamports = lamports(&env.svm, &env.rent_payer());
        let seller_lamports = lamports(&env.svm, &d.seller.pubkey());

        match ending {
            "withdraw" => {
                let ix = new_ix(
                    program_id(),
                    adspaces::instruction::WithdrawDispute {}.data(),
                    adspaces::accounts::WithdrawDispute {
                        buyer: d.buyer.pubkey(),
                        rent_payer: d.buyer.pubkey(),
                        listing: d.listing,
                        spot: d.spot,
                        dispute,
                    }
                    .to_account_metas(None),
                );
                // The dispute's rent cannot go to the platform, which never paid it.
                let mut misdirected = ix.clone();
                misdirected.accounts[4].pubkey = env.rent_payer();
                assert_program_error(
                    send(&mut env.svm, &[misdirected], &d.fee_payer, &[&d.buyer]),
                    adspaces::error::AdspacesError::Unauthorized,
                );
                send(&mut env.svm, &[ix], &d.fee_payer, &[&d.buyer]).unwrap();
                assert_eq!(
                    read_spot(&env.svm, &d.spot).status,
                    adspaces::constants::SPOT_FUNDED
                );
                assert_eq!(
                    lamports(&env.svm, &d.buyer.pubkey()),
                    buyer_lamports + dispute_rent
                );
                assert_eq!(lamports(&env.svm, &env.rent_payer()), platform_lamports);
                continue;
            }
            "resolve" => {
                let resolve = resolve_ix(
                    &env,
                    &d.seller.pubkey(),
                    &d.listing,
                    &d.spot,
                    &d.vault,
                    &d.seller_ata,
                    &d.buyer_ata,
                    &d.treasury_ata,
                );
                assert_eq!(resolve.accounts[6].pubkey, d.buyer.pubkey());
                send(
                    &mut env.svm,
                    &[resolve],
                    &d.fee_payer,
                    &[&env.mint_authority],
                )
                .unwrap();
            }
            _ => {
                env.warp_to_timestamp(
                    opened_at + adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS,
                );
                let timeout = timeout_ix(
                    &env,
                    &d.fee_payer.pubkey(),
                    &d.listing,
                    &d.spot,
                    &d.vault,
                    &d.buyer_ata,
                );
                send(&mut env.svm, &[timeout], &d.fee_payer, &[]).unwrap();
                assert_eq!(token_balance(&env.svm, &d.buyer_ata), PRICE);
            }
        }
        for closed in [d.spot, d.vault, proof, dispute] {
            assert!(env.svm.get_account(&closed).is_none(), "{ending}");
        }
        // Dispute rent back to the buyer, spot/vault/proof rent back to the platform, none to the seller.
        assert_eq!(
            lamports(&env.svm, &d.buyer.pubkey()),
            buyer_lamports + dispute_rent,
            "{ending}"
        );
        assert_eq!(
            lamports(&env.svm, &env.rent_payer()),
            platform_lamports + spot_rent,
            "{ending}"
        );
        assert_eq!(lamports(&env.svm, &d.seller.pubkey()), seller_lamports);
        assert_eq!(read_listing(&env.svm, &d.listing).closed_spot_count, 1);
    }
}

#[test]
fn dispute_rent_cannot_be_redirected() {
    use adspaces::error::AdspacesError;

    // Platform-paid and buyer-paid: each rent account must be exactly its payer.
    for buyer_paid in [false, true] {
        let mut env = Env::new();
        let d = proof_spot_in_review(&mut env, 84);
        if buyer_paid {
            buyer_paid_dispute(&mut env, &d.fee_payer, &d.buyer, &d.listing, &d.spot).unwrap();
        } else {
            dispute(&mut env, &d.buyer, &d.listing, &d.spot).unwrap();
        }
        let opened_at = read_dispute(&env.svm, &dispute_pda(&d.spot).0).opened_at;
        let resolve = resolve_ix(
            &env,
            &d.seller.pubkey(),
            &d.listing,
            &d.spot,
            &d.vault,
            &d.seller_ata,
            &d.buyer_ata,
            &d.treasury_ata,
        );
        // Accounts 5 and 6 are rent_payer and dispute_rent_payer.
        for (index, impostor) in [
            (5, d.seller.pubkey()),
            (5, d.buyer.pubkey()),
            (6, d.seller.pubkey()),
        ] {
            let mut misdirected = resolve.clone();
            if misdirected.accounts[index].pubkey == impostor {
                continue;
            }
            misdirected.accounts[index].pubkey = impostor;
            env.svm.expire_blockhash();
            assert_program_error(
                send(
                    &mut env.svm,
                    &[misdirected],
                    &d.fee_payer,
                    &[&env.mint_authority],
                ),
                AdspacesError::Unauthorized,
            );
        }
        let mut swapped = resolve.clone();
        if buyer_paid {
            swapped.accounts[6].pubkey = env.rent_payer();
            assert_program_error(
                send(
                    &mut env.svm,
                    &[swapped],
                    &d.fee_payer,
                    &[&env.mint_authority],
                ),
                AdspacesError::Unauthorized,
            );
        }

        env.warp_to_timestamp(opened_at + adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS);
        let timeout = timeout_ix(
            &env,
            &d.fee_payer.pubkey(),
            &d.listing,
            &d.spot,
            &d.vault,
            &d.buyer_ata,
        );
        for (index, impostor) in [(5, d.fee_payer.pubkey()), (6, d.fee_payer.pubkey())] {
            let mut misdirected = timeout.clone();
            misdirected.accounts[index].pubkey = impostor;
            env.svm.expire_blockhash();
            assert_program_error(
                send(&mut env.svm, &[misdirected], &d.fee_payer, &[]),
                AdspacesError::Unauthorized,
            );
        }
        assert_eq!(token_balance(&env.svm, &d.vault), PRICE);
        send(&mut env.svm, &[timeout], &d.fee_payer, &[]).unwrap();
    }
}

#[test]
fn resolve_after_timeout_is_rejected_and_timeout_refunds() {
    let mut env = Env::new();
    let d = proof_spot_in_review(&mut env, 85);
    dispute(&mut env, &d.buyer, &d.listing, &d.spot).unwrap();
    let opened_at = read_dispute(&env.svm, &dispute_pda(&d.spot).0).opened_at;
    let timeout_at = opened_at + adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS;

    // Once the timeout matures, a late resolution can no longer override the full refund.
    for at in [timeout_at, timeout_at + 86_400] {
        env.warp_to_timestamp(at);
        let resolve = resolve_ix(
            &env,
            &d.seller.pubkey(),
            &d.listing,
            &d.spot,
            &d.vault,
            &d.seller_ata,
            &d.buyer_ata,
            &d.treasury_ata,
        );
        env.svm.expire_blockhash();
        assert_program_error(
            send(
                &mut env.svm,
                &[resolve],
                &d.fee_payer,
                &[&env.mint_authority],
            ),
            adspaces::error::AdspacesError::DisputeTimeoutPassed,
        );
    }
    assert_eq!(token_balance(&env.svm, &d.vault), PRICE);
    let timeout = timeout_ix(
        &env,
        &d.fee_payer.pubkey(),
        &d.listing,
        &d.spot,
        &d.vault,
        &d.buyer_ata,
    );
    send(&mut env.svm, &[timeout], &d.fee_payer, &[]).unwrap();
    assert_eq!(token_balance(&env.svm, &d.buyer_ata), PRICE);
    assert_eq!(token_balance(&env.svm, &d.seller_ata), 0);
}

#[test]
fn resolve_just_before_timeout_still_settles() {
    let mut env = Env::new();
    let d = proof_spot_in_review(&mut env, 86);
    dispute(&mut env, &d.buyer, &d.listing, &d.spot).unwrap();
    let opened_at = read_dispute(&env.svm, &dispute_pda(&d.spot).0).opened_at;
    env.warp_to_timestamp(opened_at + adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS - 1);
    let resolve = resolve_ix(
        &env,
        &d.seller.pubkey(),
        &d.listing,
        &d.spot,
        &d.vault,
        &d.seller_ata,
        &d.buyer_ata,
        &d.treasury_ata,
    );
    let meta = send(
        &mut env.svm,
        &[resolve],
        &d.fee_payer,
        &[&env.mint_authority],
    )
    .unwrap();
    let half = PRICE / 2;
    let fee = (half as u128 * FEE_BPS / 10_000) as u64;
    assert_event(
        &meta,
        &adspaces::events::DisputeResolved {
            listing: d.listing,
            index: 0,
            dispute: dispute_pda(&d.spot).0,
            resolver: env.mint_authority.pubkey(),
            decision_hash: [8; 32],
            seller_share_bps: 5_000,
            buyer_refund: half,
            seller_amount: half - fee,
            fee,
            treasury_ata: d.treasury_ata,
        },
    );
}

/// Makes the seller's wallet owned by another program, as a wallet that `assign`s itself away
/// (or a PDA seller) would be.
fn reassign_owner(env: &mut Env, key: &Pubkey) {
    let mut account = env.svm.get_account(key).unwrap();
    account.owner = token_program_id();
    env.svm.set_account(*key, account).unwrap();
}

#[test]
fn seller_paid_even_if_wallet_owner_changes() {
    // withdraw_seller after the review window.
    let mut env = Env::new();
    let d = proof_spot_in_review(&mut env, 87);
    reassign_owner(&mut env, &d.seller.pubkey());
    let listing = read_listing(&env.svm, &d.listing);
    env.warp_to_timestamp(listing.delivery_deadline + listing.review_window_seconds);
    withdraw(
        &mut env,
        &d.fee_payer,
        &d.seller.pubkey(),
        &d.listing,
        &d.spot,
        &d.vault,
        &d.seller_ata,
        &d.treasury_ata,
    )
    .unwrap();
    let fee = (PRICE as u128 * FEE_BPS / 10_000) as u64;
    assert_eq!(token_balance(&env.svm, &d.seller_ata), PRICE - fee);

    // resolve_dispute with a seller share.
    let mut env = Env::new();
    let d = proof_spot_in_review(&mut env, 88);
    dispute(&mut env, &d.buyer, &d.listing, &d.spot).unwrap();
    reassign_owner(&mut env, &d.seller.pubkey());
    let resolve = resolve_ix(
        &env,
        &d.seller.pubkey(),
        &d.listing,
        &d.spot,
        &d.vault,
        &d.seller_ata,
        &d.buyer_ata,
        &d.treasury_ata,
    );
    // The seller account is still pinned to listing.seller (and seller_ata to its key).
    let mut impostor = resolve.clone();
    let seller_index = impostor
        .accounts
        .iter()
        .position(|a| a.pubkey == d.seller.pubkey())
        .unwrap();
    impostor.accounts[seller_index].pubkey = d.fee_payer.pubkey();
    assert!(send(
        &mut env.svm,
        &[impostor],
        &d.fee_payer,
        &[&env.mint_authority]
    )
    .is_err());
    assert_eq!(token_balance(&env.svm, &d.vault), PRICE);
    send(
        &mut env.svm,
        &[resolve],
        &d.fee_payer,
        &[&env.mint_authority],
    )
    .unwrap();
    let half = PRICE / 2;
    let fee = (half as u128 * FEE_BPS / 10_000) as u64;
    assert_eq!(token_balance(&env.svm, &d.seller_ata), half - fee);
    assert_eq!(token_balance(&env.svm, &d.buyer_ata), half);
}
