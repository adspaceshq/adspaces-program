use anchor_lang::prelude::*;

/// Emitted when a seller creates a campaign. The backend indexer
/// maps `slug_hash` back to the listing slug.
#[event]
pub struct ListingCreated {
    pub listing: Pubkey,
    pub seller: Pubkey,
    pub slug_hash: [u8; 16],
    pub mode: u8,
}

/// Emitted per spot creation (one event per `add_spot` call).
#[event]
pub struct SpotAdded {
    pub listing: Pubkey,
    pub index: u8,
    pub price: u64,
}

/// Emitted when a buyer claims a fixed-price spot (funds enter the vault).
#[event]
pub struct SpotClaimed {
    pub listing: Pubkey,
    pub index: u8,
    pub buyer: Pubkey,
    pub price: u64,
}

/// Emitted when a bidder takes the lead on an auction spot.
#[event]
pub struct BidPlaced {
    pub listing: Pubkey,
    pub index: u8,
    pub bidder: Pubkey,
    pub amount: u64,
}

/// Emitted when an outbid bidder is refunded in full inside `place_bid`.
#[event]
pub struct BidRefunded {
    pub listing: Pubkey,
    pub index: u8,
    pub bidder: Pubkey,
    pub amount: u64,
}

/// Emitted when the seller withdraws a funded spot (net of the 2.5% fee).
#[event]
pub struct SellerWithdrawn {
    pub listing: Pubkey,
    pub index: u8,
    pub seller: Pubkey,
    pub amount: u64,
    pub fee: u64,
}

/// Emitted when the seller closes an unfunded spot and reclaims vault rent.
#[event]
pub struct SpotClosed {
    pub listing: Pubkey,
    pub index: u8,
}
