use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::AdspacesError,
    events::SpotAdded,
    state::{Config, Listing, ReviewAuthority, Spot},
};

#[derive(Accounts)]
pub struct AddSpot<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,
    #[account(
        mut,
        seeds = [LISTING_SEED, seller.key().as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
        has_one = seller @ AdspacesError::Unauthorized,
        constraint = listing.status == LISTING_OPEN @ AdspacesError::ListingNotOpen,
    )]
    pub listing: Account<'info, Listing>,
    /// Spots added after listing approval need the review authority's co-signature, like the listing itself.
    pub reviewer: Signer<'info>,
    #[account(
        seeds = [REVIEW_AUTHORITY_SEED, config.key().as_ref()],
        bump = review_authority.bump,
        constraint = review_authority.config == config.key() @ AdspacesError::Unauthorized,
        constraint = review_authority.authority == reviewer.key() @ AdspacesError::InvalidReviewAuthority,
    )]
    pub review_authority: Account<'info, ReviewAuthority>,
    #[account(
        init,
        payer = seller,
        space = 8 + Spot::INIT_SPACE,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[listing.spot_count]],
        bump
    )]
    pub spot: Account<'info, Spot>,
    #[account(
        init,
        payer = seller,
        seeds = [VAULT_SEED, spot.key().as_ref()],
        bump,
        token::mint = usdc_mint,
        token::authority = spot,
    )]
    pub vault: Account<'info, TokenAccount>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = !config.paused @ AdspacesError::ProtocolPaused,
    )]
    pub config: Account<'info, Config>,
    #[account(
        constraint = usdc_mint.key() == listing.mint @ AdspacesError::InvalidMint
    )]
    pub usdc_mint: Account<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handle_add_spot(ctx: Context<AddSpot>, price: u64) -> Result<()> {
    require!(
        Clock::get()?.unix_timestamp < ctx.accounts.listing.deadline,
        AdspacesError::ListingClosed
    );

    let listing = &mut ctx.accounts.listing;
    require!(
        listing.spot_count < MAX_SPOTS_PER_LISTING,
        AdspacesError::TooManySpots
    );
    require!(price > 0, AdspacesError::ZeroPrice);

    let index = listing.spot_count;
    let spot = &mut ctx.accounts.spot;
    spot.listing = listing.key();
    spot.index = index;
    spot.price = price;
    spot.status = SPOT_OPEN;
    spot.current_bid = 0;
    spot.current_bidder = Pubkey::default();
    spot.buyer = Pubkey::default();
    spot.vault = ctx.accounts.vault.key();
    spot.bump = ctx.bumps.spot;
    spot.vault_bump = ctx.bumps.vault;
    spot.proof_hash = [0; 32];
    spot.proof_submitted_at = 0;

    listing.spot_count = index.checked_add(1).ok_or(AdspacesError::MathOverflow)?;

    emit!(SpotAdded {
        listing: listing.key(),
        index,
        price,
    });

    Ok(())
}
