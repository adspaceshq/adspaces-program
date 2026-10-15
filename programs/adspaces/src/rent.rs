use anchor_lang::prelude::*;

use crate::{error::AdspacesError, state::Listing};

/// Call after closing one of the listing's spots, so close_listing can tell when none are left.
pub fn record_spot_closed(listing: &mut Account<Listing>) -> Result<()> {
    listing.closed_spot_count = listing
        .closed_spot_count
        .checked_add(1)
        .ok_or(AdspacesError::MathOverflow)?;
    Ok(())
}
