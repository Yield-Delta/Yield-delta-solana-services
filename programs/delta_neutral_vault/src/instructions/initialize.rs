use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use crate::errors::DeltaNeutralError;
use crate::state::DeltaNeutralVaultState;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds: ["delta_neutral", usdc_mint]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// USDC mint — the deposit token.
    pub usdc_mint: Account<'info, Mint>,

    /// Delta-neutral vault state PDA.
    #[account(
        init,
        payer = authority,
        space = DeltaNeutralVaultState::LEN,
        seeds = [b"delta_neutral", usdc_mint.key().as_ref()],
        bump,
    )]
    pub vault_state: Account<'info, DeltaNeutralVaultState>,

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

pub fn handler(ctx: Context<Initialize>, rebalance_threshold_bps: u16) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    let vault = &mut ctx.accounts.vault_state;
    vault.authority               = ctx.accounts.authority.key();
    vault.usdc_mint               = ctx.accounts.usdc_mint.key();
    vault.vault_mint              = ctx.accounts.vault_mint.key();
    vault.vault_usdc_account      = ctx.accounts.vault_usdc_account.key();
    vault.total_shares            = 0;
    vault.total_assets            = 0;
    vault.long_notional           = 0;
    vault.short_notional          = 0;
    vault.delta_bps               = 0;
    vault.rebalance_threshold_bps = rebalance_threshold_bps;
    vault.last_rebalance          = now;
    vault.bump                    = ctx.bumps.vault_state;

    msg!(
        "DeltaNeutralVault initialized: vault_pda={} usdc_mint={} share_mint={} threshold_bps={}",
        ctx.accounts.vault_state.key(),
        ctx.accounts.usdc_mint.key(),
        ctx.accounts.vault_mint.key(),
        rebalance_threshold_bps,
    );
    Ok(())
}
