// meta_vault — AI auto-allocation meta vault (Strategy 5)
//
// Users deposit USDC into one vault that routes capital across up to 6
// sub-strategy slots. On devnet the routing is simulated — the vault tracks
// an allocation table (strategy_id → weight_bps) and the AI keeper calls
// `set_allocations` to rebalance. `accrue_returns` increments `total_assets`
// based on the blended simulated APY, making the share price grow over time.
//
// PDA seeds (vault): ["meta_vault"]

use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::accrue_returns::AccrueReturns;
use instructions::deposit::Deposit;
use instructions::initialize::Initialize;
use instructions::set_allocations::SetAllocations;
use instructions::withdraw::Withdraw;
use state::AllocationSlot;
pub(crate) use instructions::accrue_returns::__client_accounts_accrue_returns;
pub(crate) use instructions::deposit::__client_accounts_deposit;
pub(crate) use instructions::initialize::__client_accounts_initialize;
pub(crate) use instructions::set_allocations::__client_accounts_set_allocations;
pub(crate) use instructions::withdraw::__client_accounts_withdraw;

// Replace with: solana address -k target/deploy/meta_vault-keypair.json
declare_id!("F4x55MUt2WXxqmtVQNyXBxg822pGUdge8KoYvuH6fLDQ");

#[program]
pub mod meta_vault {
    use super::*;

    /// Creates the vault PDA, share mint, and vault USDC ATA.
    /// `fee_bps` is the annual management fee (e.g. 200 = 2%).
    pub fn initialize(ctx: Context<Initialize>, fee_bps: u16) -> Result<()> {
        instructions::initialize::handler(ctx, fee_bps)
    }

    /// Deposit USDC → receive vault share tokens.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, amount)
    }

    /// Burn vault shares → receive proportional USDC.
    pub fn withdraw(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, shares)
    }

    /// Authority-only: update the strategy allocation table.
    /// Validates ≤6 slots, weights sum to 10_000, no duplicate strategy_ids.
    pub fn set_allocations(
        ctx: Context<SetAllocations>,
        new_allocations: Vec<AllocationSlot>,
    ) -> Result<()> {
        instructions::set_allocations::handler(ctx, new_allocations)
    }

    /// Authority-only: accrue simulated returns based on blended APY.
    /// Increments `total_assets` using `accrue_simple_interest` from core math.
    pub fn accrue_returns(ctx: Context<AccrueReturns>) -> Result<()> {
        instructions::accrue_returns::handler(ctx)
    }
}
