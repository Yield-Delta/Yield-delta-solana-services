use anchor_lang::prelude::*;

use crate::errors::VaultCoreError;
use crate::state::VaultCoreState;

// ---------------------------------------------------------------------------
// Shared admin account struct (pause + unpause reuse the same shape)
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct AdminVaultCore<'info> {
    /// Only the vault authority may call admin instructions.
    pub authority: Signer<'info>,

    /// Core vault state PDA.
    /// Seeds: ["vault_core", strategy_type_byte, token_mint]
    #[account(
        mut,
        seeds = [
            b"vault_core",
            &vault_state.strategy_type.to_seed_byte(),
            vault_state.token_mint.as_ref(),
        ],
        bump = vault_state.bump,
        has_one = authority @ VaultCoreError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultCoreState>,
}

// ---------------------------------------------------------------------------
// Set pause flag
// ---------------------------------------------------------------------------

pub fn handler_pause(ctx: Context<AdminVaultCore>) -> Result<()> {
    let vault = &mut ctx.accounts.vault_state;
    vault.paused = true;
    msg!(
        "VaultCore paused: pda={} strategy={:?}",
        ctx.accounts.vault_state.key(),
        vault.strategy_type,
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Clear pause flag
// ---------------------------------------------------------------------------

pub fn handler_unpause(ctx: Context<AdminVaultCore>) -> Result<()> {
    let vault = &mut ctx.accounts.vault_state;
    vault.paused = false;
    msg!(
        "VaultCore unpaused: pda={} strategy={:?}",
        ctx.accounts.vault_state.key(),
        vault.strategy_type,
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Update fee params — authority can adjust fees post-deploy
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct UpdateFees<'info> {
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [
            b"vault_core",
            &vault_state.strategy_type.to_seed_byte(),
            vault_state.token_mint.as_ref(),
        ],
        bump = vault_state.bump,
        has_one = authority @ VaultCoreError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultCoreState>,
}

pub fn handler_update_fees(
    ctx: Context<UpdateFees>,
    performance_fee_bps: u16,
    management_fee_bps: u16,
) -> Result<()> {
    require!(
        performance_fee_bps <= 10_000,
        VaultCoreError::FeeBpsTooLarge
    );
    require!(
        management_fee_bps <= 10_000,
        VaultCoreError::FeeBpsTooLarge
    );

    let vault = &mut ctx.accounts.vault_state;
    vault.performance_fee_bps = performance_fee_bps;
    vault.management_fee_bps  = management_fee_bps;

    msg!(
        "UpdateFees: perf_bps={} mgmt_bps={}",
        performance_fee_bps,
        management_fee_bps,
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Transfer authority
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct TransferAuthority<'info> {
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [
            b"vault_core",
            &vault_state.strategy_type.to_seed_byte(),
            vault_state.token_mint.as_ref(),
        ],
        bump = vault_state.bump,
        has_one = authority @ VaultCoreError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultCoreState>,

    /// CHECK: New authority is just a Pubkey stored in state.
    pub new_authority: UncheckedAccount<'info>,
}

pub fn handler_transfer_authority(ctx: Context<TransferAuthority>) -> Result<()> {
    let vault = &mut ctx.accounts.vault_state;
    let old = vault.authority;
    vault.authority = ctx.accounts.new_authority.key();
    msg!(
        "TransferAuthority: old={} new={}",
        old,
        ctx.accounts.new_authority.key(),
    );
    Ok(())
}
