// delta_neutral_vault — simulated delta-neutral USDC vault (Strategy 4)
//
// Users deposit USDC. The vault tracks a notional "long position" (the USDC
// itself) and a "synthetic short hedge" (tracked as notional, not real on devnet).
// The net delta is near zero. An AI keeper (the vault authority) calls
// `rebalance_hedge` to restore neutrality when |delta_bps| drifts past the
// configured threshold.
//
// On devnet everything is simulated — no real perp integration. Share tokens
// work identically to the other Yield Delta vaults.
//
// PDA seeds (vault): ["delta_neutral", usdc_mint]

use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::deposit::Deposit;
use instructions::initialize::Initialize;
use instructions::rebalance_hedge::RebalanceHedge;
use instructions::withdraw::Withdraw;
pub(crate) use instructions::deposit::__client_accounts_deposit;
pub(crate) use instructions::initialize::__client_accounts_initialize;
pub(crate) use instructions::rebalance_hedge::__client_accounts_rebalance_hedge;
pub(crate) use instructions::withdraw::__client_accounts_withdraw;

// Replace with: solana address -k target/deploy/delta_neutral_vault-keypair.json
declare_id!("C48TJDYWpws9dKu8bo8nq679w9vfCd7D1Emi9Abbhfyf");

#[program]
pub mod delta_neutral_vault {
    use super::*;

    /// Creates the vault PDA, share mint, and vault USDC ATA.
    /// `rebalance_threshold_bps` sets the delta drift level that triggers rebalance.
    pub fn initialize(ctx: Context<Initialize>, rebalance_threshold_bps: u16) -> Result<()> {
        instructions::initialize::handler(ctx, rebalance_threshold_bps)
    }

    /// Deposit USDC → receive vault share tokens.
    /// Updates `long_notional += amount` and recomputes `delta_bps`.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, amount)
    }

    /// Burn vault shares → receive proportional USDC.
    /// Reduces `long_notional` proportionally and recomputes `delta_bps`.
    pub fn withdraw(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, shares)
    }

    /// Authority-only: rebalance the hedge by setting `short_notional = long_notional`.
    /// Sets `delta_bps = 0`. Warns (but does not hard-fail) if delta is within threshold.
    pub fn rebalance_hedge(ctx: Context<RebalanceHedge>) -> Result<()> {
        instructions::rebalance_hedge::handler(ctx)
    }
}
