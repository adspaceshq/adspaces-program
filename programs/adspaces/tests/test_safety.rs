mod common;

use anchor_lang::{
    solana_program::{instruction::AccountMeta, rent, system_program},
    InstructionData, ToAccountMetas,
};
use solana_keypair::Keypair;
use solana_signer::Signer;

use common::*;

const PRICE: u64 = 100_000_000;
const DAY: i64 = 86_400;

fn create_listing(env: &mut Env, seller: &Keypair, slug: u8, mode: u8, deadline: i64) -> Pubkey {
    let slug_hash = [slug; 16];
    let (listing, _) = listing_pda(&seller.pubkey(), &slug_hash);
    let auction = mode == adspaces::constants::MODE_AUCTION;
    let ix = new_ix(
        program_id(),
        adspaces::instruction::CreateListing {
            slug_hash,
            metadata_uri: "https://adspaces.test/safety".to_string(),
            mode,
            deadline,
            artwork_close: deadline,
            delivery_deadline: deadline + 7 * DAY,
            review_window_seconds: adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS,
            min_increment: if auction { 10_000_000 } else { 0 },
            double_up: false,
            fee_bps: 250,
            uplift_fee_bps: if auction { 2_000 } else { 0 },
            max_fee_bps: if auction { 1_000 } else { 250 },
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
    listing
}

fn add_spot(env: &mut Env, seller: &Keypair, listing: &Pubkey) -> (Pubkey, Pubkey) {
    let index = read_listing(&env.svm, listing).spot_count;
    let (spot, _) = spot_pda(listing, index);
    let (vault, _) = vault_pda(&spot);
    let ix = new_ix(
        program_id(),
        adspaces::instruction::AddSpot { price: PRICE }.data(),
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
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[]).unwrap();
    (spot, vault)
}

#[allow(clippy::result_large_err)]
fn claim(
    env: &mut Env,
    buyer: &Keypair,
    buyer_ata: &Pubkey,
    listing: &Pubkey,
    spot: &Pubkey,
    vault: &Pubkey,
) -> litesvm::types::TransactionResult {
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
    send(&mut env.svm, &[ix], buyer, &[])
}

#[allow(clippy::result_large_err)]
fn set_paused(env: &mut Env, paused: bool) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        adspaces::instruction::PauseProtocol { paused }.data(),
        adspaces::accounts::PauseProtocol {
            authority: env.mint_authority.pubkey(),
            config: env.config,
        }
        .to_account_metas(None),
    );
    let authority = env.mint_authority.insecure_clone();
    send(&mut env.svm, &[ix], &authority, &[])
}

#[allow(clippy::result_large_err)]
fn close_listing(
    env: &mut Env,
    seller: &Keypair,
    listing: &Pubkey,
    spots: &[Pubkey],
) -> litesvm::types::TransactionResult {
    let mut ix = new_ix(
        program_id(),
        adspaces::instruction::CloseListing {}.data(),
        adspaces::accounts::CloseListing {
            seller: seller.pubkey(),
            listing: *listing,
        }
        .to_account_metas(None),
    );
    ix.accounts
        .extend(spots.iter().map(|s| AccountMeta::new_readonly(*s, false)));
    send(&mut env.svm, &[ix], seller, &[])
}

fn new_seller(env: &mut Env) -> Keypair {
    let seller = Keypair::new();
    env.svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    seller
}

#[test]
fn pause_blocks_claims_until_unpaused() {
    let mut env = Env::new();
    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_listing(&mut env, &seller, 40, adspaces::constants::MODE_FIXED, now + DAY);
    let (spot, vault) = add_spot(&mut env, &seller, &listing);
    let (buyer, buyer_ata) = env.new_user(PRICE);

    let stranger = new_seller(&mut env);
    let mut forged = new_ix(
        program_id(),
        adspaces::instruction::PauseProtocol { paused: true }.data(),
        adspaces::accounts::PauseProtocol {
            authority: stranger.pubkey(),
            config: env.config,
        }
        .to_account_metas(None),
    );
    forged.accounts[0].is_signer = true;
    assert!(send(&mut env.svm, &[forged], &stranger, &[]).is_err());

    set_paused(&mut env, true).unwrap();
    assert!(read_config(&env.svm, &env.config).paused);
    assert!(claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault).is_err());
    assert_eq!(token_balance(&env.svm, &vault), 0);

    set_paused(&mut env, false).unwrap();
    env.svm.expire_blockhash();
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault).unwrap();
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}

#[test]
fn seller_cannot_claim_own_spot() {
    let mut env = Env::new();
    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_listing(&mut env, &seller, 41, adspaces::constants::MODE_FIXED, now + DAY);
    let (spot, vault) = add_spot(&mut env, &seller, &listing);

    // A funded USDC account owned by the seller, so only the self-dealing check can fail.
    let seller_token = {
        let account = Keypair::new();
        let create = anchor_lang::solana_program::system_instruction::create_account(
            &seller.pubkey(),
            &account.pubkey(),
            RENT_FUNDING,
            spl_token_interface::state::Account::LEN as u64,
            &token_program_id(),
        );
        send(&mut env.svm, &[create], &seller, &[&account]).unwrap();
        let init = spl_token::instruction::initialize_account(
            &token_program_id(),
            &account.pubkey(),
            &env.usdc,
            &seller.pubkey(),
        )
        .unwrap();
        send(&mut env.svm, &[init], &seller, &[]).unwrap();
        let mint_to = spl_token::instruction::mint_to(
            &token_program_id(),
            &env.usdc,
            &account.pubkey(),
            &env.mint_authority.pubkey(),
            &[],
            PRICE,
        )
        .unwrap();
        let authority = env.mint_authority.insecure_clone();
        send(&mut env.svm, &[mint_to], &authority, &[]).unwrap();
        account.pubkey()
    };

    assert!(claim(&mut env, &seller, &seller_token, &listing, &spot, &vault).is_err());
    assert_eq!(token_balance(&env.svm, &seller_token), PRICE);
}

#[test]
fn close_listing_requires_deadline_and_closed_spots() {
    let mut env = Env::new();
    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_listing(&mut env, &seller, 42, adspaces::constants::MODE_FIXED, now + DAY);
    let (open_spot, open_vault) = add_spot(&mut env, &seller, &listing);
    let (funded_spot, funded_vault) = add_spot(&mut env, &seller, &listing);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &funded_spot, &funded_vault).unwrap();

    let close_open = new_ix(
        program_id(),
        adspaces::instruction::CloseSpot {}.data(),
        adspaces::accounts::CloseSpot {
            seller: seller.pubkey(),
            listing,
            spot: open_spot,
            vault: open_vault,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[close_open], &seller, &[]).unwrap();

    let spots = [open_spot, funded_spot];
    assert!(close_listing(&mut env, &seller, &listing, &spots).is_err());

    env.warp_to_timestamp(now + DAY);
    assert!(close_listing(&mut env, &seller, &listing, &spots).is_err());
    assert!(close_listing(&mut env, &seller, &listing, &spots[..1]).is_err());
    assert!(env.svm.get_account(&listing).is_some());

    let refund = new_ix(
        program_id(),
        adspaces::instruction::RefundSpot {}.data(),
        adspaces::accounts::RefundSpot {
            caller: seller.pubkey(),
            listing,
            seller: seller.pubkey(),
            spot: funded_spot,
            vault: funded_vault,
            buyer_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[refund], &seller, &[]).unwrap();
    assert_eq!(token_balance(&env.svm, &buyer_ata), PRICE);

    env.svm.expire_blockhash();
    close_listing(&mut env, &seller, &listing, &spots).unwrap();
    assert!(env.svm.get_account(&listing).is_none());
}

#[test]
fn timed_out_dispute_refunds_buyer_in_full() {
    let mut env = Env::new();
    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_listing(&mut env, &seller, 43, adspaces::constants::MODE_FIXED, now + DAY);
    let (spot, vault) = add_spot(&mut env, &seller, &listing);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault).unwrap();

    let opened_at = now + 8 * DAY;
    env.warp_to_timestamp(opened_at);
    let (dispute, _) = dispute_pda(&spot);
    let raise = new_ix(
        program_id(),
        adspaces::instruction::RaiseDispute {
            evidence_hash: [0; 32],
            evidence_uri: String::new(),
        }
        .data(),
        adspaces::accounts::RaiseDispute {
            buyer: buyer.pubkey(),
            config: env.config,
            listing,
            spot,
            dispute,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[raise], &buyer, &[]).unwrap();

    let caller = new_seller(&mut env);
    let timeout = new_ix(
        program_id(),
        adspaces::instruction::TimeoutDispute {}.data(),
        adspaces::accounts::TimeoutDispute {
            caller: caller.pubkey(),
            config: env.config,
            listing,
            seller: seller.pubkey(),
            spot,
            dispute,
            dispute_buyer: read_dispute(&env.svm, &dispute).buyer,
            proof: None,
            vault,
            buyer_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    env.warp_to_timestamp(opened_at + adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS - 1);
    assert!(send(&mut env.svm, &[timeout.clone()], &caller, &[]).is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);

    env.warp_to_timestamp(opened_at + adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS);
    env.svm.expire_blockhash();
    send(&mut env.svm, &[timeout], &caller, &[]).unwrap();
    assert_eq!(token_balance(&env.svm, &buyer_ata), PRICE);
    for closed in [spot, vault, dispute] {
        assert!(env.svm.get_account(&closed).is_none());
    }
}

#[test]
fn migrate_config_upgrades_legacy_layout() {
    let mut env = Env::new();
    let mut account = env.svm.get_account(&env.config).unwrap();
    let new_len = account.data.len();
    account.data.truncate(new_len - 8);
    env.svm.set_account(env.config, account).unwrap();

    let config_key = env.config;
    let migrate = |authority: Pubkey| {
        new_ix(
            program_id(),
            adspaces::instruction::MigrateConfig {}.data(),
            adspaces::accounts::MigrateConfig {
                authority,
                config: config_key,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    };
    let stranger = new_seller(&mut env);
    assert!(send(&mut env.svm, &[migrate(stranger.pubkey())], &stranger, &[]).is_err());

    let authority = env.mint_authority.insecure_clone();
    send(&mut env.svm, &[migrate(authority.pubkey())], &authority, &[]).unwrap();
    let config = read_config(&env.svm, &env.config);
    assert_eq!(env.svm.get_account(&env.config).unwrap().data.len(), new_len);
    assert!(!config.paused);
    assert_eq!(config.authority, authority.pubkey());
    assert_eq!(config.fee_bps, 250);
    assert_eq!(
        config.dispute_timeout_seconds,
        adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS
    );

    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_listing(&mut env, &seller, 45, adspaces::constants::MODE_FIXED, now + DAY);
    add_spot(&mut env, &seller, &listing);
}

#[test]
fn late_bid_extends_auction_deadline() {
    let mut env = Env::new();
    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let deadline = now + DAY;
    let listing = create_listing(&mut env, &seller, 44, adspaces::constants::MODE_AUCTION, deadline);
    let (spot, vault) = add_spot(&mut env, &seller, &listing);
    let (alice, alice_ata) = env.new_user(PRICE);

    let bid = |env: &mut Env, at: i64| {
        env.warp_to_timestamp(at);
        let ix = new_ix(
            program_id(),
            adspaces::instruction::PlaceBid { amount: PRICE }.data(),
            adspaces::accounts::PlaceBid {
                bidder: alice.pubkey(),
                listing,
                spot,
                vault,
                bidder_ata: alice_ata,
                prev_bidder_ata: None,
                config: env.config,
                usdc_mint: env.usdc,
                token_program: token_program_id(),
            }
            .to_account_metas(None),
        );
        send(&mut env.svm, &[ix], &alice, &[])
    };

    bid(&mut env, deadline - adspaces::constants::SNIPE_WINDOW_SECONDS).unwrap();
    assert_eq!(
        read_listing(&env.svm, &listing).deadline,
        deadline + adspaces::constants::SNIPE_EXTENSION_SECONDS
    );
}

#[allow(clippy::result_large_err)]
fn set_timeout(env: &mut Env, signer: &Keypair, seconds: i64) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        adspaces::instruction::SetDisputeTimeout { seconds }.data(),
        adspaces::accounts::SetDisputeTimeout {
            authority: signer.pubkey(),
            config: env.config,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], signer, &[])
}

#[test]
fn dispute_timeout_is_configurable_within_bounds() {
    let mut env = Env::new();
    assert_eq!(
        read_config(&env.svm, &env.config).dispute_timeout_seconds,
        adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS
    );
    let authority = env.mint_authority.insecure_clone();
    let stranger = new_seller(&mut env);
    assert!(set_timeout(&mut env, &stranger, 2 * DAY).is_err());
    assert!(set_timeout(&mut env, &authority, adspaces::constants::MIN_DISPUTE_TIMEOUT_SECONDS - 1).is_err());
    assert!(set_timeout(&mut env, &authority, adspaces::constants::MAX_DISPUTE_TIMEOUT_SECONDS + 1).is_err());
    set_timeout(&mut env, &authority, 2 * DAY).unwrap();
    assert_eq!(read_config(&env.svm, &env.config).dispute_timeout_seconds, 2 * DAY);
}
