use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::AdspacesError,
    events::SpotClosed,
    state::{Listing, Spot},
};

#[derive(Accounts)]
pub struct CloseSpot<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,
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
        constraint = spot.status == SPOT_OPEN @ AdspacesError::SpotAlreadyFunded,
    )]
    pub spot: Account<'info, Spot>,
    #[account(
        mut,
        constraint = vault.key() == spot.vault @ AdspacesError::Unauthorized,
        constraint = vault.mint == listing.mint @ AdspacesError::InvalidMint,
        constraint = vault.owner == spot.key() @ AdspacesError::Unauthorized,
    )]
    pub vault: Account<'info, TokenAccount>,
    /// Receives anything sent to the open spot's vault (donations), so nobody can block closing it.
    #[account(
        mut,
        constraint = seller_ata.mint == listing.mint @ AdspacesError::InvalidMint,
        constraint = seller_ata.owner == seller.key() @ AdspacesError::Unauthorized,
    )]
    pub seller_ata: Account<'info, TokenAccount>,
    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_close_spot(ctx: Context<CloseSpot>) -> Result<()> {
    let listing_key = ctx.accounts.listing.key();
    let spot_index = ctx.accounts.spot.index;
    let spot_bump = ctx.accounts.spot.bump;
    let signer_seeds: &[&[&[u8]]] =
        &[&[SPOT_SEED, listing_key.as_ref(), &[spot_index], &[spot_bump]]];

    let swept = ctx.accounts.vault.amount;
    if swept > 0 {
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
            swept,
        )?;
    }

    token::close_account(CpiContext::new_with_signer(
        ctx.accounts.token_program.key(),
        token::CloseAccount {
            account: ctx.accounts.vault.to_account_info(),
            destination: ctx.accounts.seller.to_account_info(),
            authority: ctx.accounts.spot.to_account_info(),
        },
        signer_seeds,
    ))?;

    ctx.accounts.spot.status = SPOT_CANCELLED;

    emit!(SpotClosed {
        listing: listing_key,
        index: spot_index,
    });

    ctx.accounts.spot.close(ctx.accounts.seller.to_account_info())?;

    Ok(())
}
