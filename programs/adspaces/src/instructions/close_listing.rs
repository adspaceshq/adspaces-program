use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::AdspacesError,
    events::ListingClosed,
    state::Listing,
};

/// Remaining accounts: every spot PDA of the listing, in index order.
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
    let spot_count = ctx.accounts.listing.spot_count as usize;
    require!(
        ctx.remaining_accounts.len() == spot_count,
        AdspacesError::ListingHasActiveSpots
    );
    for (index, spot) in ctx.remaining_accounts.iter().enumerate() {
        let (expected, _) = Pubkey::find_program_address(
            &[SPOT_SEED, listing_key.as_ref(), &[index as u8]],
            ctx.program_id,
        );
        require_keys_eq!(spot.key(), expected, AdspacesError::ListingHasActiveSpots);
        // A closed spot has no data and belongs to the system program. Lamports don't count: anyone can
        // send 1 lamport to a closed spot address, which must not lock the listing rent forever.
        require!(
            spot.data_is_empty() && *spot.owner == anchor_lang::system_program::ID,
            AdspacesError::ListingHasActiveSpots
        );
    }

    ctx.accounts.listing.status = LISTING_CLOSED;

    emit!(ListingClosed {
        listing: listing_key,
        seller: ctx.accounts.seller.key(),
    });

    ctx.accounts.listing.close(ctx.accounts.seller.to_account_info())?;

    Ok(())
}
