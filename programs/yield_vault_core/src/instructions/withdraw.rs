use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Burn, Mint, Token, TokenAccount, Transfer},
};

use crate::errors::VaultCoreError;
use crate::state::{UserPosition, VaultCoreState};

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

/// Normal withdraw — requires the caller to own shares and not be in emergency.
#[derive(Accounts)]
pub struct WithdrawCore<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [
            b"vault_core",
            &vault_state.strategy_type.to_seed_byte(),
            vault_state.token_mint.as_ref(),
        ],
        bump = vault_state.bump,
        // Normal withdraw is blocked while paused; use emergency_withdraw instead.
        constraint = !vault_state.paused @ VaultCoreError::VaultPaused,
    )]
    pub vault_state: Account<'info, VaultCoreState>,

    #[account(
        mut,
        constraint = user_token_account.mint == vault_state.token_mint
            @ VaultCoreError::Overflow,
        constraint = user_token_account.owner == user.key()
            @ VaultCoreError::Unauthorized,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = vault_token_account.key() == vault_state.vault_token_account
            @ VaultCoreError::Overflow,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint
            @ VaultCoreError::Overflow,
    )]
    pub vault_mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = user_share_account.mint == vault_state.vault_mint
            @ VaultCoreError::Overflow,
        constraint = user_share_account.owner == user.key()
            @ VaultCoreError::Unauthorized,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    /// Seeds: ["user_position", vault_state, user]
    #[account(
        mut,
        seeds = [
            b"user_position",
            vault_state.key().as_ref(),
            user.key().as_ref(),
        ],
        bump = user_position.bump,
        constraint = user_position.owner == user.key()
            @ VaultCoreError::Unauthorized,
    )]
    pub user_position: Account<'info, UserPosition>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------------------
// Emergency withdraw — authority can call even when paused
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct EmergencyWithdraw<'info> {
    /// Authority must sign for emergency withdrawals.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// The user whose position is being force-exited.
    /// CHECK: Beneficiary can be any valid account; authority is responsible.
    pub user: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [
            b"vault_core",
            &vault_state.strategy_type.to_seed_byte(),
            vault_state.token_mint.as_ref(),
        ],
        bump = vault_state.bump,
        // Emergency withdraw requires authority == vault authority.
        constraint = vault_state.authority == authority.key()
            @ VaultCoreError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultCoreState>,

    #[account(
        mut,
        constraint = user_token_account.mint == vault_state.token_mint
            @ VaultCoreError::Overflow,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = vault_token_account.key() == vault_state.vault_token_account
            @ VaultCoreError::Overflow,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint
            @ VaultCoreError::Overflow,
    )]
    pub vault_mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = user_share_account.mint == vault_state.vault_mint
            @ VaultCoreError::Overflow,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [
            b"user_position",
            vault_state.key().as_ref(),
            user.key().as_ref(),
        ],
        bump = user_position.bump,
        constraint = user_position.owner == user.key()
            @ VaultCoreError::Unauthorized,
    )]
    pub user_position: Account<'info, UserPosition>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------------------
// Shared logic
// ---------------------------------------------------------------------------

fn do_withdraw(
    vault: &mut VaultCoreState,
    user_position: &mut UserPosition,
    shares: u64,
    user_token_account: &AccountInfo,
    vault_token_account: &AccountInfo,
    vault_mint: &AccountInfo,
    user_share_account: &AccountInfo,
    token_program: &AccountInfo,
    user_key: &Pubkey,
) -> Result<u64> {
    require!(shares > 0, VaultCoreError::InvalidWithdrawShares);
    require!(
        user_position.shares >= shares,
        VaultCoreError::InsufficientShares
    );
    require!(vault.total_shares > 0, VaultCoreError::ZeroShares);

    // assets_out = shares * total_assets / total_shares
    let tokens_out: u64 = {
        let out = (shares as u128)
            .checked_mul(vault.total_assets as u128)
            .ok_or(VaultCoreError::Overflow)?
            .checked_div(vault.total_shares as u128)
            .ok_or(VaultCoreError::Overflow)?;
        out as u64
    };

    // Burn share tokens from user
    anchor_spl::token::burn(
        CpiContext::new(
            token_program.clone(),
            Burn {
                mint:      vault_mint.clone(),
                from:      user_share_account.clone(),
                authority: user_token_account.clone(), // user signs externally
            },
        ),
        shares,
    )?;

    // Transfer deposit tokens vault → user (vault PDA signs)
    let strategy_byte = vault.strategy_type.to_seed_byte();
    let token_mint_key = vault.token_mint;
    let bump = vault.bump;
    let seeds: &[&[u8]] = &[
        b"vault_core",
        strategy_byte.as_ref(),
        token_mint_key.as_ref(),
        &[bump],
    ];

    anchor_spl::token::transfer(
        CpiContext::new_with_signer(
            token_program.clone(),
            Transfer {
                from:      vault_token_account.clone(),
                to:        user_token_account.clone(),
                authority: vault_token_account.clone(),
            },
            &[seeds],
        ),
        tokens_out,
    )?;

    // Update vault totals
    vault.total_assets = vault
        .total_assets
        .checked_sub(tokens_out)
        .ok_or(VaultCoreError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_sub(shares)
        .ok_or(VaultCoreError::Overflow)?;

    // Update user position
    user_position.shares = user_position
        .shares
        .checked_sub(shares)
        .ok_or(VaultCoreError::Overflow)?;
    user_position.total_withdrawn = user_position
        .total_withdrawn
        .checked_add(tokens_out)
        .ok_or(VaultCoreError::Overflow)?;

    msg!(
        "Withdraw: user={} shares={} tokens_out={} \
         total_assets={} total_shares={}",
        user_key,
        shares,
        tokens_out,
        vault.total_assets,
        vault.total_shares,
    );

    Ok(tokens_out)
}

// ---------------------------------------------------------------------------
// Normal withdraw handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<WithdrawCore>, shares: u64) -> Result<()> {
    require!(shares > 0, VaultCoreError::InvalidWithdrawShares);
    require!(
        ctx.accounts.user_position.shares >= shares,
        VaultCoreError::InsufficientShares
    );
    require!(
        ctx.accounts.vault_state.total_shares > 0,
        VaultCoreError::ZeroShares
    );

    let vault = &ctx.accounts.vault_state;

    // assets_out = shares * total_assets / total_shares
    let tokens_out: u64 = {
        let out = (shares as u128)
            .checked_mul(vault.total_assets as u128)
            .ok_or(VaultCoreError::Overflow)?
            .checked_div(vault.total_shares as u128)
            .ok_or(VaultCoreError::Overflow)?;
        out as u64
    };

    // Burn shares
    anchor_spl::token::burn(
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

    // Vault PDA signs the transfer out
    let strategy_byte = ctx.accounts.vault_state.strategy_type.to_seed_byte();
    let token_mint_key = ctx.accounts.vault_state.token_mint;
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[
        b"vault_core",
        strategy_byte.as_ref(),
        token_mint_key.as_ref(),
        &[bump],
    ];
    let signer_seeds = &[seeds];

    anchor_spl::token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from:      ctx.accounts.vault_token_account.to_account_info(),
                to:        ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.vault_state.to_account_info(),
            },
            signer_seeds,
        ),
        tokens_out,
    )?;

    // Mutate state
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_sub(tokens_out)
        .ok_or(VaultCoreError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_sub(shares)
        .ok_or(VaultCoreError::Overflow)?;

    let pos = &mut ctx.accounts.user_position;
    pos.shares = pos
        .shares
        .checked_sub(shares)
        .ok_or(VaultCoreError::Overflow)?;
    pos.total_withdrawn = pos
        .total_withdrawn
        .checked_add(tokens_out)
        .ok_or(VaultCoreError::Overflow)?;

    msg!(
        "WithdrawCore: user={} shares={} tokens_out={} \
         total_assets={} total_shares={}",
        ctx.accounts.user.key(),
        shares,
        tokens_out,
        ctx.accounts.vault_state.total_assets,
        ctx.accounts.vault_state.total_shares,
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Emergency withdraw handler — bypasses pause, authority-only
// ---------------------------------------------------------------------------

pub fn handler_emergency(ctx: Context<EmergencyWithdraw>, shares: u64) -> Result<()> {
    require!(shares > 0, VaultCoreError::InvalidWithdrawShares);
    require!(
        ctx.accounts.user_position.shares >= shares,
        VaultCoreError::InsufficientShares
    );
    require!(
        ctx.accounts.vault_state.total_shares > 0,
        VaultCoreError::ZeroShares
    );

    let vault = &ctx.accounts.vault_state;
    let tokens_out: u64 = {
        let out = (shares as u128)
            .checked_mul(vault.total_assets as u128)
            .ok_or(VaultCoreError::Overflow)?
            .checked_div(vault.total_shares as u128)
            .ok_or(VaultCoreError::Overflow)?;
        out as u64
    };

    // Burn shares (authority signs, user_share_account belongs to user)
    anchor_spl::token::burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Burn {
                mint:      ctx.accounts.vault_mint.to_account_info(),
                from:      ctx.accounts.user_share_account.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        shares,
    )?;

    let strategy_byte = ctx.accounts.vault_state.strategy_type.to_seed_byte();
    let token_mint_key = ctx.accounts.vault_state.token_mint;
    let bump = ctx.accounts.vault_state.bump;
    let seeds: &[&[u8]] = &[
        b"vault_core",
        strategy_byte.as_ref(),
        token_mint_key.as_ref(),
        &[bump],
    ];
    let signer_seeds = &[seeds];

    anchor_spl::token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from:      ctx.accounts.vault_token_account.to_account_info(),
                to:        ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.vault_state.to_account_info(),
            },
            signer_seeds,
        ),
        tokens_out,
    )?;

    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_sub(tokens_out)
        .ok_or(VaultCoreError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_sub(shares)
        .ok_or(VaultCoreError::Overflow)?;

    let pos = &mut ctx.accounts.user_position;
    pos.shares = pos
        .shares
        .checked_sub(shares)
        .ok_or(VaultCoreError::Overflow)?;
    pos.total_withdrawn = pos
        .total_withdrawn
        .checked_add(tokens_out)
        .ok_or(VaultCoreError::Overflow)?;

    msg!(
        "EmergencyWithdraw: authority={} user={} shares={} tokens_out={}",
        ctx.accounts.authority.key(),
        ctx.accounts.user.key(),
        shares,
        tokens_out,
    );
    Ok(())
}
