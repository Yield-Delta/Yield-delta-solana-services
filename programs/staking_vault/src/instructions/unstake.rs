use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Burn, Mint, Token, TokenAccount},
};

use crate::errors::StakingVaultError;
use crate::state::{StakingVaultState, UserPosition};
use yield_vault_core::math::calculate_assets_for_shares;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["staking_vault"]
// PDA seeds (position): ["user_position", vault_state, user]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// Staking vault state PDA.
    #[account(
        mut,
        seeds = [b"staking_vault"],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, StakingVaultState>,

    /// stSOL share-token mint.
    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint
            @ StakingVaultError::Overflow,
    )]
    pub vault_mint: Account<'info, Mint>,

    /// User's stSOL token account — shares will be burned from here.
    #[account(
        mut,
        constraint = user_share_account.mint == vault_state.vault_mint
            @ StakingVaultError::Overflow,
        constraint = user_share_account.owner == user.key()
            @ StakingVaultError::Unauthorized,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    /// Per-user position PDA.
    #[account(
        mut,
        seeds = [b"user_position", vault_state.key().as_ref(), user.key().as_ref()],
        bump = user_position.bump,
        constraint = user_position.owner == user.key()
            @ StakingVaultError::Unauthorized,
    )]
    pub user_position: Account<'info, UserPosition>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<Unstake>, shares: u64) -> Result<()> {
    require!(shares > 0, StakingVaultError::InvalidWithdrawShares);
    require!(
        ctx.accounts.user_position.shares >= shares,
        StakingVaultError::InsufficientShares
    );

    let vault = &ctx.accounts.vault_state;

    // Calculate SOL to return: shares * total_sol_staked / total_shares.
    let sol_out = calculate_assets_for_shares(shares, vault.total_shares, vault.total_sol_staked)
        .map_err(|_| StakingVaultError::Overflow)?;

    // Burn stSOL shares from user.
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

    // Transfer SOL (lamports) back to user from the vault PDA (vault PDA signs).
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[b"staking_vault", &[bump]];
    let signer_seeds = &[seeds];

    anchor_lang::system_program::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.vault_state.to_account_info(),
                to: ctx.accounts.user.to_account_info(),
            },
            signer_seeds,
        ),
        sol_out,
    )?;

    // Update vault state.
    let vault = &mut ctx.accounts.vault_state;
    vault.total_sol_staked = vault
        .total_sol_staked
        .checked_sub(sol_out)
        .ok_or(StakingVaultError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_sub(shares)
        .ok_or(StakingVaultError::Overflow)?;

    // Update user position.
    ctx.accounts.user_position.shares = ctx
        .accounts
        .user_position
        .shares
        .checked_sub(shares)
        .ok_or(StakingVaultError::Overflow)?;

    msg!(
        "Unstake: user={} shares={} sol_out={} total_sol_staked={} total_shares={}",
        ctx.accounts.user.key(),
        shares,
        sol_out,
        ctx.accounts.vault_state.total_sol_staked,
        ctx.accounts.vault_state.total_shares,
    );
    Ok(())
}
