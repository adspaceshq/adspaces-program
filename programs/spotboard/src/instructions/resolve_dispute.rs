use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::SpotboardError,
    events::DisputeResolved,
    state::{Config, Listing, Spot},
};

#[derive(Accounts)]
pub struct ResolveDispute<'info> {
    pub resolver: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = config.authority == resolver.key() @ SpotboardError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
    #[account(
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
    )]
    pub listing: Box<Account<'info, Listing>>,
    #[account(
        mut,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[spot.index]],
        bump = spot.bump,
        constraint = spot.listing == listing.key() @ SpotboardError::Unauthorized,
        constraint = spot.status == SPOT_DISPUTED @ SpotboardError::SpotNotFunded,
    )]
    pub spot: Box<Account<'info, Spot>>,
    #[account(
        mut,
        constraint = vault.key() == spot.vault @ SpotboardError::Unauthorized,
        constraint = vault.mint == listing.mint @ SpotboardError::InvalidMint,
        constraint = vault.owner == spot.key() @ SpotboardError::Unauthorized,
    )]
    pub vault: Box<Account<'info, TokenAccount>>,
    #[account(mut, token::mint = usdc_mint, token::authority = seller)]
    pub seller_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut, address = listing.seller @ SpotboardError::Unauthorized)]
    pub seller: SystemAccount<'info>,
    #[account(
        mut,
        constraint = buyer_ata.owner == if listing.mode == MODE_FIXED { spot.buyer } else { spot.current_bidder } @ SpotboardError::NotWinningBuyer,
        constraint = buyer_ata.mint == listing.mint @ SpotboardError::InvalidMint,
    )]
    pub buyer_ata: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        constraint = treasury_ata.owner == listing.treasury @ SpotboardError::Unauthorized,
        constraint = treasury_ata.mint == listing.mint @ SpotboardError::InvalidMint,
    )]
    pub treasury_ata: Box<Account<'info, TokenAccount>>,
    #[account(constraint = usdc_mint.key() == listing.mint @ SpotboardError::InvalidMint)]
    pub usdc_mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_resolve_dispute(ctx: Context<ResolveDispute>, seller_share_bps: u16) -> Result<()> {
    require!(seller_share_bps <= 10_000, SpotboardError::InvalidFeeBps);
    let balance = ctx.accounts.vault.amount;
    require!(balance > 0, SpotboardError::SpotNotFunded);
    let seller_gross = ((balance as u128) * (seller_share_bps as u128) / 10_000) as u64;
    let buyer_refund = balance
        .checked_sub(seller_gross)
        .ok_or(SpotboardError::MathOverflow)?;
    let fee = ((seller_gross as u128) * (ctx.accounts.listing.fee_bps as u128) / 10_000) as u64;
    let seller_net = seller_gross
        .checked_sub(fee)
        .ok_or(SpotboardError::MathOverflow)?;

    let listing_key = ctx.accounts.listing.key();
    let index = ctx.accounts.spot.index;
    let bump = ctx.accounts.spot.bump;
    let signer_seeds: &[&[&[u8]]] = &[&[SPOT_SEED, listing_key.as_ref(), &[index], &[bump]]];
    for (destination, amount) in [
        (ctx.accounts.buyer_ata.to_account_info(), buyer_refund),
        (ctx.accounts.seller_ata.to_account_info(), seller_net),
        (ctx.accounts.treasury_ata.to_account_info(), fee),
    ] {
        if amount > 0 {
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.key(),
                    token::Transfer {
                        from: ctx.accounts.vault.to_account_info(),
                        to: destination,
                        authority: ctx.accounts.spot.to_account_info(),
                    },
                    signer_seeds,
                ),
                amount,
            )?;
        }
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
    ctx.accounts.spot.status = SPOT_FINALIZED;
    emit!(DisputeResolved {
        listing: listing_key,
        index,
        resolver: ctx.accounts.resolver.key(),
        seller_share_bps,
        buyer_refund,
        seller_amount: seller_net,
        fee,
    });
    Ok(())
}
