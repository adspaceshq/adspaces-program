mod common;

use anchor_lang::{
    solana_program::{rent, system_program},
    InstructionData, ToAccountMetas,
};
use solana_keypair::Keypair;
use solana_signer::Signer;
use spl_token_interface::state;

use common::*;

const START: u64 = 100_000_000; // 100 USDC
const INCREMENT: u64 = 10_000_000; // Vincent-style 10 USDC

fn create_auction_listing(
    env: &mut Env,
    seller: &Keypair,
    slug_hash: [u8; 16],
    min_increment: u64,
    double_up: bool,
) -> Pubkey {
    create_auction_listing_timed(env, seller, slug_hash, min_increment, double_up, 30 * 86_400, 37 * 86_400)
}

// `deadline_offset` is the bidding deadline; artwork closes with it, and delivery is due at `delivery_offset`.
#[allow(clippy::too_many_arguments)]
fn create_auction_listing_timed(
    env: &mut Env,
    seller: &Keypair,
    slug_hash: [u8; 16],
    min_increment: u64,
    double_up: bool,
    deadline_offset: i64,
    delivery_offset: i64,
) -> Pubkey {
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let now = env.clock().unix_timestamp;
    let ix = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/api/listings/auction".to_string(),
            mode: adspaces::constants::MODE_AUCTION,
            deadline: now + deadline_offset,
            artwork_close: now + deadline_offset,
            delivery_deadline: now + delivery_offset,
            review_window_seconds: adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
            min_increment,
            double_up,
            fee_bps: 250,
            uplift_fee_bps: 1_000,
            max_fee_bps: 750,
        }
        .data(),
        adspaces::accounts::CreateListing {
            seller: seller.pubkey(),
            reviewer: env.mint_authority.pubkey(),
            listing,
            config: env.config,
            review_authority: env.review_authority,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[&env.mint_authority]).unwrap();

    let (spot, _) = spot_pda(&listing, 0);
    let (vault, _) = vault_pda(&spot);
    let add = new_ix(
        program_id(),
        adspaces::instruction::AddSpot { price: START }.data(),
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
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[add], seller, &[]).unwrap();
    listing
}

fn spot_and_vault(listing: &Pubkey) -> (Pubkey, Pubkey) {
    let (spot, _) = spot_pda(listing, 0);
    let (vault, _) = vault_pda(&spot);
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

#[allow(clippy::result_large_err, clippy::too_many_arguments)]
fn bid(
    env: &mut Env,
    bidder: &Keypair,
    bidder_ata: &Pubkey,
    listing: &Pubkey,
    spot: &Pubkey,
    vault: &Pubkey,
    prev_bidder_ata: Option<Pubkey>,
    amount: u64,
) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        adspaces::instruction::PlaceBid { amount }.data(),
        adspaces::accounts::PlaceBid {
            bidder: bidder.pubkey(),
            listing: *listing,
            spot: *spot,
            vault: *vault,
            bidder_ata: *bidder_ata,
            prev_bidder_ata,
            config: env.config,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], bidder, &[])
}

#[test]
fn auction_enforces_min_increment_and_refunds_in_full() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    let listing = create_auction_listing(&mut env, &seller, [10u8; 16], INCREMENT, false);
    let (spot, vault) = spot_and_vault(&listing);

    let (alice, alice_ata) = env.new_user(1_000_000_000);
    let (bob, bob_ata) = env.new_user(1_000_000_000);

    // Below the starting price: rejected.
    assert!(bid(
        &mut env,
        &alice,
        &alice_ata,
        &listing,
        &spot,
        &vault,
        None,
        START - 1
    )
    .is_err());

    // Opening bid at the starting price: accepted.
    bid(
        &mut env, &alice, &alice_ata, &listing, &spot, &vault, None, START,
    )
    .unwrap();
    let stored = read_spot(&env.svm, &spot);
    assert_eq!(stored.current_bid, START);
    assert_eq!(stored.current_bidder, alice.pubkey());
    assert_eq!(token_balance(&env.svm, &vault), START);

    // Below current + increment: rejected.
    assert!(bid(
        &mut env,
        &bob,
        &bob_ata,
        &listing,
        &spot,
        &vault,
        Some(alice_ata),
        START + INCREMENT - 1
    )
    .is_err());

    // Takeover at exactly current + increment: accepted, Alice refunded in full.
    bid(
        &mut env,
        &bob,
        &bob_ata,
        &listing,
        &spot,
        &vault,
        Some(alice_ata),
        START + INCREMENT,
    )
    .unwrap();
    let stored = read_spot(&env.svm, &spot);
    assert_eq!(stored.current_bid, START + INCREMENT);
    assert_eq!(stored.current_bidder, bob.pubkey());
    assert_eq!(token_balance(&env.svm, &vault), START + INCREMENT);
    assert_eq!(token_balance(&env.svm, &alice_ata), 1_000_000_000);
    assert_eq!(
        token_balance(&env.svm, &bob_ata),
        1_000_000_000 - START - INCREMENT
    );
}

#[test]
fn double_up_requires_doubling() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    let listing = create_auction_listing(&mut env, &seller, [11u8; 16], 0, true);
    let (spot, vault) = spot_and_vault(&listing);

    let (alice, alice_ata) = env.new_user(1_000_000_000);
    let (bob, bob_ata) = env.new_user(2_000_000_000);

    bid(
        &mut env, &alice, &alice_ata, &listing, &spot, &vault, None, START,
    )
    .unwrap();

    // 1.5x is not enough on a double-up spot.
    assert!(bid(
        &mut env,
        &bob,
        &bob_ata,
        &listing,
        &spot,
        &vault,
        Some(alice_ata),
        START + START / 2
    )
    .is_err());

    // 2x takes over and refunds Alice in full.
    bid(
        &mut env,
        &bob,
        &bob_ata,
        &listing,
        &spot,
        &vault,
        Some(alice_ata),
        START * 2,
    )
    .unwrap();
    let stored = read_spot(&env.svm, &spot);
    assert_eq!(stored.current_bid, START * 2);
    assert_eq!(stored.current_bidder, bob.pubkey());
    assert_eq!(token_balance(&env.svm, &alice_ata), 1_000_000_000);
    assert_eq!(token_balance(&env.svm, &vault), START * 2);
}

#[test]
fn takeover_without_refund_account_fails() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    let listing = create_auction_listing(&mut env, &seller, [12u8; 16], INCREMENT, false);
    let (spot, vault) = spot_and_vault(&listing);

    let (alice, alice_ata) = env.new_user(1_000_000_000);
    let (bob, bob_ata) = env.new_user(1_000_000_000);

    bid(
        &mut env, &alice, &alice_ata, &listing, &spot, &vault, None, START,
    )
    .unwrap();

    // Refund account missing: the takeover must fail, funds stay put.
    assert!(bid(
        &mut env,
        &bob,
        &bob_ata,
        &listing,
        &spot,
        &vault,
        None,
        START + INCREMENT
    )
    .is_err());
    assert_eq!(token_balance(&env.svm, &vault), START);
}

#[test]
fn only_final_bidder_can_dispute_and_receive_full_refund() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    let now = env.clock().unix_timestamp;
    let listing = create_auction_listing(&mut env, &seller, [13; 16], INCREMENT, false);
    let (spot, vault) = spot_and_vault(&listing);
    let (alice, alice_ata) = env.new_user(1_000_000_000);
    let (bob, bob_ata) = env.new_user(1_000_000_000);
    bid(
        &mut env, &alice, &alice_ata, &listing, &spot, &vault, None, START,
    )
    .unwrap();
    let winning_bid = START + INCREMENT;
    bid(
        &mut env,
        &bob,
        &bob_ata,
        &listing,
        &spot,
        &vault,
        Some(alice_ata),
        winning_bid,
    )
    .unwrap();
    assert_eq!(token_balance(&env.svm, &alice_ata), 1_000_000_000);

    env.warp_to_timestamp(now + 37 * 86_400);
    let (dispute, _) = dispute_pda(&spot);
    let dispute_ix = |buyer: Pubkey| {
        new_ix(
            program_id(),
            adspaces::instruction::RaiseDispute {
                evidence_hash: [0; 32],
                evidence_uri: String::new(),
            }
            .data(),
            adspaces::accounts::RaiseDispute {
                buyer,
                rent_payer: env.mint_authority.pubkey(),
                config: env.config,
                listing,
                spot,
                dispute,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    };
    assert!(send(&mut env.svm, &[dispute_ix(alice.pubkey())], &alice, &[&env.mint_authority]).is_err());
    send(&mut env.svm, &[dispute_ix(bob.pubkey())], &bob, &[&env.mint_authority]).unwrap();
    assert_eq!(
        read_spot(&env.svm, &spot).status,
        adspaces::constants::SPOT_DISPUTED
    );

    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let treasury = env.treasury;
    let treasury_ata = empty_token_account(&mut env, &seller, &treasury);
    let (dispute_record, _) = dispute_pda(&spot);
    let resolve = new_ix(
        program_id(),
        adspaces::instruction::ResolveDispute {
            seller_share_bps: 0,
            decision_hash: [7; 32],
            decision_uri: "ipfs://auction-refund-decision".to_string(),
        }
        .data(),
        adspaces::accounts::ResolveDispute {
            resolver: env.mint_authority.pubkey(),
            config: env.config,
            listing,
            spot,
            dispute: dispute_record,
            rent_payer: read_dispute(&env.svm, &dispute_record).rent_payer,
            vault,
            seller_ata,
            seller: seller.pubkey(),
            buyer_ata: bob_ata,
            treasury_ata,
            proof: None,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[resolve], &env.mint_authority, &[]).unwrap();
    assert_eq!(token_balance(&env.svm, &bob_ata), 1_000_000_000);
    assert_eq!(token_balance(&env.svm, &seller_ata), 0);
    assert_eq!(token_balance(&env.svm, &treasury_ata), 0);
    assert!(env.svm.get_account(&dispute_record).is_none());
    assert!(env.svm.get_account(&spot).is_none());
    assert!(env.svm.get_account(&vault).is_none());
}

#[test]
fn listing_rejects_dust_bid_steps() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    let slug_hash = [11u8; 16];
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let now = env.clock().unix_timestamp;
    let ix = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/api/listings/dust".to_string(),
            mode: adspaces::constants::MODE_AUCTION,
            deadline: now + 30 * 86_400,
            artwork_close: now + 30 * 86_400,
            delivery_deadline: now + 37 * 86_400,
            review_window_seconds: adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
            min_increment: 1,
            double_up: false,
            fee_bps: 250,
            uplift_fee_bps: 1_000,
            max_fee_bps: 750,
        }
        .data(),
        adspaces::accounts::CreateListing {
            seller: seller.pubkey(),
            reviewer: env.mint_authority.pubkey(),
            listing,
            config: env.config,
            review_authority: env.review_authority,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(send(&mut env.svm, &[ix], &seller, &[&env.mint_authority]).is_err());
}

#[test]
fn anti_snipe_extension_stops_at_delivery_deadline() {
    let mut env = Env::new();
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();

    // Delivery is due only 120 seconds after bidding closes, so one snipe extension would overshoot it.
    let listing = create_auction_listing_timed(&mut env, &seller, [12u8; 16], INCREMENT, false, 30 * 86_400, 30 * 86_400 + 120);
    let (spot, vault) = spot_and_vault(&listing);
    let before = read_listing(&env.svm, &listing);
    let cap = before.delivery_deadline - 1;

    let (alice, alice_ata) = env.new_user(1_000_000_000);
    env.warp_to_timestamp(before.deadline - 10);
    bid(&mut env, &alice, &alice_ata, &listing, &spot, &vault, None, START).unwrap();
    assert_eq!(read_listing(&env.svm, &listing).deadline, cap);

    // At the cap, a further late bid must not move the deadline again.
    let (bob, bob_ata) = env.new_user(1_000_000_000);
    env.warp_to_timestamp(cap - 10);
    bid(&mut env, &bob, &bob_ata, &listing, &spot, &vault, Some(alice_ata), START + INCREMENT).unwrap();
    assert_eq!(read_listing(&env.svm, &listing).deadline, cap);
}
