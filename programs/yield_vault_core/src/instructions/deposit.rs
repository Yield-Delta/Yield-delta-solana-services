use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, MintTo, Token, TokenAccount, Transfer},
};

use crate::errors::VaultCoreError;
use crate::state::{UserPosition, VaultCoreState};

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct DepositCore<'info> {
    /// Depositing user — signs the transfer of deposit tokens.
    #[account(mut)]
    pub user: Signer<'info>,

    /// Core vault state PDA.
    /// Seeds: ["vault_core", strategy_type_byte, token_mint]
    #[account(
        mut,
        seeds = [
            b"vault_core",
            &vault_state.strategy_type.to_seed_byte(),
            vault_state.token_mint.as_ref(),
        ],
        bump = vault_state.bump,
        constraint = !vault_state.paused @ VaultCoreError::VaultPaused,
    )]
    pub vault_state: Account<'info, VaultCoreState>,

    /// User's ATA for the deposit token — source of funds.
    #[account(
        mut,
        constraint = user_token_account.mint == vault_state.token_mint
            @ VaultCoreError::Overflow,
        constraint = user_token_account.owner == user.key()
            @ VaultCoreError::Unauthorized,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// Vault's ATA for the deposit token — destination of funds.
    #[account(
        mut,
        constraint = vault_token_account.key() == vault_state.vault_token_account
            @ VaultCoreError::Overflow,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    /// Share-token mint — authority is the vault PDA.
    #[account(
        mut,
        constraint = vault_mint.key() == vault_state.vault_mint
            @ VaultCoreError::Overflow,
    )]
    pub vault_mint: Account<'info, Mint>,

    /// User's ATA for share tokens — created on first deposit if absent.
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = vault_mint,
        associated_token::authority = user,
    )]
    pub user_share_account: Account<'info, TokenAccount>,

    /// Per-user position PDA — created on first deposit if absent.
    /// Seeds: ["user_position", vault_state, user]
    #[account(
        init_if_needed,
        payer = user,
        space = UserPosition::LEN,
        seeds = [
            b"user_position",
            vault_state.key().as_ref(),
            user.key().as_ref(),
        ],
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

pub fn handler(ctx: Context<DepositCore>, amount: u64) -> Result<()> {
    require!(
        amount >= VaultCoreState::MIN_DEPOSIT,
        VaultCoreError::DepositTooSmall
    );

    let vault = &ctx.accounts.vault_state;

    // -- Management-fee accrual before computing new share price -----------
    let now = Clock::get()?.unix_timestamp;
    let mgmt_fee = vault
        .accrued_management_fee(now)
        .ok_or(VaultCoreError::Overflow)?;

    // Effective total_assets after deducting fee (conceptually reduces share price
    // for late depositors, matching ERC4626 behaviour).
    // We do NOT transfer the fee here — harvest_fees does that separately.
    let effective_assets = vault
        .total_assets
        .checked_sub(mgmt_fee)
        .unwrap_or(vault.total_assets);

    // -- Compute shares to mint (ERC4626) -----------------------------------
    let shares_to_mint: u64 = if vault.total_shares == 0 {
        amount
    } else {
        require!(effective_assets > 0, VaultCoreError::ZeroAssets);
        let shares = (amount as u128)
            .checked_mul(vault.total_shares as u128)
            .ok_or(VaultCoreError::Overflow)?
            .checked_div(effective_assets as u128)
            .ok_or(VaultCoreError::Overflow)?;
        shares as u64
    };

    require!(shares_to_mint > 0, VaultCoreError::DepositTooSmall);

    // -- Transfer deposit tokens: user → vault ATA --------------------------
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to:   ctx.accounts.vault_token_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
    )?;

    // -- Mint share tokens to user (vault PDA signs) ------------------------
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

    // -- Update vault state ------------------------------------------------
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_add(amount)
        .ok_or(VaultCoreError::Overflow)?;
    vault.total_shares = vault
        .total_shares
        .checked_add(shares_to_mint)
        .ok_or(VaultCoreError::Overflow)?;
    // Advance fee-harvest cursor so management fee doesn't double-accrue
    // on the newly deposited amount.
    vault.last_fee_harvest = now;

    // -- Initialise / update user position ----------------------------------
    let pos = &mut ctx.accounts.user_position;
    if pos.owner == Pubkey::default() {
        pos.owner  = ctx.accounts.user.key();
        pos.vault  = ctx.accounts.vault_state.key();
        pos.bump   = ctx.bumps.user_position;
    }
    pos.shares = pos
        .shares
        .checked_add(shares_to_mint)
        .ok_or(VaultCoreError::Overflow)?;
    pos.total_deposited = pos
        .total_deposited
        .checked_add(amount)
        .ok_or(VaultCoreError::Overflow)?;

    msg!(
        "DepositCore: user={} amount={} shares_minted={} \
         total_assets={} total_shares={}",
        ctx.accounts.user.key(),
        amount,
        shares_to_mint,
        ctx.accounts.vault_state.total_assets,
        ctx.accounts.vault_state.total_shares,
    );
    Ok(())
}
