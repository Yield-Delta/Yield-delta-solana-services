use anchor_lang::prelude::*;

use crate::errors::StakingVaultError;
use crate::state::StakingVaultState;
use yield_vault_core::math::accrue_simple_interest;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["staking_vault"]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct AccrueYield<'info> {
    /// Only the vault authority may trigger yield accrual.
    #[account(
        constraint = authority.key() == vault_state.authority
            @ StakingVaultError::Unauthorized
    )]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [b"staking_vault"],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, StakingVaultState>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Simulates yield accrual on devnet.
///
/// Computes the interest earned since `last_accrual` using simple interest:
///
///   yield_earned = total_sol_staked × yield_bps × elapsed_seconds
///                  / (10_000 × SECONDS_PER_YEAR)
///
/// Adds `yield_earned` to `total_sol_staked` and updates `last_accrual`.
/// This increases the stSOL/SOL exchange rate, reflecting staking rewards.
pub fn handler(ctx: Context<AccrueYield>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let vault = &ctx.accounts.vault_state;

    // Guard against clock going backwards (should not happen on-chain).
    let elapsed = if now > vault.last_accrual {
        (now - vault.last_accrual) as u64
    } else {
        0u64
    };

    let yield_earned = accrue_simple_interest(
        vault.total_sol_staked,
        vault.yield_bps,
        elapsed,
    )
    .map_err(|_| StakingVaultError::Overflow)?;

    let vault = &mut ctx.accounts.vault_state;
    vault.total_sol_staked = vault
        .total_sol_staked
        .checked_add(yield_earned)
        .ok_or(StakingVaultError::Overflow)?;
    vault.last_accrual = now;

    msg!(
        "AccrueYield: elapsed_secs={} yield_earned={} total_sol_staked={} yield_bps={}",
        elapsed,
        yield_earned,
        vault.total_sol_staked,
        vault.yield_bps,
    );
    Ok(())
}
