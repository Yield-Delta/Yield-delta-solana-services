use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("Deposit amount must be greater than zero")]
    InvalidDepositAmount,
    #[msg("Withdraw shares must be greater than zero")]
    InvalidWithdrawShares,
    #[msg("Insufficient shares to withdraw")]
    InsufficientShares,
    #[msg("Arithmetic overflow")]
    Overflow,
}
