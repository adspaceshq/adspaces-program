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
        spotboard::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://spotboard.test/api/listings/demo".to_string(),
            mode: spotboard::constants::MODE_FIXED,
            deadline: now + 30 * 86_400,
            artwork_close: now + 30 * 86_400,
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

    let stored = read_listing(&env.svm, &listing);
    assert_eq!(stored.seller, seller.pubkey());
    assert_eq!(stored.mode, spotboard::constants::MODE_FIXED);
    assert_eq!(stored.spot_count, 0);
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

    let stored = read_spot(&env.svm, &spot);
    assert_eq!(stored.price, price);
    assert_eq!(stored.status, spotboard::constants::SPOT_OPEN);
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
    assert_eq!(stored.status, spotboard::constants::SPOT_FUNDED);
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
        spotboard::instruction::ClaimSpot {}.data(),
        spotboard::accounts::ClaimSpot {
            buyer: buyer2.pubkey(),
            listing,
            spot,
            vault,
            buyer_ata: buyer2_ata,
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
        spotboard::instruction::PlaceBid { amount: PRICE }.data(),
        spotboard::accounts::PlaceBid {
            bidder: bidder.pubkey(),
            listing,
            spot,
            vault,
            bidder_ata,
            prev_bidder_ata: None,
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
        spotboard::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://spotboard.test/api/listings/auction".to_string(),
            mode: spotboard::constants::MODE_AUCTION,
            deadline: now + 30 * 86_400,
            artwork_close: now + 30 * 86_400,
            min_increment: 10_000_000,
            double_up: false,
        }
        .data(),
        spotboard::accounts::CreateListing {
            seller: seller.pubkey(),
            listing: auction_listing,
            config: env.config,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[create], &seller, &[]).unwrap();
    let (a_spot, a_vault) = add_spot_to(&mut env, &seller, &auction_listing, PRICE);

    let claim_ix = new_ix(
        program_id(),
        spotboard::instruction::ClaimSpot {}.data(),
        spotboard::accounts::ClaimSpot {
            buyer: bidder.pubkey(),
            listing: auction_listing,
            spot: a_spot,
            vault: a_vault,
            buyer_ata: bidder_ata,
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
        spotboard::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://spotboard.test/api/listings/flash".to_string(),
            mode: spotboard::constants::MODE_FIXED,
            deadline: now + 10,
            artwork_close: now + 10,
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
    send(&mut env.svm, &[create], &seller, &[]).unwrap();
    let (spot, vault) = add_spot_to(&mut env, &seller, &listing, PRICE);

    env.warp_to_timestamp(now + 1_000);

    let (buyer, buyer_ata) = env.new_user(1_000_000_000);
    let claim_ix = new_ix(
        program_id(),
        spotboard::instruction::ClaimSpot {}.data(),
        spotboard::accounts::ClaimSpot {
            buyer: buyer.pubkey(),
            listing,
            spot,
            vault,
            buyer_ata,
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
        spotboard::instruction::AddSpot { price: PRICE }.data(),
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

    assert!(send(&mut env.svm, &[ix], &seller, &[]).is_err());
}
