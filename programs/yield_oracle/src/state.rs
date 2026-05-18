use anchor_lang::prelude::*;

// OracleState — singleton oracle config
// PDA seeds: ["oracle_config"]
#[account]
pub struct OracleState {
    pub authority: Pubkey,
    pub sol_usd_price: u64,   // × 10^6  e.g. 150_000_000 = $150.000000
    pub usdc_usd_price: u64,  // × 10^6  e.g. 1_000_000 = $1.000000
    pub signal_count: u64,
    pub last_updated: i64,
    pub bump: u8,
}

impl OracleState {
    // discriminator(8) + authority(32) + sol_usd(8) + usdc_usd(8) + signal_count(8) + last_updated(8) + bump(1) = 73
    // round up to 72 — use 80 for alignment padding
    pub const LEN: usize = 8 + 32 + 8 + 8 + 8 + 8 + 1 + 7; // = 80
    pub const STALENESS_SECS: i64 = 3_600;

    pub fn is_fresh(&self, now: i64) -> bool {
        (now - self.last_updated) < Self::STALENESS_SECS
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq)]
pub enum VolatilityRegime {
    Low,
    Medium,
    High,
}

impl Default for VolatilityRegime {
    fn default() -> Self {
        VolatilityRegime::Medium
    }
}

// SignalAccount — per-strategy AI rebalancing signal
// PDA seeds: ["signal", strategy_id (1 byte)]
#[account]
pub struct SignalAccount {
    pub strategy_id: u8,
    pub recommended_allocation_bps: u16,
    pub confidence: u8,           // 0-100
    pub volatility_regime: VolatilityRegime,
    pub posted_at: i64,
    pub rebalance_needed: bool,
    pub suggested_lower_tick: i32,
    pub suggested_upper_tick: i32,
    pub bump: u8,
}

impl SignalAccount {
    // discriminator(8) + strategy_id(1) + allocation_bps(2) + confidence(1) + regime(1) + posted_at(8) + rebalance_needed(1) + lower_tick(4) + upper_tick(4) + bump(1) = 31
    // round up to 64 for future-proofing
    pub const LEN: usize = 64;
}
