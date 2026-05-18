// lp_vault — LP auto-compounding vault (Strategy 3)
//
// Users deposit a mock LP token. The vault tracks their shares. An admin
// `simulate_compound` instruction periodically adds compounded fee tokens
// to total_lp_tokens, increasing the share price over time.
//
// On devnet there are no real AMM pools, so the LP token is a mock SPL mint
// that the test harness controls.
//
// PDA seeds (vault):    ["lp_vault", lp_token_mint]
// PDA seeds (position): ["user_position", vault_state, user]

use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::deposit::*;
use instructions::initialize::*;
use instructions::simulate_compound::*;
use instructions::withdraw::*;
pub(crate) use instructions::deposit::__client_accounts_deposit;
pub(crate) use instructions::initialize::__client_accounts_initialize;
pub(crate) use instructions::simulate_compound::__client_accounts_simulate_compound;
pub(crate) use instructions::withdraw::__client_accounts_withdraw;

// Replace with: solana address -k target/deploy/lp_vault-keypair.json
declare_id!("7UWS2aFyvNXiCHj1BTuWC7QU9iMBZcvjGBNABi7ByN4A");

#[program]
pub mod lp_vault {
    use super::*;

    /// Creates the vault PDA, registers the LP token mint, and creates the share mint.
    /// `compound_fee_bps` sets the simulated compounding rate per period.
    pub fn initialize(
        ctx: Context<Initialize>,
        compound_fee_bps: u16,
    ) -> Result<()> {
        instructions::initialize::handler(ctx, compound_fee_bps)
    }

    /// Deposit LP tokens → receive vault share tokens.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, amount)
    }

    /// Burn vault shares → receive LP tokens back.
    pub fn withdraw(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, shares)
    }

    /// Admin-only: simulate fee compounding — adds compounded LP tokens to total_lp_tokens.
    pub fn simulate_compound(ctx: Context<SimulateCompound>) -> Result<()> {
        instructions::simulate_compound::handler(ctx)
    }
}
