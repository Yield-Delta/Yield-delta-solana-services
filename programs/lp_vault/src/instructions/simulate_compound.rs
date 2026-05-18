use anchor_lang::prelude::*;

use crate::errors::LpVaultError;
use crate::state::LpVaultState;
use yield_vault_core::math::apply_bps;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["lp_vault", lp_token_mint]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct SimulateCompound<'info> {
    /// Only the vault authority may trigger compounding.
    #[account(
        constraint = authority.key() == vault_state.authority
            @ LpVaultError::Unauthorized
    )]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [b"lp_vault", vault_state.lp_token_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, LpVaultState>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// Simulates fee compounding on devnet.
///
/// Adds `compound_fee_bps` basis points of `total_lp_tokens` as additional
/// LP tokens to the vault, simulating AMM fees being reinvested.  This
/// increases the LP token per share ratio, rewarding all current shareholders.
pub fn handler(ctx: Context<SimulateCompound>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    let vault = &ctx.accounts.vault_state;

    // compound_tokens = total_lp_tokens * compound_fee_bps / 10_000
    let compound_tokens = apply_bps(vault.total_lp_tokens, vault.compound_fee_bps)
        .map_err(|_| LpVaultError::Overflow)?;

    let vault = &mut ctx.accounts.vault_state;
    vault.total_lp_tokens = vault
        .total_lp_tokens
        .checked_add(compound_tokens)
        .ok_or(LpVaultError::Overflow)?;
    vault.last_compound = now;

    msg!(
        "LpVault SimulateCompound: compound_tokens={} total_lp_tokens={} compound_fee_bps={} timestamp={}",
        compound_tokens,
        vault.total_lp_tokens,
        vault.compound_fee_bps,
        now,
    );
    Ok(())
}
