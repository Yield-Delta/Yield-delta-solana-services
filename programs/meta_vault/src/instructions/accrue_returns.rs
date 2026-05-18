use anchor_lang::prelude::*;

use crate::errors::MetaVaultError;
use crate::state::MetaVaultState;
use yield_vault_core::math::accrue_simple_interest;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["meta_vault"]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct AccrueReturns<'info> {
    /// Only the vault authority (AI keeper) may trigger yield accrual.
    #[account(
        constraint = authority.key() == vault_state.authority
            @ MetaVaultError::Unauthorized
    )]
    pub authority: Signer<'info>,

    /// Meta vault state PDA.
    #[account(
        mut,
        seeds = [b"meta_vault"],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, MetaVaultState>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<AccrueReturns>) -> Result<()> {
    let vault = &ctx.accounts.vault_state;
    let now = Clock::get()?.unix_timestamp;

    // Elapsed seconds since last rebalance (clamped to 0 to avoid negative).
    let elapsed_secs = if now > vault.last_rebalance {
        (now - vault.last_rebalance) as u64
    } else {
        0u64
    };

    // Compute blended APY in bps:
    //   blended_bps = Σ (weight_bps × simulated_apy_bps) / 10_000
    let mut blended_numerator: u64 = 0;
    for slot in vault.allocations.iter() {
        if slot.strategy_id == 0 {
            continue;
        }
        let contribution = (slot.weight_bps as u64)
            .checked_mul(slot.simulated_apy_bps as u64)
            .ok_or(MetaVaultError::Overflow)?;
        blended_numerator = blended_numerator
            .checked_add(contribution)
            .ok_or(MetaVaultError::Overflow)?;
    }
    // Divide by 10_000 to get the effective blended bps.
    let blended_bps = blended_numerator
        .checked_div(10_000)
        .ok_or(MetaVaultError::Overflow)? as u16;

    // Accrue interest on total_assets using core math.
    let yield_earned = accrue_simple_interest(
        vault.total_assets,
        blended_bps,
        elapsed_secs,
    )
    .map_err(|_| MetaVaultError::Overflow)?;

    msg!(
        "MetaVault AccrueReturns: blended_bps={} elapsed_secs={} \
         total_assets_before={} yield_earned={}",
        blended_bps,
        elapsed_secs,
        ctx.accounts.vault_state.total_assets,
        yield_earned,
    );

    // Update vault state.
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_add(yield_earned)
        .ok_or(MetaVaultError::Overflow)?;
    vault.last_rebalance = now;

    msg!(
        "MetaVault AccrueReturns complete: total_assets_after={} timestamp={}",
        ctx.accounts.vault_state.total_assets,
        now,
    );
    Ok(())
}
