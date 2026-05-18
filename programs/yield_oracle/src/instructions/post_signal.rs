use anchor_lang::prelude::*;

use crate::errors::OracleError;
use crate::state::{OracleState, SignalAccount, VolatilityRegime};

// ---------------------------------------------------------------------------
// Accounts
// PDA seeds (signal): ["signal", strategy_id_byte]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(
    strategy_id: u8,
    allocation_bps: u16,
    confidence: u8,
    volatility_regime: VolatilityRegime,
    suggested_lower_tick: i32,
    suggested_upper_tick: i32,
)]
pub struct PostSignal<'info> {
    /// Keeper / authority that posts signals.
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Global oracle config — validates the caller is the registered authority.
    #[account(
        mut,
        seeds = [b"oracle_config"],
        bump = oracle_state.bump,
        has_one = authority @ OracleError::Unauthorized,
    )]
    pub oracle_state: Account<'info, OracleState>,

    /// Signal account for this strategy — created on first post, overwritten on updates.
    /// Seeds: ["signal", strategy_id (1 byte)]
    #[account(
        init_if_needed,
        payer = authority,
        space = SignalAccount::LEN,
        seeds = [b"signal".as_ref(), strategy_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub signal_account: Account<'info, SignalAccount>,

    pub system_program: Program<'info, System>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(
    ctx: Context<PostSignal>,
    strategy_id: u8,
    allocation_bps: u16,
    confidence: u8,
    volatility_regime: VolatilityRegime,
    suggested_lower_tick: i32,
    suggested_upper_tick: i32,
) -> Result<()> {
    // Validate inputs
    require!(allocation_bps <= 10_000, OracleError::InvalidAllocationBps);
    require!(confidence <= 100, OracleError::InvalidConfidence);

    let now = Clock::get()?.unix_timestamp;
    let is_new = ctx.accounts.signal_account.strategy_id == 0
        && ctx.accounts.signal_account.posted_at == 0;

    let signal = &mut ctx.accounts.signal_account;
    signal.strategy_id                 = strategy_id;
    signal.recommended_allocation_bps  = allocation_bps;
    signal.confidence                  = confidence;
    signal.volatility_regime           = volatility_regime.clone();
    signal.posted_at                   = now;
    signal.rebalance_needed            = true;
    signal.suggested_lower_tick        = suggested_lower_tick;
    signal.suggested_upper_tick        = suggested_upper_tick;
    signal.bump                        = ctx.bumps.signal_account;

    // Increment count only on first creation
    if is_new {
        ctx.accounts.oracle_state.signal_count = ctx
            .accounts
            .oracle_state
            .signal_count
            .checked_add(1)
            .ok_or(OracleError::Overflow)?;
    }

    msg!(
        "PostSignal: strategy_id={} allocation_bps={} confidence={} regime={:?} \
         lower_tick={} upper_tick={} posted_at={}",
        strategy_id,
        allocation_bps,
        confidence,
        volatility_regime,
        suggested_lower_tick,
        suggested_upper_tick,
        now,
    );
    Ok(())
}
