use anchor_lang::prelude::*;

#[error_code]
pub enum DeltaNeutralError {
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

    #[msg("Zero shares would be minted — deposit too small relative to vault size")]
    ZeroSharesMinted,
}
