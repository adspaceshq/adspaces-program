use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::AdspacesError,
    events::ListingClosed,
    state::Listing,
};

#[derive(Accounts)]
pub struct CloseListing<'info> {
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
}

pub fn handle_close_listing(ctx: Context<CloseListing>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(now >= ctx.accounts.listing.deadline, AdspacesError::ListingStillActive);

    let listing_key = ctx.accounts.listing.key();
    ctx.accounts.listing.status = LISTING_CLOSED;

    emit!(ListingClosed {
        listing: listing_key,
        seller: ctx.accounts.seller.key(),
    });

    ctx.accounts.listing.close(ctx.accounts.seller.to_account_info())?;

    Ok(())
}
