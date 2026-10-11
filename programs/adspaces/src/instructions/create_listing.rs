use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

use crate::{
    constants::*,
    error::AdspacesError,
    events::ListingCreated,
    state::{Config, Listing, ReviewAuthority},
};

#[derive(Accounts)]
#[instruction(slug_hash: [u8; 16])]
pub struct CreateListing<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,
    pub reviewer: Signer<'info>,
    #[account(
        init,
        payer = seller,
        space = 8 + Listing::INIT_SPACE,
        seeds = [LISTING_SEED, seller.key().as_ref(), slug_hash.as_ref()],
        bump
    )]
    pub listing: Account<'info, Listing>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = !config.paused @ AdspacesError::ProtocolPaused,
    )]
    pub config: Account<'info, Config>,
    #[account(
        seeds = [REVIEW_AUTHORITY_SEED, config.key().as_ref()],
        bump = review_authority.bump,
        constraint = review_authority.config == config.key() @ AdspacesError::Unauthorized,
        constraint = review_authority.authority == reviewer.key() @ AdspacesError::InvalidReviewAuthority,
    )]
    pub review_authority: Account<'info, ReviewAuthority>,
    #[account(
        constraint = usdc_mint.key() == config.payment_mint @ AdspacesError::InvalidMint
    )]
    pub usdc_mint: Account<'info, Mint>,
    pub system_program: Program<'info, System>,
}

#[allow(clippy::too_many_arguments)]
pub fn handle_create_listing(
    ctx: Context<CreateListing>,
    slug_hash: [u8; 16],
    metadata_uri: String,
    mode: u8,
    deadline: i64,
    artwork_close: i64,
    delivery_deadline: i64,
    review_window_seconds: i64,
    min_increment: u64,
    double_up: bool,
    fee_bps: u16,
    uplift_fee_bps: u16,
    max_fee_bps: u16,
) -> Result<()> {
    require!(
        mode == MODE_FIXED || mode == MODE_AUCTION,
        AdspacesError::InvalidMode
    );
    require!(
        metadata_uri.len() <= METADATA_URI_MAX_LEN,
        AdspacesError::MetadataUriTooLong
    );

    let now = Clock::get()?.unix_timestamp;
    require!(deadline > now, AdspacesError::InvalidDeadline);
    require!(
        artwork_close >= deadline,
        AdspacesError::InvalidArtworkClose
    );
    require!(
        delivery_deadline > artwork_close,
        AdspacesError::InvalidDeliveryDeadline
    );
    require!(
        (MIN_REVIEW_WINDOW_SECONDS..=MAX_REVIEW_WINDOW_SECONDS).contains(&review_window_seconds),
        AdspacesError::InvalidReviewWindow
    );

    if mode == MODE_AUCTION && !double_up {
        // Floor of 0.01 USDC: a 1-unit step lets two accounts keep an auction alive with dust bids.
        require!(min_increment >= MIN_BID_INCREMENT, AdspacesError::InvalidMinIncrement);
    }
    validate_fee_policy(
        mode,
        ctx.accounts.config.fee_bps,
        fee_bps,
        uplift_fee_bps,
        max_fee_bps,
    )?;

    let listing = &mut ctx.accounts.listing;
    listing.seller = ctx.accounts.seller.key();
    listing.slug_hash = slug_hash;
    listing.metadata_uri = metadata_uri;
    listing.mint = ctx.accounts.usdc_mint.key();
    listing.mode = mode;
    listing.deadline = deadline;
    listing.artwork_close = artwork_close;
    listing.delivery_deadline = delivery_deadline;
    listing.review_window_seconds = review_window_seconds;
    listing.min_increment = min_increment;
    listing.double_up = double_up;
    listing.spot_count = 0;
    listing.status = LISTING_OPEN;
    listing.treasury = ctx.accounts.config.treasury;
    listing.fee_bps = fee_bps;
    listing.uplift_fee_bps = uplift_fee_bps;
    listing.max_fee_bps = max_fee_bps;
    listing.bump = ctx.bumps.listing;

    emit!(ListingCreated {
        listing: listing.key(),
        seller: listing.seller,
        slug_hash,
        mode,
        review_window_seconds,
        fee_bps,
        uplift_fee_bps,
        max_fee_bps,
    });

    Ok(())
}

fn validate_fee_policy(
    mode: u8,
    minimum_fee_bps: u16,
    fee_bps: u16,
    uplift_fee_bps: u16,
    max_fee_bps: u16,
) -> Result<()> {
    require!(
        fee_bps >= minimum_fee_bps && fee_bps <= 10_000,
        AdspacesError::InvalidFeePolicy
    );
    if mode == MODE_FIXED {
        require!(
            uplift_fee_bps == 0 && max_fee_bps == fee_bps,
            AdspacesError::InvalidFeePolicy
        );
    } else {
        require!(
            fee_bps <= max_fee_bps && max_fee_bps <= uplift_fee_bps && uplift_fee_bps <= 10_000,
            AdspacesError::InvalidFeePolicy
        );
    }
    Ok(())
}
