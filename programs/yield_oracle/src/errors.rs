use anchor_lang::prelude::*;

#[error_code]
pub enum OracleError {
    #[msg("Caller is not the oracle authority")]
    Unauthorized,
    #[msg("Oracle price data is stale — post a fresh update before reading")]
    StalePrice,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Signal has already been marked as rebalanced")]
    AlreadyRebalanced,
    #[msg("allocation_bps must be <= 10 000")]
    InvalidAllocationBps,
    #[msg("confidence must be 0-100")]
    InvalidConfidence,
}
