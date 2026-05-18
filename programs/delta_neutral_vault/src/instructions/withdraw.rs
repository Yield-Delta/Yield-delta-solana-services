use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Burn, Mint, Token, TokenAccount, Transfer},
};

use crate::errors::DeltaNeutralError;
use crate::state::{compute_delta, DeltaNeutralVaultState};
use yield_vault_core::math::calculate_assets_for_shares;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["delta_neutral", usdc_mint]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// Delta-neutral vault state PDA.
    #[account(
        mut,
        seeds = [b"delta_neutral", vault_state.usdc_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, DeltaNeutralVaultState>,

    /// User's USDC token account — receives redeemed USDC.
    #[account(
        mut,
        constraint = user_usdc_account.mint == vault_state.usdc_mint
            @ DeltaNeutralError::Overflow,
        constraint = user_usdc_account.owner == user.key()
            @ DeltaNeutralError::Unauthorized,
    )]
    pub user_usdc_account: Account<'info, TokenAccount>,

    /// Vault's USDC ATA — source of USDC to return.
    #[account(
        mut,
        constraint = vault_usdc_account.key() == vault_state.vault_usdc_account
            @ DeltaNeutralError::Overflow,
    )]
    pub vault_usdc_account: Account<'info, TokenAccount>,

    /// Vault share-token mint — for burning.
    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint
            @ DeltaNeutralError::Overflow,
    )]
    pub vault_mint: Account<'info, Mint>,

    /// User's vault share token ATA — shares burned from here.
    #[account(
        mut,
        constraint = user_share_account.mint == vault_state.vault_mint
            @ DeltaNeutralError::Overflow,
        constraint = user_share_account.owner == user.key()
            @ DeltaNeutralError::Unauthorized,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
    require!(shares > 0, DeltaNeutralError::InvalidWithdrawShares);

    let vault = &ctx.accounts.vault_state;

    require!(
        ctx.accounts.user_share_account.amount >= shares,
        DeltaNeutralError::InsufficientShares
    );

    // USDC to return: shares * total_assets / total_shares.
    let usdc_out = calculate_assets_for_shares(
        shares,
        vault.total_shares,
        vault.total_assets,
    )
    .map_err(|_| DeltaNeutralError::Overflow)?;

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

    // Transfer USDC from vault ATA → user (vault PDA signs).
    let usdc_mint_key = ctx.accounts.vault_state.usdc_mint;
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[b"delta_neutral", usdc_mint_key.as_ref(), &[bump]];
    let signer_seeds = &[seeds];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault_usdc_account.to_account_info(),
                to: ctx.accounts.user_usdc_account.to_account_info(),
                authority: ctx.accounts.vault_state.to_account_info(),
            },
            signer_seeds,
        ),
        usdc_out,
    )?;

    // Reduce long_notional proportionally to the share of assets withdrawn.
    let vault = &ctx.accounts.vault_state;
    let total_assets = vault.total_assets;
    let long_notional = vault.long_notional;
    let short_notional = vault.short_notional;

    let long_reduction = if total_assets > 0 {
        (long_notional as u128)
            .checked_mul(usdc_out as u128)
            .ok_or(DeltaNeutralError::Overflow)?
            .checked_div(total_assets as u128)
            .ok_or(DeltaNeutralError::Overflow)? as u64
    } else {
        0
    };

    let short_reduction = if total_assets > 0 {
        (short_notional as u128)
            .checked_mul(usdc_out as u128)
            .ok_or(DeltaNeutralError::Overflow)?
            .checked_div(total_assets as u128)
            .ok_or(DeltaNeutralError::Overflow)? as u64
    } else {
        0
    };

    // Update vault state.
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_sub(usdc_out)
        .ok_or(DeltaNeutralError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_sub(shares)
        .ok_or(DeltaNeutralError::Overflow)?;
    vault.long_notional = vault
        .long_notional
        .checked_sub(long_reduction)
        .ok_or(DeltaNeutralError::Overflow)?;
    vault.short_notional = vault
        .short_notional
        .checked_sub(short_reduction)
        .ok_or(DeltaNeutralError::Overflow)?;

    // Recompute delta after the withdrawal.
    vault.delta_bps = compute_delta(vault.long_notional, vault.short_notional);

    msg!(
        "DeltaNeutralVault Withdraw: user={} shares={} usdc_out={} \
         total_assets={} total_shares={} long_notional={} short_notional={} delta_bps={}",
        ctx.accounts.user.key(),
        shares,
        usdc_out,
        vault.total_assets,
        vault.total_shares,
        vault.long_notional,
        vault.short_notional,
        vault.delta_bps,
    );
    Ok(())
}
