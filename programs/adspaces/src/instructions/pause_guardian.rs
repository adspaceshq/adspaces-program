use anchor_lang::prelude::*;

use crate::{
    constants::{CONFIG_SEED, PAUSE_GUARDIAN_SEED},
    error::AdspacesError,
    events::{PauseGuardianSet, ProtocolPauseSet},
    state::{Config, PauseGuardian},
};

#[derive(Accounts)]
pub struct InitializePauseGuardian<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ AdspacesError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
    #[account(
        init,
        payer = authority,
        space = 8 + PauseGuardian::INIT_SPACE,
        seeds = [PAUSE_GUARDIAN_SEED, config.key().as_ref()],
        bump,
    )]
    pub pause_guardian: Account<'info, PauseGuardian>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_pause_guardian(
    ctx: Context<InitializePauseGuardian>,
    guardian: Pubkey,
) -> Result<()> {
    require!(
        guardian != Pubkey::default(),
        AdspacesError::InvalidPauseGuardian
    );
    let pause_guardian = &mut ctx.accounts.pause_guardian;
    pause_guardian.config = ctx.accounts.config.key();
    pause_guardian.guardian = guardian;
    pause_guardian.bump = ctx.bumps.pause_guardian;
    emit!(PauseGuardianSet {
        authority: ctx.accounts.authority.key(),
        previous_guardian: Pubkey::default(),
        guardian,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct UpdatePauseGuardian<'info> {
    pub authority: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ AdspacesError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [PAUSE_GUARDIAN_SEED, config.key().as_ref()],
        bump = pause_guardian.bump,
        constraint = pause_guardian.config == config.key() @ AdspacesError::Unauthorized,
    )]
    pub pause_guardian: Account<'info, PauseGuardian>,
}

/// `Pubkey::default()` disables the guardian (nobody can sign for it); the account stays so it can
/// be re-enabled later without re-initializing.
pub fn handle_update_pause_guardian(
    ctx: Context<UpdatePauseGuardian>,
    guardian: Pubkey,
) -> Result<()> {
    let pause_guardian = &mut ctx.accounts.pause_guardian;
    let previous_guardian = pause_guardian.guardian;
    pause_guardian.guardian = guardian;
    emit!(PauseGuardianSet {
        authority: ctx.accounts.authority.key(),
        previous_guardian,
        guardian,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct GuardianPause<'info> {
    pub guardian: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(
        seeds = [PAUSE_GUARDIAN_SEED, config.key().as_ref()],
        bump = pause_guardian.bump,
        constraint = pause_guardian.config == config.key() @ AdspacesError::Unauthorized,
        constraint = pause_guardian.guardian != Pubkey::default() @ AdspacesError::Unauthorized,
        has_one = guardian @ AdspacesError::Unauthorized,
    )]
    pub pause_guardian: Account<'info, PauseGuardian>,
}

/// Pause only; resuming stays with the config authority through `pause_protocol`. Succeeds when
/// already paused, so a guardian racing the authority in an incident never sees a failure.
pub fn handle_guardian_pause(ctx: Context<GuardianPause>) -> Result<()> {
    ctx.accounts.config.paused = true;
    emit!(ProtocolPauseSet {
        authority: ctx.accounts.guardian.key(),
        paused: true,
    });
    Ok(())
}
