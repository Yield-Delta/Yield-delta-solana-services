// adaptive_yield_vault — volatility-reactive adaptive yield vault (Experimental)
//
// Reads the yield_oracle SignalAccount on-chain to get the live VolatilityRegime
// and adjusts the accrual rate in real time:
//
//   Low    → base_yield_bps × low_mult_bps  / 10_000  (conservative)
//   Medium → base_yield_bps                            (standard)
//   High   → base_yield_bps × high_mult_bps / 10_000  (vol-premium harvest)
//
// Solana-native features used:
//   • Cross-program account deserialization of yield_oracle SignalAccount
//   • Slot-clock withdrawal lock activated on High-regime transitions
//   • Shared math via yield_vault_core (no code duplication across programs)
//
// PDA seeds (vault):    ["adaptive_vault", token_mint]
// PDA seeds (position): ["user_position", vault_state, user]

use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::accrue_adaptive::AccrueAdaptive;
use instructions::deposit::Deposit;
use instructions::initialize::Initialize;
use instructions::withdraw::Withdraw;
pub(crate) use instructions::accrue_adaptive::__client_accounts_accrue_adaptive;
pub(crate) use instructions::deposit::__client_accounts_deposit;
pub(crate) use instructions::initialize::__client_accounts_initialize;
pub(crate) use instructions::withdraw::__client_accounts_withdraw;

declare_id!("StyNEoSmEPwNUgJDE4bjbp6FbPoHEwHYnRySdQLCM64");

#[program]
pub mod adaptive_yield_vault {
    use super::*;

    /// Creates the vault PDA, share mint, and vault token ATA.
    /// `base_yield_bps` — baseline annualised yield (e.g. 1200 = 12%).
    /// `low_mult_bps`   — Low-regime multiplier   (e.g. 7000 = 0.70×).
    /// `high_mult_bps`  — High-regime multiplier  (e.g. 15000 = 1.50×).
    /// `lock_slots_high`— withdrawal lock after High-regime entry (~150 slots ≈ 60 s).
    pub fn initialize(
        ctx: Context<Initialize>,
        base_yield_bps: u16,
        low_mult_bps: u16,
        high_mult_bps: u16,
        lock_slots_high: u64,
    ) -> Result<()> {
        instructions::initialize::handler(ctx, base_yield_bps, low_mult_bps, high_mult_bps, lock_slots_high)
    }

    /// Deposit underlying token → receive adaptive share tokens.
    /// Share price reflects accrued vol-adjusted yield at the time of deposit.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        instructions::deposit::handler(ctx, amount)
    }

    /// Burn adaptive shares → receive proportional underlying token.
    /// Reverts if the vault is in High regime and the slot-lock has not expired.
    pub fn withdraw(ctx: Context<Withdraw>, shares: u64) -> Result<()> {
        instructions::withdraw::handler(ctx, shares)
    }

    /// Authority-only: read the oracle signal, update the regime, and accrue
    /// yield at the vol-adjusted rate onto total_assets.
    pub fn accrue_adaptive(ctx: Context<AccrueAdaptive>, strategy_id: u8) -> Result<()> {
        instructions::accrue_adaptive::handler(ctx, strategy_id)
    }
}
