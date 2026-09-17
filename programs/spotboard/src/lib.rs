pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use error::*;
pub use events::*;
pub use instructions::*;
pub use state::*;

declare_id!("6ft49ExLRbHUcY6crReW7qibZLrS5rdCudKWQRp2LMSW");

#[program]
pub mod spotboard {
    use super::*;

    pub fn close_spot(ctx: Context<CloseSpot>) -> Result<()> {
        instructions::close_spot::handle_close_spot(ctx)
    }

    pub fn withdraw_seller(ctx: Context<WithdrawSeller>) -> Result<()> {
        instructions::withdraw_seller::handle_withdraw_seller(ctx)
    }

    pub fn place_bid(ctx: Context<PlaceBid>, amount: u64) -> Result<()> {
        instructions::place_bid::handle_place_bid(ctx, amount)
    }

    pub fn claim_spot(ctx: Context<ClaimSpot>) -> Result<()> {
        instructions::claim_spot::handle_claim_spot(ctx)
    }

    pub fn add_spot(ctx: Context<AddSpot>, price: u64) -> Result<()> {
        instructions::add_spot::handle_add_spot(ctx, price)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_listing(
        ctx: Context<CreateListing>,
        slug_hash: [u8; 16],
        metadata_uri: String,
        mode: u8,
        deadline: i64,
        artwork_close: i64,
        min_increment: u64,
        double_up: bool,
    ) -> Result<()> {
        instructions::create_listing::handle_create_listing(
            ctx,
            slug_hash,
            metadata_uri,
            mode,
            deadline,
            artwork_close,
            min_increment,
            double_up,
        )
    }
}
