use anchor_lang::prelude::*;

use crate::constants::METADATA_URI_MAX_LEN;

/// A seller's campaign: one surface split into spots.
/// Settlement truth lives here; story/images/mockup stay off-chain
/// behind `metadata_uri` (backend slug URL).
#[account]
#[derive(InitSpace)]
pub struct Listing {
    /// Seller authority: creates spots, withdraws, cancels.
    pub seller: Pubkey,
    /// First 16 bytes of sha256(slug). Links on-chain listing to backend slug.
    pub slug_hash: [u8; 16],
    /// Off-chain metadata (backend `/api/listings/{slug}` URL or IPFS).
    #[max_len(METADATA_URI_MAX_LEN)]
    pub metadata_uri: String,
    /// USDC mint (devnet test USDC or mainnet USDC, enforced at creation).
    pub mint: Pubkey,
    /// MODE_FIXED (claim) or MODE_AUCTION (bid).
    pub mode: u8,
    /// Unix timestamp: claims/bids rejected at or after this.
    pub deadline: i64,
    /// Unix timestamp: artwork close, informational for fulfillment.
    pub artwork_close: i64,
    /// Auction only: minimum raise over the current bid (base units).
    /// Ignored when `double_up` is set.
    pub min_increment: u64,
    /// Auction only (Marc mode): each takeover must double the current bid.
    pub double_up: bool,
    /// Spots created so far. Also the index of the next spot. Max 60.
    pub spot_count: u8,
    /// LISTING_OPEN; per-spot status carries the rest.
    pub status: u8,
    /// Destination authority for the platform fee (fee goes to its USDC ATA).
    pub treasury: Pubkey,
    /// Basis points, always PLATFORM_FEE_BPS at creation.
    pub fee_bps: u16,
    pub bump: u8,
}

/// One numbered ad slot. Escrow for this spot sits in `vault`,
/// a per-spot USDC token account owned by this PDA.
#[account]
#[derive(InitSpace)]
pub struct Spot {
    /// Parent listing.
    pub listing: Pubkey,
    /// Index within the listing (0-based, assigned at creation).
    pub index: u8,
    /// Fixed price (claim) or starting price (auction), USDC base units.
    pub price: u64,
    /// SPOT_OPEN / SPOT_FUNDED / SPOT_FINALIZED / SPOT_CANCELLED.
    pub status: u8,
    /// Auction only: current top bid, USDC base units.
    pub current_bid: u64,
    /// Auction only: current top bidder. Pubkey::default() when no bids yet.
    pub current_bidder: Pubkey,
    /// Per-spot USDC vault (PDA token account).
    pub vault: Pubkey,
    pub bump: u8,
    pub vault_bump: u8,
}
