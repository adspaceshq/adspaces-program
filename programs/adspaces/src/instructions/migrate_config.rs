use anchor_lang::{prelude::*, system_program};

use crate::{constants::CONFIG_SEED, error::AdspacesError, state::Config};

const CONFIG_AUTHORITY_OFFSET: usize = 8;

#[derive(Accounts)]
pub struct MigrateConfig<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    /// CHECK: Pre-migration layout cannot deserialize as `Config`; owner, seeds and authority are checked by hand.
    #[account(mut, seeds = [CONFIG_SEED], bump, owner = crate::ID)]
    pub config: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handle_migrate_config(ctx: Context<MigrateConfig>) -> Result<()> {
    let config = ctx.accounts.config.to_account_info();
    let new_len = 8 + Config::INIT_SPACE;
    {
        let data = config.try_borrow_data()?;
        require!(data.len() >= CONFIG_AUTHORITY_OFFSET + 32, AdspacesError::Unauthorized);
        let authority = Pubkey::try_from(&data[CONFIG_AUTHORITY_OFFSET..CONFIG_AUTHORITY_OFFSET + 32])
            .map_err(|_| AdspacesError::Unauthorized)?;
        require_keys_eq!(authority, ctx.accounts.authority.key(), AdspacesError::Unauthorized);
        if data.len() >= new_len {
            return Ok(());
        }
    }

    let shortfall = Rent::get()?
        .minimum_balance(new_len)
        .saturating_sub(config.lamports());
    if shortfall > 0 {
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.key(),
                system_program::Transfer {
                    from: ctx.accounts.authority.to_account_info(),
                    to: config.clone(),
                },
            ),
            shortfall,
        )?;
    }
    config.resize(new_len)?;
    Ok(())
}
