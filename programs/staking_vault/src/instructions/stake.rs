use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, MintTo, Token, TokenAccount},
};

use crate::errors::StakingVaultError;
use crate::state::{StakingVaultState, UserPosition};
use yield_vault_core::math::calculate_shares_to_mint;

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (vault): ["staking_vault"]
// PDA seeds (position): ["user_position", vault_state, user]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct Stake<'info> {
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

    /// User's stSOL ATA — created if it doesn't exist.
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = vault_mint,
        associated_token::authority = user,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    /// Per-user position PDA — created on first stake.
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

pub fn handler(ctx: Context<Stake>, amount_lamports: u64) -> Result<()> {
    require!(amount_lamports > 0, StakingVaultError::InvalidDepositAmount);

    let vault = &ctx.accounts.vault_state;

    // Calculate stSOL shares to mint.
    let shares_to_mint = calculate_shares_to_mint(
        amount_lamports,
        vault.total_shares,
        vault.total_sol_staked,
    )
    .map_err(|_| StakingVaultError::Overflow)?;

    require!(shares_to_mint > 0, StakingVaultError::ZeroSharesMinted);

    // Transfer SOL from user to vault (system program CPI — native SOL transfer).
    anchor_lang::system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.user.to_account_info(),
                to: ctx.accounts.vault_state.to_account_info(),
            },
        ),
        amount_lamports,
    )?;

    // Mint stSOL shares to user (vault PDA signs).
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[b"staking_vault", &[bump]];
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
    vault.total_sol_staked = vault
        .total_sol_staked
        .checked_add(amount_lamports)
        .ok_or(StakingVaultError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_add(shares_to_mint)
        .ok_or(StakingVaultError::Overflow)?;

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
        .ok_or(StakingVaultError::Overflow)?;

    msg!(
        "Stake: user={} lamports={} shares_minted={} total_sol_staked={} total_shares={}",
        ctx.accounts.user.key(),
        amount_lamports,
        shares_to_mint,
        ctx.accounts.vault_state.total_sol_staked,
        ctx.accounts.vault_state.total_shares,
    );
    Ok(())
}
