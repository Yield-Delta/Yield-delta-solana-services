use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Burn, Mint, Token, TokenAccount, Transfer},
};

use crate::errors::MetaVaultError;
use crate::state::MetaVaultState;
use yield_vault_core::fees::calculate_management_fee;
use yield_vault_core::math::calculate_assets_for_shares;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["meta_vault"]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// Meta vault state PDA.
    #[account(
        mut,
        seeds = [b"meta_vault"],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, MetaVaultState>,

    /// User's USDC token account — receives redeemed USDC.
    #[account(
        mut,
        constraint = user_usdc_account.mint == vault_state.usdc_mint
            @ MetaVaultError::Overflow,
        constraint = user_usdc_account.owner == user.key()
            @ MetaVaultError::Unauthorized,
    )]
    pub user_usdc_account: Account<'info, TokenAccount>,

    /// Vault's USDC ATA — source of USDC to return.
    #[account(
        mut,
        constraint = vault_usdc_account.key() == vault_state.vault_usdc_account
            @ MetaVaultError::Overflow,
    )]
    pub vault_usdc_account: Account<'info, TokenAccount>,

    /// Vault share-token mint — for burning.
    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint
            @ MetaVaultError::Overflow,
    )]
    pub vault_mint: Account<'info, Mint>,

    /// User's vault share token ATA — shares burned from here.
    #[account(
        mut,
        constraint = user_share_account.mint == vault_state.vault_mint
            @ MetaVaultError::Overflow,
        constraint = user_share_account.owner == user.key()
            @ MetaVaultError::Unauthorized,
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
    require!(shares > 0, MetaVaultError::InvalidWithdrawShares);

    require!(
        ctx.accounts.user_share_account.amount >= shares,
        MetaVaultError::InsufficientShares
    );

    let now = Clock::get()?.unix_timestamp;

    // Step 1: accrue management fee for elapsed time.
    {
        let vault = &mut ctx.accounts.vault_state;
        let elapsed_secs = if now > vault.last_fee_accrual {
            (now - vault.last_fee_accrual) as u64
        } else {
            0u64
        };

        if elapsed_secs > 0 && vault.total_assets > 0 {
            let fee = calculate_management_fee(vault.total_assets, elapsed_secs)
                .map_err(|_| MetaVaultError::Overflow)?;
            vault.accumulated_fees = vault
                .accumulated_fees
                .checked_add(fee)
                .ok_or(MetaVaultError::Overflow)?;
        }
        vault.last_fee_accrual = now;
    }

    // Step 2: calculate USDC out using calculate_assets_for_shares.
    let vault = &ctx.accounts.vault_state;
    let usdc_out = calculate_assets_for_shares(
        shares,
        vault.total_shares,
        vault.total_assets,
    )
    .map_err(|_| MetaVaultError::Overflow)?;

    // Step 3: burn vault share tokens from user.
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

    // Step 4: transfer USDC from vault ATA → user (vault PDA signs).
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[b"meta_vault", &[bump]];
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

    // Step 5: update vault state.
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_sub(usdc_out)
        .ok_or(MetaVaultError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_sub(shares)
        .ok_or(MetaVaultError::Overflow)?;

    msg!(
        "MetaVault Withdraw: user={} shares={} usdc_out={} \
         total_assets={} total_shares={} accumulated_fees={}",
        ctx.accounts.user.key(),
        shares,
        usdc_out,
        vault.total_assets,
        vault.total_shares,
        vault.accumulated_fees,
    );
    Ok(())
}
