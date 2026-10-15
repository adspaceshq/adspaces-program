use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::AdspacesError,
    events::DisputeWithdrawn,
    state::{DisputeRecord, Listing, Spot},
};

#[derive(Accounts)]
pub struct WithdrawDispute<'info> {
    #[account(mut)]
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
        constraint = spot.listing == listing.key() @ AdspacesError::Unauthorized,
        constraint = spot.status == SPOT_DISPUTED @ AdspacesError::SpotNotFunded,
    )]
    pub spot: Account<'info, Spot>,
    #[account(
        mut,
        close = rent_payer,
        seeds = [DISPUTE_SEED, spot.key().as_ref()],
        bump = dispute.bump,
        constraint = dispute.spot == spot.key() @ AdspacesError::Unauthorized,
        constraint = dispute.buyer == buyer.key() @ AdspacesError::NotWinningBuyer,
        constraint = dispute.decided_at == 0 @ AdspacesError::Unauthorized,
    )]
    pub dispute: Account<'info, DisputeRecord>,
    /// Gets the dispute's rent back: the platform account that paid it.
    #[account(
        mut,
        address = dispute.rent_payer @ AdspacesError::Unauthorized,
        constraint = rent_payer.key() == listing.rent_payer @ AdspacesError::Unauthorized,
    )]
    pub rent_payer: SystemAccount<'info>,
}

pub fn handle_withdraw_dispute(ctx: Context<WithdrawDispute>) -> Result<()> {
    let winner = if ctx.accounts.listing.mode == MODE_FIXED {
        ctx.accounts.spot.buyer
    } else {
        ctx.accounts.spot.current_bidder
    };
    require!(
        winner == ctx.accounts.buyer.key(),
        AdspacesError::NotWinningBuyer
    );
    ctx.accounts.spot.status = SPOT_FUNDED;
    emit!(DisputeWithdrawn {
        listing: ctx.accounts.listing.key(),
        index: ctx.accounts.spot.index,
        dispute: ctx.accounts.dispute.key(),
        buyer: ctx.accounts.buyer.key(),
    });
    Ok(())
}
