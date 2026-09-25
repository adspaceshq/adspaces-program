use anchor_lang::prelude::*;

use crate::constants::{DISPUTE_URI_MAX_LEN, METADATA_URI_MAX_LEN, PROOF_URI_MAX_LEN};

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub authority: Pubkey,
    pub payment_mint: Pubkey,
    pub treasury: Pubkey,
    pub fee_bps: u16,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct AuthorityTransfer {
    pub config: Pubkey,
    pub current_authority: Pubkey,
    pub proposed_authority: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct ReviewAuthority {
    pub config: Pubkey,
    pub authority: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Listing {
    pub seller: Pubkey,
    /// First 16 bytes of sha256(slug).
    pub slug_hash: [u8; 16],
    #[max_len(METADATA_URI_MAX_LEN)]
    pub metadata_uri: String,
    pub mint: Pubkey,
    pub mode: u8,
    pub deadline: i64,
    pub artwork_close: i64,
    pub delivery_deadline: i64,
    pub min_increment: u64,
    pub double_up: bool,
    pub spot_count: u8,
    pub status: u8,
    pub treasury: Pubkey,
    pub fee_bps: u16,
    pub uplift_fee_bps: u16,
    pub max_fee_bps: u16,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Spot {
    pub listing: Pubkey,
    pub index: u8,
    pub price: u64,
    pub status: u8,
    pub current_bid: u64,
    pub current_bidder: Pubkey,
    pub buyer: Pubkey,
    pub vault: Pubkey,
    pub bump: u8,
    pub vault_bump: u8,
    pub proof_hash: [u8; 32],
    pub proof_submitted_at: i64,
}

#[account]
#[derive(InitSpace)]
pub struct ProofRecord {
    pub spot: Pubkey,
    pub proof_hash: [u8; 32],
    #[max_len(PROOF_URI_MAX_LEN)]
    pub proof_uri: String,
    pub submitted_at: i64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct DisputeRecord {
    pub spot: Pubkey,
    pub buyer: Pubkey,
    pub proof_submitted: bool,
    pub evidence_hash: [u8; 32],
    #[max_len(DISPUTE_URI_MAX_LEN)]
    pub evidence_uri: String,
    pub opened_at: i64,
    pub decision_hash: [u8; 32],
    #[max_len(DISPUTE_URI_MAX_LEN)]
    pub decision_uri: String,
    pub decided_at: i64,
    pub seller_share_bps: u16,
    pub buyer_refund: u64,
    pub seller_amount: u64,
    pub fee: u64,
    pub bump: u8,
}
