use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::SpotboardError,
    events::SellerWithdrawn,
    state::{Listing, Spot},
};

#[derive(Accounts)]
pub struct WithdrawSeller<'info> {
    pub caller: Signer<'info>,
    #[account(
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
    )]
    pub listing: Account<'info, Listing>,
    #[account(mut, address = listing.seller @ SpotboardError::Unauthorized)]
    pub seller: SystemAccount<'info>,
    #[account(
        mut,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[spot.index]],
        bump = spot.bump,
        constraint = spot.listing == listing.key() @ SpotboardError::Unauthorized,
        constraint = spot.status == SPOT_FUNDED @ SpotboardError::SpotNotFunded,
    )]
    pub spot: Account<'info, Spot>,
    #[account(
        mut,
        constraint = vault.key() == spot.vault @ SpotboardError::Unauthorized,
        constraint = vault.mint == listing.mint @ SpotboardError::InvalidMint,
        constraint = vault.owner == spot.key() @ SpotboardError::Unauthorized,
    )]
    pub vault: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = usdc_mint,
        token::authority = seller,
    )]
    pub seller_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = treasury_ata.owner == listing.treasury @ SpotboardError::Unauthorized,
        constraint = treasury_ata.mint == listing.mint @ SpotboardError::InvalidMint,
    )]
    pub treasury_ata: Account<'info, TokenAccount>,
    #[account(
        constraint = usdc_mint.key() == listing.mint @ SpotboardError::InvalidMint
    )]
    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_withdraw_seller(ctx: Context<WithdrawSeller>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        now >= ctx.accounts.listing.deadline,
        SpotboardError::WithdrawTooEarly
    );

    let balance = ctx.accounts.vault.amount;
    require!(balance > 0, SpotboardError::SpotNotFunded);

    let fee = (balance as u128)
        .checked_mul(ctx.accounts.listing.fee_bps as u128)
        .and_then(|v| v.checked_div(10_000))
        .and_then(|v| u64::try_from(v).ok())
        .ok_or(SpotboardError::MathOverflow)?;
    let seller_amount = balance
        .checked_sub(fee)
        .ok_or(SpotboardError::MathOverflow)?;

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
