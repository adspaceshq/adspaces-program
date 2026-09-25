use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::AdspacesError,
    events::SellerWithdrawn,
    fees::calculate_fee,
    state::{Listing, Spot},
};

#[derive(Accounts)]
pub struct WithdrawSeller<'info> {
    pub caller: Signer<'info>,
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
        .checked_add(REVIEW_WINDOW_SECONDS)
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
            destination: ctx.accounts.seller.to_account_info(),
            authority: ctx.accounts.spot.to_account_info(),
        },
        signer_seeds,
    ))?;

    let spot = &mut ctx.accounts.spot;
    spot.status = SPOT_FINALIZED;

    emit!(SellerWithdrawn {
        listing: ctx.accounts.listing.key(),
        index: spot_index,
        seller: ctx.accounts.seller.key(),
        amount: seller_amount,
        fee,
    });

    Ok(())
}
