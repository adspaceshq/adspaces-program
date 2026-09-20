use anchor_lang::prelude::*;
use anchor_spl::token::Mint;

use crate::{
    constants::CONFIG_SEED, error::SpotboardError, events::ConfigInitialized, state::Config,
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
    require!(treasury != Pubkey::default(), SpotboardError::Unauthorized);
    require!(fee_bps <= 10_000, SpotboardError::InvalidFeeBps);
    Ok(())
}
