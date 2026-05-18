use anchor_lang::prelude::*;

use crate::errors::OracleError;
use crate::state::{OracleState, SignalAccount};

// ---------------------------------------------------------------------------
// Accounts
// Called by vault programs (or keeper) after a successful rebalance.
// ---------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(strategy_id: u8)]
pub struct MarkRebalanced<'info> {
    /// Only the oracle authority may clear the rebalance_needed flag.
    pub authority: Signer<'info>,

    /// Oracle config — authority check.
    #[account(
        seeds = [b"oracle_config"],
        bump = oracle_state.bump,
        has_one = authority @ OracleError::Unauthorized,
    )]
    pub oracle_state: Account<'info, OracleState>,

    /// Signal account to mark.
    /// Seeds: ["signal", strategy_id (1 byte)]
    #[account(
        mut,
        seeds = [b"signal".as_ref(), strategy_id.to_le_bytes().as_ref()],
        bump = signal_account.bump,
    )]
    pub signal_account: Account<'info, SignalAccount>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub fn handler(ctx: Context<MarkRebalanced>, strategy_id: u8) -> Result<()> {
    let signal = &mut ctx.accounts.signal_account;

    require!(
        signal.rebalance_needed,
        OracleError::AlreadyRebalanced
    );

    signal.rebalance_needed = false;

    msg!(
        "MarkRebalanced: strategy_id={} signal_pda={}",
        strategy_id,
        ctx.accounts.signal_account.key(),
    );
    Ok(())
}
