use anchor_lang::prelude::*;

use crate::{
    constants::{CONFIG_SEED, REVIEW_AUTHORITY_SEED},
    error::AdspacesError,
    events::{ReviewAuthorityInitialized, ReviewAuthorityUpdated},
    state::{Config, ReviewAuthority},
};

#[derive(Accounts)]
pub struct InitializeReviewAuthority<'info> {
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
        space = 8 + ReviewAuthority::INIT_SPACE,
        seeds = [REVIEW_AUTHORITY_SEED, config.key().as_ref()],
        bump,
    )]
    pub review_authority: Account<'info, ReviewAuthority>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_review_authority(
    ctx: Context<InitializeReviewAuthority>,
    authority: Pubkey,
) -> Result<()> {
    require!(
        authority != Pubkey::default(),
        AdspacesError::InvalidReviewAuthority
    );
    let review_authority = &mut ctx.accounts.review_authority;
    review_authority.config = ctx.accounts.config.key();
    review_authority.authority = authority;
    review_authority.bump = ctx.bumps.review_authority;
    emit!(ReviewAuthorityInitialized { authority });
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateReviewAuthority<'info> {
    pub authority: Signer<'info>,
    #[account(
        seeds = [CONFIG_SEED],
        bump = config.bump,
        has_one = authority @ AdspacesError::Unauthorized,
    )]
    pub config: Account<'info, Config>,
    #[account(
        mut,
        seeds = [REVIEW_AUTHORITY_SEED, config.key().as_ref()],
        bump = review_authority.bump,
        constraint = review_authority.config == config.key() @ AdspacesError::Unauthorized,
    )]
    pub review_authority: Account<'info, ReviewAuthority>,
}

pub fn handle_update_review_authority(
    ctx: Context<UpdateReviewAuthority>,
    new_authority: Pubkey,
) -> Result<()> {
    require!(
        new_authority != Pubkey::default()
            && new_authority != ctx.accounts.review_authority.authority,
        AdspacesError::InvalidReviewAuthority
    );
    let review_authority = &mut ctx.accounts.review_authority;
    let previous_authority = review_authority.authority;
    review_authority.authority = new_authority;
    emit!(ReviewAuthorityUpdated {
        previous_authority,
        new_authority,
    });
    Ok(())
}
