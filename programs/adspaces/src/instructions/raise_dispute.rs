use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::AdspacesError,
    events::DisputeRaised,
    state::{DisputeRecord, Listing, Spot},
};

#[derive(Accounts)]
pub struct RaiseDispute<'info> {
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
        constraint = spot.status == SPOT_FUNDED @ AdspacesError::SpotNotFunded,
    )]
    pub spot: Account<'info, Spot>,
    #[account(
        init,
        payer = buyer,
        space = 8 + DisputeRecord::INIT_SPACE,
        seeds = [DISPUTE_SEED, spot.key().as_ref()],
        bump,
    )]
    pub dispute: Account<'info, DisputeRecord>,
    pub system_program: Program<'info, System>,
}

pub fn handle_raise_dispute(
    ctx: Context<RaiseDispute>,
    evidence_hash: [u8; 32],
    evidence_uri: String,
) -> Result<()> {
    let spot = &mut ctx.accounts.spot;
    let winner = if ctx.accounts.listing.mode == MODE_FIXED {
        spot.buyer
    } else {
        spot.current_bidder
    };
    require!(
        winner == ctx.accounts.buyer.key(),
        AdspacesError::NotWinningBuyer
    );
    let now = Clock::get()?.unix_timestamp;
    require!(
        now >= ctx.accounts.listing.delivery_deadline,
        AdspacesError::DeliveryDeadlinePending
    );
    if spot.proof_submitted_at > 0 {
        let review_end = ctx
            .accounts
            .listing
            .delivery_deadline
            .checked_add(REVIEW_WINDOW_SECONDS)
            .ok_or(AdspacesError::MathOverflow)?;
        require!(now < review_end, AdspacesError::ReviewWindowClosed);
    }
    let proof_submitted = spot.proof_submitted_at > 0;
    let has_evidence = evidence_hash != [0; 32];
    require!(
        has_evidence != evidence_uri.is_empty()
            && evidence_uri.len() <= DISPUTE_URI_MAX_LEN
            && (!proof_submitted || has_evidence),
        AdspacesError::InvalidDisputeEvidence
    );
    let dispute = &mut ctx.accounts.dispute;
    dispute.spot = spot.key();
    dispute.buyer = ctx.accounts.buyer.key();
    dispute.proof_submitted = proof_submitted;
    dispute.evidence_hash = evidence_hash;
    dispute.evidence_uri = evidence_uri;
    dispute.opened_at = now;
    dispute.decision_hash = [0; 32];
    dispute.decision_uri = String::new();
    dispute.decided_at = 0;
    dispute.seller_share_bps = 0;
    dispute.buyer_refund = 0;
    dispute.seller_amount = 0;
    dispute.fee = 0;
    dispute.bump = ctx.bumps.dispute;
    spot.status = SPOT_DISPUTED;
    emit!(DisputeRaised {
        listing: ctx.accounts.listing.key(),
        index: spot.index,
        dispute: dispute.key(),
        buyer: ctx.accounts.buyer.key(),
        proof_submitted,
        evidence_hash,
        raised_at: now,
    });
    Ok(())
}
