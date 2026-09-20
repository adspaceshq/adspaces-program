use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

use crate::{constants::*, error::SpotboardError, events::ListingCreated, state::Listing};

#[derive(Accounts)]
#[instruction(slug_hash: [u8; 16])]
pub struct CreateListing<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,
    #[account(
        init,
        payer = seller,
        space = 8 + Listing::INIT_SPACE,
        seeds = [LISTING_SEED, seller.key().as_ref(), slug_hash.as_ref()],
        bump
    )]
    pub listing: Account<'info, Listing>,
    /// USDC mint. Must be devnet test USDC or mainnet USDC (checked in handler).
    pub usdc_mint: Account<'info, Mint>,
    /// CHECK: only the pubkey is stored; it must not be the default pubkey.
    pub treasury: UncheckedAccount<'info>,
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
    min_increment: u64,
    double_up: bool,
) -> Result<()> {
    require!(
        mode == MODE_FIXED || mode == MODE_AUCTION,
        SpotboardError::InvalidMode
    );
    require!(
        metadata_uri.len() <= METADATA_URI_MAX_LEN,
        SpotboardError::MetadataUriTooLong
    );

    let now = Clock::get()?.unix_timestamp;
    require!(deadline > now, SpotboardError::InvalidDeadline);
    require!(
        artwork_close >= deadline,
        SpotboardError::InvalidArtworkClose
    );

    if mode == MODE_AUCTION && !double_up {
        require!(min_increment > 0, SpotboardError::InvalidMinIncrement);
    }

    let mint_key = ctx.accounts.usdc_mint.key();
    let devnet: Pubkey = USDC_MINT_DEVNET
        .parse()
        .map_err(|_| SpotboardError::InvalidMint)?;
    let mainnet: Pubkey = USDC_MINT_MAINNET
        .parse()
        .map_err(|_| SpotboardError::InvalidMint)?;
    require!(
        mint_key == devnet || mint_key == mainnet,
        SpotboardError::InvalidMint
    );

    require!(
        ctx.accounts.treasury.key() != Pubkey::default(),
        SpotboardError::Unauthorized
    );

    let listing = &mut ctx.accounts.listing;
    listing.seller = ctx.accounts.seller.key();
    listing.slug_hash = slug_hash;
    listing.metadata_uri = metadata_uri;
    listing.mint = mint_key;
    listing.mode = mode;
    listing.deadline = deadline;
    listing.artwork_close = artwork_close;
    listing.min_increment = min_increment;
    listing.double_up = double_up;
    listing.spot_count = 0;
    listing.status = LISTING_OPEN;
    listing.treasury = ctx.accounts.treasury.key();
    listing.fee_bps = PLATFORM_FEE_BPS;
    listing.bump = ctx.bumps.listing;

    emit!(ListingCreated {
        listing: listing.key(),
        seller: listing.seller,
        slug_hash,
        mode,
    });

    Ok(())
}
