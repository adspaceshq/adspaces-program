use anchor_lang::prelude::*;

use crate::{
    constants::{CONFIG_SEED, MAX_DISPUTE_TIMEOUT_SECONDS, MIN_DISPUTE_TIMEOUT_SECONDS},
    error::AdspacesError,
    events::DisputeTimeoutUpdated,
    state::Config,
};

#[derive(Accounts)]
pub struct SetDisputeTimeout<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ AdspacesError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
}

// Applies to every open dispute, including ones raised before the change.
pub fn handle_set_dispute_timeout(ctx: Context<SetDisputeTimeout>, seconds: i64) -> Result<()> {
    require!(
        (MIN_DISPUTE_TIMEOUT_SECONDS..=MAX_DISPUTE_TIMEOUT_SECONDS).contains(&seconds),
        AdspacesError::InvalidDisputeTimeout
    );
    let previous = ctx.accounts.config.dispute_timeout_seconds;
    ctx.accounts.config.dispute_timeout_seconds = seconds;
    emit!(DisputeTimeoutUpdated {
        previous_seconds: previous,
        new_seconds: seconds,
    });
    Ok(())
}
