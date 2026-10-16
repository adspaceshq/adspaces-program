use anchor_lang::prelude::*;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

#[constant]
pub const AUTHORITY_TRANSFER_SEED: &[u8] = b"authority_transfer";

#[constant]
pub const REVIEW_AUTHORITY_SEED: &[u8] = b"review_authority";

#[constant]
pub const PAUSE_GUARDIAN_SEED: &[u8] = b"pause_guardian";

#[constant]
pub const LISTING_SEED: &[u8] = b"listing";

#[constant]
pub const SPOT_SEED: &[u8] = b"spot";

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

#[constant]
pub const PROOF_SEED: &[u8] = b"proof";

#[constant]
pub const DISPUTE_SEED: &[u8] = b"dispute";

#[constant]
pub const MAX_SPOTS_PER_LISTING: u8 = 60;

/// Plain const: `usize` has no IDL representation, so `#[constant]` cannot be used.
pub const METADATA_URI_MAX_LEN: usize = 200;

pub const PROOF_URI_MAX_LEN: usize = 200;

pub const DISPUTE_URI_MAX_LEN: usize = 200;

#[constant]
pub const USDC_MINT_DEVNET: &str = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";

#[constant]
pub const USDC_MINT_MAINNET: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

#[constant]
pub const MODE_FIXED: u8 = 0;

#[constant]
pub const MODE_AUCTION: u8 = 1;

#[constant]
pub const LISTING_OPEN: u8 = 0;

#[constant]
pub const LISTING_CLOSED: u8 = 1;

#[constant]
pub const SPOT_OPEN: u8 = 0;

#[constant]
pub const SPOT_FUNDED: u8 = 1;

#[constant]
pub const SPOT_FINALIZED: u8 = 2;

#[constant]
pub const SPOT_CANCELLED: u8 = 3;

#[constant]
pub const SPOT_DISPUTED: u8 = 4;

#[constant]
pub const DEFAULT_REVIEW_WINDOW_SECONDS: i64 = 48 * 60 * 60;

#[constant]
#[cfg(feature = "devnet-short-review")]
pub const MIN_REVIEW_WINDOW_SECONDS: i64 = 60;

#[constant]
#[cfg(not(feature = "devnet-short-review"))]
pub const MIN_REVIEW_WINDOW_SECONDS: i64 = 60 * 60;

#[constant]
pub const MAX_REVIEW_WINDOW_SECONDS: i64 = 30 * 24 * 60 * 60;

#[constant]
pub const DEFAULT_DISPUTE_TIMEOUT_SECONDS: i64 = 3 * 24 * 60 * 60;

#[constant]
#[cfg(feature = "devnet-short-review")]
pub const MIN_DISPUTE_TIMEOUT_SECONDS: i64 = 60;

#[constant]
#[cfg(not(feature = "devnet-short-review"))]
pub const MIN_DISPUTE_TIMEOUT_SECONDS: i64 = 60 * 60;

#[constant]
pub const MAX_DISPUTE_TIMEOUT_SECONDS: i64 = 30 * 24 * 60 * 60;

#[constant]
#[cfg(feature = "devnet-short-review")]
pub const SNIPE_WINDOW_SECONDS: i64 = 30;

#[constant]
#[cfg(not(feature = "devnet-short-review"))]
pub const SNIPE_WINDOW_SECONDS: i64 = 5 * 60;

/// Smallest allowed auction bid step, in USDC base units (6 decimals): 0.01 USDC.
#[constant]
pub const MIN_BID_INCREMENT: u64 = 10_000;

#[constant]
#[cfg(feature = "devnet-short-review")]
pub const SNIPE_EXTENSION_SECONDS: i64 = 30;

#[constant]
#[cfg(not(feature = "devnet-short-review"))]
pub const SNIPE_EXTENSION_SECONDS: i64 = 5 * 60;

/// Furthest a listing's delivery deadline may lie in the future at creation: 365 days. It bounds how
/// long escrow can stay locked and keeps `delivery_deadline + review_window_seconds` far from overflow.
#[constant]
pub const MAX_LISTING_HORIZON_SECONDS: i64 = 365 * 24 * 60 * 60;

/// Shortest guaranteed proof window `[artwork_close, delivery_deadline)`.
#[constant]
#[cfg(feature = "devnet-short-review")]
pub const MIN_PROOF_WINDOW_SECONDS: i64 = 60;

#[constant]
#[cfg(not(feature = "devnet-short-review"))]
pub const MIN_PROOF_WINDOW_SECONDS: i64 = 60 * 60;

/// Largest allowed auction bid step, in USDC base units (6 decimals): 1,000,000 USDC. Keeps
/// `current_bid + min_increment` representable.
#[constant]
pub const MAX_BID_INCREMENT: u64 = 1_000_000_000_000_u64;

/// Decimals the payment mint must have; every price and the increment floor assume USDC's 6.
#[constant]
pub const PAYMENT_MINT_DECIMALS: u8 = 6;
