use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::deposit::*;
use instructions::initialize::*;
use instructions::withdraw::*;
pub(crate) use instructions::deposit::__client_accounts_deposit;
pub(crate) use instructions::initialize::__client_accounts_initialize;
pub(crate) use instructions::withdraw::__client_accounts_withdraw;

// Replace with the address printed by `solana address -k target/deploy/yield_vault-keypair.json`
// after the first `anchor build`, then rebuild before deploying.
declare_id!("5hp22e1bv9HG8QXZKE1YC48pgWZc1zNWdh3v1Z65h4zD");

#[program]
pub mod yield_vault {
    use super::*;

    /// Creates the vault PDA, share mint, and vault token account.
    /// Call once per deposit-token mint.
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        instructions::initialize::handler(ctx)
    }

    /// Transfers `amount` deposit tokens from the user into the vault
    /// and mints proportional share tokens back to the user.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, amount)
    }

    /// Burns `shares` share tokens from the user and releases the
    /// proportional amount of deposit tokens back to the user.
    pub fn withdraw(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, shares)
    }
}
