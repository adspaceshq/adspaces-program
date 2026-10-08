use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

use crate::{
    constants::CONFIG_SEED, error::AdspacesError, events::ConfigInitialized, state::Config,
};

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + Config::INIT_SPACE,
        seeds = [CONFIG_SEED],
        bump,
    )]
    pub config: Account<'info, Config>,
    #[account(constraint = program.programdata_address()? == Some(program_data.key()))]
    pub program: Program<'info, crate::program::Adspaces>,
    #[account(
        constraint = program_data.upgrade_authority_address == Some(authority.key()) @ AdspacesError::InvalidConfigAuthority,
    )]
    pub program_data: Account<'info, ProgramData>,
    pub payment_mint: Account<'info, Mint>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_config(
    ctx: Context<InitializeConfig>,
    treasury: Pubkey,
    fee_bps: u16,
) -> Result<()> {
    validate_config(treasury, fee_bps)?;

    let config = &mut ctx.accounts.config;
    config.authority = ctx.accounts.authority.key();
    config.payment_mint = ctx.accounts.payment_mint.key();
    config.treasury = treasury;
    config.fee_bps = fee_bps;
    config.paused = false;
    config.dispute_timeout_seconds = crate::constants::DEFAULT_DISPUTE_TIMEOUT_SECONDS;
    config.bump = ctx.bumps.config;

    emit!(ConfigInitialized {
        authority: config.authority,
        payment_mint: config.payment_mint,
        treasury,
        fee_bps,
    });

    Ok(())
}

pub(crate) fn validate_config(treasury: Pubkey, fee_bps: u16) -> Result<()> {
    require!(treasury != Pubkey::default(), AdspacesError::Unauthorized);
    require!(fee_bps <= 10_000, AdspacesError::InvalidFeeBps);
    Ok(())
}
