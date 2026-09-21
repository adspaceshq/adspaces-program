use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::SpotboardError,
    events::SpotRefunded,
    state::{Listing, Spot},
};

#[derive(Accounts)]
pub struct RefundSpot<'info> {
    pub caller: Signer<'info>,
    #[account(
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
    )]
    pub listing: Box<Account<'info, Listing>>,
    #[account(mut, address = listing.seller @ SpotboardError::Unauthorized)]
    pub seller: SystemAccount<'info>,
    #[account(
        mut,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[spot.index]],
        bump = spot.bump,
        constraint = spot.listing == listing.key() @ SpotboardError::Unauthorized,
        constraint = spot.status == SPOT_FUNDED @ SpotboardError::SpotNotFunded,
    )]
    pub spot: Box<Account<'info, Spot>>,
    #[account(
        mut,
        constraint = vault.key() == spot.vault @ SpotboardError::Unauthorized,
        constraint = vault.mint == listing.mint @ SpotboardError::InvalidMint,
        constraint = vault.owner == spot.key() @ SpotboardError::Unauthorized,
    )]
    pub vault: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = buyer_ata.owner == if listing.mode == MODE_FIXED { spot.buyer } else { spot.current_bidder } @ SpotboardError::NotWinningBuyer,
        constraint = buyer_ata.mint == listing.mint @ SpotboardError::InvalidMint,
    )]
    pub buyer_ata: Box<Account<'info, TokenAccount>>,
    #[account(constraint = usdc_mint.key() == listing.mint @ SpotboardError::InvalidMint)]
    pub usdc_mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_refund_spot(ctx: Context<RefundSpot>) -> Result<()> {
    require!(
        ctx.accounts.caller.key() == ctx.accounts.listing.seller,
        SpotboardError::Unauthorized
    );
    let amount = ctx.accounts.vault.amount;
    require!(amount > 0, SpotboardError::SpotNotFunded);
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
    ctx.accounts.spot.status = SPOT_CANCELLED;
    emit!(SpotRefunded {
        listing: listing_key,
        index,
        buyer: ctx.accounts.buyer_ata.owner,
        amount,
        seller_initiated: true,
    });
    Ok(())
}
