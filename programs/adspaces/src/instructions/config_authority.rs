use anchor_lang::prelude::*;

use crate::{
    constants::{AUTHORITY_TRANSFER_SEED, CONFIG_SEED},
    error::AdspacesError,
    events::{ConfigAuthorityAccepted, ConfigAuthorityCancelled, ConfigAuthorityProposed},
    state::{AuthorityTransfer, Config},
};

#[derive(Accounts)]
pub struct ProposeConfigAuthority<'info> {
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
        space = 8 + AuthorityTransfer::INIT_SPACE,
        seeds = [AUTHORITY_TRANSFER_SEED, config.key().as_ref()],
        bump,
    )]
    pub transfer: Account<'info, AuthorityTransfer>,
    pub system_program: Program<'info, System>,
}

pub fn handle_propose_config_authority(
    ctx: Context<ProposeConfigAuthority>,
    proposed_authority: Pubkey,
) -> Result<()> {
    require!(
        proposed_authority != Pubkey::default()
            && proposed_authority != ctx.accounts.authority.key(),
        AdspacesError::InvalidAuthorityTransfer
    );
    let transfer = &mut ctx.accounts.transfer;
    transfer.config = ctx.accounts.config.key();
    transfer.current_authority = ctx.accounts.authority.key();
    transfer.proposed_authority = proposed_authority;
    transfer.bump = ctx.bumps.transfer;
    emit!(ConfigAuthorityProposed {
        current_authority: transfer.current_authority,
        proposed_authority,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct AcceptConfigAuthority<'info> {
    pub proposed_authority: Signer<'info>,
    #[account(mut, seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        close = previous_authority,
        seeds = [AUTHORITY_TRANSFER_SEED, config.key().as_ref()],
        bump = transfer.bump,
        constraint = transfer.config == config.key() @ AdspacesError::Unauthorized,
        constraint = transfer.current_authority == config.authority @ AdspacesError::Unauthorized,
        constraint = transfer.proposed_authority == proposed_authority.key() @ AdspacesError::Unauthorized,
    )]
    pub transfer: Account<'info, AuthorityTransfer>,
    /// CHECK: This is the stored current authority and only receives the closed PDA's rent.
    #[account(mut, address = config.authority @ AdspacesError::Unauthorized)]
    pub previous_authority: UncheckedAccount<'info>,
}

pub fn handle_accept_config_authority(ctx: Context<AcceptConfigAuthority>) -> Result<()> {
    let previous_authority = ctx.accounts.config.authority;
    let new_authority = ctx.accounts.proposed_authority.key();
    ctx.accounts.config.authority = new_authority;
    emit!(ConfigAuthorityAccepted {
        previous_authority,
        new_authority,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct CancelConfigAuthority<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ AdspacesError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        close = authority,
        seeds = [AUTHORITY_TRANSFER_SEED, config.key().as_ref()],
        bump = transfer.bump,
        constraint = transfer.config == config.key() @ AdspacesError::Unauthorized,
        constraint = transfer.current_authority == authority.key() @ AdspacesError::Unauthorized,
    )]
    pub transfer: Account<'info, AuthorityTransfer>,
}

pub fn handle_cancel_config_authority(ctx: Context<CancelConfigAuthority>) -> Result<()> {
    emit!(ConfigAuthorityCancelled {
        authority: ctx.accounts.authority.key(),
        proposed_authority: ctx.accounts.transfer.proposed_authority,
    });
    Ok(())
}
