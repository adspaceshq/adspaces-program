use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::SpotboardError,
    events::{BidPlaced, BidRefunded},
    state::{Listing, Spot},
};

/// Places an auction bid. The new bid is escrowed in the per-spot vault
/// and the previous top bidder is refunded in full inside the same
/// transaction, so no funds ever get stuck.
///
/// Minimums mirror the live campaigns:
/// - first bid must meet the starting price (Vincent / Marc),
/// - later bids must clear `min_increment` (Vincent: 10 EUR),
/// - or must double the current bid when `double_up` is set (Marc).
#[derive(Accounts)]
pub struct PlaceBid<'info> {
    #[account(mut)]
    pub bidder: Signer<'info>,
    #[account(
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
        constraint = listing.status == LISTING_OPEN @ SpotboardError::ListingNotOpen,
        constraint = listing.mode == MODE_AUCTION @ SpotboardError::WrongMode,
    )]
    pub listing: Account<'info, Listing>,
    #[account(
        mut,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[spot.index]],
        bump = spot.bump,
        constraint = spot.listing == listing.key() @ SpotboardError::Unauthorized,
        constraint = spot.status == SPOT_OPEN || spot.status == SPOT_FUNDED @ SpotboardError::SpotNotOpen,
    )]
    pub spot: Account<'info, Spot>,
    #[account(
        mut,
        constraint = vault.key() == spot.vault @ SpotboardError::Unauthorized,
    )]
    pub vault: Account<'info, TokenAccount>,
    #[account(
        mut,
        token::mint = usdc_mint,
        token::authority = bidder,
    )]
    pub bidder_ata: Account<'info, TokenAccount>,
    /// Previous top bidder's USDC account. Required when outbidding;
    /// pass the program id as a placeholder for the opening bid.
    #[account(mut)]
    pub prev_bidder_ata: Option<Account<'info, TokenAccount>>,
    #[account(
        constraint = usdc_mint.key() == listing.mint @ SpotboardError::InvalidMint
    )]
    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

pub fn handle(ctx: Context<PlaceBid>, amount: u64) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        now < ctx.accounts.listing.deadline,
        SpotboardError::ListingClosed
    );

    let current_bid = ctx.accounts.spot.current_bid;
    let current_bidder = ctx.accounts.spot.current_bidder;

    if current_bidder == Pubkey::default() {
        require!(amount >= ctx.accounts.spot.price, SpotboardError::FirstBidTooLow);
    } else if ctx.accounts.listing.double_up {
        let doubled = current_bid
            .checked_mul(2)
            .ok_or(SpotboardError::MathOverflow)?;
        require!(amount >= doubled, SpotboardError::BidMustDouble);
    } else {
        let minimum = current_bid
            .checked_add(ctx.accounts.listing.min_increment)
            .ok_or(SpotboardError::MathOverflow)?;
        require!(amount >= minimum, SpotboardError::BidTooLow);
    }

    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            token::Transfer {
                from: ctx.accounts.bidder_ata.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.bidder.to_account_info(),
            },
        ),
        amount,
    )?;

    if current_bidder != Pubkey::default() {
        let prev = ctx
            .accounts
            .prev_bidder_ata
            .as_ref()
            .ok_or(SpotboardError::MissingRefundAccount)?;
        require!(
            prev.mint == ctx.accounts.listing.mint,
            SpotboardError::InvalidMint
        );
        require!(
            prev.owner == current_bidder,
            SpotboardError::Unauthorized
        );

        let listing_key = ctx.accounts.listing.key();
        let spot_index = ctx.accounts.spot.index;
        let spot_bump = ctx.accounts.spot.bump;
        let signer_seeds: &[&[&[u8]]] = &[&[
            SPOT_SEED,
            listing_key.as_ref(),
            &[spot_index],
            &[spot_bump],
        ]];
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                token::Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: prev.to_account_info(),
                    authority: ctx.accounts.spot.to_account_info(),
                },
                signer_seeds,
            ),
            current_bid,
        )?;

        emit!(BidRefunded {
            listing: ctx.accounts.listing.key(),
            index: spot_index,
            bidder: current_bidder,
            amount: current_bid,
        });
    }

    let spot = &mut ctx.accounts.spot;
    spot.current_bid = amount;
    spot.current_bidder = ctx.accounts.bidder.key();
    spot.status = SPOT_FUNDED;

    emit!(BidPlaced {
        listing: ctx.accounts.listing.key(),
        index: spot.index,
        bidder: ctx.accounts.bidder.key(),
        amount,
    });

    Ok(())
}
