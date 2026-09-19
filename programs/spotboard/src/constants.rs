use anchor_lang::prelude::*;

/// Seed for the Listing PDA: ["listing", seller, slug_hash]
#[constant]
pub const LISTING_SEED: &[u8] = b"listing";

/// Seed for the Spot PDA: ["spot", listing, index]
#[constant]
pub const SPOT_SEED: &[u8] = b"spot";

/// Seed for the per-spot USDC vault: ["vault", spot]
#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

/// Hard cap per listing, mirrors the backend rule (CONTRACT.md).
#[constant]
pub const MAX_SPOTS_PER_LISTING: u8 = 60;

/// Max length of the off-chain metadata URI stored on the listing.
/// Plain const: `usize` has no IDL representation, so `#[constant]` cannot be used.
pub const METADATA_URI_MAX_LEN: usize = 200;

/// Platform fee in basis points: 250 = 2.5%. Taken once at seller withdraw,
/// never on outbid refunds (refunds are always full).
#[constant]
pub const PLATFORM_FEE_BPS: u16 = 250;

/// USDC mint on devnet (test USDC).
#[constant]
pub const USDC_MINT_DEVNET: &str = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";

/// USDC mint on mainnet-beta.
#[constant]
pub const USDC_MINT_MAINNET: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

/// Listing pricing mode: fixed-price claims (vana / winny).
#[constant]
pub const MODE_FIXED: u8 = 0;

/// Listing pricing mode: auction bids (Vincent / Marc).
#[constant]
pub const MODE_AUCTION: u8 = 1;

/// Listing status: accepting claims/bids.
#[constant]
pub const LISTING_OPEN: u8 = 0;

/// Spot status: open for claim/bid.
#[constant]
pub const SPOT_OPEN: u8 = 0;

/// Spot status: paid, funds sit in the per-spot vault awaiting seller withdraw.
#[constant]
pub const SPOT_FUNDED: u8 = 1;

/// Spot status: seller withdrew, vault closed, rent reclaimed.
#[constant]
pub const SPOT_FINALIZED: u8 = 2;

/// Spot status: cancelled before any payment, vault closed.
#[constant]
pub const SPOT_CANCELLED: u8 = 3;
