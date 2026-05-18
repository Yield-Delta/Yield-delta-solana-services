use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use crate::errors::VaultCoreError;
use crate::state::{StrategyType, VaultCoreState};

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds: ["vault_core", strategy_type_byte, token_mint]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(strategy_type: StrategyType, performance_fee_bps: u16, management_fee_bps: u16)]
pub struct InitializeVaultCore<'info> {
    /// Payer and vault authority.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Core vault state PDA.
    /// Seed: ["vault_core", strategy_type as u8, token_mint]
    #[account(
        init,
        payer = authority,
        space = VaultCoreState::LEN,
        seeds = [
            b"vault_core",
            &strategy_type.to_seed_byte(),
            token_mint.key().as_ref(),
        ],
        bump,
    )]
    pub vault_state: Account<'info, VaultCoreState>,

    /// The SPL token mint users deposit (e.g. devnet USDC, wSOL).
    pub token_mint: Account<'info, Mint>,

    /// Share-token mint; vault PDA will be mint authority.
    /// Created by this instruction — caller provides a fresh keypair.
    #[account(
        init,
        payer = authority,
        mint::decimals = token_mint.decimals,
        mint::authority = vault_state,
        mint::freeze_authority = vault_state,
    )]
    pub vault_mint: Account<'info, Mint>,

    /// ATA owned by the vault PDA — holds idle deposited tokens.
    #[account(
        init,
        payer = authority,
        associated_token::mint = token_mint,
        associated_token::authority = vault_state,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    /// Designated fee-receiver wallet (does not need to sign).
    /// CHECK: Authority supplies this address; no on-chain constraint needed
    /// beyond storing it.
    pub fee_receiver: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(
    ctx: Context<InitializeVaultCore>,
    strategy_type: StrategyType,
    performance_fee_bps: u16,
    management_fee_bps: u16,
) -> Result<()> {
    // Validate fee params — protect against misconfigured vaults.
    require!(
        performance_fee_bps <= 10_000,
        VaultCoreError::FeeBpsTooLarge
    );
    require!(
        management_fee_bps <= 10_000,
        VaultCoreError::FeeBpsTooLarge
    );

    let now = Clock::get()?.unix_timestamp;

    let vault = &mut ctx.accounts.vault_state;
    vault.authority             = ctx.accounts.authority.key();
    vault.fee_receiver          = ctx.accounts.fee_receiver.key();
    vault.token_mint            = ctx.accounts.token_mint.key();
    vault.vault_mint            = ctx.accounts.vault_mint.key();
    vault.vault_token_account   = ctx.accounts.vault_token_account.key();
    vault.strategy_type         = strategy_type;
    vault.total_shares          = 0;
    vault.total_assets          = 0;
    vault.last_fee_harvest      = now;
    vault.performance_fee_bps   = performance_fee_bps;
    vault.management_fee_bps    = management_fee_bps;
    vault.paused                = false;
    vault.bump                  = ctx.bumps.vault_state;

    msg!(
        "VaultCore initialized: strategy={:?} token_mint={} share_mint={} pda={} \
         perf_fee_bps={} mgmt_fee_bps={}",
        strategy_type,
        ctx.accounts.token_mint.key(),
        ctx.accounts.vault_mint.key(),
        ctx.accounts.vault_state.key(),
        performance_fee_bps,
        management_fee_bps,
    );
    Ok(())
}
