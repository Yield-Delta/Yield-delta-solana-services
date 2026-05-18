use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, MintTo, Token, TokenAccount, Transfer},
};

use crate::errors::VaultError;
use crate::state::{UserPosition, VaultState};

#[derive(Accounts)]
pub struct Deposit<'info> {
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

    /// User's ATA for the share token — created if it doesn't exist.
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

pub fn handler(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    require!(amount > 0, VaultError::InvalidDepositAmount);

    let vault = &ctx.accounts.vault_state;

    // shares_to_mint = amount                                  (first deposit)
    // shares_to_mint = amount * total_shares / total_assets    (subsequent deposits)
    let shares_to_mint: u64 = if vault.total_shares == 0 {
        amount
    } else {
        (amount as u128)
            .checked_mul(vault.total_shares as u128)
            .ok_or(VaultError::Overflow)?
            .checked_div(vault.total_assets as u128)
            .ok_or(VaultError::Overflow)? as u64
    };

    // Transfer deposit tokens: user → vault ATA
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.vault_token_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
    )?;

    // Mint share tokens to user (vault PDA signs)
    let token_mint_key = ctx.accounts.vault_state.token_mint;
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[b"vault", token_mint_key.as_ref(), &[bump]];
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

    // Update vault totals
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_add(amount)
        .ok_or(VaultError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_add(shares_to_mint)
        .ok_or(VaultError::Overflow)?;

    // Update (or initialise) user position
    let pos = &mut ctx.accounts.user_position;
    if pos.owner == Pubkey::default() {
        pos.owner = ctx.accounts.user.key();
        pos.vault = ctx.accounts.vault_state.key();
        pos.bump = ctx.bumps.user_position;
    }
    pos.shares = pos
        .shares
        .checked_add(shares_to_mint)
        .ok_or(VaultError::Overflow)?;

    msg!(
        "Deposit: user={} amount={} shares_minted={} total_assets={} total_shares={}",
        ctx.accounts.user.key(),
        amount,
        shares_to_mint,
        ctx.accounts.vault_state.total_assets,
        ctx.accounts.vault_state.total_shares,
    );
    Ok(())
}
