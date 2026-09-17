pub mod constants;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use state::*;

declare_id!("6ft49ExLRbHUcY6crReW7qibZLrS5rdCudKWQRp2LMSW");

#[program]
pub mod spotboard {}
