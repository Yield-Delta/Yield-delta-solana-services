// yield_oracle — admin-updatable on-chain price oracle + AI signal board for devnet
//
// Provides SOL/USD and USDC/USD prices at 6-decimal precision.
// An authorised admin (or AI keeper) posts price updates and strategy signals on-chain.
// Vault programs and off-chain clients read the oracle PDA directly.
//
// PDA seeds (oracle state): ["oracle_config"]
// PDA seeds (signal):       ["signal", strategy_id (1 byte)]

use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use errors::OracleError;
pub use state::{OracleState, SignalAccount, VolatilityRegime};
use instructions::initialize_oracle::InitializeOracle;
use instructions::mark_rebalanced::MarkRebalanced;
use instructions::post_signal::PostSignal;
pub(crate) use instructions::initialize_oracle::__client_accounts_initialize_oracle;
pub(crate) use instructions::mark_rebalanced::__client_accounts_mark_rebalanced;
pub(crate) use instructions::post_signal::__client_accounts_post_signal;

// Replace with: solana address -k target/deploy/yield_oracle-keypair.json
declare_id!("CRZ13p9bH4hVcStuGFUZ1sjPf94J1q9H2fsGs5nCeoqG");

#[program]
pub mod yield_oracle {
    use super::*;

    /// Creates the singleton oracle PDA.
    /// Call once per deployment — the signer becomes the oracle authority.
    pub fn initialize_oracle(ctx: Context<InitializeOracle>) -> Result<()> {
        instructions::initialize_oracle::handler(ctx)
    }

    /// Update SOL/USD and USDC/USD prices (authority-gated).
    ///
    /// Prices are expressed with 6 decimal places of precision.
    /// e.g. sol_usd = 150_000_000 means $150.000000
    pub fn update_prices(
        ctx: Context<UpdatePrices>,
        sol_usd: u64,
        usdc_usd: u64,
    ) -> Result<()> {
        let oracle = &mut ctx.accounts.oracle_state;
        oracle.sol_usd_price  = sol_usd;
        oracle.usdc_usd_price = usdc_usd;
        oracle.last_updated   = Clock::get()?.unix_timestamp;
        msg!("Oracle prices updated: sol_usd={} usdc_usd={}", sol_usd, usdc_usd);
        Ok(())
    }

    /// Post an AI-generated rebalancing signal for a given strategy.
    pub fn post_signal(
        ctx: Context<PostSignal>,
        strategy_id: u8,
        allocation_bps: u16,
        confidence: u8,
        volatility_regime: VolatilityRegime,
        suggested_lower_tick: i32,
        suggested_upper_tick: i32,
    ) -> Result<()> {
        instructions::post_signal::handler(
            ctx,
            strategy_id,
            allocation_bps,
            confidence,
            volatility_regime,
            suggested_lower_tick,
            suggested_upper_tick,
        )
    }

    /// Mark a strategy signal as rebalanced (clears the `rebalance_needed` flag).
    pub fn mark_rebalanced(ctx: Context<MarkRebalanced>, strategy_id: u8) -> Result<()> {
        instructions::mark_rebalanced::handler(ctx, strategy_id)
    }

    /// Returns the current SOL/USD price (view helper — logs the price).
    pub fn get_sol_price(ctx: Context<GetPrice>) -> Result<u64> {
        let oracle = &ctx.accounts.oracle_state;
        let now = Clock::get()?.unix_timestamp;
        require!(oracle.is_fresh(now), OracleError::StalePrice);
        let price = oracle.sol_usd_price;
        msg!("Oracle SOL/USD price: {}", price);
        Ok(price)
    }
}

// ---------------------------------------------------------------------------
// UpdatePrices account context (authority-gated price update)
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct UpdatePrices<'info> {
    #[account(
        constraint = authority.key() == oracle_state.authority @ OracleError::Unauthorized
    )]
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [b"oracle_config"],
        bump = oracle_state.bump,
    )]
    pub oracle_state: Account<'info, OracleState>,
}

// ---------------------------------------------------------------------------
// GetPrice account context (view-only, no mutation needed)
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct GetPrice<'info> {
    #[account(
        seeds = [b"oracle_config"],
        bump = oracle_state.bump,
    )]
    pub oracle_state: Account<'info, OracleState>,
}
