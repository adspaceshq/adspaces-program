use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

use crate::{
    constants::*,
    error::AdspacesError,
    events::{BidPlaced, BidRefunded, DeadlineExtended},
    state::{Config, Listing, Spot},
};

#[derive(Accounts)]
pub struct PlaceBid<'info> {
    #[account(mut)]
    pub bidder: Signer<'info>,
    #[account(
        mut,
        seeds = [LISTING_SEED, listing.seller.as_ref(), listing.slug_hash.as_ref()],
        bump = listing.bump,
        constraint = listing.status == LISTING_OPEN @ AdspacesError::ListingNotOpen,
        constraint = listing.mode == MODE_AUCTION @ AdspacesError::WrongMode,
    )]
    pub listing: Box<Account<'info, Listing>>,
    #[account(
        mut,
        seeds = [SPOT_SEED, listing.key().as_ref(), &[spot.index]],
        bump = spot.bump,
        constraint = spot.listing == listing.key() @ AdspacesError::Unauthorized,
        constraint = spot.status == SPOT_OPEN || spot.status == SPOT_FUNDED @ AdspacesError::SpotNotOpen,
    )]
    pub spot: Box<Account<'info, Spot>>,
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
        token::authority = bidder,
    )]
    pub bidder_ata: Account<'info, TokenAccount>,
    /// Previous top bidder's USDC account. Required when outbidding;
    /// pass the program id as a placeholder for the opening bid.
    #[account(mut)]
    pub prev_bidder_ata: Option<Account<'info, TokenAccount>>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        constraint = !config.paused @ AdspacesError::ProtocolPaused,
    )]
    pub config: Account<'info, Config>,
    #[account(
        constraint = usdc_mint.key() == listing.mint @ AdspacesError::InvalidMint
    )]
    pub usdc_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
}

pub fn handle_place_bid(ctx: Context<PlaceBid>, amount: u64) -> Result<()> {
    require!(
        ctx.accounts.bidder.key() != ctx.accounts.listing.seller,
        AdspacesError::SelfDealing
    );
    let now = Clock::get()?.unix_timestamp;
    require!(
        now < ctx.accounts.listing.deadline,
        AdspacesError::ListingClosed
    );

    let current_bid = ctx.accounts.spot.current_bid;
    let current_bidder = ctx.accounts.spot.current_bidder;

    if current_bidder == Pubkey::default() {
        require!(
            amount >= ctx.accounts.spot.price,
            AdspacesError::FirstBidTooLow
        );
    } else if ctx.accounts.listing.double_up {
        let doubled = current_bid
            .checked_mul(2)
            .ok_or(AdspacesError::MathOverflow)?;
        require!(amount >= doubled, AdspacesError::BidMustDouble);
    } else {
        let minimum = current_bid
            .checked_add(ctx.accounts.listing.min_increment)
            .ok_or(AdspacesError::MathOverflow)?;
        require!(amount >= minimum, AdspacesError::BidTooLow);
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
            .ok_or(AdspacesError::MissingRefundAccount)?;
        require!(
            prev.mint == ctx.accounts.listing.mint,
            AdspacesError::InvalidMint
        );
        require!(prev.owner == current_bidder, AdspacesError::Unauthorized);

        let listing_key = ctx.accounts.listing.key();
        let spot_index = ctx.accounts.spot.index;
        let spot_bump = ctx.accounts.spot.bump;
        let signer_seeds: &[&[&[u8]]] =
            &[&[SPOT_SEED, listing_key.as_ref(), &[spot_index], &[spot_bump]]];
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

    let deadline = ctx.accounts.listing.deadline;
    let snipe_threshold = deadline
        .checked_sub(SNIPE_WINDOW_SECONDS)
        .ok_or(AdspacesError::MathOverflow)?;
    // Extensions stop at artwork_close, so bidders can never eat into the window the seller set for
    // delivering proof (artwork_close to delivery_deadline), nor run into the refund window.
    let extension_cap = ctx.accounts.listing.artwork_close;
    if now >= snipe_threshold && deadline < extension_cap {
        let new_deadline = deadline
            .checked_add(SNIPE_EXTENSION_SECONDS)
            .ok_or(AdspacesError::MathOverflow)?
            .min(extension_cap);
        let previous_deadline = ctx.accounts.listing.deadline;
        ctx.accounts.listing.deadline = new_deadline;
        emit!(DeadlineExtended {
            listing: ctx.accounts.listing.key(),
            previous_deadline,
            new_deadline,
        });
    }

    Ok(())
}
