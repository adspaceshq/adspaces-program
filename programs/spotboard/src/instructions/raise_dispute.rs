use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::SpotboardError,
    events::DisputeRaised,
    state::{Listing, Spot},
};

#[derive(Accounts)]
pub struct RaiseDispute<'info> {
    pub buyer: Signer<'info>,
    #[account(
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
    )]
    pub listing: Account<'info, Listing>,
    #[account(
        mut,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[spot.index]],
        bump = spot.bump,
        constraint = spot.listing == listing.key() @ SpotboardError::Unauthorized,
        constraint = spot.status == SPOT_FUNDED @ SpotboardError::SpotNotFunded,
    )]
    pub spot: Account<'info, Spot>,
}

pub fn handle_raise_dispute(ctx: Context<RaiseDispute>) -> Result<()> {
    let spot = &mut ctx.accounts.spot;
    let winner = if ctx.accounts.listing.mode == MODE_FIXED {
        spot.buyer
    } else {
        spot.current_bidder
    };
    require!(
        winner == ctx.accounts.buyer.key(),
        SpotboardError::NotWinningBuyer
    );
    let now = Clock::get()?.unix_timestamp;
    require!(
        now >= ctx.accounts.listing.delivery_deadline,
        SpotboardError::DeliveryDeadlinePending
    );
    if spot.proof_submitted_at > 0 {
        let review_end = ctx
            .accounts
            .listing
            .delivery_deadline
            .checked_add(REVIEW_WINDOW_SECONDS)
            .ok_or(SpotboardError::MathOverflow)?;
        require!(now < review_end, SpotboardError::ReviewWindowClosed);
    }
    let proof_submitted = spot.proof_submitted_at > 0;
    spot.status = SPOT_DISPUTED;
    emit!(DisputeRaised {
        listing: ctx.accounts.listing.key(),
        index: spot.index,
        buyer: ctx.accounts.buyer.key(),
        proof_submitted,
        raised_at: now,
    });
    Ok(())
}
