use anchor_lang::prelude::*;

use crate::{constants::*, error::AdspacesError, events::ListingClosed, state::Listing};

/// Closes a listing once every spot is closed. Only the platform account that paid the listing's
/// rent can do it, and the rent returns to it.
#[derive(Accounts)]
pub struct CloseListing<'info> {
    #[account(mut, address = listing.rent_payer @ AdspacesError::Unauthorized)]
    pub rent_payer: Signer<'info>,
    #[account(
        mut,
        close = rent_payer,
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
        constraint = listing.status == LISTING_OPEN @ AdspacesError::ListingNotOpen,
    )]
    pub listing: Account<'info, Listing>,
}

pub fn handle_close_listing(ctx: Context<CloseListing>) -> Result<()> {
    let listing = &mut ctx.accounts.listing;
    require!(
        listing.closed_spot_count == listing.spot_count,
        AdspacesError::ListingHasActiveSpots
    );
    listing.status = LISTING_CLOSED;
    emit!(ListingClosed {
        listing: listing.key(),
        seller: listing.seller,
    });
    Ok(())
}
