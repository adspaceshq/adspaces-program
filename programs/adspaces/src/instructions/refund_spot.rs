use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::AdspacesError,
    events::SpotRefunded,
    rent::{record_spot_closed, require_proof_account},
    state::{Listing, ProofRecord, Spot},
};

#[derive(Accounts)]
pub struct RefundSpot<'info> {
    pub caller: Signer<'info>,
    #[account(
        mut,
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
    )]
    pub listing: Box<Account<'info, Listing>>,
    #[account(mut, address = listing.rent_payer @ AdspacesError::Unauthorized)]
    pub rent_payer: SystemAccount<'info>,
    #[account(
        mut,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[spot.index]],
        bump = spot.bump,
        constraint = spot.listing == listing.key() @ AdspacesError::Unauthorized,
        constraint = spot.status == SPOT_FUNDED @ AdspacesError::SpotNotFunded,
    )]
    pub spot: Box<Account<'info, Spot>>,
    #[account(
        mut,
        constraint = vault.key() == spot.vault @ AdspacesError::Unauthorized,
        constraint = vault.mint == listing.mint @ AdspacesError::InvalidMint,
        constraint = vault.owner == spot.key() @ AdspacesError::Unauthorized,
    )]
    pub vault: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = buyer_ata.owner == if listing.mode == MODE_FIXED { spot.buyer } else { spot.current_bidder } @ AdspacesError::NotWinningBuyer,
        constraint = buyer_ata.mint == listing.mint @ AdspacesError::InvalidMint,
    )]
    pub buyer_ata: Box<Account<'info, TokenAccount>>,
    #[account(constraint = usdc_mint.key() == listing.mint @ AdspacesError::InvalidMint)]
    pub usdc_mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
    /// Proof submitted before a seller-initiated refund; closed so its rent returns to the rent payer.
    #[account(
        mut,
        seeds = [PROOF_SEED, spot.key().as_ref()],
        bump = proof.bump,
        constraint = proof.spot == spot.key() @ AdspacesError::Unauthorized,
    )]
    pub proof: Option<Box<Account<'info, ProofRecord>>>,
}

pub fn handle_refund_spot(ctx: Context<RefundSpot>) -> Result<()> {
    require_proof_account(&ctx.accounts.spot, ctx.accounts.proof.is_some())?;
    let seller_initiated = ctx.accounts.caller.key() == ctx.accounts.listing.seller;
    if !seller_initiated {
        require!(
            Clock::get()?.unix_timestamp >= ctx.accounts.listing.delivery_deadline,
            AdspacesError::DeliveryDeadlinePending
        );
        require!(
            ctx.accounts.spot.proof_submitted_at == 0,
            AdspacesError::ProofExists
        );
    }
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
            destination: ctx.accounts.rent_payer.to_account_info(),
            authority: ctx.accounts.spot.to_account_info(),
        },
        signer_seeds,
    ))?;
    ctx.accounts.spot.status = SPOT_CANCELLED;

    emit!(SpotRefunded {
        listing: listing_key,
        index,
        buyer: ctx.accounts.buyer_ata.owner,
        amount,
        seller_initiated,
    });

    if let Some(ref proof) = ctx.accounts.proof {
        proof.close(ctx.accounts.rent_payer.to_account_info())?;
    }
    ctx.accounts
        .spot
        .close(ctx.accounts.rent_payer.to_account_info())?;
    record_spot_closed(&mut ctx.accounts.listing)?;

    Ok(())
}
