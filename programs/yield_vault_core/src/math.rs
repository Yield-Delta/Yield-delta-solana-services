use crate::errors::VaultError;
use anchor_lang::prelude::*;

/// Calculate how many share tokens to mint for a given deposit.
///
/// On the very first deposit (total_shares == 0) the share price is 1:1
/// so `shares_to_mint = amount`.  On subsequent deposits:
///
///   shares_to_mint = amount * total_shares / total_assets
///
/// All intermediate calculations use u128 to avoid overflow before the
/// final truncation back to u64.
pub fn calculate_shares_to_mint(
    amount: u64,
    total_shares: u64,
    total_assets: u64,
) -> Result<u64> {
    if total_shares == 0 || total_assets == 0 {
        // First deposit — 1:1 pricing.
        return Ok(amount);
    }

    let shares = (amount as u128)
        .checked_mul(total_shares as u128)
        .ok_or(VaultError::Overflow)?
        .checked_div(total_assets as u128)
        .ok_or(VaultError::Overflow)?;

    let shares_u64 = u64::try_from(shares).map_err(|_| VaultError::Overflow)?;
    require!(shares_u64 > 0, VaultError::ZeroSharesMinted);
    Ok(shares_u64)
}

/// Calculate the underlying asset tokens returned for a given number of shares.
///
///   assets_out = shares * total_assets / total_shares
///
/// All intermediate calculations use u128.
pub fn calculate_assets_for_shares(
    shares: u64,
    total_shares: u64,
    total_assets: u64,
) -> Result<u64> {
    if total_shares == 0 {
        return Ok(0);
    }

    let assets = (shares as u128)
        .checked_mul(total_assets as u128)
        .ok_or(VaultError::Overflow)?
        .checked_div(total_shares as u128)
        .ok_or(VaultError::Overflow)?;

    Ok(u64::try_from(assets).map_err(|_| VaultError::Overflow)?)
}

/// Convert basis points to a fraction of a u64 value.
///
///   result = value * bps / 10_000
pub fn apply_bps(value: u64, bps: u16) -> Result<u64> {
    let result = (value as u128)
        .checked_mul(bps as u128)
        .ok_or(VaultError::Overflow)?
        .checked_div(10_000u128)
        .ok_or(VaultError::Overflow)?;

    Ok(u64::try_from(result).map_err(|_| VaultError::Overflow)?)
}

/// Convert an annualised basis-points yield into the amount earned over
/// `elapsed_seconds`, using integer arithmetic (truncating).
///
///   yield_earned = principal * yield_bps * elapsed_seconds
///                  / (10_000 * SECONDS_PER_YEAR)
///
/// Uses u128 throughout to avoid overflow on large principals.
pub fn accrue_simple_interest(
    principal: u64,
    yield_bps: u16,
    elapsed_seconds: u64,
) -> Result<u64> {
    const SECONDS_PER_YEAR: u128 = 365 * 24 * 60 * 60;

    let earned = (principal as u128)
        .checked_mul(yield_bps as u128)
        .ok_or(VaultError::Overflow)?
        .checked_mul(elapsed_seconds as u128)
        .ok_or(VaultError::Overflow)?
        .checked_div(10_000u128.checked_mul(SECONDS_PER_YEAR).ok_or(VaultError::Overflow)?)
        .ok_or(VaultError::Overflow)?;

    Ok(u64::try_from(earned).map_err(|_| VaultError::Overflow)?)
}
