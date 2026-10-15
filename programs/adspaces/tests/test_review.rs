mod common;

use anchor_lang::{solana_program::system_program, InstructionData, ToAccountMetas};
use solana_keypair::Keypair;
use solana_signer::Signer;

use common::*;

#[allow(clippy::too_many_arguments, clippy::result_large_err)]
fn create_listing(
    env: &mut Env,
    seller: &Keypair,
    reviewer: &Keypair,
    include_reviewer_signature: bool,
    mode: u8,
    fee_bps: u16,
    uplift_fee_bps: u16,
    max_fee_bps: u16,
    review_window_seconds: i64,
    slug_hash: [u8; 16],
) -> litesvm::types::TransactionResult {
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let now = env.clock().unix_timestamp;
    // Not the reviewer, so leaving out the reviewer's signature really leaves it out.
    let rent_payer = Keypair::new();
    env.svm
        .airdrop(&rent_payer.pubkey(), 1_000_000_000)
        .unwrap();
    let ix = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/reviewed".to_string(),
            mode,
            deadline: now + 86_400,
            artwork_close: now + 86_400,
            delivery_deadline: now + 2 * 86_400,
            review_window_seconds,
            min_increment: if mode == adspaces::constants::MODE_AUCTION {
                1
            } else {
                0
            },
            double_up: false,
            fee_bps,
            uplift_fee_bps,
            max_fee_bps,
        }
        .data(),
        adspaces::accounts::CreateListing {
            seller: seller.pubkey(),
            reviewer: reviewer.pubkey(),
            rent_payer: rent_payer.pubkey(),
            listing,
            config: env.config,
            review_authority: env.review_authority,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let signers = if include_reviewer_signature {
        vec![reviewer, &rent_payer]
    } else {
        vec![&rent_payer]
    };
    send(&mut env.svm, &[ix], seller, &signers)
}

fn copy_keypair(keypair: &Keypair) -> Keypair {
    Keypair::new_from_array(keypair.to_bytes()[..32].try_into().unwrap())
}

#[test]
fn listing_requires_the_configured_reviewer_signature() {
    let mut env = Env::new();
    let seller = Keypair::new();
    let reviewer = copy_keypair(&env.mint_authority);
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        create_listing(
            &mut env,
            &seller,
            &reviewer,
            false,
            adspaces::constants::MODE_FIXED,
            250,
            0,
            250,
            adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
            [41; 16],
        )
    }));
    assert!(result.is_err());
}

#[test]
fn listing_rejects_an_unconfigured_reviewer() {
    let mut env = Env::new();
    let seller = Keypair::new();
    let reviewer = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    assert!(create_listing(
        &mut env,
        &seller,
        &reviewer,
        true,
        adspaces::constants::MODE_FIXED,
        250,
        0,
        250,
        adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
        [42; 16],
    )
    .is_err());
}

#[test]
fn fixed_listing_rejects_variable_fee_terms() {
    let mut env = Env::new();
    let seller = Keypair::new();
    let reviewer = copy_keypair(&env.mint_authority);
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    assert!(create_listing(
        &mut env,
        &seller,
        &reviewer,
        true,
        adspaces::constants::MODE_FIXED,
        250,
        1_000,
        750,
        adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
        [43; 16],
    )
    .is_err());
}

#[test]
fn auction_listing_rejects_invalid_fee_ordering() {
    let mut env = Env::new();
    let seller = Keypair::new();
    let reviewer = copy_keypair(&env.mint_authority);
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    assert!(create_listing(
        &mut env,
        &seller,
        &reviewer,
        true,
        adspaces::constants::MODE_AUCTION,
        500,
        750,
        250,
        adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
        [44; 16],
    )
    .is_err());
}

#[test]
fn listing_rejects_a_base_fee_below_the_config_floor() {
    let mut env = Env::new();
    let seller = Keypair::new();
    let reviewer = copy_keypair(&env.mint_authority);
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    assert!(create_listing(
        &mut env,
        &seller,
        &reviewer,
        true,
        adspaces::constants::MODE_FIXED,
        249,
        0,
        249,
        adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
        [45; 16],
    )
    .is_err());
}

#[test]
fn reviewer_sets_a_bounded_listing_review_window() {
    let mut env = Env::new();
    let seller = Keypair::new();
    let reviewer = copy_keypair(&env.mint_authority);
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    let selected_window = 3 * 86_400;
    create_listing(
        &mut env,
        &seller,
        &reviewer,
        true,
        adspaces::constants::MODE_FIXED,
        250,
        0,
        250,
        selected_window,
        [46; 16],
    )
    .unwrap();
    let (listing, _) = listing_pda(&seller.pubkey(), &[46; 16]);
    assert_eq!(
        read_listing(&env.svm, &listing).review_window_seconds,
        selected_window
    );

    for (window, slug_hash) in [
        (adspaces::constants::MIN_REVIEW_WINDOW_SECONDS - 1, [47; 16]),
        (adspaces::constants::MAX_REVIEW_WINDOW_SECONDS + 1, [48; 16]),
    ] {
        assert!(create_listing(
            &mut env,
            &seller,
            &reviewer,
            true,
            adspaces::constants::MODE_FIXED,
            250,
            0,
            250,
            window,
            slug_hash,
        )
        .is_err());
    }
}
