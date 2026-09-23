use anchor_lang::prelude::*;

use crate::{
    constants::CONFIG_SEED, error::AdspacesError, events::ConfigUpdated,
    instructions::initialize_config::validate_config, state::Config,
};

#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ AdspacesError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
}

pub fn handle_update_config(
    ctx: Context<UpdateConfig>,
    treasury: Pubkey,
    fee_bps: u16,
) -> Result<()> {
    validate_config(treasury, fee_bps)?;

    let config = &mut ctx.accounts.config;
    config.treasury = treasury;
    config.fee_bps = fee_bps;

    emit!(ConfigUpdated { treasury, fee_bps });

    Ok(())
}
