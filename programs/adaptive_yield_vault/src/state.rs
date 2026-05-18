use anchor_lang::prelude::*;
use yield_oracle::state::VolatilityRegime;

/// Vault PDA  seeds: ["adaptive_vault", token_mint]
#[account]
pub struct AdaptiveVaultState {
    /// Vault admin / AI keeper.
    pub authority: Pubkey,
    /// SPL mint deposited by users.
    pub token_mint: Pubkey,
    /// Share token mint (mint authority = vault PDA).
    pub vault_mint: Pubkey,
    /// ATA owned by the vault PDA that holds deposited tokens.
    pub vault_token_account: Pubkey,
    /// The yield_oracle SignalAccount this vault watches.
    pub oracle_signal: Pubkey,
    /// Total share tokens in circulation.
    pub total_shares: u64,
    /// Total underlying tokens managed by the vault (grows with accrual).
    pub total_assets: u64,
    /// Baseline annualised yield in basis points (e.g. 1200 = 12%).
    pub base_yield_bps: u16,
    /// Low-regime yield multiplier in bps (e.g. 7000 = 0.70×).
    pub low_mult_bps: u16,
    /// High-regime yield multiplier in bps (e.g. 15000 = 1.50×).
    pub high_mult_bps: u16,
    /// Slots after a High-regime transition during which withdrawals are locked.
    pub lock_slots_high: u64,
    /// Slot at which the vault last entered the High regime (0 = never).
    pub high_regime_started_at_slot: u64,
    /// Currently observed volatility regime (updated by accrue_adaptive).
    pub current_regime: VolatilityRegime,
    /// Unix timestamp of the last yield accrual.
    pub last_accrual: i64,
    /// PDA bump.
    pub bump: u8,
}

impl AdaptiveVaultState {
    // discriminator(8) + 5×pubkey(160) + 2×u64(16) + 3×u16(6) + 2×u64(16)
    // + regime_enum(1) + last_accrual_i64(8) + bump(1) + padding(9)
    pub const LEN: usize = 8 + 160 + 16 + 6 + 16 + 1 + 8 + 1 + 9; // 225 → 232

    /// Effective annual yield bps for the current regime, capped at u16::MAX.
    pub fn effective_yield_bps(&self) -> u16 {
        let bps: u64 = match self.current_regime {
            VolatilityRegime::Low => {
                (self.base_yield_bps as u64)
                    .saturating_mul(self.low_mult_bps as u64)
                    / 10_000
            }
            VolatilityRegime::Medium => self.base_yield_bps as u64,
            VolatilityRegime::High => {
                (self.base_yield_bps as u64)
                    .saturating_mul(self.high_mult_bps as u64)
                    / 10_000
            }
        };
        bps.min(u16::MAX as u64) as u16
    }
}

/// User position PDA  seeds: ["user_position", vault_state, user]
#[account]
pub struct AdaptiveUserPosition {
    pub owner: Pubkey,
    pub vault: Pubkey,
    pub shares: u64,
    pub bump: u8,
}

impl AdaptiveUserPosition {
    pub const LEN: usize = 8 + 32 + 32 + 8 + 1 + 7;
}
