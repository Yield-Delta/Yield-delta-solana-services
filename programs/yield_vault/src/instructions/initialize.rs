use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use crate::state::VaultState;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = VaultState::LEN,
        seeds = [b"vault", token_mint.key().as_ref()],
        bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    /// The deposit token mint (e.g. devnet USDC or native SOL mint).
    pub token_mint: Account<'info, Mint>,

    /// Share token mint — authority will be set to the vault PDA.
    #[account(
        init,
        payer = authority,
        mint::decimals = token_mint.decimals,
        mint::authority = vault_state,
        mint::freeze_authority = vault_state,
    )]
    pub vault_mint: Account<'info, Mint>,

    /// ATA owned by the vault PDA that will hold deposited tokens.
    #[account(
        init,
        payer = authority,
        associated_token::mint = token_mint,
        associated_token::authority = vault_state,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(ctx: Context<Initialize>) -> Result<()> {
    let vault_state = &mut ctx.accounts.vault_state;
    vault_state.authority = ctx.accounts.authority.key();
    vault_state.token_mint = ctx.accounts.token_mint.key();
    vault_state.vault_mint = ctx.accounts.vault_mint.key();
    vault_state.vault_token_account = ctx.accounts.vault_token_account.key();
    vault_state.total_shares = 0;
    vault_state.total_assets = 0;
    vault_state.bump = ctx.bumps.vault_state;

    msg!(
        "Vault initialized: mint={} share_mint={} vault_pda={}",
        ctx.accounts.token_mint.key(),
        ctx.accounts.vault_mint.key(),
        ctx.accounts.vault_state.key(),
    );
    Ok(())
}
