//! Invariant and fuzz tests for delta_neutral_vault.
//!
//! Covers:
//!   • `compute_delta` pure-function properties (stateless fuzz)
//!   • Hedge model invariants after deposit / rebalance / withdraw (stateful)
//!
//! Run: cargo test -p delta-neutral-vault

use delta_neutral_vault::state::compute_delta;
use proptest::prelude::*;
use yield_vault_core::math::{calculate_assets_for_shares, calculate_shares_to_mint};

// ── compute_delta stateless fuzz ─────────────────────────────────────────────

proptest! {
    /// No input combination panics.
    #[test]
    fn compute_delta_no_panic(long in 0u64..=u64::MAX, short in 0u64..=u64::MAX) {
        let _ = compute_delta(long, short);
    }
}

proptest! {
    /// When long == 0 the vault has no exposure — delta is 0 regardless of short.
    #[test]
    fn compute_delta_zero_when_no_long(short in 0u64..=u64::MAX) {
        prop_assert_eq!(compute_delta(0, short), 0);
    }
}

proptest! {
    /// Perfectly hedged (long == short) → delta == 0.
    #[test]
    fn compute_delta_zero_when_perfectly_hedged(notional in 1u64..=u64::MAX) {
        prop_assert_eq!(compute_delta(notional, notional), 0);
    }
}

proptest! {
    /// Unhedged long (short == 0) → delta == +10 000 bps (fully long).
    #[test]
    fn compute_delta_full_long_when_no_short(long in 1u64..=u64::MAX) {
        prop_assert_eq!(compute_delta(long, 0), 10_000);
    }
}

proptest! {
    /// Net long (long > short) → delta > 0.
    #[test]
    fn compute_delta_positive_when_net_long(
        long  in 1u64..=u64::MAX,
        short in 0u64..=u64::MAX,
    ) {
        prop_assume!(long > short);
        prop_assert!(compute_delta(long, short) > 0,
            "expected positive delta for long={} short={}", long, short);
    }
}

proptest! {
    /// Net short (short > long > 0) → delta < 0.
    #[test]
    fn compute_delta_negative_when_net_short(
        long  in 1u64..=u64::MAX,
        short in 1u64..=u64::MAX,
    ) {
        prop_assume!(short > long);
        prop_assert!(compute_delta(long, short) < 0,
            "expected negative delta for long={} short={}", long, short);
    }
}

proptest! {
    /// Delta is monotone decreasing in short: more short → smaller delta.
    #[test]
    fn compute_delta_monotone_in_short(
        long    in 1u64..1_000_000_000u64,
        short_a in 0u64..1_000_000_000u64,
        short_b in 0u64..1_000_000_000u64,
    ) {
        let (lo_short, hi_short) = if short_a <= short_b {
            (short_a, short_b)
        } else {
            (short_b, short_a)
        };
        let delta_lo = compute_delta(long, lo_short);
        let delta_hi = compute_delta(long, hi_short);
        prop_assert!(delta_lo >= delta_hi,
            "monotone violated: short {}→{} gave delta {}>{}", lo_short, hi_short, delta_lo, delta_hi);
    }
}

proptest! {
    /// |delta_bps| ≤ 10 000 for any long > 0 and any short in [0, long].
    /// (Short > long produces negative delta beyond -10 000 only if short > 2×long;
    ///  we restrict to realistic over/under hedge within the long exposure.)
    #[test]
    fn compute_delta_bounded_within_long_exposure(
        long  in 1u64..1_000_000_000u64,
        short in 0u64..1_000_000_000u64,
    ) {
        prop_assume!(short <= long); // within exposure range
        let d = compute_delta(long, short);
        prop_assert!(d >= 0 && d <= 10_000,
            "delta {} out of [0, 10_000] for long={} short={}", d, long, short);
    }
}

// ── Stateful hedge model ─────────────────────────────────────────────────────

/// In-memory model of the delta-neutral vault's hedge state.
#[derive(Debug, Clone, Default)]
struct HedgeVault {
    total_shares: u64,
    total_assets: u64,
    long_notional: u64,
    short_notional: u64,
    delta_bps: i64,
}

impl HedgeVault {
    fn deposit(&mut self, amount: u64) -> Option<u64> {
        if amount == 0 {
            return None;
        }
        let minted = if self.total_shares == 0 {
            amount
        } else {
            calculate_shares_to_mint(amount, self.total_shares, self.total_assets).ok()?
        };
        if minted == 0 {
            return None;
        }
        self.total_assets = self.total_assets.checked_add(amount)?;
        self.total_shares = self.total_shares.checked_add(minted)?;
        self.long_notional = self.long_notional.checked_add(amount)?;
        // Delta widens: short stays the same, long grows.
        self.delta_bps = compute_delta(self.long_notional, self.short_notional);
        Some(minted)
    }

    fn rebalance(&mut self) {
        // Perfect hedge: set short = long → delta_bps = 0.
        self.short_notional = self.long_notional;
        self.delta_bps = 0;
    }

    fn withdraw(&mut self, shares: u64) -> Option<u64> {
        if shares == 0 || shares > self.total_shares || self.total_shares == 0 {
            return None;
        }
        let assets =
            calculate_assets_for_shares(shares, self.total_shares, self.total_assets).ok()?;
        // Reduce long proportionally.
        let long_reduction = (shares as u128)
            .checked_mul(self.long_notional as u128)?
            .checked_div(self.total_shares as u128)? as u64;
        self.total_assets = self.total_assets.checked_sub(assets)?;
        self.total_shares = self.total_shares.checked_sub(shares)?;
        self.long_notional = self.long_notional.saturating_sub(long_reduction);
        // Short stays (approximate; on-chain short would also reduce proportionally).
        self.short_notional = self.short_notional.min(self.long_notional);
        self.delta_bps = compute_delta(self.long_notional, self.short_notional);
        Some(assets)
    }
}

proptest! {
    /// After rebalance, delta_bps is always exactly 0.
    #[test]
    fn rebalance_zeroes_delta(
        amounts in proptest::collection::vec(1u64..500_000u64, 1..10),
    ) {
        let mut v = HedgeVault::default();
        for &a in &amounts {
            v.deposit(a);
        }
        v.rebalance();
        prop_assert_eq!(v.delta_bps, 0,
            "delta_bps {} ≠ 0 after rebalance", v.delta_bps);
        prop_assert_eq!(v.short_notional, v.long_notional,
            "short {} ≠ long {} after rebalance", v.short_notional, v.long_notional);
    }
}

proptest! {
    /// Deposit increases long_notional by exactly the deposit amount.
    #[test]
    fn deposit_increases_long_notional_by_amount(
        seed   in 1u64..100_000u64,
        amount in 1u64..100_000u64,
    ) {
        let mut v = HedgeVault::default();
        v.deposit(seed);
        v.rebalance();
        let long_before = v.long_notional;
        v.deposit(amount);
        prop_assert_eq!(v.long_notional, long_before + amount,
            "long_notional should increase by deposit amount");
    }
}

proptest! {
    /// After deposit without rebalance, delta_bps > 0 (long side grew, short didn't).
    #[test]
    fn deposit_without_rebalance_creates_positive_delta(
        seed   in 1u64..500_000u64,
        amount in 1u64..500_000u64,
    ) {
        let mut v = HedgeVault::default();
        v.deposit(seed);
        v.rebalance();
        // Deposit again without rebalancing.
        v.deposit(amount);
        prop_assert!(v.delta_bps > 0,
            "expected positive delta after unhedged deposit, got {}", v.delta_bps);
    }
}

proptest! {
    /// No-free-money: user recovers ≤ deposited after deposit→rebalance→withdraw.
    #[test]
    fn deposit_then_withdraw_no_free_money(amount in 1u64..500_000u64) {
        let mut v = HedgeVault::default();
        if let Some(minted) = v.deposit(amount) {
            v.rebalance();
            if let Some(redeemed) = v.withdraw(minted) {
                prop_assert!(redeemed <= amount,
                    "recovered {} > deposited {}", redeemed, amount);
            }
        }
    }
}
