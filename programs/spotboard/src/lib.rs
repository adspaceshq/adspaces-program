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
        instructions::create_listing::handle(
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
