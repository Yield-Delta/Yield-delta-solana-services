use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use crate::errors::LpVaultError;
use crate::state::LpVaultState;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds: ["lp_vault", lp_token_mint]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The mock LP token mint users will deposit.
    pub lp_token_mint: Account<'info, Mint>,

    /// LP vault state PDA.
    #[account(
        init,
        payer = authority,
        space = LpVaultState::LEN,
        seeds = [b"lp_vault", lp_token_mint.key().as_ref()],
        bump,
    )]
    pub vault_state: Account<'info, LpVaultState>,

    /// Vault share-token mint — vault PDA is mint authority.
    #[account(
        init,
        payer = authority,
        mint::decimals = lp_token_mint.decimals,
        mint::authority = vault_state,
        mint::freeze_authority = vault_state,
    )]
    pub vault_mint: Account<'info, Mint>,

    /// Vault's ATA for LP tokens.
    #[account(
        init,
        payer = authority,
        associated_token::mint = lp_token_mint,
        associated_token::authority = vault_state,
    )]
    pub vault_lp_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<Initialize>, compound_fee_bps: u16) -> Result<()> {
    require!(compound_fee_bps <= 10_000, LpVaultError::InvalidCompoundFeeBps);

    let now = Clock::get()?.unix_timestamp;

    let vault = &mut ctx.accounts.vault_state;
    vault.authority       = ctx.accounts.authority.key();
    vault.lp_token_mint   = ctx.accounts.lp_token_mint.key();
    vault.vault_mint      = ctx.accounts.vault_mint.key();
    vault.vault_lp_account = ctx.accounts.vault_lp_account.key();
    vault.total_shares    = 0;
    vault.total_lp_tokens = 0;
    vault.compound_fee_bps = compound_fee_bps;
    vault.last_compound   = now;
    vault.bump            = ctx.bumps.vault_state;

    msg!(
        "LpVault initialized: lp_mint={} share_mint={} vault_pda={} compound_fee_bps={}",
        ctx.accounts.lp_token_mint.key(),
        ctx.accounts.vault_mint.key(),
        ctx.accounts.vault_state.key(),
        compound_fee_bps,
    );
    Ok(())
}
