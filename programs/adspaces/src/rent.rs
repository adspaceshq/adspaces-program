use anchor_lang::prelude::*;

use crate::{
    error::AdspacesError,
    state::{Listing, Spot},
};

/// A spot with a submitted proof must close its proof record too; otherwise a caller could leave
/// that account and its rent behind for good.
pub fn require_proof_account(spot: &Spot, proof_passed: bool) -> Result<()> {
    require!(
        spot.proof_submitted_at == 0 || proof_passed,
        AdspacesError::ProofAccountRequired
    );
    Ok(())
}

/// Call after closing one of the listing's spots, so close_listing can tell when none are left.
pub fn record_spot_closed(listing: &mut Account<Listing>) -> Result<()> {
    listing.closed_spot_count = listing
        .closed_spot_count
        .checked_add(1)
        .ok_or(AdspacesError::MathOverflow)?;
    Ok(())
}
