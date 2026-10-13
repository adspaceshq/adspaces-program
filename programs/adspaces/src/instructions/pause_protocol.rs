use anchor_lang::prelude::*;

use crate::{constants::CONFIG_SEED, error::AdspacesError, events::ProtocolPauseSet, state::Config};

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
    // Emitted so monitoring and indexers can see every pause and resume on-chain.
    emit!(ProtocolPauseSet {
        authority: ctx.accounts.authority.key(),
        paused,
    });
    Ok(())
}
