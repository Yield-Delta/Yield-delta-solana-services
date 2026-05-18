use anchor_lang::prelude::*;
use yield_oracle::state::{SignalAccount, VolatilityRegime};
use yield_vault_core::math::accrue_simple_interest;

use crate::errors::AdaptiveVaultError;
use crate::state::AdaptiveVaultState;

#[derive(Accounts)]
#[instruction(strategy_id: u8)]
pub struct AccrueAdaptive<'info> {
    #[account(
        constraint = authority.key() == vault_state.authority @ AdaptiveVaultError::Unauthorized
    )]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [b"adaptive_vault", vault_state.token_mint.as_ref()],
        bump  = vault_state.bump,
    )]
    pub vault_state: Account<'info, AdaptiveVaultState>,

    /// Oracle SignalAccount owned by the yield_oracle program.
    /// Seeds verified against the oracle program's derivation.
    #[account(
        constraint = oracle_signal.key() == vault_state.oracle_signal
            @ AdaptiveVaultError::OracleMismatch,
        seeds = [b"signal", strategy_id.to_le_bytes().as_ref()],
        bump  = oracle_signal.bump,
        seeds::program = yield_oracle::ID,
    )]
    pub oracle_signal: Account<'info, SignalAccount>,
}

/// Reads the live VolatilityRegime from the oracle, adjusts the vault's
/// effective yield rate, accrues simple interest onto total_assets, and
/// enforces the High-regime withdrawal lock by recording the transition slot.
pub fn handler(ctx: Context<AccrueAdaptive>, _strategy_id: u8) -> Result<()> {
    let signal = &ctx.accounts.oracle_signal;
    let now    = Clock::get()?.unix_timestamp;

    // Require a fresh oracle signal (posted within the last 2 hours).
    require!(
        now - signal.posted_at < 7_200,
        AdaptiveVaultError::StaleOracleSignal
    );

    let new_regime = signal.volatility_regime.clone();
    let vault      = &mut ctx.accounts.vault_state;

    // Record slot when vault first enters High regime — activates the withdraw lock.
    if matches!(new_regime, VolatilityRegime::High)
        && !matches!(vault.current_regime, VolatilityRegime::High)
    {
        vault.high_regime_started_at_slot = Clock::get()?.slot;
        msg!("AdaptiveVault: entered High regime at slot {}", vault.high_regime_started_at_slot);
    }
    vault.current_regime = new_regime;

    // Accrue interest since the last call using the vol-adjusted rate.
    let elapsed = now.saturating_sub(vault.last_accrual).max(0) as u64;
    let effective_bps = vault.effective_yield_bps();

    let yield_earned = accrue_simple_interest(vault.total_assets, effective_bps, elapsed)
        .map_err(|_| AdaptiveVaultError::Overflow)?;

    vault.total_assets = vault.total_assets
        .checked_add(yield_earned).ok_or(AdaptiveVaultError::Overflow)?;
    vault.last_accrual = now;

    msg!(
        "AccrueAdaptive: regime={:?} effective_bps={} elapsed_secs={} \
         yield_earned={} total_assets={}",
        vault.current_regime, effective_bps, elapsed,
        yield_earned, vault.total_assets,
    );
    Ok(())
}
