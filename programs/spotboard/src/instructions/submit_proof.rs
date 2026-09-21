use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::SpotboardError,
    state::{Listing, Spot},
};

#[derive(Accounts)]
pub struct SubmitProof<'info> {
    pub seller: Signer<'info>,
    #[account(
        seeds = [LISTING_SEED, seller.key().as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
        has_one = seller @ SpotboardError::Unauthorized,
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

pub fn handle_submit_proof(ctx: Context<SubmitProof>, proof_hash: [u8; 32]) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        now >= ctx.accounts.listing.deadline,
        SpotboardError::WithdrawTooEarly
    );
    require!(
        now < ctx.accounts.listing.delivery_deadline,
        SpotboardError::ProofDeadlinePassed
    );
    require!(proof_hash != [0; 32], SpotboardError::InvalidProof);
    let spot = &mut ctx.accounts.spot;
    require!(
        spot.proof_submitted_at == 0,
        SpotboardError::ProofAlreadySubmitted
    );
    spot.proof_hash = proof_hash;
    spot.proof_submitted_at = now;
    Ok(())
}
