use anchor_lang::prelude::*;

#[error_code]
pub enum AdaptiveVaultError {
    #[msg("Deposit amount must be greater than zero")]
    InvalidDepositAmount,
    #[msg("Shares must be greater than zero")]
    InvalidShareAmount,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Zero shares would be minted for this deposit")]
    ZeroSharesMinted,
    #[msg("Amount exceeds available balance")]
    InsufficientBalance,
    #[msg("Caller is not the vault authority")]
    Unauthorized,
    #[msg("Withdrawals are slot-locked during a high-volatility regime transition")]
    WithdrawLockedHighVolatility,
    #[msg("Oracle signal is stale — post a fresh signal before accruing")]
    StaleOracleSignal,
    #[msg("Oracle signal account does not match the vault's registered oracle")]
    OracleMismatch,
}
