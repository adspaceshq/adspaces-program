use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::SpotboardError,
    events::SpotAdded,
    state::{Listing, Spot},
};

#[derive(Accounts)]
pub struct AddSpot<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,
    #[account(
        mut,
        seeds = [LISTING_SEED, seller.key().as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
        has_one = seller @ SpotboardError::Unauthorized,
        constraint = listing.status == LISTING_OPEN @ SpotboardError::ListingNotOpen,
    )]
    pub listing: Account<'info, Listing>,
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
        constraint = usdc_mint.key() == listing.mint @ SpotboardError::InvalidMint
    )]
    pub usdc_mint: Account<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handle_add_spot(ctx: Context<AddSpot>, price: u64) -> Result<()> {
    let listing = &mut ctx.accounts.listing;
    require!(
        listing.spot_count < MAX_SPOTS_PER_LISTING,
        SpotboardError::TooManySpots
    );
    require!(price > 0, SpotboardError::ZeroPrice);

    let index = listing.spot_count;
    let spot = &mut ctx.accounts.spot;
    spot.listing = listing.key();
    spot.index = index;
    spot.price = price;
    spot.status = SPOT_OPEN;
    spot.current_bid = 0;
    spot.current_bidder = Pubkey::default();
    spot.vault = ctx.accounts.vault.key();
    spot.bump = ctx.bumps.spot;
    spot.vault_bump = ctx.bumps.vault;

    listing.spot_count = index.checked_add(1).ok_or(SpotboardError::MathOverflow)?;

    emit!(SpotAdded {
        listing: listing.key(),
        index,
        price,
    });

    Ok(())
}
