use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Burn, Mint, Token, TokenAccount, Transfer},
};
use yield_oracle::state::VolatilityRegime;
use yield_vault_core::math::calculate_assets_for_shares;

use crate::errors::AdaptiveVaultError;
use crate::state::{AdaptiveUserPosition, AdaptiveVaultState};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"adaptive_vault", vault_state.token_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, AdaptiveVaultState>,

    #[account(
        mut,
        constraint = user_token_account.mint  == vault_state.token_mint,
        constraint = user_token_account.owner == user.key(),
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = vault_token_account.key() == vault_state.vault_token_account,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint,
    )]
    pub vault_mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = user_share_account.mint  == vault_state.vault_mint,
        constraint = user_share_account.owner == user.key(),
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"user_position", vault_state.key().as_ref(), user.key().as_ref()],
        bump  = user_position.bump,
        constraint = user_position.owner == user.key(),
    )]
    pub user_position: Account<'info, AdaptiveUserPosition>,

    pub token_program:            Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program:           Program<'info, System>,
}

pub fn handler(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
    require!(shares > 0, AdaptiveVaultError::InvalidShareAmount);

    let vault = &ctx.accounts.vault_state;

    // Slot-lock guard: block withdrawals immediately after entering High regime.
    if matches!(vault.current_regime, VolatilityRegime::High) {
        let current_slot = Clock::get()?.slot;
        let lock_until   = vault.high_regime_started_at_slot
            .saturating_add(vault.lock_slots_high);
        require!(
            current_slot >= lock_until,
            AdaptiveVaultError::WithdrawLockedHighVolatility
        );
    }

    require!(
        ctx.accounts.user_position.shares >= shares,
        AdaptiveVaultError::InsufficientBalance
    );

    let assets_out = calculate_assets_for_shares(shares, vault.total_shares, vault.total_assets)
        .map_err(|_| AdaptiveVaultError::Overflow)?;

    // Burn user's share tokens
    token::burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint:      ctx.accounts.vault_mint.to_account_info(),
                from:      ctx.accounts.user_share_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        shares,
    )?;

    // Transfer underlying tokens: vault ATA → user (vault PDA signs)
    let token_mint_key      = ctx.accounts.vault_state.token_mint;
    let bump                = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]]     = &[b"adaptive_vault", token_mint_key.as_ref(), &[bump]];
    let signer_seeds        = &[seeds];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from:      ctx.accounts.vault_token_account.to_account_info(),
                to:        ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.vault_state.to_account_info(),
            },
            signer_seeds,
        ),
        assets_out,
    )?;

    // Update vault state
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault.total_assets
        .checked_sub(assets_out).ok_or(AdaptiveVaultError::Overflow)?;
    vault.total_shares = vault.total_shares
        .checked_sub(shares).ok_or(AdaptiveVaultError::Overflow)?;

    let pos = &mut ctx.accounts.user_position;
    pos.shares = pos.shares
        .checked_sub(shares).ok_or(AdaptiveVaultError::Overflow)?;

    msg!(
        "AdaptiveWithdraw: user={} shares={} assets_out={} regime={:?}",
        ctx.accounts.user.key(), shares, assets_out, vault.current_regime,
    );
    Ok(())
}
