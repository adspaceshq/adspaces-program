use anchor_lang::prelude::*;

#[constant]
pub const CONFIG_SEED: &[u8] = b"config";

#[constant]
pub const LISTING_SEED: &[u8] = b"listing";

#[constant]
pub const SPOT_SEED: &[u8] = b"spot";

#[constant]
pub const VAULT_SEED: &[u8] = b"vault";

#[constant]
pub const PROOF_SEED: &[u8] = b"proof";

#[constant]
pub const MAX_SPOTS_PER_LISTING: u8 = 60;

/// Plain const: `usize` has no IDL representation, so `#[constant]` cannot be used.
pub const METADATA_URI_MAX_LEN: usize = 200;

pub const PROOF_URI_MAX_LEN: usize = 200;

#[constant]
pub const PLATFORM_FEE_BPS: u16 = 250;

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
pub const REVIEW_WINDOW_SECONDS: i64 = 48 * 60 * 60;
