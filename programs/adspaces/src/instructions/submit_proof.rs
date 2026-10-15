use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::AdspacesError,
    events::ProofSubmitted,
    state::{Listing, ProofRecord, Spot},
};

#[derive(Accounts)]
pub struct SubmitProof<'info> {
    pub seller: Signer<'info>,
    #[account(mut, address = listing.rent_payer @ AdspacesError::Unauthorized)]
    pub rent_payer: Signer<'info>,
    #[account(
        seeds = [LISTING_SEED, seller.key().as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
        has_one = seller @ AdspacesError::Unauthorized,
    )]
    pub listing: Account<'info, Listing>,
    #[account(
        mut,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[spot.index]],
        bump = spot.bump,
        constraint = spot.listing == listing.key() @ AdspacesError::Unauthorized,
        constraint = spot.status == SPOT_FUNDED @ AdspacesError::SpotNotFunded,
    )]
    pub spot: Account<'info, Spot>,
    #[account(
        init,
        payer = rent_payer,
        space = 8 + ProofRecord::INIT_SPACE,
        seeds = [PROOF_SEED, spot.key().as_ref()],
        bump,
    )]
    pub proof: Account<'info, ProofRecord>,
    pub system_program: Program<'info, System>,
}

pub fn handle_submit_proof(
    ctx: Context<SubmitProof>,
    proof_hash: [u8; 32],
    proof_uri: String,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        now >= ctx.accounts.listing.deadline,
        AdspacesError::WithdrawTooEarly
    );
    require!(
        now < ctx.accounts.listing.delivery_deadline,
        AdspacesError::ProofDeadlinePassed
    );
    require!(proof_hash != [0; 32], AdspacesError::InvalidProof);
    require!(
        !proof_uri.is_empty() && proof_uri.len() <= PROOF_URI_MAX_LEN,
        AdspacesError::InvalidProofUri
    );
    let spot = &mut ctx.accounts.spot;
    require!(
        spot.proof_submitted_at == 0,
        AdspacesError::ProofAlreadySubmitted
    );
    spot.proof_hash = proof_hash;
    spot.proof_submitted_at = now;
    let proof = &mut ctx.accounts.proof;
    proof.spot = spot.key();
    proof.proof_hash = proof_hash;
    proof.proof_uri = proof_uri;
    proof.submitted_at = now;
    proof.bump = ctx.bumps.proof;
    emit!(ProofSubmitted {
        listing: ctx.accounts.listing.key(),
        index: spot.index,
        proof: proof.key(),
        seller: ctx.accounts.seller.key(),
        proof_hash,
        submitted_at: now,
    });
    Ok(())
}
