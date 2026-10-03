pub mod constants;
pub mod error;
pub mod events;
pub mod fees;
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
pub mod adspaces {
    use super::*;

    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        treasury: Pubkey,
        fee_bps: u16,
    ) -> Result<()> {
        instructions::initialize_config::handle_initialize_config(ctx, treasury, fee_bps)
    }

    pub fn update_config(ctx: Context<UpdateConfig>, treasury: Pubkey, fee_bps: u16) -> Result<()> {
        instructions::update_config::handle_update_config(ctx, treasury, fee_bps)
    }

    pub fn propose_config_authority(
        ctx: Context<ProposeConfigAuthority>,
        proposed_authority: Pubkey,
    ) -> Result<()> {
        instructions::config_authority::handle_propose_config_authority(ctx, proposed_authority)
    }

    pub fn accept_config_authority(ctx: Context<AcceptConfigAuthority>) -> Result<()> {
        instructions::config_authority::handle_accept_config_authority(ctx)
    }

    pub fn cancel_config_authority(ctx: Context<CancelConfigAuthority>) -> Result<()> {
        instructions::config_authority::handle_cancel_config_authority(ctx)
    }

    pub fn initialize_review_authority(
        ctx: Context<InitializeReviewAuthority>,
        authority: Pubkey,
    ) -> Result<()> {
        instructions::review_authority::handle_initialize_review_authority(ctx, authority)
    }

    pub fn update_review_authority(
        ctx: Context<UpdateReviewAuthority>,
        new_authority: Pubkey,
    ) -> Result<()> {
        instructions::review_authority::handle_update_review_authority(ctx, new_authority)
    }

    pub fn close_spot(ctx: Context<CloseSpot>) -> Result<()> {
        instructions::close_spot::handle_close_spot(ctx)
    }

    pub fn withdraw_seller(ctx: Context<WithdrawSeller>) -> Result<()> {
        instructions::withdraw_seller::handle_withdraw_seller(ctx)
    }

    pub fn submit_proof(
        ctx: Context<SubmitProof>,
        proof_hash: [u8; 32],
        proof_uri: String,
    ) -> Result<()> {
        instructions::submit_proof::handle_submit_proof(ctx, proof_hash, proof_uri)
    }

    pub fn raise_dispute(
        ctx: Context<RaiseDispute>,
        evidence_hash: [u8; 32],
        evidence_uri: String,
    ) -> Result<()> {
        instructions::raise_dispute::handle_raise_dispute(ctx, evidence_hash, evidence_uri)
    }

    pub fn withdraw_dispute(ctx: Context<WithdrawDispute>) -> Result<()> {
        instructions::withdraw_dispute::handle_withdraw_dispute(ctx)
    }

    pub fn refund_spot(ctx: Context<RefundSpot>) -> Result<()> {
        instructions::refund_spot::handle_refund_spot(ctx)
    }

    pub fn resolve_dispute(
        ctx: Context<ResolveDispute>,
        seller_share_bps: u16,
        decision_hash: [u8; 32],
        decision_uri: String,
    ) -> Result<()> {
        instructions::resolve_dispute::handle_resolve_dispute(
            ctx,
            seller_share_bps,
            decision_hash,
            decision_uri,
        )
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
        delivery_deadline: i64,
        review_window_seconds: i64,
        min_increment: u64,
        double_up: bool,
        fee_bps: u16,
        uplift_fee_bps: u16,
        max_fee_bps: u16,
    ) -> Result<()> {
        instructions::create_listing::handle_create_listing(
            ctx,
            slug_hash,
            metadata_uri,
            mode,
            deadline,
            artwork_close,
            delivery_deadline,
            review_window_seconds,
            min_increment,
            double_up,
            fee_bps,
            uplift_fee_bps,
            max_fee_bps,
        )
    }
}
