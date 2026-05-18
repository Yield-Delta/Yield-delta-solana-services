use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use crate::errors::MetaVaultError;
use crate::state::{AllocationSlot, MetaVaultState, MAX_ALLOCATIONS};

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds: ["meta_vault"]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// USDC mint — the deposit token.
    pub usdc_mint: Account<'info, Mint>,

    /// Meta vault state PDA.
    #[account(
        init,
        payer = authority,
        space = MetaVaultState::LEN,
        seeds = [b"meta_vault"],
        bump,
    )]
    pub vault_state: Account<'info, MetaVaultState>,

    /// Vault share-token mint — vault PDA is the mint authority.
    #[account(
        init,
        payer = authority,
        mint::decimals = 6,
        mint::authority = vault_state,
        mint::freeze_authority = vault_state,
    )]
    pub vault_mint: Account<'info, Mint>,

    /// Vault's USDC associated token account.
    #[account(
        init,
        payer = authority,
        associated_token::mint = usdc_mint,
        associated_token::authority = vault_state,
    )]
    pub vault_usdc_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<Initialize>, fee_bps: u16) -> Result<()> {
    require!(fee_bps <= 10_000, MetaVaultError::InvalidFeeBps);

    let now = Clock::get()?.unix_timestamp;

    let vault = &mut ctx.accounts.vault_state;
    vault.authority          = ctx.accounts.authority.key();
    vault.usdc_mint          = ctx.accounts.usdc_mint.key();
    vault.vault_mint         = ctx.accounts.vault_mint.key();
    vault.vault_usdc_account = ctx.accounts.vault_usdc_account.key();
    vault.total_shares       = 0;
    vault.total_assets       = 0;
    vault.fee_bps            = fee_bps;
    vault.accumulated_fees   = 0;
    vault.last_fee_accrual   = now;
    vault.last_rebalance     = now;
    vault.allocation_count   = 0;
    vault.high_water_mark    = 0;
    vault.bump               = ctx.bumps.vault_state;

    msg!(
        "MetaVault initialized: vault_pda={} usdc_mint={} share_mint={} fee_bps={}",
        ctx.accounts.vault_state.key(),
        ctx.accounts.usdc_mint.key(),
        ctx.accounts.vault_mint.key(),
        fee_bps,
    );
    Ok(())
}

/// Validate that active allocation slots have weights summing to 10_000
/// and no duplicate strategy_ids.
pub fn validate_allocations(allocations: &[AllocationSlot]) -> Result<()> {
    require!(allocations.len() <= MAX_ALLOCATIONS, MetaVaultError::TooManyAllocations);

    let mut weight_sum: u32 = 0;
    let mut seen = [0u8; MAX_ALLOCATIONS];
    let mut count = 0usize;

    for slot in allocations.iter().filter(|s| s.strategy_id != 0) {
        weight_sum = weight_sum
            .checked_add(slot.weight_bps as u32)
            .ok_or(error!(MetaVaultError::Overflow))?;

        for i in 0..count {
            require!(seen[i] != slot.strategy_id, MetaVaultError::InvalidAllocationWeights);
        }
        seen[count] = slot.strategy_id;
        count += 1;
    }

    require!(weight_sum == 10_000, MetaVaultError::InvalidAllocationWeights);
    Ok(())
}
