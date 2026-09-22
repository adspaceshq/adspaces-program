use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::SpotboardError,
    events::ProofSubmitted,
    state::{Listing, ProofRecord, Spot},
};

#[derive(Accounts)]
pub struct SubmitProof<'info> {
    #[account(mut)]
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
    #[account(
        init,
        payer = seller,
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
        SpotboardError::WithdrawTooEarly
    );
    require!(
        now < ctx.accounts.listing.delivery_deadline,
        SpotboardError::ProofDeadlinePassed
    );
    require!(proof_hash != [0; 32], SpotboardError::InvalidProof);
    require!(
        !proof_uri.is_empty() && proof_uri.len() <= PROOF_URI_MAX_LEN,
        SpotboardError::InvalidProofUri
    );
    let spot = &mut ctx.accounts.spot;
    require!(
        spot.proof_submitted_at == 0,
        SpotboardError::ProofAlreadySubmitted
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
