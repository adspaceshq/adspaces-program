use anchor_lang::prelude::*;

use crate::{constants::CONFIG_SEED, error::AdspacesError, state::Config};

#[derive(Accounts)]
pub struct PauseProtocol<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ AdspacesError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
}

pub fn handle_pause_protocol(ctx: Context<PauseProtocol>, paused: bool) -> Result<()> {
    ctx.accounts.config.paused = paused;
    Ok(())
}
