use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, MintTo, Token, TokenAccount, Transfer},
};

use crate::errors::LpVaultError;
use crate::state::{LpVaultState, UserPosition};
use yield_vault_core::math::calculate_shares_to_mint;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault):    ["lp_vault", lp_token_mint]
// PDA seeds (position): ["user_position", vault_state, user]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// LP vault state PDA.
    #[account(
        mut,
        seeds = [b"lp_vault", vault_state.lp_token_mint.as_ref()],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, LpVaultState>,

    /// User's LP token account — source of LP tokens to deposit.
    #[account(
        mut,
        constraint = user_lp_account.mint == vault_state.lp_token_mint
            @ LpVaultError::Overflow,
        constraint = user_lp_account.owner == user.key()
            @ LpVaultError::Unauthorized,
    )]
    pub user_lp_account: Account<'info, TokenAccount>,

    /// Vault's LP token ATA — receives the deposited tokens.
    #[account(
        mut,
        constraint = vault_lp_account.key() == vault_state.vault_lp_account
            @ LpVaultError::Overflow,
    )]
    pub vault_lp_account: Account<'info, TokenAccount>,

    /// Vault share-token mint.
    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint
            @ LpVaultError::Overflow,
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

    /// Per-user position PDA — created on first deposit.
    #[account(
        init_if_needed,
        payer = user,
        space = UserPosition::LEN,
        seeds = [b"user_position", vault_state.key().as_ref(), user.key().as_ref()],
        bump,
    )]
    pub user_position: Account<'info, UserPosition>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    require!(amount > 0, LpVaultError::InvalidDepositAmount);

    let vault = &ctx.accounts.vault_state;

    // Calculate shares to mint using core math.
    let shares_to_mint = calculate_shares_to_mint(
        amount,
        vault.total_shares,
        vault.total_lp_tokens,
    )
    .map_err(|_| LpVaultError::Overflow)?;

    require!(shares_to_mint > 0, LpVaultError::ZeroSharesMinted);

    // Transfer LP tokens from user → vault ATA.
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_lp_account.to_account_info(),
                to: ctx.accounts.vault_lp_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
    )?;

    // Mint vault share tokens to user (vault PDA signs).
    let lp_mint_key = ctx.accounts.vault_state.lp_token_mint;
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[b"lp_vault", lp_mint_key.as_ref(), &[bump]];
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
    vault.total_lp_tokens = vault
        .total_lp_tokens
        .checked_add(amount)
        .ok_or(LpVaultError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_add(shares_to_mint)
        .ok_or(LpVaultError::Overflow)?;

    // Update (or initialise) user position.
    let pos = &mut ctx.accounts.user_position;
    if pos.owner == Pubkey::default() {
        pos.owner = ctx.accounts.user.key();
        pos.vault = ctx.accounts.vault_state.key();
        pos.bump = ctx.bumps.user_position;
    }
    pos.shares = pos
        .shares
        .checked_add(shares_to_mint)
        .ok_or(LpVaultError::Overflow)?;

    msg!(
        "LpVault Deposit: user={} amount={} shares_minted={} total_lp_tokens={} total_shares={}",
        ctx.accounts.user.key(),
        amount,
        shares_to_mint,
        ctx.accounts.vault_state.total_lp_tokens,
        ctx.accounts.vault_state.total_shares,
    );
    Ok(())
}
