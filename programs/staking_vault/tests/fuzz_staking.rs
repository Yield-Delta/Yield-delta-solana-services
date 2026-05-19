//! Invariant and fuzz tests for staking_vault.
//!
//! Covers:
//!   • accrue_yield math: earned = accrue_simple_interest(sol_staked, bps, elapsed) (stateless)
//!   • SOL-per-stSOL exchange rate monotonicity (stateful)
//!   • Stake/unstake no-free-money and drain properties (stateful)
//!
//! Run: cargo test -p staking-vault

use proptest::prelude::*;
use yield_vault_core::math::{accrue_simple_interest, calculate_assets_for_shares, calculate_shares_to_mint};

// ── Stateless: yield accrual math invariants ─────────────────────────────────

proptest! {
    /// 0 elapsed → 0 yield, always.
    #[test]
    fn zero_elapsed_yields_nothing(
        principal in 0u64..=u64::MAX,
        yield_bps in 0u16..=u16::MAX,
    ) {
        prop_assert_eq!(accrue_simple_interest(principal, yield_bps, 0).unwrap(), 0);
    }
}

proptest! {
    /// 0 bps → 0 yield, always.
    #[test]
    fn zero_yield_bps_earns_nothing(
        principal in 0u64..=u64::MAX,
        elapsed   in 0u64..=u64::MAX,
    ) {
        prop_assert_eq!(accrue_simple_interest(principal, 0, elapsed).unwrap(), 0);
    }
}

proptest! {
    /// 0 principal → 0 yield.
    #[test]
    fn zero_principal_earns_nothing(
        yield_bps in 0u16..=u16::MAX,
        elapsed   in 0u64..=u64::MAX,
    ) {
        prop_assert_eq!(accrue_simple_interest(0, yield_bps, elapsed).unwrap(), 0);
    }
}

proptest! {
    /// Yield earned is monotone in elapsed time: longer window → more earned.
    #[test]
    fn yield_monotone_with_elapsed(
        principal in 1u64..1_000_000_000u64,
        yield_bps in 1u16..10_000u16,
        t1        in 0u64..31_536_000u64,
        t2        in 0u64..31_536_000u64,
    ) {
        let (lo, hi) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
        if let (Ok(e_lo), Ok(e_hi)) = (
            accrue_simple_interest(principal, yield_bps, lo),
            accrue_simple_interest(principal, yield_bps, hi),
        ) {
            prop_assert!(e_lo <= e_hi,
                "yield not monotone with time: t {}→{} gave {}>{}", lo, hi, e_lo, e_hi);
        }
    }
}

proptest! {
    /// Yield earned is monotone in bps: higher yield rate → more earned.
    #[test]
    fn yield_monotone_with_bps(
        principal in 1u64..1_000_000_000u64,
        elapsed   in 1u64..31_536_000u64,
        bps_a     in 0u16..10_000u16,
        bps_b     in 0u16..10_000u16,
    ) {
        let (lo, hi) = if bps_a <= bps_b { (bps_a, bps_b) } else { (bps_b, bps_a) };
        if let (Ok(e_lo), Ok(e_hi)) = (
            accrue_simple_interest(principal, lo, elapsed),
            accrue_simple_interest(principal, hi, elapsed),
        ) {
            prop_assert!(e_lo <= e_hi,
                "yield not monotone with bps: bps {}→{} gave {}>{}", lo, hi, e_lo, e_hi);
        }
    }
}

proptest! {
    /// One full year at bps ≤ 10 000: earned ≤ principal (simple interest, ≤ 100% APY).
    #[test]
    fn annual_yield_bounded_by_principal(
        principal in 1u64..1_000_000_000u64,
        yield_bps in 0u16..10_000u16,
    ) {
        const ONE_YEAR: u64 = 365 * 24 * 60 * 60;
        if let Ok(earned) = accrue_simple_interest(principal, yield_bps, ONE_YEAR) {
            prop_assert!(earned <= principal,
                "annual yield {} > principal {} at bps {}", earned, principal, yield_bps);
        }
    }
}

// ── In-memory staking vault model ────────────────────────────────────────────

#[derive(Debug, Clone)]
struct StakingVault {
    total_sol_staked: u64,
    total_shares: u64,
    yield_bps: u16,
}

impl StakingVault {
    fn new(yield_bps: u16) -> Self {
        StakingVault { total_sol_staked: 0, total_shares: 0, yield_bps }
    }

    /// Stake lamports → mint stSOL shares.
    fn stake(&mut self, lamports: u64) -> Option<u64> {
        if lamports == 0 {
            return None;
        }
        let minted = if self.total_shares == 0 {
            lamports
        } else {
            calculate_shares_to_mint(lamports, self.total_shares, self.total_sol_staked).ok()?
        };
        if minted == 0 {
            return None;
        }
        self.total_sol_staked = self.total_sol_staked.checked_add(lamports)?;
        self.total_shares = self.total_shares.checked_add(minted)?;
        Some(minted)
    }

    /// Accrue simulated yield for `elapsed_secs` seconds.
    fn accrue(&mut self, elapsed_secs: u64) -> Option<u64> {
        let earned =
            accrue_simple_interest(self.total_sol_staked, self.yield_bps, elapsed_secs).ok()?;
        self.total_sol_staked = self.total_sol_staked.checked_add(earned)?;
        Some(earned)
    }

    /// Unstake stSOL shares → receive lamports.
    fn unstake(&mut self, shares: u64) -> Option<u64> {
        if shares == 0 || shares > self.total_shares || self.total_shares == 0 {
            return None;
        }
        let lamports = calculate_assets_for_shares(
            shares,
            self.total_shares,
            self.total_sol_staked,
        )
        .ok()?;
        self.total_sol_staked = self.total_sol_staked.checked_sub(lamports)?;
        self.total_shares = self.total_shares.checked_sub(shares)?;
        Some(lamports)
    }

    /// SOL per stSOL × 10^9 (exchange rate).
    fn sol_per_share_e9(&self) -> u128 {
        if self.total_shares == 0 {
            return 1_000_000_000;
        }
        (self.total_sol_staked as u128) * 1_000_000_000 / (self.total_shares as u128)
    }
}

// ── Stateful invariants ───────────────────────────────────────────────────────

proptest! {
    /// Exchange rate (SOL/stSOL) never decreases after accrual.
    #[test]
    fn exchange_rate_never_decreases_on_accrue(
        seed       in 1u64..1_000_000u64,
        yield_bps  in 1u16..10_000u16,
        n_accruals in 1usize..10usize,
        elapsed    in 1u64..86_400u64,
    ) {
        let mut vault = StakingVault::new(yield_bps);
        vault.stake(seed);
        let mut last_rate = vault.sol_per_share_e9();

        for _ in 0..n_accruals {
            vault.accrue(elapsed);
            let new_rate = vault.sol_per_share_e9();
            prop_assert!(new_rate >= last_rate,
                "exchange rate fell after accrue: {} → {}", last_rate, new_rate);
            last_rate = new_rate;
        }
    }
}

proptest! {
    /// total_sol_staked never decreases after accrue (yield is always non-negative).
    #[test]
    fn accrual_never_reduces_total_sol(
        seed      in 1u64..1_000_000u64,
        yield_bps in 0u16..10_000u16,
        elapsed   in 0u64..31_536_000u64,
    ) {
        let mut vault = StakingVault::new(yield_bps);
        vault.stake(seed);
        let sol_before = vault.total_sol_staked;
        vault.accrue(elapsed);
        prop_assert!(vault.total_sol_staked >= sol_before,
            "sol decreased: {} → {}", sol_before, vault.total_sol_staked);
    }
}

proptest! {
    /// Unstake after accrual returns ≥ originally staked (accrual increased the rate).
    #[test]
    fn unstake_after_accrue_returns_at_least_staked(
        stake_lamports in 1u64..500_000u64,
        yield_bps      in 1u16..5_000u16,
        elapsed        in 1u64..31_536_000u64,
    ) {
        let mut vault = StakingVault::new(yield_bps);
        if let Some(minted) = vault.stake(stake_lamports) {
            vault.accrue(elapsed);
            if let Some(returned) = vault.unstake(minted) {
                prop_assert!(returned >= stake_lamports,
                    "returned {} < staked {} after accrue (bps={} elapsed={})",
                    returned, stake_lamports, yield_bps, elapsed);
            }
        }
    }
}

proptest! {
    /// No-free-money without accrual: unstake immediately returns ≤ staked.
    #[test]
    fn no_free_money_without_accrue(lamports in 1u64..500_000u64) {
        let mut vault = StakingVault::new(700); // 7% APY
        if let Some(minted) = vault.stake(lamports) {
            if let Some(returned) = vault.unstake(minted) {
                prop_assert!(returned <= lamports,
                    "returned {} > staked {} (no accrue)", returned, lamports);
            }
        }
    }
}

proptest! {
    /// Later stakers receive fewer stSOL per SOL than earlier stakers (after accrual).
    /// First staker's exchange rate remains ≥ later staker's entry price.
    #[test]
    fn later_staker_gets_fewer_shares(
        first_stake  in 1u64..500_000u64,
        second_stake in 1u64..500_000u64,
        yield_bps    in 1u16..5_000u16,
        elapsed      in 86_400u64..31_536_000u64,
    ) {
        let mut vault = StakingVault::new(yield_bps);

        // First staker deposits before accrual — 1:1 rate.
        let first_minted = vault.stake(first_stake).unwrap_or(0);
        prop_assume!(first_minted > 0);

        // Accrue yield, increasing the exchange rate.
        vault.accrue(elapsed);

        // Second staker deposits after accrual — gets fewer shares per SOL.
        let second_minted = vault.stake(second_stake).unwrap_or(0);

        if second_minted > 0 {
            let first_rate = (first_stake as u128) * 1_000 / (first_minted as u128);
            let second_rate = (second_stake as u128) * 1_000 / (second_minted as u128);
            // Second staker pays more SOL per share (higher rate = more expensive entry).
            prop_assert!(second_rate >= first_rate,
                "second staker's entry rate {} < first staker's {}", second_rate, first_rate);
        }
    }
}

proptest! {
    /// total_shares accurately tracks all staked/unstaked positions.
    #[test]
    fn total_shares_tracks_stakes(
        stakes in proptest::collection::vec(1u64..100_000u64, 1..8),
    ) {
        let mut vault = StakingVault::new(700);
        let mut minted_sum: u64 = 0;

        for &s in &stakes {
            if let Some(m) = vault.stake(s) {
                minted_sum = minted_sum.saturating_add(m);
            }
        }
        prop_assert_eq!(vault.total_shares, minted_sum);
    }
}
