use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::AdspacesError,
    events::SellerWithdrawn,
    fees::calculate_fee,
    rent::record_spot_closed,
    state::{Listing, ProofRecord, Spot},
};

#[derive(Accounts)]
pub struct WithdrawSeller<'info> {
    pub caller: Signer<'info>,
    #[account(
        mut,
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
    )]
    pub listing: Box<Account<'info, Listing>>,
    /// CHECK: only compared by key and used as the seller_ata authority; any owner is accepted so a
    /// program-owned seller key can still be paid.
    #[account(address = listing.seller @ AdspacesError::Unauthorized)]
    pub seller: UncheckedAccount<'info>,
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
        token::mint = usdc_mint,
        token::authority = seller,
    )]
    pub seller_ata: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = treasury_ata.owner == listing.treasury @ AdspacesError::Unauthorized,
        constraint = treasury_ata.mint == listing.mint @ AdspacesError::InvalidMint,
    )]
    pub treasury_ata: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [PROOF_SEED, spot.key().as_ref()],
        bump = proof.bump,
        constraint = proof.spot == spot.key() @ AdspacesError::Unauthorized,
    )]
    pub proof: Box<Account<'info, ProofRecord>>,
    #[account(
        constraint = usdc_mint.key() == listing.mint @ AdspacesError::InvalidMint
    )]
    pub usdc_mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_withdraw_seller(ctx: Context<WithdrawSeller>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        now >= ctx.accounts.listing.deadline,
        AdspacesError::WithdrawTooEarly
    );
    require!(
        ctx.accounts.spot.proof_submitted_at > 0,
        AdspacesError::ProofRequired
    );
    let review_end = ctx
        .accounts
        .listing
        .delivery_deadline
        .checked_add(ctx.accounts.listing.review_window_seconds)
        .ok_or(AdspacesError::MathOverflow)?;
    require!(now >= review_end, AdspacesError::ReviewWindowOpen);

    let balance = ctx.accounts.vault.amount;
    require!(balance > 0, AdspacesError::SpotNotFunded);

    let fee = calculate_fee(&ctx.accounts.listing, &ctx.accounts.spot, balance)?;
    let seller_amount = balance
        .checked_sub(fee)
        .ok_or(AdspacesError::MathOverflow)?;

    let listing_key = ctx.accounts.listing.key();
    let spot_index = ctx.accounts.spot.index;
    let spot_bump = ctx.accounts.spot.bump;
    let signer_seeds: &[&[&[u8]]] =
        &[&[SPOT_SEED, listing_key.as_ref(), &[spot_index], &[spot_bump]]];

    if fee > 0 {
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                token::Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.treasury_ata.to_account_info(),
                    authority: ctx.accounts.spot.to_account_info(),
                },
                signer_seeds,
            ),
            fee,
        )?;
    }

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            token::Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.seller_ata.to_account_info(),
                authority: ctx.accounts.spot.to_account_info(),
            },
            signer_seeds,
        ),
        seller_amount,
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

    ctx.accounts.spot.status = SPOT_FINALIZED;

    emit!(SellerWithdrawn {
        listing: ctx.accounts.listing.key(),
        index: spot_index,
        seller: ctx.accounts.seller.key(),
        amount: seller_amount,
        fee,
        treasury_ata: ctx.accounts.treasury_ata.key(),
    });

    ctx.accounts
        .proof
        .close(ctx.accounts.rent_payer.to_account_info())?;
    ctx.accounts
        .spot
        .close(ctx.accounts.rent_payer.to_account_info())?;
    record_spot_closed(&mut ctx.accounts.listing)?;

    Ok(())
}
