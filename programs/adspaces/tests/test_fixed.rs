mod common;

use anchor_lang::{
    solana_program::{rent, system_program},
    InstructionData, ToAccountMetas,
};
use solana_keypair::Keypair;
use solana_signer::Signer;

use common::*;

const PRICE: u64 = 150_000_000; // 150 USDC

fn create_fixed_listing(env: &mut Env, seller: &Keypair, slug_hash: [u8; 16]) -> Pubkey {
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let now = env.clock().unix_timestamp;
    let ix = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/api/listings/demo".to_string(),
            mode: adspaces::constants::MODE_FIXED,
            deadline: now + 30 * 86_400,
            artwork_close: now + 30 * 86_400,
            delivery_deadline: now + 37 * 86_400,
            review_window_seconds: adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
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

    let stored = read_listing(&env.svm, &listing);
    assert_eq!(stored.seller, seller.pubkey());
    assert_eq!(stored.mode, adspaces::constants::MODE_FIXED);
    assert_eq!(stored.spot_count, 0);
    assert_eq!(stored.closed_spot_count, 0);
    assert_eq!(stored.rent_payer, env.rent_payer());
    assert_eq!(stored.delivery_deadline, now + 37 * 86_400);
    listing
}

#[test]
fn delivery_deadline_must_follow_artwork_close() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let slug_hash = [31; 16];
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let ix = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/listings/invalid-delivery".to_string(),
            mode: adspaces::constants::MODE_FIXED,
            deadline: now + 86_400,
            artwork_close: now + 2 * 86_400,
            delivery_deadline: now + 2 * 86_400,
            review_window_seconds: adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
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
    assert!(send(&mut env.svm, &[ix], &seller, &[&env.mint_authority]).is_err());
    assert!(env.svm.get_account(&listing).is_none());
}

#[test]
fn metadata_uri_accepts_exact_account_limit() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;

    for (slug_hash, length, succeeds) in [
        ([32; 16], adspaces::constants::METADATA_URI_MAX_LEN, true),
        (
            [33; 16],
            adspaces::constants::METADATA_URI_MAX_LEN + 1,
            false,
        ),
    ] {
        let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
        let ix = new_ix(
            program_id(),
            adspaces::instruction::CreateListing {
                slug_hash,
                metadata_uri: "m".repeat(length),
                mode: adspaces::constants::MODE_FIXED,
                deadline: now + 86_400,
                artwork_close: now + 86_400,
                delivery_deadline: now + 2 * 86_400,
                review_window_seconds: adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
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
        assert_eq!(
            send(&mut env.svm, &[ix], &seller, &[&env.mint_authority]).is_ok(),
            succeeds
        );
        assert_eq!(env.svm.get_account(&listing).is_some(), succeeds);
    }
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

    let stored = read_spot(&env.svm, &spot);
    assert_eq!(stored.price, price);
    assert_eq!(stored.status, adspaces::constants::SPOT_OPEN);
    (spot, vault)
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

#[test]
fn fixed_claim_moves_funds_into_vault() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    let listing = create_fixed_listing(&mut env, &seller, [1u8; 16]);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);
    add_spot_to(&mut env, &seller, &listing, PRICE);
    assert_eq!(read_listing(&env.svm, &listing).spot_count, 2);

    let (buyer, buyer_ata) = env.new_user(1_000_000_000);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);

    let stored = read_spot(&env.svm, &spot);
    assert_eq!(stored.status, adspaces::constants::SPOT_FUNDED);
    assert_eq!(stored.buyer, buyer.pubkey());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
    assert_eq!(token_balance(&env.svm, &buyer_ata), 1_000_000_000 - PRICE);
}

#[test]
fn fixed_claim_twice_fails() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    let listing = create_fixed_listing(&mut env, &seller, [2u8; 16]);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);

    let (buyer, buyer_ata) = env.new_user(1_000_000_000);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault);

    let (buyer2, buyer2_ata) = env.new_user(1_000_000_000);
    let ix = new_ix(
        program_id(),
        adspaces::instruction::ClaimSpot {}.data(),
        adspaces::accounts::ClaimSpot {
            buyer: buyer2.pubkey(),
            listing,
            spot,
            vault,
            buyer_ata: buyer2_ata,
            config: env.config,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    let res = send(&mut env.svm, &[ix], &buyer2, &[]);
    assert!(res.is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}

#[test]
fn fixed_listing_rejects_bids_and_vice_versa() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    let listing = create_fixed_listing(&mut env, &seller, [3u8; 16]);
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);

    let (bidder, bidder_ata) = env.new_user(1_000_000_000);
    let bid = new_ix(
        program_id(),
        adspaces::instruction::PlaceBid { amount: PRICE }.data(),
        adspaces::accounts::PlaceBid {
            bidder: bidder.pubkey(),
            listing,
            spot,
            vault,
            bidder_ata,
            prev_bidder_ata: None,
            config: env.config,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    assert!(send(&mut env.svm, &[bid], &bidder, &[]).is_err());

    // An auction spot rejects claims.
    let now = env.clock().unix_timestamp;
    let slug_hash = [4u8; 16];
    let (auction_listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let create = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/api/listings/auction".to_string(),
            mode: adspaces::constants::MODE_AUCTION,
            deadline: now + 30 * 86_400,
            artwork_close: now + 30 * 86_400,
            delivery_deadline: now + 37 * 86_400,
            review_window_seconds: adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
            min_increment: 10_000_000,
            double_up: false,
            fee_bps: 250,
            uplift_fee_bps: 1_000,
            max_fee_bps: 750,
        }
        .data(),
        adspaces::accounts::CreateListing {
            seller: seller.pubkey(),
            reviewer: env.mint_authority.pubkey(),
            rent_payer: env.rent_payer(),
            listing: auction_listing,
            config: env.config,
            review_authority: env.review_authority,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[create], &seller, &[&env.mint_authority]).unwrap();
    let (a_spot, a_vault) = add_spot_to(&mut env, &seller, &auction_listing, PRICE);

    let claim_ix = new_ix(
        program_id(),
        adspaces::instruction::ClaimSpot {}.data(),
        adspaces::accounts::ClaimSpot {
            buyer: bidder.pubkey(),
            listing: auction_listing,
            spot: a_spot,
            vault: a_vault,
            buyer_ata: bidder_ata,
            config: env.config,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    assert!(send(&mut env.svm, &[claim_ix], &bidder, &[]).is_err());
}

#[test]
fn claim_after_deadline_fails() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    let now = env.clock().unix_timestamp;
    let slug_hash = [5u8; 16];
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let create = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/api/listings/flash".to_string(),
            mode: adspaces::constants::MODE_FIXED,
            deadline: now + 10,
            artwork_close: now + 10,
            delivery_deadline: now + 10 + adspaces::constants::MIN_PROOF_WINDOW_SECONDS,
            review_window_seconds: adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
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
    send(&mut env.svm, &[create], &seller, &[&env.mint_authority]).unwrap();
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);

    env.warp_to_timestamp(now + 1_000);

    let (buyer, buyer_ata) = env.new_user(1_000_000_000);
    let claim_ix = new_ix(
        program_id(),
        adspaces::instruction::ClaimSpot {}.data(),
        adspaces::accounts::ClaimSpot {
            buyer: buyer.pubkey(),
            listing,
            spot,
            vault,
            buyer_ata,
            config: env.config,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    assert!(send(&mut env.svm, &[claim_ix], &buyer, &[]).is_err());
    assert_eq!(token_balance(&env.svm, &vault), 0);
}

#[test]
fn add_spot_after_deadline_fails() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_fixed_listing(&mut env, &seller, [6u8; 16]);
    env.warp_to_timestamp(now + 31 * 86_400);

    let (spot, _) = spot_pda(&listing, 0);
    let (vault, _) = vault_pda(&spot);
    let ix = new_ix(
        program_id(),
        adspaces::instruction::AddSpot { price: PRICE }.data(),
        adspaces::accounts::AddSpot {
            seller: seller.pubkey(),
            listing,
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

    assert!(send(&mut env.svm, &[ix], &seller, &[&env.mint_authority]).is_err());
}

#[test]
fn spot_added_without_the_review_authority_is_refused() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let listing = create_fixed_listing(&mut env, &seller, [31u8; 16]);
    let (spot, _) = spot_pda(&listing, 0);
    let (vault, _) = vault_pda(&spot);
    // The seller alone can't add a spot after approval: a stranger posing as the reviewer is rejected.
    let stranger = Keypair::new();
    env.svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    let ix = new_ix(
        program_id(),
        adspaces::instruction::AddSpot { price: PRICE }.data(),
        adspaces::accounts::AddSpot {
            seller: seller.pubkey(),
            listing,
            spot,
            vault,
            config: env.config,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
            token_program: token_program_id(),
            rent: rent::ID,
            reviewer: stranger.pubkey(),
            rent_payer: env.rent_payer(),
            review_authority: env.review_authority,
        }
        .to_account_metas(None),
    );
    assert_program_error(
        send(
            &mut env.svm,
            &[ix],
            &seller,
            &[&stranger, &env.mint_authority],
        ),
        adspaces::error::AdspacesError::InvalidReviewAuthority,
    );
}

#[allow(clippy::result_large_err, clippy::too_many_arguments)]
fn try_create_listing_with(
    env: &mut Env,
    seller: &Keypair,
    slug_hash: [u8; 16],
    mode: u8,
    artwork_close: i64,
    delivery_deadline: i64,
    min_increment: u64,
    double_up: bool,
) -> litesvm::types::TransactionResult {
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let now = env.clock().unix_timestamp;
    let auction = mode == adspaces::constants::MODE_AUCTION;
    let ix = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/api/listings/bounds".to_string(),
            mode,
            deadline: now + 86_400,
            artwork_close,
            delivery_deadline,
            review_window_seconds: adspaces::constants::MAX_REVIEW_WINDOW_SECONDS,
            min_increment,
            double_up,
            fee_bps: 250,
            uplift_fee_bps: if auction { 2_000 } else { 0 },
            max_fee_bps: if auction { 1_000 } else { 250 },
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
    env.svm.expire_blockhash();
    send(&mut env.svm, &[ix], seller, &[&env.mint_authority])
}

#[test]
fn listing_rejects_a_delivery_deadline_beyond_the_horizon() {
    use adspaces::constants::{MAX_LISTING_HORIZON_SECONDS, MODE_FIXED};

    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let horizon = now + MAX_LISTING_HORIZON_SECONDS;
    let artwork_close = now + 2 * 86_400;

    assert_program_error(
        try_create_listing_with(
            &mut env,
            &seller,
            [70; 16],
            MODE_FIXED,
            artwork_close,
            horizon + 1,
            0,
            false,
        ),
        adspaces::error::AdspacesError::InvalidDeliveryDeadline,
    );
    // Far-future values that used to overflow delivery_deadline + review_window are refused too.
    assert_program_error(
        try_create_listing_with(
            &mut env,
            &seller,
            [71; 16],
            MODE_FIXED,
            artwork_close,
            i64::MAX,
            0,
            false,
        ),
        adspaces::error::AdspacesError::InvalidDeliveryDeadline,
    );
    try_create_listing_with(
        &mut env,
        &seller,
        [72; 16],
        MODE_FIXED,
        artwork_close,
        horizon,
        0,
        false,
    )
    .unwrap();
    let (listing, _) = listing_pda(&seller.pubkey(), &[72; 16]);
    assert_eq!(read_listing(&env.svm, &listing).delivery_deadline, horizon);
}

#[test]
fn listing_rejects_a_proof_window_below_the_minimum() {
    use adspaces::constants::{MIN_PROOF_WINDOW_SECONDS, MODE_FIXED};

    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let artwork_close = now + 2 * 86_400;

    assert_program_error(
        try_create_listing_with(
            &mut env,
            &seller,
            [73; 16],
            MODE_FIXED,
            artwork_close,
            artwork_close + MIN_PROOF_WINDOW_SECONDS - 1,
            0,
            false,
        ),
        adspaces::error::AdspacesError::InvalidDeliveryDeadline,
    );
    try_create_listing_with(
        &mut env,
        &seller,
        [74; 16],
        MODE_FIXED,
        artwork_close,
        artwork_close + MIN_PROOF_WINDOW_SECONDS,
        0,
        false,
    )
    .unwrap();
}

#[test]
fn listing_rejects_an_oversized_min_increment() {
    use adspaces::constants::{MAX_BID_INCREMENT, MODE_AUCTION};

    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let artwork_close = now + 2 * 86_400;
    let delivery = now + 9 * 86_400;

    // The cap holds with and without double-up, so the stored step is always representable.
    for (slug, double_up) in [([75; 16], false), ([76; 16], true)] {
        assert_program_error(
            try_create_listing_with(
                &mut env,
                &seller,
                slug,
                MODE_AUCTION,
                artwork_close,
                delivery,
                MAX_BID_INCREMENT + 1,
                double_up,
            ),
            adspaces::error::AdspacesError::InvalidMinIncrement,
        );
    }
    try_create_listing_with(
        &mut env,
        &seller,
        [77; 16],
        MODE_AUCTION,
        artwork_close,
        delivery,
        MAX_BID_INCREMENT,
        false,
    )
    .unwrap();
    let (listing, _) = listing_pda(&seller.pubkey(), &[77; 16]);
    assert_eq!(
        read_listing(&env.svm, &listing).min_increment,
        MAX_BID_INCREMENT
    );
}
