use anchor_lang::prelude::*;

#[error_code]
pub enum SpotboardError {
    #[msg("Unknown pricing mode. Use 0 for fixed-price, 1 for auction.")]
    InvalidMode,
    #[msg("Deadline must be in the future.")]
    InvalidDeadline,
    #[msg("Artwork close must not be before the deadline.")]
    InvalidArtworkClose,
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
}
