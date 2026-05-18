use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Token, TokenAccount, Transfer},
};

use crate::errors::VaultCoreError;
use crate::state::VaultCoreState;

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct HarvestFees<'info> {
    /// Only the vault authority may trigger a fee harvest.
    pub authority: Signer<'info>,

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
        has_one = authority @ VaultCoreError::Unauthorized,
        has_one = fee_receiver @ VaultCoreError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultCoreState>,

    /// Vault ATA — source of fee tokens.
    #[account(
        mut,
        constraint = vault_token_account.key() == vault_state.vault_token_account
            @ VaultCoreError::Overflow,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    /// Fee-receiver wallet — must match vault_state.fee_receiver.
    /// CHECK: Validated via has_one on vault_state.
    pub fee_receiver: UncheckedAccount<'info>,

    /// Fee-receiver's ATA for the deposit token — created if absent.
    #[account(
        init_if_needed,
        payer = authority,
        associated_token::mint = token_mint,
        associated_token::authority = fee_receiver,
    )]
    pub fee_receiver_token_account: Account<'info, TokenAccount>,

    /// The deposit-token mint (needed for init_if_needed ATA creation).
    #[account(
        constraint = token_mint.key() == vault_state.token_mint
            @ VaultCoreError::Overflow,
    )]
    pub token_mint: Account<'info, anchor_spl::token::Mint>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<HarvestFees>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        now >= ctx.accounts.vault_state.last_fee_harvest,
        VaultCoreError::InvalidTimestamp
    );

    let vault = &ctx.accounts.vault_state;

    // Management fee: pro-rata over elapsed time
    let mgmt_fee = vault
        .accrued_management_fee(now)
        .ok_or(VaultCoreError::Overflow)?;

    // Total fees to transfer (performance fees are booked separately by strategy
    // programs; here we only distribute the accrued management fee).
    let total_fee = mgmt_fee;

    if total_fee == 0 {
        msg!("HarvestFees: nothing to harvest (elapsed since last harvest is too short)");
        return Ok(());
    }

    // Cap fee at available vault balance to prevent over-drawing.
    let actual_fee = total_fee.min(vault.total_assets);

    // Vault PDA signs the transfer to fee_receiver ATA.
    let strategy_byte = vault.strategy_type.to_seed_byte();
    let token_mint_key = vault.token_mint;
    let bump = vault.bump;
    let seeds: &[&[u8]] = &[
        b"vault_core",
        strategy_byte.as_ref(),
        token_mint_key.as_ref(),
        &[bump],
    ];
    let signer_seeds = &[seeds];

    token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from:      ctx.accounts.vault_token_account.to_account_info(),
                to:        ctx.accounts.fee_receiver_token_account.to_account_info(),
                authority: ctx.accounts.vault_state.to_account_info(),
            },
            signer_seeds,
        ),
        actual_fee,
    )?;

    // Reduce total_assets by the fee withdrawn and advance harvest cursor.
    let vault = &mut ctx.accounts.vault_state;
    vault.total_assets = vault
        .total_assets
        .checked_sub(actual_fee)
        .ok_or(VaultCoreError::Overflow)?;
    vault.last_fee_harvest = now;

    msg!(
        "HarvestFees: fee_transferred={} total_assets_after={} fee_receiver={}",
        actual_fee,
        vault.total_assets,
        ctx.accounts.fee_receiver.key(),
    );
    Ok(())
}
