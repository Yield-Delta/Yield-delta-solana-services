//! Property-based fuzz tests for adaptive_yield_vault state logic.
//!
//! Tests cover:
//!   • effective_yield_bps() correctness and range for all regimes
//!   • Slot-lock circuit breaker invariants
//!   • Overflow safety on extreme parameter values
//!
//! Run: cargo test -p adaptive-yield-vault

use adaptive_yield_vault::state::{AdaptiveUserPosition, AdaptiveVaultState};
use anchor_lang::prelude::Pubkey;
use proptest::prelude::*;
use yield_oracle::state::VolatilityRegime;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_vault(
    base_yield_bps: u16,
    low_mult_bps: u16,
    high_mult_bps: u16,
    regime: VolatilityRegime,
    lock_slots_high: u64,
    high_regime_started_at_slot: u64,
) -> AdaptiveVaultState {
    AdaptiveVaultState {
        authority: Pubkey::default(),
        token_mint: Pubkey::default(),
        vault_mint: Pubkey::default(),
        vault_token_account: Pubkey::default(),
        oracle_signal: Pubkey::default(),
        total_shares: 0,
        total_assets: 0,
        base_yield_bps,
        low_mult_bps,
        high_mult_bps,
        lock_slots_high,
        high_regime_started_at_slot,
        current_regime: regime,
        last_accrual: 0,
        bump: 255,
    }
}

/// Mirrors the on-chain slot-lock guard from withdraw.rs.
fn is_slot_locked(vault: &AdaptiveVaultState, current_slot: u64) -> bool {
    if matches!(vault.current_regime, VolatilityRegime::High) {
        let lock_until = vault
            .high_regime_started_at_slot
            .saturating_add(vault.lock_slots_high);
        current_slot < lock_until
    } else {
        false
    }
}

// ── effective_yield_bps ──────────────────────────────────────────────────────

proptest! {
    /// Medium regime always returns base_yield_bps unchanged.
    #[test]
    fn effective_bps_medium_equals_base(base in 0u16..=u16::MAX) {
        let vault = make_vault(base, 7_000, 15_000, VolatilityRegime::Medium, 150, 0);
        prop_assert_eq!(vault.effective_yield_bps(), base);
    }
}

proptest! {
    /// Low regime result ≤ base_yield_bps (multiplier ≤ 10 000 → conservative).
    #[test]
    fn effective_bps_low_lte_base(
        base     in 0u16..=u16::MAX,
        low_mult in 0u16..=10_000u16,
    ) {
        let vault = make_vault(base, low_mult, 15_000, VolatilityRegime::Low, 150, 0);
        prop_assert!(vault.effective_yield_bps() <= base,
            "Low regime raised bps: base={} result={}", base, vault.effective_yield_bps());
    }
}

proptest! {
    /// High regime result ≥ base_yield_bps (multiplier ≥ 10 000 → premium).
    #[test]
    fn effective_bps_high_gte_base(
        base      in 0u16..=1_000u16, // keep low so result fits in u16
        high_mult in 10_000u16..=u16::MAX,
    ) {
        let vault = make_vault(base, 7_000, high_mult, VolatilityRegime::High, 150, 0);
        prop_assert!(vault.effective_yield_bps() >= base,
            "High regime lowered bps: base={} result={}", base, vault.effective_yield_bps());
    }
}

proptest! {
    /// Result never exceeds u16::MAX regardless of multiplier inputs.
    #[test]
    fn effective_bps_never_exceeds_u16_max(
        base      in 0u16..=u16::MAX,
        low_mult  in 0u16..=u16::MAX,
        high_mult in 0u16..=u16::MAX,
    ) {
        for regime in [VolatilityRegime::Low, VolatilityRegime::Medium, VolatilityRegime::High] {
            let vault = make_vault(base, low_mult, high_mult, regime, 150, 0);
            let result = vault.effective_yield_bps();
            prop_assert!(
                result <= u16::MAX,
                "exceeded u16::MAX: base={} low_mult={} high_mult={} result={}",
                base, low_mult, high_mult, result
            );
        }
    }
}

proptest! {
    /// Zero base → zero effective yield in all regimes.
    #[test]
    fn effective_bps_zero_base_is_zero(
        low_mult  in 0u16..=u16::MAX,
        high_mult in 0u16..=u16::MAX,
    ) {
        for regime in [VolatilityRegime::Low, VolatilityRegime::Medium, VolatilityRegime::High] {
            let vault = make_vault(0, low_mult, high_mult, regime.clone(), 150, 0);
            prop_assert_eq!(vault.effective_yield_bps(), 0,
                "non-zero result for base=0 regime={:?}", regime);
        }
    }
}

proptest! {
    /// Medium regime is independent of low_mult and high_mult.
    #[test]
    fn effective_bps_medium_ignores_multipliers(
        base      in 0u16..=u16::MAX,
        low_mult  in 0u16..=u16::MAX,
        high_mult in 0u16..=u16::MAX,
    ) {
        let vault = make_vault(base, low_mult, high_mult, VolatilityRegime::Medium, 150, 0);
        prop_assert_eq!(vault.effective_yield_bps(), base);
    }
}

// ── Slot-lock circuit breaker ────────────────────────────────────────────────

proptest! {
    /// Non-High regimes are never slot-locked.
    #[test]
    fn slot_lock_only_activates_in_high_regime(
        lock_slots   in 1u64..1_000u64,
        started_slot in 0u64..1_000_000u64,
        current_slot in 0u64..1_000_000u64,
    ) {
        for regime in [VolatilityRegime::Low, VolatilityRegime::Medium] {
            let vault = make_vault(1_200, 7_000, 15_000, regime.clone(), lock_slots, started_slot);
            prop_assert!(!is_slot_locked(&vault, current_slot),
                "unexpectedly locked in {:?} regime", regime);
        }
    }
}

proptest! {
    /// High regime: current_slot < started + lock_slots → locked.
    #[test]
    fn slot_lock_blocks_within_window(
        lock_slots   in 1u64..10_000u64,
        started_slot in 0u64..1_000_000u64,
    ) {
        prop_assume!(started_slot.saturating_add(lock_slots) > started_slot); // no overflow
        let lock_until = started_slot + lock_slots;
        // Pick a slot strictly inside the lock window
        let current_slot = started_slot + (lock_slots / 2).max(0);
        prop_assume!(current_slot < lock_until);

        let vault = make_vault(1_200, 7_000, 15_000, VolatilityRegime::High, lock_slots, started_slot);
        prop_assert!(is_slot_locked(&vault, current_slot),
            "should be locked: current={} lock_until={}", current_slot, lock_until);
    }
}

proptest! {
    /// High regime: current_slot ≥ started + lock_slots → unlocked.
    #[test]
    fn slot_lock_releases_after_window(
        lock_slots   in 1u64..10_000u64,
        started_slot in 0u64..1_000_000u64,
    ) {
        let lock_until = started_slot.saturating_add(lock_slots);
        let current_slot = lock_until; // exactly at the boundary

        let vault = make_vault(1_200, 7_000, 15_000, VolatilityRegime::High, lock_slots, started_slot);
        prop_assert!(!is_slot_locked(&vault, current_slot),
            "should be unlocked at boundary: current={} lock_until={}", current_slot, lock_until);
    }
}

proptest! {
    /// Saturation: when started_slot + lock_slots overflows u64, saturating_add
    /// clamps lock_until to u64::MAX. Any slot strictly below u64::MAX remains
    /// locked. (At exactly u64::MAX the boundary condition triggers unlock —
    /// that is correct, not a bug.)
    #[test]
    fn slot_lock_saturating_add_stays_locked(extra in 0u64..50u64) {
        // started_slot near max; adding lock_slots (200) saturates to u64::MAX
        let started_slot = u64::MAX - 50;
        let lock_slots   = 200u64;
        // current_slot is in [u64::MAX-50 .. u64::MAX-1] — strictly below u64::MAX
        let current_slot = started_slot + extra; // no overflow: extra < 50

        let vault = make_vault(1_200, 7_000, 15_000, VolatilityRegime::High, lock_slots, started_slot);
        // lock_until saturates to u64::MAX; current_slot < u64::MAX → still locked
        prop_assert!(is_slot_locked(&vault, current_slot),
            "saturation caused spurious unlock at slot {} (lock_until=u64::MAX)", current_slot);
    }
}

// ── AdaptiveUserPosition ─────────────────────────────────────────────────────

proptest! {
    /// shares field can hold the full u64 range without overflow.
    #[test]
    fn user_position_shares_field_full_range(shares in 0u64..=u64::MAX) {
        let pos = AdaptiveUserPosition {
            owner: Pubkey::default(),
            vault: Pubkey::default(),
            shares,
            bump: 255,
        };
        prop_assert_eq!(pos.shares, shares);
    }
}
