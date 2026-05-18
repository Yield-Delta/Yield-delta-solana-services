use anchor_lang::prelude::*;

use crate::errors::MetaVaultError;
use crate::state::{AllocationSlot, MetaVaultState, MAX_ALLOCATIONS};
use super::initialize::validate_allocations;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["meta_vault"]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct SetAllocations<'info> {
    /// Only the vault authority (AI keeper) may set allocations.
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

pub fn handler(
    ctx: Context<SetAllocations>,
    new_allocations: Vec<AllocationSlot>,
) -> Result<()> {
    require!(
        new_allocations.len() <= MAX_ALLOCATIONS,
        MetaVaultError::TooManyAllocations
    );

    // Validate: weights sum to 10_000, no duplicate strategy_ids.
    validate_allocations(&new_allocations)?;

    let now = Clock::get()?.unix_timestamp;

    let vault = &mut ctx.accounts.vault_state;

    // Reset the allocation array to default (zero-fill).
    vault.allocations = Default::default();

    // Write the new allocations.
    for (i, slot) in new_allocations.iter().enumerate() {
        vault.allocations[i] = slot.clone();
    }

    vault.last_rebalance = now;

    msg!(
        "MetaVault SetAllocations: authority={} num_slots={} timestamp={}",
        ctx.accounts.authority.key(),
        new_allocations.len(),
        now,
    );

    // Log each active slot.
    for slot in new_allocations.iter().filter(|s| s.strategy_id != 0) {
        msg!(
            "  strategy_id={} weight_bps={} simulated_apy_bps={}",
            slot.strategy_id,
            slot.weight_bps,
            slot.simulated_apy_bps,
        );
    }

    Ok(())
}
