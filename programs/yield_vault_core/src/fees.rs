use crate::errors::VaultError;
use anchor_lang::prelude::*;

/// Annualised management fee in basis points (2 % / year).
pub const MANAGEMENT_FEE_BPS: u64 = 200;

/// Performance fee in basis points taken on profits (20 %).
pub const PERFORMANCE_FEE_BPS: u64 = 2_000;

/// Number of seconds in a year (non-leap).
pub const SECONDS_PER_YEAR: u64 = 365 * 24 * 60 * 60;

/// Calculate the management fee accrued over `elapsed_seconds`.
///
///   fee = total_assets * MANAGEMENT_FEE_BPS * elapsed_seconds
///          / (10_000 * SECONDS_PER_YEAR)
///
/// Returns the fee amount denominated in the vault's underlying asset.
/// All intermediate calculations use u128 to prevent overflow.
pub fn calculate_management_fee(total_assets: u64, elapsed_seconds: u64) -> Result<u64> {
    let fee = (total_assets as u128)
        .checked_mul(MANAGEMENT_FEE_BPS as u128)
        .ok_or(VaultError::Overflow)?
        .checked_mul(elapsed_seconds as u128)
        .ok_or(VaultError::Overflow)?
        .checked_div(
            10_000u128
                .checked_mul(SECONDS_PER_YEAR as u128)
                .ok_or(VaultError::Overflow)?,
        )
        .ok_or(VaultError::Overflow)?;

    Ok(u64::try_from(fee).map_err(|_| VaultError::Overflow)?)
}

/// Calculate the performance fee on profits.
///
///   fee = profit * PERFORMANCE_FEE_BPS / 10_000
///
/// `profit` should be the increase in total_assets since the last high-water mark.
/// Returns 0 if profit is 0.
pub fn calculate_performance_fee(profit: u64) -> Result<u64> {
    if profit == 0 {
        return Ok(0);
    }

    let fee = (profit as u128)
        .checked_mul(PERFORMANCE_FEE_BPS as u128)
        .ok_or(VaultError::Overflow)?
        .checked_div(10_000u128)
        .ok_or(VaultError::Overflow)?;

    Ok(u64::try_from(fee).map_err(|_| VaultError::Overflow)?)
}
