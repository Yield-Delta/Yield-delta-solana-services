use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, MintTo, Token, TokenAccount, Transfer},
};
use yield_vault_core::math::calculate_shares_to_mint;

use crate::errors::AdaptiveVaultError;
use crate::state::{AdaptiveUserPosition, AdaptiveVaultState};

#[derive(Accounts)]
pub struct Deposit<'info> {
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
        init_if_needed,
        payer = user,
        associated_token::mint      = vault_mint,
        associated_token::authority = user,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = user,
        space = AdaptiveUserPosition::LEN,
        seeds = [b"user_position", vault_state.key().as_ref(), user.key().as_ref()],
        bump,
    )]
    pub user_position: Account<'info, AdaptiveUserPosition>,

    pub token_program:            Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program:           Program<'info, System>,
}

pub fn handler(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    require!(amount > 0, AdaptiveVaultError::InvalidDepositAmount);

    let vault = &ctx.accounts.vault_state;
    let shares_to_mint = calculate_shares_to_mint(amount, vault.total_shares, vault.total_assets)
        .map_err(|_| AdaptiveVaultError::ZeroSharesMinted)?;

    // Transfer tokens: user → vault ATA
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from:      ctx.accounts.user_token_account.to_account_info(),
                to:        ctx.accounts.vault_token_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
    )?;

    // Mint share tokens to the user (vault PDA signs)
    let token_mint_key = ctx.accounts.vault_state.token_mint;
    let bump           = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]]    = &[b"adaptive_vault", token_mint_key.as_ref(), &[bump]];
    let signer_seeds        = &[seeds];

    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint:      ctx.accounts.vault_mint.to_account_info(),
                to:        ctx.accounts.user_share_account.to_account_info(),
                authority: ctx.accounts.vault_state.to_account_info(),
            },
            signer_seeds,
        ),
        shares_to_mint,
    )?;

    // Capture keys before mutable borrows
    let vault_key = ctx.accounts.vault_state.key();
    let user_key  = ctx.accounts.user.key();

    // Update vault totals
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault.total_assets
        .checked_add(amount).ok_or(AdaptiveVaultError::Overflow)?;
    vault.total_shares = vault.total_shares
        .checked_add(shares_to_mint).ok_or(AdaptiveVaultError::Overflow)?;

    let log_regime = vault.current_regime.clone();
    let log_bps    = vault.effective_yield_bps();

    // Init or update user position
    let pos = &mut ctx.accounts.user_position;
    if pos.owner == Pubkey::default() {
        pos.owner = user_key;
        pos.vault = vault_key;
        pos.bump  = ctx.bumps.user_position;
    }
    pos.shares = pos.shares
        .checked_add(shares_to_mint).ok_or(AdaptiveVaultError::Overflow)?;

    msg!(
        "AdaptiveDeposit: user={} amount={} shares={} regime={:?} effective_bps={}",
        user_key, amount, shares_to_mint, log_regime, log_bps,
    );
    Ok(())
}
