use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};
use yield_oracle::state::{SignalAccount, VolatilityRegime};

use crate::state::AdaptiveVaultState;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    pub token_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        space = AdaptiveVaultState::LEN,
        seeds = [b"adaptive_vault", token_mint.key().as_ref()],
        bump,
    )]
    pub vault_state: Account<'info, AdaptiveVaultState>,

    #[account(
        init,
        payer = authority,
        mint::decimals = token_mint.decimals,
        mint::authority = vault_state,
    )]
    pub vault_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = token_mint,
        associated_token::authority = vault_state,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    /// The yield_oracle SignalAccount this vault will watch for regime data.
    pub oracle_signal: Account<'info, SignalAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<Initialize>,
    base_yield_bps: u16,
    low_mult_bps: u16,
    high_mult_bps: u16,
    lock_slots_high: u64,
) -> Result<()> {
    let vault = &mut ctx.accounts.vault_state;
    vault.authority                   = ctx.accounts.authority.key();
    vault.token_mint                  = ctx.accounts.token_mint.key();
    vault.vault_mint                  = ctx.accounts.vault_mint.key();
    vault.vault_token_account         = ctx.accounts.vault_token_account.key();
    vault.oracle_signal               = ctx.accounts.oracle_signal.key();
    vault.total_shares                = 0;
    vault.total_assets                = 0;
    vault.base_yield_bps              = base_yield_bps;
    vault.low_mult_bps                = low_mult_bps;
    vault.high_mult_bps               = high_mult_bps;
    vault.lock_slots_high             = lock_slots_high;
    vault.high_regime_started_at_slot = 0;
    vault.current_regime              = VolatilityRegime::Medium;
    vault.last_accrual                = Clock::get()?.unix_timestamp;
    vault.bump                        = ctx.bumps.vault_state;

    msg!(
        "AdaptiveVault initialized: base_bps={} low_mult={} high_mult={} lock_slots={}",
        base_yield_bps, low_mult_bps, high_mult_bps, lock_slots_high,
    );
    Ok(())
}
