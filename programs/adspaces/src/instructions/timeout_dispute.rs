use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::AdspacesError,
    events::DisputeTimedOut,
    state::{Config, DisputeRecord, Listing, ProofRecord, Spot},
};

#[derive(Accounts)]
pub struct TimeoutDispute<'info> {
    pub caller: Signer<'info>,
    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Box<Account<'info, Config>>,
    #[account(
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
    )]
    pub listing: Box<Account<'info, Listing>>,
    #[account(mut, address = listing.seller @ AdspacesError::Unauthorized)]
    pub seller: SystemAccount<'info>,
    #[account(
        mut,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[spot.index]],
        bump = spot.bump,
        constraint = spot.listing == listing.key() @ AdspacesError::Unauthorized,
        constraint = spot.status == SPOT_DISPUTED @ AdspacesError::SpotNotFunded,
    )]
    pub spot: Box<Account<'info, Spot>>,
    #[account(
        mut,
        seeds = [DISPUTE_SEED, spot.key().as_ref()],
        bump = dispute.bump,
        constraint = dispute.spot == spot.key() @ AdspacesError::Unauthorized,
        constraint = dispute.decided_at == 0 @ AdspacesError::Unauthorized,
    )]
    pub dispute: Box<Account<'info, DisputeRecord>>,
    /// Receives the dispute account's rent: the buyer paid it when raising the dispute.
    #[account(mut, address = dispute.buyer @ AdspacesError::Unauthorized)]
    pub dispute_buyer: SystemAccount<'info>,
    #[account(
        mut,
        seeds = [PROOF_SEED, spot.key().as_ref()],
        bump = proof.bump,
        constraint = proof.spot == spot.key() @ AdspacesError::Unauthorized,
    )]
    pub proof: Option<Box<Account<'info, ProofRecord>>>,
    #[account(
        mut,
        constraint = vault.key() == spot.vault @ AdspacesError::Unauthorized,
        constraint = vault.mint == listing.mint @ AdspacesError::InvalidMint,
        constraint = vault.owner == spot.key() @ AdspacesError::Unauthorized,
    )]
    pub vault: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = buyer_ata.owner == dispute.buyer @ AdspacesError::NotWinningBuyer,
        constraint = buyer_ata.mint == listing.mint @ AdspacesError::InvalidMint,
    )]
    pub buyer_ata: Box<Account<'info, TokenAccount>>,
    #[account(constraint = usdc_mint.key() == listing.mint @ AdspacesError::InvalidMint)]
    pub usdc_mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_timeout_dispute(ctx: Context<TimeoutDispute>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let timeout_at = ctx
        .accounts
        .dispute
        .opened_at
        .checked_add(ctx.accounts.dispute.timeout_seconds)
        .ok_or(AdspacesError::MathOverflow)?;
    require!(now >= timeout_at, AdspacesError::DisputeNotTimedOut);

    let amount = ctx.accounts.vault.amount;
    require!(amount > 0, AdspacesError::SpotNotFunded);

    let listing_key = ctx.accounts.listing.key();
    let index = ctx.accounts.spot.index;
    let bump = ctx.accounts.spot.bump;
    let signer_seeds: &[&[&[u8]]] = &[&[SPOT_SEED, listing_key.as_ref(), &[index], &[bump]]];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            token::Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.buyer_ata.to_account_info(),
                authority: ctx.accounts.spot.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )?;

    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        token::CloseAccount {
            account: ctx.accounts.vault.to_account_info(),
            destination: ctx.accounts.seller.to_account_info(),
            authority: ctx.accounts.spot.to_account_info(),
        },
        signer_seeds,
    ))?;

    let dispute_key = ctx.accounts.dispute.key();
    ctx.accounts.spot.status = SPOT_CANCELLED;

    emit!(DisputeTimedOut {
        listing: listing_key,
        index,
        dispute: dispute_key,
        buyer: ctx.accounts.dispute.buyer,
        amount,
    });

    if let Some(ref proof) = ctx.accounts.proof {
        proof.close(ctx.accounts.seller.to_account_info())?;
    }
    ctx.accounts.dispute.close(ctx.accounts.dispute_buyer.to_account_info())?;
    ctx.accounts.spot.close(ctx.accounts.seller.to_account_info())?;

    Ok(())
}
