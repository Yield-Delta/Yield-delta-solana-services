use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

use crate::errors::StakingVaultError;
use crate::state::StakingVaultState;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds: ["staking_vault"]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Staking vault state PDA.
    #[account(
        init,
        payer = authority,
        space = StakingVaultState::LEN,
        seeds = [b"staking_vault"],
        bump,
    )]
    pub vault_state: Account<'info, StakingVaultState>,

    /// stSOL share-token mint — vault PDA is the mint authority.
    /// Decimals = 9 to match SOL lamport precision.
    #[account(
        init,
        payer = authority,
        mint::decimals = 9,
        mint::authority = vault_state,
        mint::freeze_authority = vault_state,
    )]
    pub vault_mint: Account<'info, Mint>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<Initialize>, yield_bps: u16) -> Result<()> {
    require!(yield_bps <= 10_000, StakingVaultError::InvalidYieldBps);

    let now = Clock::get()?.unix_timestamp;

    let vault = &mut ctx.accounts.vault_state;
    vault.authority      = ctx.accounts.authority.key();
    vault.vault_mint     = ctx.accounts.vault_mint.key();
    vault.total_sol_staked = 0;
    vault.total_shares   = 0;
    vault.yield_bps      = yield_bps;
    vault.last_accrual   = now;
    vault.bump           = ctx.bumps.vault_state;

    msg!(
        "StakingVault initialized: vault_pda={} share_mint={} yield_bps={}",
        ctx.accounts.vault_state.key(),
        ctx.accounts.vault_mint.key(),
        yield_bps,
    );
    Ok(())
}
