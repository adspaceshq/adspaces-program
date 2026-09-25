use anchor_lang::prelude::*;

#[error_code]
pub enum AdspacesError {
    #[msg("Unknown pricing mode. Use 0 for fixed-price, 1 for auction.")]
    InvalidMode,
    #[msg("Deadline must be in the future.")]
    InvalidDeadline,
    #[msg("Artwork close must not be before the deadline.")]
    InvalidArtworkClose,
    #[msg("Delivery deadline must be after artwork close.")]
    InvalidDeliveryDeadline,
    #[msg("Auction without double-up needs a minimum increment above zero.")]
    InvalidMinIncrement,
    #[msg("Metadata URI is too long.")]
    MetadataUriTooLong,
    #[msg("Mint is not a known USDC mint (devnet or mainnet-beta).")]
    InvalidMint,
    #[msg("Fee must be at most 10,000 basis points.")]
    InvalidFeeBps,
    #[msg("Listing is past its deadline.")]
    ListingClosed,
    #[msg("Listing is not open.")]
    ListingNotOpen,
    #[msg("Spot is not open for claims or bids.")]
    SpotNotOpen,
    #[msg("Spot was already paid. Seller withdraw or cancel only.")]
    SpotAlreadyFunded,
    #[msg("This instruction does not match the listing pricing mode.")]
    WrongMode,
    #[msg("Spot price must be above zero.")]
    ZeroPrice,
    #[msg("Listing already has the maximum of 60 spots.")]
    TooManySpots,
    #[msg("First bid must meet the starting price.")]
    FirstBidTooLow,
    #[msg("Bid must beat the current bid by the minimum increment.")]
    BidTooLow,
    #[msg("Takeover bid must be at least double the current bid.")]
    BidMustDouble,
    #[msg("Settlement is only available after the deadline.")]
    WithdrawTooEarly,
    #[msg("Spot has no funds to withdraw.")]
    SpotNotFunded,
    #[msg("Vault still holds funds. Only unfunded spots can be closed.")]
    VaultNotEmpty,
    #[msg("Only the seller can perform this action.")]
    Unauthorized,
    #[msg("Previous bidder token account is required to refund an outbid.")]
    MissingRefundAccount,
    #[msg("Arithmetic overflow.")]
    MathOverflow,
    #[msg("Proof is required before settlement.")]
    ProofRequired,
    #[msg("Proof was already submitted.")]
    ProofAlreadySubmitted,
    #[msg("Proof hash cannot be empty.")]
    InvalidProof,
    #[msg("Buyer review period is still open.")]
    ReviewWindowOpen,
    #[msg("Buyer review period has ended.")]
    ReviewWindowClosed,
    #[msg("Only the winning buyer can dispute this spot.")]
    NotWinningBuyer,
    #[msg("Delivery deadline has passed; proof can no longer be submitted.")]
    ProofDeadlinePassed,
    #[msg("Delivery deadline has not passed yet.")]
    DeliveryDeadlinePending,
    #[msg("Proof exists; refund requires dispute resolution or seller cancellation.")]
    ProofExists,
    #[msg("Proof URI must be nonempty and at most 200 bytes.")]
    InvalidProofUri,
    #[msg("Dispute evidence hash and URI must both be present or both absent, and URI at most 200 bytes.")]
    InvalidDisputeEvidence,
    #[msg("Decision hash and URI must be present, and URI at most 200 bytes.")]
    InvalidDecisionEvidence,
    #[msg("Proposed authority must be a different, nondefault address.")]
    InvalidAuthorityTransfer,
    #[msg("Only the program upgrade authority can initialize config.")]
    InvalidConfigAuthority,
    #[msg("Only the configured review authority can approve a listing.")]
    InvalidReviewAuthority,
    #[msg("Listing fee policy is invalid for its pricing mode.")]
    InvalidFeePolicy,
}
