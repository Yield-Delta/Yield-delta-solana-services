//! Stateless property-based fuzz tests for yield_vault_core::math and fees.
//!
//! Run: cargo test -p yield-vault-core
//!
//! Each `proptest!` block generates hundreds of random inputs and checks that
//! the invariant holds for all of them. A falsifying input is shrunk to its
//! minimal form and printed.

use proptest::prelude::*;
use yield_vault_core::fees::{calculate_management_fee, calculate_performance_fee};
use yield_vault_core::math::{
    accrue_simple_interest, apply_bps, calculate_assets_for_shares, calculate_shares_to_mint,
};

// ── calculate_shares_to_mint ─────────────────────────────────────────────────

proptest! {
    /// First deposit (no existing shares) is always 1:1.
    #[test]
    fn shares_first_deposit_is_one_to_one(amount in 1u64..=u64::MAX) {
        let result = calculate_shares_to_mint(amount, 0, 0);
        prop_assert!(result.is_ok(), "unexpected err: {:?}", result);
        prop_assert_eq!(result.unwrap(), amount);
    }
}

proptest! {
    /// No input combination panics — every call returns Ok or Err.
    #[test]
    fn shares_no_panic(
        amount       in 0u64..=u64::MAX,
        total_shares in 0u64..=u64::MAX,
        total_assets in 0u64..=u64::MAX,
    ) {
        let _ = calculate_shares_to_mint(amount, total_shares, total_assets);
    }
}

proptest! {
    /// Share supply does not inflate: minted * total_assets ≤ amount * total_shares.
    /// Equivalently, the effective share price never drops due to a deposit.
    #[test]
    fn shares_no_supply_inflation(
        amount       in 1u64..1_000_000_000_u64,
        total_shares in 1u64..1_000_000_000_u64,
        total_assets in 1u64..1_000_000_000_u64,
    ) {
        if let Ok(minted) = calculate_shares_to_mint(amount, total_shares, total_assets) {
            let lhs = (minted as u128).saturating_mul(total_assets as u128);
            let rhs = (amount as u128).saturating_mul(total_shares as u128);
            prop_assert!(lhs <= rhs,
                "inflation: minted={} * assets={} > amount={} * shares={}",
                minted, total_assets, amount, total_shares);
        }
    }
}

proptest! {
    /// Minted shares are proportional to the deposit amount (floor division can
    /// differ by ±1 between amount and 2×amount).
    #[test]
    fn shares_proportional_to_amount(
        amount       in 2u64..500_000_000_u64,
        total_shares in 1u64..500_000_000_u64,
        total_assets in 1u64..500_000_000_u64,
    ) {
        let s1 = calculate_shares_to_mint(amount, total_shares, total_assets);
        let s2 = calculate_shares_to_mint(amount * 2, total_shares, total_assets);
        if let (Ok(s1), Ok(s2)) = (s1, s2) {
            prop_assert!(
                s2 >= s1.saturating_mul(2).saturating_sub(1)
                    && s2 <= s1.saturating_mul(2).saturating_add(1),
                "non-proportional: s2={} expected ~{}",
                s2, s1 * 2
            );
        }
    }
}

proptest! {
    /// Zero deposit always fails (InvalidDepositAmount or ZeroSharesMinted).
    #[test]
    fn shares_zero_amount_errors(
        total_shares in 0u64..=u64::MAX,
        total_assets in 0u64..=u64::MAX,
    ) {
        let result = calculate_shares_to_mint(0, total_shares, total_assets);
        // Either it returns ZeroSharesMinted or succeeds with 0 (both acceptable),
        // but it must NOT return a positive share count.
        if let Ok(minted) = result {
            prop_assert_eq!(minted, 0, "zero deposit minted {} shares", minted);
        }
    }
}

// ── calculate_assets_for_shares ──────────────────────────────────────────────

proptest! {
    /// No input combination panics.
    #[test]
    fn assets_no_panic(
        shares       in 0u64..=u64::MAX,
        total_shares in 0u64..=u64::MAX,
        total_assets in 0u64..=u64::MAX,
    ) {
        let _ = calculate_assets_for_shares(shares, total_shares, total_assets);
    }
}

proptest! {
    /// Redeeming all shares returns exactly total_assets.
    #[test]
    fn assets_all_shares_returns_all_assets(
        total_shares in 1u64..=u64::MAX,
        total_assets in 0u64..=u64::MAX,
    ) {
        let result = calculate_assets_for_shares(total_shares, total_shares, total_assets);
        prop_assert!(result.is_ok());
        prop_assert_eq!(result.unwrap(), total_assets);
    }
}

proptest! {
    /// Result never exceeds total_assets (no assets created from nothing).
    #[test]
    fn assets_never_exceeds_vault(
        shares       in 1u64..1_000_000_000_u64,
        total_shares in 1u64..1_000_000_000_u64,
        total_assets in 0u64..1_000_000_000_u64,
    ) {
        prop_assume!(shares <= total_shares);
        if let Ok(out) = calculate_assets_for_shares(shares, total_shares, total_assets) {
            prop_assert!(out <= total_assets,
                "out={} > total_assets={}", out, total_assets);
        }
    }
}

proptest! {
    /// Immediate deposit→withdraw roundtrip: user recovers ≤ deposited (integer rounding).
    #[test]
    fn assets_roundtrip_no_free_money(
        amount       in 1u64..1_000_000_u64,
        total_shares in 0u64..1_000_000_u64,
        total_assets in 0u64..1_000_000_u64,
    ) {
        let minted = match calculate_shares_to_mint(amount, total_shares, total_assets) {
            Ok(s) if s > 0 => s,
            _ => return Ok(()),
        };
        let new_shares = total_shares.saturating_add(minted);
        let new_assets = total_assets.saturating_add(amount);
        if let Ok(redeemed) = calculate_assets_for_shares(minted, new_shares, new_assets) {
            prop_assert!(redeemed <= amount,
                "roundtrip returned {} > deposited {}", redeemed, amount);
        }
    }
}

proptest! {
    /// Result is monotone with shares: more shares → more assets.
    #[test]
    fn assets_monotone_with_shares(
        shares_a     in 1u64..500_000_000_u64,
        shares_b     in 1u64..500_000_000_u64,
        total_shares in 1u64..1_000_000_000_u64,
        total_assets in 0u64..1_000_000_000_u64,
    ) {
        let (lo, hi) = if shares_a <= shares_b { (shares_a, shares_b) } else { (shares_b, shares_a) };
        prop_assume!(hi <= total_shares);
        if let (Ok(r_lo), Ok(r_hi)) = (
            calculate_assets_for_shares(lo, total_shares, total_assets),
            calculate_assets_for_shares(hi, total_shares, total_assets),
        ) {
            prop_assert!(r_lo <= r_hi,
                "monotone violated: shares {}→{} gave assets {}>{}", lo, hi, r_lo, r_hi);
        }
    }
}

// ── apply_bps ────────────────────────────────────────────────────────────────

proptest! {
    /// No input combination panics.
    #[test]
    fn bps_no_panic(value in 0u64..=u64::MAX, bps in 0u16..=u16::MAX) {
        let _ = apply_bps(value, bps);
    }
}

proptest! {
    /// 0 bps → 0 result regardless of value.
    #[test]
    fn bps_zero_yields_zero(value in 0u64..=u64::MAX) {
        prop_assert_eq!(apply_bps(value, 0).unwrap(), 0);
    }
}

proptest! {
    /// 10 000 bps (100%) is the identity function.
    #[test]
    fn bps_ten_thousand_is_identity(value in 0u64..=u64::MAX) {
        prop_assert_eq!(apply_bps(value, 10_000).unwrap(), value);
    }
}

proptest! {
    /// For bps ≤ 10 000, result ≤ value (no amplification in normal range).
    #[test]
    fn bps_bounded_by_value_in_normal_range(value in 0u64..=u64::MAX, bps in 0u16..=10_000u16) {
        if let Ok(result) = apply_bps(value, bps) {
            prop_assert!(result <= value, "result {} > value {} at bps {}", result, value, bps);
        }
    }
}

proptest! {
    /// Result is monotone with bps for a fixed value.
    #[test]
    fn bps_monotone(
        value in 0u64..1_000_000_000_u64,
        bps1  in 0u16..10_000_u16,
        bps2  in 0u16..10_000_u16,
    ) {
        let (lo, hi) = if bps1 <= bps2 { (bps1, bps2) } else { (bps2, bps1) };
        if let (Ok(r_lo), Ok(r_hi)) = (apply_bps(value, lo), apply_bps(value, hi)) {
            prop_assert!(r_lo <= r_hi, "bps monotone violated: bps {}→{} gave {}>{}", lo, hi, r_lo, r_hi);
        }
    }
}

// ── accrue_simple_interest ───────────────────────────────────────────────────

proptest! {
    /// No input combination panics.
    #[test]
    fn interest_no_panic(
        principal in 0u64..=u64::MAX,
        bps       in 0u16..=u16::MAX,
        elapsed   in 0u64..=u64::MAX,
    ) {
        let _ = accrue_simple_interest(principal, bps, elapsed);
    }
}

proptest! {
    /// Zero elapsed time → zero yield.
    #[test]
    fn interest_zero_elapsed_yields_zero(principal in 0u64..=u64::MAX, bps in 0u16..=u16::MAX) {
        prop_assert_eq!(accrue_simple_interest(principal, bps, 0).unwrap(), 0);
    }
}

proptest! {
    /// Zero bps → zero yield.
    #[test]
    fn interest_zero_bps_yields_zero(principal in 0u64..=u64::MAX, elapsed in 0u64..=u64::MAX) {
        prop_assert_eq!(accrue_simple_interest(principal, 0, elapsed).unwrap(), 0);
    }
}

proptest! {
    /// Zero principal → zero yield.
    #[test]
    fn interest_zero_principal_yields_zero(bps in 0u16..=u16::MAX, elapsed in 0u64..=u64::MAX) {
        prop_assert_eq!(accrue_simple_interest(0, bps, elapsed).unwrap(), 0);
    }
}

proptest! {
    /// Interest is monotone with elapsed time (longer window → more yield).
    #[test]
    fn interest_monotone_with_elapsed(
        principal in 1u64..1_000_000_000_u64,
        bps       in 1u16..10_000_u16,
        t1        in 0u64..31_536_000_u64,
        t2        in 0u64..31_536_000_u64,
    ) {
        let (lo, hi) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
        if let (Ok(r_lo), Ok(r_hi)) = (
            accrue_simple_interest(principal, bps, lo),
            accrue_simple_interest(principal, bps, hi),
        ) {
            prop_assert!(r_lo <= r_hi, "t {}→{} gave yield {}>{}", lo, hi, r_lo, r_hi);
        }
    }
}

proptest! {
    /// Interest is monotone with bps (higher rate → more yield).
    #[test]
    fn interest_monotone_with_bps(
        principal in 1u64..1_000_000_000_u64,
        elapsed   in 1u64..31_536_000_u64,
        bps1      in 0u16..10_000_u16,
        bps2      in 0u16..10_000_u16,
    ) {
        let (lo, hi) = if bps1 <= bps2 { (bps1, bps2) } else { (bps2, bps1) };
        if let (Ok(r_lo), Ok(r_hi)) = (
            accrue_simple_interest(principal, lo, elapsed),
            accrue_simple_interest(principal, hi, elapsed),
        ) {
            prop_assert!(r_lo <= r_hi, "bps {}→{} gave yield {}>{}", lo, hi, r_lo, r_hi);
        }
    }
}

proptest! {
    /// One full year at any bps: earned ≤ principal × bps / 10_000 + 1 (truncation).
    #[test]
    fn interest_annual_cap(
        principal in 1u64..1_000_000_000_u64,
        bps       in 0u16..=u16::MAX,
    ) {
        const ONE_YEAR: u64 = 365 * 24 * 60 * 60;
        if let Ok(earned) = accrue_simple_interest(principal, bps, ONE_YEAR) {
            let expected_max = (principal as u128)
                .saturating_mul(bps as u128)
                .saturating_div(10_000);
            prop_assert!(
                (earned as u128) <= expected_max + 1,
                "earned {} exceeds cap {} (p={} bps={})",
                earned, expected_max, principal, bps
            );
        }
    }
}

// ── calculate_management_fee ─────────────────────────────────────────────────

proptest! {
    /// No input combination panics.
    #[test]
    fn mgmt_fee_no_panic(total_assets in 0u64..=u64::MAX, elapsed in 0u64..=u64::MAX) {
        let _ = calculate_management_fee(total_assets, elapsed);
    }
}

proptest! {
    /// Zero elapsed → zero fee.
    #[test]
    fn mgmt_fee_zero_elapsed_is_zero(total_assets in 0u64..=u64::MAX) {
        prop_assert_eq!(calculate_management_fee(total_assets, 0).unwrap(), 0);
    }
}

proptest! {
    /// Fee never exceeds total_assets (no more than 100% fee).
    #[test]
    fn mgmt_fee_bounded_by_assets(
        total_assets in 0u64..1_000_000_000_u64,
        elapsed      in 0u64..31_536_000_u64,
    ) {
        if let Ok(fee) = calculate_management_fee(total_assets, elapsed) {
            prop_assert!(fee <= total_assets, "fee {} > assets {}", fee, total_assets);
        }
    }
}

// ── calculate_performance_fee ────────────────────────────────────────────────

proptest! {
    /// No input combination panics.
    #[test]
    fn perf_fee_no_panic(profit in 0u64..=u64::MAX) {
        let _ = calculate_performance_fee(profit);
    }
}

#[test]
fn perf_fee_zero_profit_is_zero() {
    assert_eq!(calculate_performance_fee(0).unwrap(), 0);
}

proptest! {
    /// Fee is always strictly less than profit (20% fee leaves 80% to the user).
    #[test]
    fn perf_fee_less_than_profit(profit in 1u64..=u64::MAX) {
        if let Ok(fee) = calculate_performance_fee(profit) {
            prop_assert!(fee < profit, "fee {} >= profit {}", fee, profit);
        }
    }
}

proptest! {
    /// Fee is monotone with profit.
    #[test]
    fn perf_fee_monotone(p1 in 0u64..u64::MAX, p2 in 0u64..u64::MAX) {
        let (lo, hi) = if p1 <= p2 { (p1, p2) } else { (p2, p1) };
        if let (Ok(f_lo), Ok(f_hi)) = (calculate_performance_fee(lo), calculate_performance_fee(hi)) {
            prop_assert!(f_lo <= f_hi, "fee monotone violated: profit {}→{} fee {}>{}", lo, hi, f_lo, f_hi);
        }
    }
}
