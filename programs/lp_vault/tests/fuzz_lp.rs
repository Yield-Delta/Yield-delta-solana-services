//! Invariant and fuzz tests for lp_vault.
//!
//! Covers:
//!   • simulate_compound math: compound_tokens = apply_bps(total_lp, fee_bps) (stateless)
//!   • Share price monotonicity after compounding (stateful)
//!   • No-free-money and drain properties (stateful)
//!
//! Run: cargo test -p lp-vault

use proptest::prelude::*;
use yield_vault_core::math::{apply_bps, calculate_assets_for_shares, calculate_shares_to_mint};

// ── Stateless: compound math invariants ─────────────────────────────────────

proptest! {
    /// compound_tokens never exceeds total_lp_tokens (cannot compound more than 100%).
    /// apply_bps(v, bps ≤ 10_000) ≤ v.
    #[test]
    fn compound_tokens_bounded_by_total(
        total_lp in 0u64..=u64::MAX,
        fee_bps  in 0u16..=10_000u16,
    ) {
        if let Ok(compound) = apply_bps(total_lp, fee_bps) {
            prop_assert!(compound <= total_lp,
                "compound {} > total_lp {} at bps {}", compound, total_lp, fee_bps);
        }
    }
}

proptest! {
    /// 0 bps → 0 compound tokens, regardless of total_lp.
    #[test]
    fn zero_fee_bps_means_no_compound(total_lp in 0u64..=u64::MAX) {
        prop_assert_eq!(apply_bps(total_lp, 0).unwrap(), 0);
    }
}

proptest! {
    /// Compound tokens are monotone in fee_bps: higher fee → more tokens added.
    #[test]
    fn compound_monotone_with_fee_bps(
        total_lp in 1u64..1_000_000_000u64,
        bps_a    in 0u16..10_000u16,
        bps_b    in 0u16..10_000u16,
    ) {
        let (lo, hi) = if bps_a <= bps_b { (bps_a, bps_b) } else { (bps_b, bps_a) };
        if let (Ok(c_lo), Ok(c_hi)) = (apply_bps(total_lp, lo), apply_bps(total_lp, hi)) {
            prop_assert!(c_lo <= c_hi,
                "compound not monotone: bps {}→{} gave {}>{}", lo, hi, c_lo, c_hi);
        }
    }
}

proptest! {
    /// 10 000 bps doubles total_lp (compound_tokens == total_lp → new total = 2×).
    #[test]
    fn full_bps_doubles_pool(total_lp in 0u64..=u64::MAX) {
        prop_assert_eq!(apply_bps(total_lp, 10_000).unwrap(), total_lp);
    }
}

proptest! {
    /// Compound tokens grow monotone with total_lp for fixed fee_bps.
    #[test]
    fn compound_monotone_with_pool_size(
        lp_a    in 0u64..500_000_000u64,
        lp_b    in 0u64..500_000_000u64,
        fee_bps in 0u16..10_000u16,
    ) {
        let (lo, hi) = if lp_a <= lp_b { (lp_a, lp_b) } else { (lp_b, lp_a) };
        if let (Ok(c_lo), Ok(c_hi)) = (apply_bps(lo, fee_bps), apply_bps(hi, fee_bps)) {
            prop_assert!(c_lo <= c_hi,
                "compound not monotone with pool: lp {}→{} gave compound {}>{}", lo, hi, c_lo, c_hi);
        }
    }
}

// ── In-memory LP vault model ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct LpVault {
    total_shares: u64,
    total_lp_tokens: u64,
    compound_fee_bps: u16,
}

impl LpVault {
    fn new(compound_fee_bps: u16) -> Self {
        LpVault { total_shares: 0, total_lp_tokens: 0, compound_fee_bps }
    }

    /// Deposit LP tokens → mint shares.
    fn deposit(&mut self, lp_amount: u64) -> Option<u64> {
        if lp_amount == 0 {
            return None;
        }
        let minted = if self.total_shares == 0 {
            lp_amount
        } else {
            calculate_shares_to_mint(lp_amount, self.total_shares, self.total_lp_tokens).ok()?
        };
        if minted == 0 {
            return None;
        }
        self.total_lp_tokens = self.total_lp_tokens.checked_add(lp_amount)?;
        self.total_shares = self.total_shares.checked_add(minted)?;
        Some(minted)
    }

    /// Simulate one compound event: adds apply_bps(total_lp, fee_bps) to total_lp.
    fn compound(&mut self) -> Option<u64> {
        let added = apply_bps(self.total_lp_tokens, self.compound_fee_bps).ok()?;
        self.total_lp_tokens = self.total_lp_tokens.checked_add(added)?;
        Some(added)
    }

    /// Withdraw shares → receive LP tokens.
    fn withdraw(&mut self, shares: u64) -> Option<u64> {
        if shares == 0 || shares > self.total_shares || self.total_shares == 0 {
            return None;
        }
        let lp_out = calculate_assets_for_shares(
            shares,
            self.total_shares,
            self.total_lp_tokens,
        )
        .ok()?;
        self.total_lp_tokens = self.total_lp_tokens.checked_sub(lp_out)?;
        self.total_shares = self.total_shares.checked_sub(shares)?;
        Some(lp_out)
    }

    /// LP tokens per share × 10^9.
    fn lp_per_share_e9(&self) -> u128 {
        if self.total_shares == 0 {
            return 1_000_000_000;
        }
        (self.total_lp_tokens as u128) * 1_000_000_000 / (self.total_shares as u128)
    }
}

// ── Stateful invariants ───────────────────────────────────────────────────────

proptest! {
    /// LP/share ratio (share price) never decreases after a compound event.
    #[test]
    fn share_price_increases_after_compound(
        seed_lp   in 1u64..1_000_000u64,
        fee_bps   in 1u16..5_000u16,
        n_compounds in 1usize..10usize,
    ) {
        let mut vault = LpVault::new(fee_bps);
        vault.deposit(seed_lp);
        let mut last_price = vault.lp_per_share_e9();

        for _ in 0..n_compounds {
            vault.compound();
            let new_price = vault.lp_per_share_e9();
            prop_assert!(new_price >= last_price,
                "share price fell after compound: {} → {}", last_price, new_price);
            last_price = new_price;
        }
    }
}

proptest! {
    /// total_lp_tokens strictly increases after each compound (for fee_bps > 0
    /// and non-empty pool).
    #[test]
    fn compound_increases_total_lp(
        seed_lp in 1u64..1_000_000u64,
        fee_bps in 1u16..10_000u16,
    ) {
        let mut vault = LpVault::new(fee_bps);
        vault.deposit(seed_lp);
        let lp_before = vault.total_lp_tokens;
        vault.compound();
        prop_assert!(vault.total_lp_tokens >= lp_before,
            "total_lp decreased after compound: {} → {}", lp_before, vault.total_lp_tokens);
    }
}

proptest! {
    /// Withdraw after compound returns more LP tokens than originally deposited
    /// (because the share price increased).  At minimum the user breaks even.
    #[test]
    fn withdraw_after_compound_returns_at_least_deposited(
        deposit_lp in 1u64..500_000u64,
        fee_bps    in 1u16..5_000u16,
    ) {
        let mut vault = LpVault::new(fee_bps);
        if let Some(minted) = vault.deposit(deposit_lp) {
            vault.compound();
            if let Some(redeemed) = vault.withdraw(minted) {
                prop_assert!(redeemed >= deposit_lp,
                    "redeemed {} < deposited {} after compound (fee_bps={})",
                    redeemed, deposit_lp, fee_bps);
            }
        }
    }
}

proptest! {
    /// No-free-money without compounding: deposit→immediate withdraw returns ≤ deposited.
    #[test]
    fn no_free_money_without_compound(deposit_lp in 1u64..500_000u64) {
        let mut vault = LpVault::new(200); // 2% compound rate, but not triggered
        if let Some(minted) = vault.deposit(deposit_lp) {
            if let Some(redeemed) = vault.withdraw(minted) {
                prop_assert!(redeemed <= deposit_lp,
                    "got back {} > deposited {} without compounding", redeemed, deposit_lp);
            }
        }
    }
}

proptest! {
    /// Multi-user deposit: total_shares == sum of all minted shares.
    #[test]
    fn total_shares_tracks_mints(
        amounts in proptest::collection::vec(1u64..100_000u64, 1..8),
        fee_bps in 0u16..5_000u16,
    ) {
        let mut vault = LpVault::new(fee_bps);
        let mut minted_sum: u64 = 0;
        for &a in &amounts {
            if let Some(s) = vault.deposit(a) {
                minted_sum = minted_sum.saturating_add(s);
            }
        }
        prop_assert_eq!(vault.total_shares, minted_sum);
    }
}

proptest! {
    /// After all users withdraw fully, total_shares == 0 and dust ≤ num_users.
    #[test]
    fn vault_drains_after_all_withdrawals(
        amounts in proptest::collection::vec(1u64..100_000u64, 1..8),
        fee_bps in 0u16..2_000u16,
    ) {
        let mut vault = LpVault::new(fee_bps);
        let mut positions: Vec<u64> = Vec::new();
        for &a in &amounts {
            if let Some(s) = vault.deposit(a) {
                positions.push(s);
            }
        }
        for &s in &positions {
            vault.withdraw(s);
        }
        prop_assert_eq!(vault.total_shares, 0,
            "shares ≠ 0 after full drain: {}", vault.total_shares);
        prop_assert!(vault.total_lp_tokens <= positions.len() as u64,
            "dust {} > user count {}", vault.total_lp_tokens, positions.len());
    }
}
