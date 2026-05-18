use anchor_lang::prelude::*;

use crate::errors::DeltaNeutralError;
use crate::state::DeltaNeutralVaultState;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["delta_neutral", usdc_mint]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct RebalanceHedge<'info> {
    /// Only the vault authority (AI keeper) may rebalance the hedge.
    #[account(
        constraint = authority.key() == vault_state.authority
            @ DeltaNeutralError::Unauthorized
    )]
    pub authority: Signer<'info>,

    /// Delta-neutral vault state PDA.
    #[account(
        mut,
        seeds = [b"delta_neutral", vault_state.usdc_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, DeltaNeutralVaultState>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<RebalanceHedge>) -> Result<()> {
    let vault = &ctx.accounts.vault_state;
    let current_delta = vault.delta_bps;
    let threshold = vault.rebalance_threshold_bps as i64;

    // Emit a warning log if the delta doesn't exceed the threshold.
    // On devnet we do not hard-fail so the keeper can always rebalance.
    if current_delta.abs() <= threshold {
        msg!(
            "RebalanceHedge: delta_bps={} is within threshold={} — rebalancing anyway (devnet permissive)",
            current_delta,
            threshold,
        );
    }

    let now = Clock::get()?.unix_timestamp;
    let long_before = vault.long_notional;
    let short_before = vault.short_notional;

    // Perfect hedge: set short_notional = long_notional → delta_bps = 0.
    let vault = &mut ctx.accounts.vault_state;
    vault.short_notional = vault.long_notional;
    vault.delta_bps      = 0;
    vault.last_rebalance = now;

    msg!(
        "RebalanceHedge: authority={} long_before={} short_before={} short_after={} \
         delta_bps_before={} delta_bps_after=0 timestamp={}",
        ctx.accounts.authority.key(),
        long_before,
        short_before,
        vault.short_notional,
        current_delta,
        now,
    );
    Ok(())
}
