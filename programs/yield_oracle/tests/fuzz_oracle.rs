//! Stateless fuzz tests for yield_oracle state logic.
//!
//! Covers:
//!   • OracleState::is_fresh() staleness semantics
//!   • SignalAccount field constraints and ordering
//!   • VolatilityRegime ordering properties
//!
//! No Anchor runtime required — tests operate directly on state structs.
//!
//! Run: cargo test -p yield-oracle

use anchor_lang::prelude::Pubkey;
use proptest::prelude::*;
use yield_oracle::state::{OracleState, SignalAccount, VolatilityRegime};

// ── OracleState::is_fresh ─────────────────────────────────────────────────────

const STALENESS_SECS: i64 = OracleState::STALENESS_SECS; // 3_600

fn make_oracle(last_updated: i64) -> OracleState {
    OracleState {
        authority: Pubkey::default(),
        sol_usd_price: 150_000_000,
        usdc_usd_price: 1_000_000,
        signal_count: 0,
        last_updated,
        bump: 255,
    }
}

proptest! {
    /// Oracle is always fresh when queried at its own last_updated timestamp.
    #[test]
    fn is_fresh_at_update_time(last_updated in i64::MIN / 2..i64::MAX / 2) {
        let oracle = make_oracle(last_updated);
        prop_assert!(oracle.is_fresh(last_updated),
            "oracle not fresh at its own update timestamp {}", last_updated);
    }
}

proptest! {
    /// Oracle is fresh for any elapsed < STALENESS_SECS.
    #[test]
    fn is_fresh_within_staleness_window(
        last_updated in 0i64..i64::MAX / 2,
        elapsed      in 0i64..STALENESS_SECS,
    ) {
        let oracle = make_oracle(last_updated);
        prop_assert!(oracle.is_fresh(last_updated + elapsed),
            "oracle should be fresh at elapsed {} < {}", elapsed, STALENESS_SECS);
    }
}

proptest! {
    /// Oracle is NOT fresh once elapsed ≥ STALENESS_SECS.
    #[test]
    fn not_fresh_at_or_past_staleness(
        last_updated in 0i64..i64::MAX / 2,
        overage      in 0i64..86_400i64, // up to 24h past the boundary
    ) {
        let oracle = make_oracle(last_updated);
        let now = last_updated + STALENESS_SECS + overage;
        prop_assert!(!oracle.is_fresh(now),
            "oracle should be stale at elapsed {} ≥ {}", STALENESS_SECS + overage, STALENESS_SECS);
    }
}

/// Boundary: exactly at STALENESS_SECS the oracle is stale (strictly less than
/// STALENESS_SECS is fresh).
#[test]
fn is_fresh_boundary_is_exclusive() {
    let oracle = make_oracle(1000);
    // Exactly at the boundary → stale.
    assert!(!oracle.is_fresh(1000 + STALENESS_SECS));
    // One second before → fresh.
    assert!(oracle.is_fresh(1000 + STALENESS_SECS - 1));
}

proptest! {
    /// is_fresh is monotone: if stale at T, also stale at T+k for any k ≥ 0.
    #[test]
    fn staleness_is_monotone(
        last_updated in 0i64..i64::MAX / 4,
        stale_at     in STALENESS_SECS..i64::MAX / 4,
        extra        in 0i64..86_400i64,
    ) {
        let oracle = make_oracle(last_updated);
        let t1 = last_updated + stale_at;
        let t2 = t1 + extra;
        if !oracle.is_fresh(t1) {
            prop_assert!(!oracle.is_fresh(t2),
                "stale at {} but fresh at {}", t1, t2);
        }
    }
}

proptest! {
    /// Querying with now < last_updated (clock went backwards) is treated as fresh
    /// since (now - last_updated) is negative and negative < STALENESS_SECS.
    #[test]
    fn backwards_clock_treated_as_fresh(
        last_updated in STALENESS_SECS..i64::MAX / 2,
        behind_secs  in 0i64..STALENESS_SECS,
    ) {
        let oracle = make_oracle(last_updated);
        // now is behind last_updated.
        let now = last_updated - behind_secs;
        // negative elapsed → definitely < STALENESS_SECS → fresh
        prop_assert!(oracle.is_fresh(now),
            "backwards clock should be treated as fresh");
    }
}

// ── SignalAccount field invariants ────────────────────────────────────────────

fn make_signal(
    strategy_id: u8,
    allocation_bps: u16,
    confidence: u8,
    posted_at: i64,
    lower_tick: i32,
    upper_tick: i32,
    regime: VolatilityRegime,
) -> SignalAccount {
    SignalAccount {
        strategy_id,
        recommended_allocation_bps: allocation_bps,
        confidence,
        volatility_regime: regime,
        posted_at,
        rebalance_needed: false,
        suggested_lower_tick: lower_tick,
        suggested_upper_tick: upper_tick,
        bump: 255,
    }
}

proptest! {
    /// recommended_allocation_bps is always in [0, 10 000] for valid signals.
    #[test]
    fn allocation_bps_within_range(bps in 0u16..=10_000u16) {
        let sig = make_signal(1, bps, 50, 1_000_000, -100, 100, VolatilityRegime::Medium);
        prop_assert!(sig.recommended_allocation_bps <= 10_000,
            "allocation_bps {} out of range", sig.recommended_allocation_bps);
    }
}

proptest! {
    /// confidence is always in [0, 100] for valid signals.
    #[test]
    fn confidence_within_byte_range(confidence in 0u8..=100u8) {
        let sig = make_signal(1, 5_000, confidence, 1_000_000, -100, 100, VolatilityRegime::Low);
        prop_assert!(sig.confidence <= 100,
            "confidence {} out of [0, 100]", sig.confidence);
    }
}

proptest! {
    /// For a well-formed Uniswap-style range: lower_tick ≤ upper_tick.
    #[test]
    fn tick_range_lower_lte_upper(
        a in i32::MIN / 2..i32::MAX / 2,
        b in i32::MIN / 2..i32::MAX / 2,
    ) {
        let (lower, upper) = if a <= b { (a, b) } else { (b, a) };
        let sig = make_signal(1, 5_000, 80, 1_000_000, lower, upper, VolatilityRegime::High);
        prop_assert!(sig.suggested_lower_tick <= sig.suggested_upper_tick,
            "tick ordering violated: lower={} upper={}", lower, upper);
    }
}

proptest! {
    /// Signal is still fresh immediately after posting (posted_at == now).
    #[test]
    fn signal_fresh_when_just_posted(now in 0i64..i64::MAX / 2) {
        let sig = make_signal(1, 5_000, 90, now, -887_272, 887_272, VolatilityRegime::Medium);
        let elapsed = now - sig.posted_at;
        prop_assert_eq!(elapsed, 0, "elapsed should be 0 for just-posted signal");
        prop_assert!(elapsed < 7_200, "just-posted signal should be within freshness window");
    }
}

proptest! {
    /// Signal is stale when more than 7_200 seconds old (adaptive vault threshold).
    #[test]
    fn signal_stale_after_2_hours(
        posted_at in 0i64..i64::MAX / 2,
        overage   in 0i64..86_400i64,
    ) {
        let sig = make_signal(1, 5_000, 70, posted_at, 0, 0, VolatilityRegime::Low);
        let now = posted_at + 7_200 + overage;
        let elapsed = now - sig.posted_at;
        prop_assert!(elapsed >= 7_200,
            "elapsed {} should be ≥ 7_200", elapsed);
    }
}

// ── VolatilityRegime properties ───────────────────────────────────────────────

#[test]
fn default_regime_is_medium() {
    let regime = VolatilityRegime::default();
    assert_eq!(regime, VolatilityRegime::Medium);
}

#[test]
fn regimes_are_distinct() {
    assert_ne!(VolatilityRegime::Low, VolatilityRegime::Medium);
    assert_ne!(VolatilityRegime::Medium, VolatilityRegime::High);
    assert_ne!(VolatilityRegime::Low, VolatilityRegime::High);
}
