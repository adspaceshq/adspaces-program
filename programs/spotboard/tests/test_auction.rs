mod common;

use anchor_lang::{
    solana_program::{rent, system_program},
    InstructionData, ToAccountMetas,
};
use solana_keypair::Keypair;
use solana_signer::Signer;

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
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let now = env.clock().unix_timestamp;
    let ix = new_ix(
        program_id(),
        spotboard::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://spotboard.test/api/listings/auction".to_string(),
            mode: spotboard::constants::MODE_AUCTION,
            deadline: now + 30 * 86_400,
            artwork_close: now + 30 * 86_400,
            min_increment,
            double_up,
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

    let (spot, _) = spot_pda(&listing, 0);
    let (vault, _) = vault_pda(&spot);
    let add = new_ix(
        program_id(),
        spotboard::instruction::AddSpot { price: START }.data(),
        spotboard::accounts::AddSpot {
            seller: seller.pubkey(),
            listing,
            spot,
            vault,
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
        spotboard::instruction::PlaceBid { amount }.data(),
        spotboard::accounts::PlaceBid {
            bidder: bidder.pubkey(),
            listing: *listing,
            spot: *spot,
            vault: *vault,
            bidder_ata: *bidder_ata,
            prev_bidder_ata,
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
