use anchor_lang::prelude::*;

#[event]
pub struct ConfigInitialized {
    pub authority: Pubkey,
    pub payment_mint: Pubkey,
    pub treasury: Pubkey,
    pub fee_bps: u16,
}

#[event]
pub struct ConfigUpdated {
    pub treasury: Pubkey,
    pub fee_bps: u16,
}

#[event]
pub struct ConfigAuthorityProposed {
    pub current_authority: Pubkey,
    pub proposed_authority: Pubkey,
}

#[event]
pub struct ConfigAuthorityAccepted {
    pub previous_authority: Pubkey,
    pub new_authority: Pubkey,
}

#[event]
pub struct ConfigAuthorityCancelled {
    pub authority: Pubkey,
    pub proposed_authority: Pubkey,
}

#[event]
pub struct ReviewAuthorityInitialized {
    pub authority: Pubkey,
}

#[event]
pub struct ReviewAuthorityUpdated {
    pub previous_authority: Pubkey,
    pub new_authority: Pubkey,
}

#[event]
pub struct ListingCreated {
    pub listing: Pubkey,
    pub seller: Pubkey,
    pub slug_hash: [u8; 16],
    pub mode: u8,
    pub review_window_seconds: i64,
    pub fee_bps: u16,
    pub uplift_fee_bps: u16,
    pub max_fee_bps: u16,
}

#[event]
pub struct SpotAdded {
    pub listing: Pubkey,
    pub index: u8,
    pub price: u64,
}

#[event]
pub struct SpotClaimed {
    pub listing: Pubkey,
    pub index: u8,
    pub buyer: Pubkey,
    pub price: u64,
}

#[event]
pub struct BidPlaced {
    pub listing: Pubkey,
    pub index: u8,
    pub bidder: Pubkey,
    pub amount: u64,
}

#[event]
pub struct DeadlineExtended {
    pub listing: Pubkey,
    pub previous_deadline: i64,
    pub new_deadline: i64,
}

#[event]
pub struct BidRefunded {
    pub listing: Pubkey,
    pub index: u8,
    pub bidder: Pubkey,
    pub amount: u64,
}

#[event]
pub struct SellerWithdrawn {
    pub listing: Pubkey,
    pub index: u8,
    pub seller: Pubkey,
    pub amount: u64,
    pub fee: u64,
}

#[event]
pub struct SpotClosed {
    pub listing: Pubkey,
    pub index: u8,
}

#[event]
pub struct ProofSubmitted {
    pub listing: Pubkey,
    pub index: u8,
    pub proof: Pubkey,
    pub seller: Pubkey,
    pub proof_hash: [u8; 32],
    pub submitted_at: i64,
}

#[event]
pub struct DisputeRaised {
    pub listing: Pubkey,
    pub index: u8,
    pub dispute: Pubkey,
    pub buyer: Pubkey,
    pub proof_submitted: bool,
    pub evidence_hash: [u8; 32],
    pub raised_at: i64,
}

#[event]
pub struct DisputeWithdrawn {
    pub listing: Pubkey,
    pub index: u8,
    pub dispute: Pubkey,
    pub buyer: Pubkey,
}

#[event]
pub struct DisputeResolved {
    pub listing: Pubkey,
    pub index: u8,
    pub dispute: Pubkey,
    pub resolver: Pubkey,
    pub decision_hash: [u8; 32],
    pub seller_share_bps: u16,
    pub buyer_refund: u64,
    pub seller_amount: u64,
    pub fee: u64,
}

#[event]
pub struct ListingClosed {
    pub listing: Pubkey,
    pub seller: Pubkey,
}

#[event]
pub struct DisputeTimedOut {
    pub listing: Pubkey,
    pub index: u8,
    pub dispute: Pubkey,
    pub buyer: Pubkey,
    pub amount: u64,
}

#[event]
pub struct SpotRefunded {
    pub listing: Pubkey,
    pub index: u8,
    pub buyer: Pubkey,
    pub amount: u64,
    pub seller_initiated: bool,
}
