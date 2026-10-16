mod common;

use anchor_lang::{
    solana_program::{rent, system_program},
    InstructionData, ToAccountMetas,
};
use solana_keypair::Keypair;
use solana_signer::Signer;

use base64::Engine;

use common::*;

const PRICE: u64 = 100_000_000;
const DAY: i64 = 86_400;

fn create_listing(env: &mut Env, seller: &Keypair, slug: u8, mode: u8, deadline: i64) -> Pubkey {
    try_create_listing(env, seller, slug, mode, deadline).unwrap();
    listing_pda(&seller.pubkey(), &[slug; 16]).0
}

#[allow(clippy::result_large_err)]
fn try_create_listing(
    env: &mut Env,
    seller: &Keypair,
    slug: u8,
    mode: u8,
    deadline: i64,
) -> litesvm::types::TransactionResult {
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
            artwork_close: deadline + 3_600,
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
            rent_payer: env.rent_payer(),
            listing,
            config: env.config,
            review_authority: env.review_authority,
            usdc_mint: env.usdc,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[&env.mint_authority])
}

fn add_spot(env: &mut Env, seller: &Keypair, listing: &Pubkey) -> (Pubkey, Pubkey) {
    let index = read_listing(&env.svm, listing).spot_count;
    let (spot, _) = spot_pda(listing, index);
    try_add_spot(env, seller, listing).unwrap();
    (spot, vault_pda(&spot).0)
}

#[allow(clippy::result_large_err)]
fn try_add_spot(
    env: &mut Env,
    seller: &Keypair,
    listing: &Pubkey,
) -> litesvm::types::TransactionResult {
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
            reviewer: env.mint_authority.pubkey(),
            rent_payer: env.rent_payer(),
            review_authority: env.review_authority,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], seller, &[&env.mint_authority])
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
    let authority = env.mint_authority.insecure_clone();
    pause_protocol_as(env, &authority, paused)
}

#[allow(clippy::result_large_err)]
fn pause_protocol_as(
    env: &mut Env,
    signer: &Keypair,
    paused: bool,
) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        adspaces::instruction::PauseProtocol { paused }.data(),
        adspaces::accounts::PauseProtocol {
            authority: signer.pubkey(),
            config: env.config,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[ix], signer, &[])
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
    let listing = create_listing(
        &mut env,
        &seller,
        40,
        adspaces::constants::MODE_FIXED,
        now + DAY,
    );
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

    let paused = set_paused(&mut env, true).unwrap();
    let expected = base64::engine::general_purpose::STANDARD.encode(anchor_lang::Event::data(
        &adspaces::events::ProtocolPauseSet {
            authority: env.mint_authority.pubkey(),
            paused: true,
        },
    ));
    assert!(
        paused
            .logs
            .iter()
            .any(|log| log == &format!("Program data: {expected}")),
        "pausing must emit ProtocolPauseSet"
    );
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
    let listing = create_listing(
        &mut env,
        &seller,
        41,
        adspaces::constants::MODE_FIXED,
        now + DAY,
    );
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
fn only_the_rent_payer_closes_spots_and_listing_once_every_spot_is_closed() {
    use adspaces::error::AdspacesError;

    let mut env = Env::new();
    let seller = new_seller(&mut env);
    let stranger = new_seller(&mut env);
    let fee_payer = new_seller(&mut env);
    let rent_payer = env.mint_authority.insecure_clone();
    let now = env.clock().unix_timestamp;
    let listing = create_listing(
        &mut env,
        &seller,
        42,
        adspaces::constants::MODE_FIXED,
        now + DAY,
    );
    let (open_spot, open_vault) = add_spot(&mut env, &seller, &listing);
    let (funded_spot, funded_vault) = add_spot(&mut env, &seller, &listing);
    let (buyer, buyer_ata) = env.new_user(PRICE);
    claim(
        &mut env,
        &buyer,
        &buyer_ata,
        &listing,
        &funded_spot,
        &funded_vault,
    )
    .unwrap();
    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());

    // Open and funded spots both block closing the listing, even for the rent payer.
    assert_program_error(
        close_listing(&mut env, &fee_payer, &rent_payer, &listing),
        AdspacesError::ListingHasActiveSpots,
    );
    // Neither the seller nor a third party can close a spot or the listing.
    for intruder in [&seller, &stranger] {
        assert_program_error(
            close_spot(
                &mut env,
                intruder,
                intruder,
                &listing,
                &open_spot,
                &open_vault,
                &seller_ata,
            ),
            AdspacesError::Unauthorized,
        );
        assert_program_error(
            close_listing(&mut env, intruder, intruder, &listing),
            AdspacesError::Unauthorized,
        );
    }
    // A funded spot can't be closed out from under its buyer.
    assert_program_error(
        close_spot(
            &mut env,
            &fee_payer,
            &rent_payer,
            &listing,
            &funded_spot,
            &funded_vault,
            &seller_ata,
        ),
        AdspacesError::SpotAlreadyFunded,
    );

    // The rent payer closes the unsold spot before the deadline; its rent comes back, not to the seller.
    assert!(env.clock().unix_timestamp < read_listing(&env.svm, &listing).deadline);
    let seller_lamports = lamports(&env.svm, &seller.pubkey());
    let payer_lamports = lamports(&env.svm, &rent_payer.pubkey());
    let open_rent = lamports(&env.svm, &open_spot) + lamports(&env.svm, &open_vault);
    close_spot(
        &mut env,
        &fee_payer,
        &rent_payer,
        &listing,
        &open_spot,
        &open_vault,
        &seller_ata,
    )
    .unwrap();
    assert!(env.svm.get_account(&open_spot).is_none());
    assert!(env.svm.get_account(&open_vault).is_none());
    assert_eq!(
        lamports(&env.svm, &rent_payer.pubkey()),
        payer_lamports + open_rent
    );
    assert_eq!(lamports(&env.svm, &seller.pubkey()), seller_lamports);
    assert_eq!(read_listing(&env.svm, &listing).closed_spot_count, 1);

    // Past the deadline, the funded spot still blocks closing the listing.
    env.warp_to_timestamp(now + DAY);
    assert_program_error(
        close_listing(&mut env, &fee_payer, &rent_payer, &listing),
        AdspacesError::ListingHasActiveSpots,
    );
    assert!(env.svm.get_account(&listing).is_some());

    let refund = new_ix(
        program_id(),
        adspaces::instruction::RefundSpot {}.data(),
        adspaces::accounts::RefundSpot {
            caller: seller.pubkey(),
            listing,
            rent_payer: env.rent_payer(),
            spot: funded_spot,
            vault: funded_vault,
            buyer_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
            proof: None,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[refund], &seller, &[]).unwrap();
    assert_eq!(token_balance(&env.svm, &buyer_ata), PRICE);
    let stored = read_listing(&env.svm, &listing);
    assert_eq!(stored.closed_spot_count, stored.spot_count);

    // Once every spot is closed, only the rent payer can close the listing, and its rent comes back.
    for intruder in [&seller, &stranger] {
        assert_program_error(
            close_listing(&mut env, intruder, intruder, &listing),
            AdspacesError::Unauthorized,
        );
    }
    let seller_lamports = lamports(&env.svm, &seller.pubkey());
    let payer_lamports = lamports(&env.svm, &rent_payer.pubkey());
    let listing_rent = lamports(&env.svm, &listing);
    close_listing(&mut env, &fee_payer, &rent_payer, &listing).unwrap();
    assert!(env.svm.get_account(&listing).is_none());
    assert_eq!(
        lamports(&env.svm, &rent_payer.pubkey()),
        payer_lamports + listing_rent
    );
    assert_eq!(lamports(&env.svm, &seller.pubkey()), seller_lamports);
    assert!(close_listing(&mut env, &fee_payer, &rent_payer, &listing).is_err());
}

#[test]
fn timed_out_dispute_refunds_buyer_in_full() {
    let mut env = Env::new();
    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_listing(
        &mut env,
        &seller,
        43,
        adspaces::constants::MODE_FIXED,
        now + DAY,
    );
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
            rent_payer: env.mint_authority.pubkey(),
            config: env.config,
            listing,
            spot,
            dispute,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[raise], &buyer, &[&env.mint_authority]).unwrap();

    let caller = new_seller(&mut env);
    let timeout = new_ix(
        program_id(),
        adspaces::instruction::TimeoutDispute {}.data(),
        adspaces::accounts::TimeoutDispute {
            caller: caller.pubkey(),
            config: env.config,
            listing,
            spot,
            dispute,
            rent_payer: read_dispute(&env.svm, &dispute).rent_payer,
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
    // Rent for the dispute, spot and vault must go back to the listing's rent payer only.
    let mut misdirected = timeout.clone();
    let rent_payer_index = misdirected
        .accounts
        .iter()
        .position(|account| account.pubkey == env.rent_payer())
        .unwrap();
    misdirected.accounts[rent_payer_index].pubkey = caller.pubkey();
    assert_program_error(
        send(&mut env.svm, &[misdirected], &caller, &[]),
        adspaces::error::AdspacesError::Unauthorized,
    );
    let rent = [spot, vault, dispute]
        .iter()
        .map(|account| lamports(&env.svm, account))
        .sum::<u64>();
    let payer_lamports = lamports(&env.svm, &env.rent_payer());
    let seller_lamports = lamports(&env.svm, &seller.pubkey());
    env.svm.expire_blockhash();
    send(&mut env.svm, &[timeout], &caller, &[]).unwrap();
    assert_eq!(token_balance(&env.svm, &buyer_ata), PRICE);
    for closed in [spot, vault, dispute] {
        assert!(env.svm.get_account(&closed).is_none());
    }
    assert_eq!(lamports(&env.svm, &env.rent_payer()), payer_lamports + rent);
    assert_eq!(lamports(&env.svm, &seller.pubkey()), seller_lamports);
    assert_eq!(read_listing(&env.svm, &listing).closed_spot_count, 1);
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
    send(
        &mut env.svm,
        &[migrate(authority.pubkey())],
        &authority,
        &[],
    )
    .unwrap();
    let config = read_config(&env.svm, &env.config);
    assert_eq!(
        env.svm.get_account(&env.config).unwrap().data.len(),
        new_len
    );
    assert!(!config.paused);
    assert_eq!(config.authority, authority.pubkey());
    assert_eq!(config.fee_bps, 250);
    assert_eq!(
        config.dispute_timeout_seconds,
        adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS
    );

    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_listing(
        &mut env,
        &seller,
        45,
        adspaces::constants::MODE_FIXED,
        now + DAY,
    );
    add_spot(&mut env, &seller, &listing);
}

#[test]
fn late_bid_extends_auction_deadline() {
    let mut env = Env::new();
    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let deadline = now + DAY;
    let listing = create_listing(
        &mut env,
        &seller,
        44,
        adspaces::constants::MODE_AUCTION,
        deadline,
    );
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

    bid(
        &mut env,
        deadline - adspaces::constants::SNIPE_WINDOW_SECONDS,
    )
    .unwrap();
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
    assert!(set_timeout(
        &mut env,
        &authority,
        adspaces::constants::MIN_DISPUTE_TIMEOUT_SECONDS - 1
    )
    .is_err());
    assert!(set_timeout(
        &mut env,
        &authority,
        adspaces::constants::MAX_DISPUTE_TIMEOUT_SECONDS + 1
    )
    .is_err());
    set_timeout(&mut env, &authority, 2 * DAY).unwrap();
    assert_eq!(
        read_config(&env.svm, &env.config).dispute_timeout_seconds,
        2 * DAY
    );
}

fn empty_token_account(env: &mut Env, funder: &Keypair, owner: &Pubkey) -> Pubkey {
    let account = Keypair::new();
    let create = anchor_lang::solana_program::system_instruction::create_account(
        &funder.pubkey(),
        &account.pubkey(),
        RENT_FUNDING,
        spl_token_interface::state::Account::LEN as u64,
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

#[test]
fn shortening_the_timeout_does_not_end_an_open_dispute_early() {
    let mut env = Env::new();
    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_listing(
        &mut env,
        &seller,
        44,
        adspaces::constants::MODE_FIXED,
        now + DAY,
    );
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
            rent_payer: env.mint_authority.pubkey(),
            config: env.config,
            listing,
            spot,
            dispute,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[raise], &buyer, &[&env.mint_authority]).unwrap();

    // The platform shortens the timeout to its minimum while this dispute is open.
    let authority = env.mint_authority.insecure_clone();
    set_timeout(
        &mut env,
        &authority,
        adspaces::constants::MIN_DISPUTE_TIMEOUT_SECONDS,
    )
    .unwrap();

    let caller = new_seller(&mut env);
    let timeout = new_ix(
        program_id(),
        adspaces::instruction::TimeoutDispute {}.data(),
        adspaces::accounts::TimeoutDispute {
            caller: caller.pubkey(),
            config: env.config,
            listing,
            spot,
            dispute,
            rent_payer: read_dispute(&env.svm, &dispute).rent_payer,
            proof: None,
            vault,
            buyer_ata,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    // Well past the new minimum, but still inside the timeout the dispute was raised with.
    env.warp_to_timestamp(opened_at + adspaces::constants::MIN_DISPUTE_TIMEOUT_SECONDS + 60);
    assert!(send(&mut env.svm, &[timeout.clone()], &caller, &[]).is_err());
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}

#[allow(clippy::result_large_err)]
fn initialize_guardian(
    env: &mut Env,
    signer: &Keypair,
    guardian: Pubkey,
) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        adspaces::instruction::InitializePauseGuardian { guardian }.data(),
        adspaces::accounts::InitializePauseGuardian {
            authority: signer.pubkey(),
            config: env.config,
            pause_guardian: pause_guardian_pda(&env.config).0,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    env.svm.expire_blockhash();
    send(&mut env.svm, &[ix], signer, &[])
}

#[allow(clippy::result_large_err)]
fn update_guardian(
    env: &mut Env,
    signer: &Keypair,
    guardian: Pubkey,
) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        adspaces::instruction::UpdatePauseGuardian { guardian }.data(),
        adspaces::accounts::UpdatePauseGuardian {
            authority: signer.pubkey(),
            config: env.config,
            pause_guardian: pause_guardian_pda(&env.config).0,
        }
        .to_account_metas(None),
    );
    env.svm.expire_blockhash();
    send(&mut env.svm, &[ix], signer, &[])
}

#[allow(clippy::result_large_err)]
fn guardian_pause(env: &mut Env, signer: &Keypair) -> litesvm::types::TransactionResult {
    let ix = new_ix(
        program_id(),
        adspaces::instruction::GuardianPause {}.data(),
        adspaces::accounts::GuardianPause {
            guardian: signer.pubkey(),
            config: env.config,
            pause_guardian: pause_guardian_pda(&env.config).0,
        }
        .to_account_metas(None),
    );
    env.svm.expire_blockhash();
    send(&mut env.svm, &[ix], signer, &[])
}

fn emitted(result: &litesvm::types::TransactionMetadata, event: &impl anchor_lang::Event) -> bool {
    let expected = base64::engine::general_purpose::STANDARD.encode(event.data());
    result
        .logs
        .iter()
        .any(|log| log == &format!("Program data: {expected}"))
}

/// Config authority with a guardian already set, as on a live deployment after the handover.
fn env_with_guardian() -> (Env, Keypair, Keypair) {
    let mut env = Env::new();
    let authority = env.mint_authority.insecure_clone();
    let guardian = new_seller(&mut env);
    initialize_guardian(&mut env, &authority, guardian.pubkey()).unwrap();
    (env, authority, guardian)
}

#[test]
fn only_config_authority_sets_the_pause_guardian() {
    use adspaces::error::AdspacesError;

    let mut env = Env::new();
    let authority = env.mint_authority.insecure_clone();
    let stranger = new_seller(&mut env);
    let guardian = new_seller(&mut env);
    let (pause_guardian, bump) = pause_guardian_pda(&env.config);

    // Existing configs have no guardian until one is initialized, so nobody can guardian-pause.
    assert!(guardian_pause(&mut env, &guardian).is_err());
    assert!(!read_config(&env.svm, &env.config).paused);

    assert_program_error(
        initialize_guardian(&mut env, &stranger, guardian.pubkey()),
        AdspacesError::Unauthorized,
    );
    assert_program_error(
        initialize_guardian(&mut env, &authority, Pubkey::default()),
        AdspacesError::InvalidPauseGuardian,
    );
    let initialized = initialize_guardian(&mut env, &authority, guardian.pubkey()).unwrap();
    assert!(
        emitted(
            &initialized,
            &adspaces::events::PauseGuardianSet {
                authority: authority.pubkey(),
                previous_guardian: Pubkey::default(),
                guardian: guardian.pubkey(),
            }
        ),
        "initializing must emit PauseGuardianSet"
    );
    let stored = read_pause_guardian(&env.svm, &pause_guardian);
    assert_eq!(stored.config, env.config);
    assert_eq!(stored.guardian, guardian.pubkey());
    assert_eq!(stored.bump, bump);
    assert!(initialize_guardian(&mut env, &authority, stranger.pubkey()).is_err());

    // Neither a stranger nor the guardian itself can rotate the guardian.
    for intruder in [&stranger, &guardian] {
        assert_program_error(
            update_guardian(&mut env, intruder, intruder.pubkey()),
            AdspacesError::Unauthorized,
        );
    }
    let replacement = new_seller(&mut env);
    let updated = update_guardian(&mut env, &authority, replacement.pubkey()).unwrap();
    assert!(
        emitted(
            &updated,
            &adspaces::events::PauseGuardianSet {
                authority: authority.pubkey(),
                previous_guardian: guardian.pubkey(),
                guardian: replacement.pubkey(),
            }
        ),
        "updating must emit PauseGuardianSet"
    );
    assert_eq!(
        read_pause_guardian(&env.svm, &pause_guardian).guardian,
        replacement.pubkey()
    );
    assert_program_error(
        guardian_pause(&mut env, &guardian),
        AdspacesError::Unauthorized,
    );
    guardian_pause(&mut env, &replacement).unwrap();
    assert!(read_config(&env.svm, &env.config).paused);
}

#[test]
fn guardian_pauses_but_only_config_authority_unpauses() {
    use adspaces::error::AdspacesError;

    let (mut env, authority, guardian) = env_with_guardian();
    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_listing(
        &mut env,
        &seller,
        45,
        adspaces::constants::MODE_FIXED,
        now + DAY,
    );
    let (spot, vault) = add_spot(&mut env, &seller, &listing);
    let (buyer, buyer_ata) = env.new_user(PRICE);

    let stranger = new_seller(&mut env);
    assert_program_error(
        guardian_pause(&mut env, &stranger),
        AdspacesError::Unauthorized,
    );
    assert!(!read_config(&env.svm, &env.config).paused);

    let paused = guardian_pause(&mut env, &guardian).unwrap();
    assert!(
        emitted(
            &paused,
            &adspaces::events::ProtocolPauseSet {
                authority: guardian.pubkey(),
                paused: true,
            }
        ),
        "guardian pause must emit ProtocolPauseSet"
    );
    assert!(read_config(&env.svm, &env.config).paused);
    // Pausing again is a no-op that still succeeds, so a guardian racing the multisig never fails.
    guardian_pause(&mut env, &guardian).unwrap();
    assert!(read_config(&env.svm, &env.config).paused);

    assert_program_error(
        claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault),
        AdspacesError::ProtocolPaused,
    );
    assert_eq!(token_balance(&env.svm, &vault), 0);
    assert_program_error(
        try_add_spot(&mut env, &seller, &listing),
        AdspacesError::ProtocolPaused,
    );
    assert_program_error(
        try_create_listing(
            &mut env,
            &seller,
            46,
            adspaces::constants::MODE_FIXED,
            now + DAY,
        ),
        AdspacesError::ProtocolPaused,
    );

    // The guardian has no say over pause_protocol, in either direction.
    for paused in [false, true] {
        env.svm.expire_blockhash();
        assert_program_error(
            pause_protocol_as(&mut env, &guardian, paused),
            AdspacesError::Unauthorized,
        );
    }
    assert!(read_config(&env.svm, &env.config).paused);

    env.svm.expire_blockhash();
    pause_protocol_as(&mut env, &authority, false).unwrap();
    assert!(!read_config(&env.svm, &env.config).paused);
    env.svm.expire_blockhash();
    claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault).unwrap();
    assert_eq!(token_balance(&env.svm, &vault), PRICE);
}

#[test]
fn disabled_guardian_cannot_pause() {
    use adspaces::error::AdspacesError;

    let (mut env, authority, guardian) = env_with_guardian();
    // Pubkey::default() is the documented disabled value; nobody holds its key.
    update_guardian(&mut env, &authority, Pubkey::default()).unwrap();
    assert_eq!(
        read_pause_guardian(&env.svm, &pause_guardian_pda(&env.config).0).guardian,
        Pubkey::default()
    );
    assert_program_error(
        guardian_pause(&mut env, &guardian),
        AdspacesError::Unauthorized,
    );
    assert!(!read_config(&env.svm, &env.config).paused);

    // Re-enabling needs no re-initialization.
    update_guardian(&mut env, &authority, guardian.pubkey()).unwrap();
    guardian_pause(&mut env, &guardian).unwrap();
    assert!(read_config(&env.svm, &env.config).paused);
}

#[test]
fn payouts_refunds_and_disputes_still_settle_while_guardian_paused() {
    let (mut env, _, guardian) = env_with_guardian();
    let seller = new_seller(&mut env);
    let now = env.clock().unix_timestamp;
    let listing = create_listing(
        &mut env,
        &seller,
        47,
        adspaces::constants::MODE_FIXED,
        now + DAY,
    );
    let (paid_spot, paid_vault) = add_spot(&mut env, &seller, &listing);
    let (refunded_spot, refunded_vault) = add_spot(&mut env, &seller, &listing);
    let (disputed_spot, disputed_vault) = add_spot(&mut env, &seller, &listing);
    let mut buyers = Vec::new();
    for (spot, vault) in [
        (paid_spot, paid_vault),
        (refunded_spot, refunded_vault),
        (disputed_spot, disputed_vault),
    ] {
        let (buyer, buyer_ata) = env.new_user(PRICE);
        claim(&mut env, &buyer, &buyer_ata, &listing, &spot, &vault).unwrap();
        buyers.push((buyer, buyer_ata));
    }
    guardian_pause(&mut env, &guardian).unwrap();

    // The seller delivers on one spot and refunds another, both while paused.
    env.warp_to_timestamp(now + DAY);
    let (paid_proof, _) = proof_pda(&paid_spot);
    let proof = new_ix(
        program_id(),
        adspaces::instruction::SubmitProof {
            proof_hash: [7; 32],
            proof_uri: "https://adspaces.test/proof".to_string(),
        }
        .data(),
        adspaces::accounts::SubmitProof {
            seller: seller.pubkey(),
            rent_payer: env.rent_payer(),
            listing,
            spot: paid_spot,
            proof: paid_proof,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[proof], &seller, &[&env.mint_authority]).unwrap();
    let refund = new_ix(
        program_id(),
        adspaces::instruction::RefundSpot {}.data(),
        adspaces::accounts::RefundSpot {
            caller: seller.pubkey(),
            listing,
            rent_payer: env.rent_payer(),
            spot: refunded_spot,
            vault: refunded_vault,
            buyer_ata: buyers[1].1,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
            proof: None,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[refund], &seller, &[]).unwrap();
    assert_eq!(token_balance(&env.svm, &buyers[1].1), PRICE);

    // The undelivered spot's buyer can still dispute it.
    let opened_at = now + 8 * DAY;
    env.warp_to_timestamp(opened_at);
    let (dispute, _) = dispute_pda(&disputed_spot);
    let raise = new_ix(
        program_id(),
        adspaces::instruction::RaiseDispute {
            evidence_hash: [0; 32],
            evidence_uri: String::new(),
        }
        .data(),
        adspaces::accounts::RaiseDispute {
            buyer: buyers[2].0.pubkey(),
            rent_payer: env.mint_authority.pubkey(),
            config: env.config,
            listing,
            spot: disputed_spot,
            dispute,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[raise], &buyers[2].0, &[&env.mint_authority]).unwrap();

    // The delivered spot pays out once its review window ends.
    env.warp_to_timestamp(opened_at + adspaces::constants::DEFAULT_REVIEW_WINDOW_SECONDS);
    let seller_ata = empty_token_account(&mut env, &seller, &seller.pubkey());
    let treasury = env.treasury;
    let treasury_ata = empty_token_account(&mut env, &seller, &treasury);
    let withdraw = new_ix(
        program_id(),
        adspaces::instruction::WithdrawSeller {}.data(),
        adspaces::accounts::WithdrawSeller {
            caller: seller.pubkey(),
            listing,
            seller: seller.pubkey(),
            rent_payer: env.rent_payer(),
            spot: paid_spot,
            vault: paid_vault,
            seller_ata,
            treasury_ata,
            proof: paid_proof,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[withdraw], &seller, &[]).unwrap();
    let fee = PRICE * 250 / 10_000;
    assert_eq!(token_balance(&env.svm, &seller_ata), PRICE - fee);
    assert_eq!(token_balance(&env.svm, &treasury_ata), fee);

    // The unresolved dispute still times out to a full refund.
    env.warp_to_timestamp(opened_at + adspaces::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS);
    let caller = new_seller(&mut env);
    let timeout = new_ix(
        program_id(),
        adspaces::instruction::TimeoutDispute {}.data(),
        adspaces::accounts::TimeoutDispute {
            caller: caller.pubkey(),
            config: env.config,
            listing,
            spot: disputed_spot,
            dispute,
            rent_payer: env.rent_payer(),
            proof: None,
            vault: disputed_vault,
            buyer_ata: buyers[2].1,
            usdc_mint: env.usdc,
            token_program: token_program_id(),
        }
        .to_account_metas(None),
    );
    send(&mut env.svm, &[timeout], &caller, &[]).unwrap();
    assert_eq!(token_balance(&env.svm, &buyers[2].1), PRICE);
    assert!(read_config(&env.svm, &env.config).paused);
}
