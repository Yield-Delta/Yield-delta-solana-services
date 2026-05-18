use anchor_lang::prelude::*;

/// Common error codes shared across all Yield Delta vault programs.
///
/// Individual programs may define additional program-specific errors but
/// should use these codes for conditions that can occur in any vault.
#[error_code]
pub enum VaultError {
    #[msg("Deposit amount must be greater than zero")]
    InvalidDepositAmount,

    #[msg("Withdraw shares must be greater than zero")]
    InvalidWithdrawShares,

    #[msg("Insufficient shares to withdraw")]
    InsufficientShares,

    #[msg("Arithmetic overflow or underflow")]
    Overflow,

    #[msg("Caller is not the vault authority")]
    Unauthorized,

    #[msg("Allocation weights do not sum to 10 000 basis points")]
    InvalidAllocationWeights,

    #[msg("Too many allocations — maximum is 6")]
    TooManyAllocations,

    #[msg("Vault is currently paused")]
    VaultPaused,

    #[msg("Oracle price data is stale")]
    StaleOraclePrice,

    #[msg("Yield basis-points value is out of range (max 10 000)")]
    InvalidYieldBps,

    #[msg("Delta rebalance threshold basis-points value is out of range")]
    InvalidThresholdBps,

    #[msg("Zero shares would be minted — deposit too small relative to vault size")]
    ZeroSharesMinted,
}
