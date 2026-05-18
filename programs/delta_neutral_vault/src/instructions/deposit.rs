use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, MintTo, Token, TokenAccount, Transfer},
};

use crate::errors::DeltaNeutralError;
use crate::state::{compute_delta, DeltaNeutralVaultState};
use yield_vault_core::math::calculate_shares_to_mint;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["delta_neutral", usdc_mint]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// Delta-neutral vault state PDA.
    #[account(
        mut,
        seeds = [b"delta_neutral", vault_state.usdc_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, DeltaNeutralVaultState>,

    /// User's USDC token account — source of USDC to deposit.
    #[account(
        mut,
        constraint = user_usdc_account.mint == vault_state.usdc_mint
            @ DeltaNeutralError::Overflow,
        constraint = user_usdc_account.owner == user.key()
            @ DeltaNeutralError::Unauthorized,
    )]
    pub user_usdc_account: Account<'info, TokenAccount>,

    /// Vault's USDC ATA — receives the deposited tokens.
    #[account(
        mut,
        constraint = vault_usdc_account.key() == vault_state.vault_usdc_account
            @ DeltaNeutralError::Overflow,
    )]
    pub vault_usdc_account: Account<'info, TokenAccount>,

    /// Vault share-token mint.
    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint
            @ DeltaNeutralError::Overflow,
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
    require!(amount > 0, DeltaNeutralError::InvalidDepositAmount);

    let vault = &ctx.accounts.vault_state;

    // Calculate shares to mint using core math.
    let shares_to_mint = calculate_shares_to_mint(
        amount,
        vault.total_shares,
        vault.total_assets,
    )
    .map_err(|_| DeltaNeutralError::Overflow)?;

    require!(shares_to_mint > 0, DeltaNeutralError::ZeroSharesMinted);

    // Transfer USDC from user → vault ATA.
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

    // Mint vault share tokens to user (vault PDA signs).
    let usdc_mint_key = ctx.accounts.vault_state.usdc_mint;
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[b"delta_neutral", usdc_mint_key.as_ref(), &[bump]];
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

    // Update vault state.
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_add(amount)
        .ok_or(DeltaNeutralError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_add(shares_to_mint)
        .ok_or(DeltaNeutralError::Overflow)?;

    // Update long_notional: on devnet long = total_assets.
    vault.long_notional = vault
        .long_notional
        .checked_add(amount)
        .ok_or(DeltaNeutralError::Overflow)?;

    // Recompute delta after the deposit.
    vault.delta_bps = compute_delta(vault.long_notional, vault.short_notional);

    msg!(
        "DeltaNeutralVault Deposit: user={} amount={} shares_minted={} \
         total_assets={} total_shares={} long_notional={} short_notional={} delta_bps={}",
        ctx.accounts.user.key(),
        amount,
        shares_to_mint,
        vault.total_assets,
        vault.total_shares,
        vault.long_notional,
        vault.short_notional,
        vault.delta_bps,
    );
    Ok(())
}
