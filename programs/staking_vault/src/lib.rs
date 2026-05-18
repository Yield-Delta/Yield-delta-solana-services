// staking_vault — SOL liquid staking vault (Strategy 2)
//
// Users deposit SOL and receive stSOL share tokens.  On devnet, staking yield is
// simulated: the admin calls `accrue_yield` which adds a configurable bps yield
// to total_sol_staked based on elapsed time since the last accrual.
//
// PDA seeds (vault):    ["staking_vault"]
// PDA seeds (position): ["user_position", vault_state, user]

use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::accrue_yield::*;
use instructions::initialize::*;
use instructions::stake::*;
use instructions::unstake::*;
pub(crate) use instructions::accrue_yield::__client_accounts_accrue_yield;
pub(crate) use instructions::initialize::__client_accounts_initialize;
pub(crate) use instructions::stake::__client_accounts_stake;
pub(crate) use instructions::unstake::__client_accounts_unstake;

// Replace with: solana address -k target/deploy/staking_vault-keypair.json
declare_id!("Bhmqob5GG4gBjEpJSYN17bGhWXnDS7rsrDH4UF7SduQ4");

#[program]
pub mod staking_vault {
    use super::*;

    /// Creates the vault PDA and the stSOL share mint.
    /// `yield_bps` sets the simulated annual APY (e.g. 700 = 7 %).
    pub fn initialize(ctx: Context<Initialize>, yield_bps: u16) -> Result<()> {
        instructions::initialize::handler(ctx, yield_bps)
    }

    /// Deposit SOL (lamports) → receive stSOL share tokens.
    pub fn stake(ctx: Context<Stake>, amount_lamports: u64) -> Result<()> {
        instructions::stake::handler(ctx, amount_lamports)
    }

    /// Burn stSOL share tokens → receive SOL back.
    pub fn unstake(ctx: Context<Unstake>, shares: u64) -> Result<()> {
        instructions::unstake::handler(ctx, shares)
    }

    /// Admin-only: simulate yield accrual based on elapsed time × yield_bps.
    pub fn accrue_yield(ctx: Context<AccrueYield>) -> Result<()> {
        instructions::accrue_yield::handler(ctx)
    }
}
