use anchor_lang::prelude::*;

#[error_code]
pub enum StakingVaultError {
    #[msg("Deposit amount must be greater than zero")]
    InvalidDepositAmount,

    #[msg("Withdraw shares must be greater than zero")]
    InvalidWithdrawShares,

    #[msg("Insufficient shares to withdraw")]
    InsufficientShares,

    #[msg("Caller is not the vault authority")]
    Unauthorized,

    #[msg("Arithmetic overflow or underflow")]
    Overflow,

    #[msg("Zero shares would be minted — deposit too small relative to vault size")]
    ZeroSharesMinted,

    #[msg("Yield basis-points value is out of range (max 10 000)")]
    InvalidYieldBps,
}
