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
pub struct ListingCreated {
    pub listing: Pubkey,
    pub seller: Pubkey,
    pub slug_hash: [u8; 16],
    pub mode: u8,
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
