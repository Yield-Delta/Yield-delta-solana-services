use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Burn, Mint, Token, TokenAccount, Transfer},
};

use crate::errors::LpVaultError;
use crate::state::{LpVaultState, UserPosition};
use yield_vault_core::math::calculate_assets_for_shares;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault):    ["lp_vault", lp_token_mint]
// PDA seeds (position): ["user_position", vault_state, user]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// LP vault state PDA.
    #[account(
        mut,
        seeds = [b"lp_vault", vault_state.lp_token_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, LpVaultState>,

    /// User's LP token account — receives redeemed LP tokens.
    #[account(
        mut,
        constraint = user_lp_account.mint == vault_state.lp_token_mint
            @ LpVaultError::Overflow,
        constraint = user_lp_account.owner == user.key()
            @ LpVaultError::Unauthorized,
    )]
    pub user_lp_account: Account<'info, TokenAccount>,

    /// Vault's LP token ATA — source of LP tokens to return.
    #[account(
        mut,
        constraint = vault_lp_account.key() == vault_state.vault_lp_account
            @ LpVaultError::Overflow,
    )]
    pub vault_lp_account: Account<'info, TokenAccount>,

    /// Vault share-token mint — for burning.
    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint
            @ LpVaultError::Overflow,
    )]
    pub vault_mint: Account<'info, Mint>,

    /// User's vault share token ATA — shares burned from here.
    #[account(
        mut,
        constraint = user_share_account.mint == vault_state.vault_mint
            @ LpVaultError::Overflow,
        constraint = user_share_account.owner == user.key()
            @ LpVaultError::Unauthorized,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    /// Per-user position PDA.
    #[account(
        mut,
        seeds = [b"user_position", vault_state.key().as_ref(), user.key().as_ref()],
        bump = user_position.bump,
        constraint = user_position.owner == user.key()
            @ LpVaultError::Unauthorized,
    )]
    pub user_position: Account<'info, UserPosition>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
    require!(shares > 0, LpVaultError::InvalidWithdrawShares);
    require!(
        ctx.accounts.user_position.shares >= shares,
        LpVaultError::InsufficientShares
    );

    let vault = &ctx.accounts.vault_state;

    // LP tokens to return: shares * total_lp_tokens / total_shares.
    let lp_tokens_out = calculate_assets_for_shares(
        shares,
        vault.total_shares,
        vault.total_lp_tokens,
    )
    .map_err(|_| LpVaultError::Overflow)?;

    // Burn vault share tokens from user.
    token::burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint: ctx.accounts.vault_mint.to_account_info(),
                from: ctx.accounts.user_share_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        shares,
    )?;

    // Transfer LP tokens from vault ATA → user (vault PDA signs).
    let lp_mint_key = ctx.accounts.vault_state.lp_token_mint;
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[b"lp_vault", lp_mint_key.as_ref(), &[bump]];
    let signer_seeds = &[seeds];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault_lp_account.to_account_info(),
                to: ctx.accounts.user_lp_account.to_account_info(),
                authority: ctx.accounts.vault_state.to_account_info(),
            },
            signer_seeds,
        ),
        lp_tokens_out,
    )?;

    // Update vault state.
    let vault = &mut ctx.accounts.vault_state;
    vault.total_lp_tokens = vault
        .total_lp_tokens
        .checked_sub(lp_tokens_out)
        .ok_or(LpVaultError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_sub(shares)
        .ok_or(LpVaultError::Overflow)?;

    // Update user position.
    ctx.accounts.user_position.shares = ctx
        .accounts
        .user_position
        .shares
        .checked_sub(shares)
        .ok_or(LpVaultError::Overflow)?;

    msg!(
        "LpVault Withdraw: user={} shares={} lp_tokens_out={} total_lp_tokens={} total_shares={}",
        ctx.accounts.user.key(),
        shares,
        lp_tokens_out,
        ctx.accounts.vault_state.total_lp_tokens,
        ctx.accounts.vault_state.total_shares,
    );
    Ok(())
}
