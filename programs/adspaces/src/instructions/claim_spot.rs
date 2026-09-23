use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::AdspacesError,
    events::SpotClaimed,
    state::{Listing, Spot},
};

#[derive(Accounts)]
pub struct ClaimSpot<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,
    #[account(
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
        constraint = listing.status == LISTING_OPEN @ AdspacesError::ListingNotOpen,
        constraint = listing.mode == MODE_FIXED @ AdspacesError::WrongMode,
    )]
    pub listing: Account<'info, Listing>,
    #[account(
        mut,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[spot.index]],
        bump = spot.bump,
        constraint = spot.listing == listing.key() @ AdspacesError::Unauthorized,
        constraint = spot.status == SPOT_OPEN @ AdspacesError::SpotNotOpen,
    )]
    pub spot: Account<'info, Spot>,
    #[account(
        mut,
        constraint = vault.key() == spot.vault @ AdspacesError::Unauthorized,
        constraint = vault.mint == listing.mint @ AdspacesError::InvalidMint,
        constraint = vault.owner == spot.key() @ AdspacesError::Unauthorized,
    )]
    pub vault: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = usdc_mint,
        token::authority = buyer,
    )]
    pub buyer_ata: Account<'info, TokenAccount>,
    #[account(
        constraint = usdc_mint.key() == listing.mint @ AdspacesError::InvalidMint
    )]
    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_claim_spot(ctx: Context<ClaimSpot>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        now < ctx.accounts.listing.deadline,
        AdspacesError::ListingClosed
    );

    let price = ctx.accounts.spot.price;
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            token::Transfer {
                from: ctx.accounts.buyer_ata.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.buyer.to_account_info(),
            },
        ),
        price,
    )?;

    let spot = &mut ctx.accounts.spot;
    spot.status = SPOT_FUNDED;
    spot.buyer = ctx.accounts.buyer.key();

    emit!(SpotClaimed {
        listing: ctx.accounts.listing.key(),
        index: spot.index,
        buyer: ctx.accounts.buyer.key(),
        price,
    });

    Ok(())
}
