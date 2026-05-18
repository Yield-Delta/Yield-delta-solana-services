use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, MintTo, Token, TokenAccount, Transfer},
};

use crate::errors::MetaVaultError;
use crate::state::MetaVaultState;
use yield_vault_core::fees::calculate_management_fee;
use yield_vault_core::math::calculate_shares_to_mint;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["meta_vault"]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// Meta vault state PDA.
    #[account(
        mut,
        seeds = [b"meta_vault"],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, MetaVaultState>,

    /// User's USDC token account — source of USDC to deposit.
    #[account(
        mut,
        constraint = user_usdc_account.mint == vault_state.usdc_mint
            @ MetaVaultError::Overflow,
        constraint = user_usdc_account.owner == user.key()
            @ MetaVaultError::Unauthorized,
    )]
    pub user_usdc_account: Account<'info, TokenAccount>,

    /// Vault's USDC ATA — receives the deposited tokens.
    #[account(
        mut,
        constraint = vault_usdc_account.key() == vault_state.vault_usdc_account
            @ MetaVaultError::Overflow,
    )]
    pub vault_usdc_account: Account<'info, TokenAccount>,

    /// Vault share-token mint.
    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint
            @ MetaVaultError::Overflow,
    )]
    pub vault_mint: Account<'info, Mint>,

    /// User's vault share token ATA — created if it doesn't exist.
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = vault_mint,
        associated_token::authority = user,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    require!(amount > 0, MetaVaultError::InvalidDepositAmount);

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

    // Step 2: calculate shares using net assets (total_assets - accumulated_fees).
    let vault = &ctx.accounts.vault_state;
    let net_assets = vault
        .total_assets
        .saturating_sub(vault.accumulated_fees);

    let shares_to_mint = calculate_shares_to_mint(
        amount,
        vault.total_shares,
        net_assets,
    )
    .map_err(|_| MetaVaultError::Overflow)?;

    require!(shares_to_mint > 0, MetaVaultError::ZeroSharesMinted);

    // Step 3: transfer USDC from user → vault ATA.
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_usdc_account.to_account_info(),
                to: ctx.accounts.vault_usdc_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
    )?;

    // Step 4: mint vault share tokens to user (vault PDA signs).
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[b"meta_vault", &[bump]];
    let signer_seeds = &[seeds];

    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.vault_mint.to_account_info(),
                to: ctx.accounts.user_share_account.to_account_info(),
                authority: ctx.accounts.vault_state.to_account_info(),
            },
            signer_seeds,
        ),
        shares_to_mint,
    )?;

    // Step 5: update vault state.
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_add(amount)
        .ok_or(MetaVaultError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_add(shares_to_mint)
        .ok_or(MetaVaultError::Overflow)?;

    msg!(
        "MetaVault Deposit: user={} amount={} shares_minted={} \
         total_assets={} total_shares={} accumulated_fees={}",
        ctx.accounts.user.key(),
        amount,
        shares_to_mint,
        vault.total_assets,
        vault.total_shares,
        vault.accumulated_fees,
    );
    Ok(())
}
