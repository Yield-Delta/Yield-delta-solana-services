use anchor_lang::prelude::*;
use crate::state::OracleState;

#[derive(Accounts)]
pub struct InitializeOracle<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = OracleState::LEN,
        seeds = [b"oracle_config"],
        bump,
    )]
    pub oracle_state: Account<'info, OracleState>,
    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<InitializeOracle>) -> Result<()> {
    let oracle = &mut ctx.accounts.oracle_state;
    oracle.authority      = ctx.accounts.authority.key();
    oracle.sol_usd_price  = 0;
    oracle.usdc_usd_price = 1_000_000;
    oracle.last_updated   = Clock::get()?.unix_timestamp;
    oracle.signal_count   = 0;
    oracle.bump           = ctx.bumps.oracle_state;
    msg!("Oracle initialized: authority={}", ctx.accounts.authority.key());
    Ok(())
}
