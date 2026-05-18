use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Burn, Mint, Token, TokenAccount, Transfer},
};

use crate::errors::VaultError;
use crate::state::{UserPosition, VaultState};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault", vault_state.token_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        constraint = user_token_account.mint == vault_state.token_mint,
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
        constraint = user_share_account.mint == vault_state.vault_mint,
        constraint = user_share_account.owner == user.key(),
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"user_position", vault_state.key().as_ref(), user.key().as_ref()],
        bump = user_position.bump,
        constraint = user_position.owner == user.key(),
    )]
    pub user_position: Account<'info, UserPosition>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
    require!(shares > 0, VaultError::InvalidWithdrawShares);
    require!(
        ctx.accounts.user_position.shares >= shares,
        VaultError::InsufficientShares
    );

    let vault = &ctx.accounts.vault_state;

    // tokens_out = shares * total_assets / total_shares
    let tokens_out: u64 = (shares as u128)
        .checked_mul(vault.total_assets as u128)
        .ok_or(VaultError::Overflow)?
        .checked_div(vault.total_shares as u128)
        .ok_or(VaultError::Overflow)? as u64;

    // Burn share tokens from user
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

    // Transfer deposit tokens from vault to user (vault PDA signs)
    let token_mint_key = ctx.accounts.vault_state.token_mint;
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[b"vault", token_mint_key.as_ref(), &[bump]];
    let signer_seeds = &[seeds];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault_token_account.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.vault_state.to_account_info(),
            },
            signer_seeds,
        ),
        tokens_out,
    )?;

    // Update vault totals
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_sub(tokens_out)
        .ok_or(VaultError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_sub(shares)
        .ok_or(VaultError::Overflow)?;

    // Update user position
    ctx.accounts.user_position.shares = ctx
        .accounts
        .user_position
        .shares
        .checked_sub(shares)
        .ok_or(VaultError::Overflow)?;

    msg!(
        "Withdraw: user={} shares={} tokens_out={} total_assets={} total_shares={}",
        ctx.accounts.user.key(),
        shares,
        tokens_out,
        ctx.accounts.vault_state.total_assets,
        ctx.accounts.vault_state.total_shares,
    );
    Ok(())
}
