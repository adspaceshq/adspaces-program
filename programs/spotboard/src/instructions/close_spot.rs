use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount};

use crate::{
    constants::*,
    error::SpotboardError,
    events::SpotClosed,
    state::{Listing, Spot},
};

/// Cancels a spot that was never paid: closes the empty vault so its rent
/// returns to the seller. Funded spots can only exit via `withdraw_seller`.
#[derive(Accounts)]
pub struct CloseSpot<'info> {
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
        constraint = spot.status == SPOT_OPEN @ SpotboardError::SpotAlreadyFunded,
    )]
    pub spot: Account<'info, Spot>,
    #[account(
        mut,
        constraint = vault.key() == spot.vault @ SpotboardError::Unauthorized,
        constraint = vault.amount == 0 @ SpotboardError::VaultNotEmpty,
    )]
    pub vault: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_close_spot(ctx: Context<CloseSpot>) -> Result<()> {
    let listing_key = ctx.accounts.listing.key();
    let spot_index = ctx.accounts.spot.index;
    let spot_bump = ctx.accounts.spot.bump;
    let signer_seeds: &[&[&[u8]]] = &[&[
        SPOT_SEED,
        listing_key.as_ref(),
        &[spot_index],
        &[spot_bump],
    ]];

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
    spot.status = SPOT_CANCELLED;

    emit!(SpotClosed {
        listing: listing_key,
        index: spot_index,
    });

    Ok(())
}
