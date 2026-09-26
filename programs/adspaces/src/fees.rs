use anchor_lang::prelude::*;

use crate::{
    constants::MODE_FIXED,
    error::AdspacesError,
    state::{Listing, Spot},
};

pub fn calculate_fee(listing: &Listing, spot: &Spot, balance: u64) -> Result<u64> {
    if listing.mode == MODE_FIXED {
        return apply_bps(balance, listing.fee_bps);
    }

    let base_value = balance.min(spot.price);
    let uplift = balance
        .checked_sub(base_value)
        .ok_or(AdspacesError::MathOverflow)?;
    let base_fee = apply_bps(base_value, listing.fee_bps)?;
    let uplift_fee = apply_bps(uplift, listing.uplift_fee_bps)?;
    let calculated = base_fee
        .checked_add(uplift_fee)
        .ok_or(AdspacesError::MathOverflow)?;
    let maximum = apply_bps(balance, listing.max_fee_bps)?;
    Ok(calculated.min(maximum))
}

pub fn calculate_dispute_fee(
    listing: &Listing,
    spot: &Spot,
    balance: u64,
    seller_share_bps: u16,
) -> Result<u64> {
    let full_fee = calculate_fee(listing, spot, balance)?;
    apply_bps(full_fee, seller_share_bps)
}

fn apply_bps(amount: u64, bps: u16) -> Result<u64> {
    (amount as u128)
        .checked_mul(bps as u128)
        .and_then(|value| value.checked_div(10_000))
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| error!(AdspacesError::MathOverflow))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{DEFAULT_REVIEW_WINDOW_SECONDS, MODE_AUCTION, MODE_FIXED};

    fn listing(mode: u8, fee_bps: u16, uplift_fee_bps: u16, max_fee_bps: u16) -> Listing {
        Listing {
            seller: Pubkey::default(),
            slug_hash: [0; 16],
            metadata_uri: String::new(),
            mint: Pubkey::default(),
            mode,
            deadline: 0,
            artwork_close: 0,
            delivery_deadline: 0,
            review_window_seconds: DEFAULT_REVIEW_WINDOW_SECONDS,
            min_increment: 0,
            double_up: false,
            spot_count: 0,
            status: 0,
            treasury: Pubkey::default(),
            fee_bps,
            uplift_fee_bps,
            max_fee_bps,
            bump: 0,
        }
    }

    fn spot(price: u64) -> Spot {
        Spot {
            listing: Pubkey::default(),
            index: 0,
            price,
            status: 0,
            current_bid: 0,
            current_bidder: Pubkey::default(),
            buyer: Pubkey::default(),
            vault: Pubkey::default(),
            bump: 0,
            vault_bump: 0,
            proof_hash: [0; 32],
            proof_submitted_at: 0,
        }
    }

    #[test]
    fn fixed_fee_applies_to_the_full_balance() {
        let fee = calculate_fee(&listing(MODE_FIXED, 250, 0, 250), &spot(100), 400).unwrap();
        assert_eq!(fee, 10);
    }

    #[test]
    fn auction_fee_charges_uplift_above_the_starting_price() {
        let fee = calculate_fee(&listing(MODE_AUCTION, 250, 1_000, 750), &spot(100), 200).unwrap();
        assert_eq!(fee, 12);
    }

    #[test]
    fn auction_fee_respects_the_effective_cap() {
        let fee = calculate_fee(&listing(MODE_AUCTION, 250, 1_000, 750), &spot(100), 400).unwrap();
        assert_eq!(fee, 30);
    }

    #[test]
    fn dispute_fee_tracks_the_seller_share() {
        let fee = calculate_dispute_fee(
            &listing(MODE_AUCTION, 250, 1_000, 750),
            &spot(100),
            400,
            5_000,
        )
        .unwrap();
        assert_eq!(fee, 15);
    }
}
