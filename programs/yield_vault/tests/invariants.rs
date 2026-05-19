//! Invariant tests for yield_vault deposit/withdraw logic.
//!
//! Uses an in-memory vault model that mirrors the inline math in the program's
//! deposit.rs and withdraw.rs instructions.  No Anchor runtime required.
//!
//! Run: cargo test -p yield-vault

use proptest::prelude::*;

// ── In-memory model ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct Vault {
    total_shares: u64,
    total_assets: u64,
}

impl Vault {
    /// Mirrors yield_vault deposit.rs handler math exactly.
    fn deposit(&mut self, amount: u64) -> Option<u64> {
        if amount == 0 {
            return None;
        }
        let minted: u64 = if self.total_shares == 0 {
            amount
        } else {
            let s = (amount as u128)
                .checked_mul(self.total_shares as u128)?
                .checked_div(self.total_assets as u128)? as u64;
            if s == 0 {
                return None;
            }
            s
        };
        self.total_assets = self.total_assets.checked_add(amount)?;
        self.total_shares = self.total_shares.checked_add(minted)?;
        Some(minted)
    }

    /// Mirrors yield_vault withdraw.rs handler math exactly.
    fn withdraw(&mut self, shares: u64) -> Option<u64> {
        if shares == 0 || shares > self.total_shares || self.total_shares == 0 {
            return None;
        }
        let assets = ((shares as u128)
            .checked_mul(self.total_assets as u128)?
            .checked_div(self.total_shares as u128)?) as u64;
        self.total_assets = self.total_assets.checked_sub(assets)?;
        self.total_shares = self.total_shares.checked_sub(shares)?;
        Some(assets)
    }

    fn share_price_e9(&self) -> u128 {
        if self.total_shares == 0 {
            return 1_000_000_000;
        }
        (self.total_assets as u128) * 1_000_000_000 / (self.total_shares as u128)
    }
}

// ── Stateless invariants ─────────────────────────────────────────────────────

proptest! {
    /// First deposit (empty vault) is always 1:1 — no panic.
    #[test]
    fn first_deposit_is_one_to_one(amount in 1u64..=u64::MAX) {
        let mut v = Vault::default();
        let minted = v.deposit(amount);
        prop_assert_eq!(minted, Some(amount));
        prop_assert_eq!(v.total_shares, amount);
        prop_assert_eq!(v.total_assets, amount);
    }
}

proptest! {
    /// Zero deposit is always rejected (returns None).
    #[test]
    fn zero_deposit_is_rejected(
        total_shares in 0u64..=u64::MAX,
        total_assets in 0u64..=u64::MAX,
    ) {
        let mut v = Vault { total_shares, total_assets };
        prop_assert!(v.deposit(0).is_none());
    }
}

proptest! {
    /// Immediate deposit → withdraw roundtrip: user recovers ≤ deposited amount.
    /// Integer floor-division means the user cannot profit from a single roundtrip.
    #[test]
    fn deposit_withdraw_no_free_money(amount in 1u64..1_000_000u64) {
        let mut v = Vault::default();
        if let Some(minted) = v.deposit(amount) {
            if let Some(redeemed) = v.withdraw(minted) {
                prop_assert!(redeemed <= amount,
                    "recovered {} > deposited {}", redeemed, amount);
            }
        }
    }
}

proptest! {
    /// Depositing with an existing vault never inflates the share supply.
    /// Equivalently: minted × total_assets ≤ amount × total_shares.
    #[test]
    fn deposit_does_not_inflate_share_supply(
        amount       in 1u64..500_000_000u64,
        total_shares in 1u64..500_000_000u64,
        total_assets in 1u64..500_000_000u64,
    ) {
        let mut v = Vault { total_shares, total_assets };
        if let Some(minted) = v.deposit(amount) {
            let lhs = (minted as u128).saturating_mul(total_assets as u128);
            let rhs = (amount as u128).saturating_mul(total_shares as u128);
            prop_assert!(lhs <= rhs,
                "inflation: minted={} * assets={} > amount={} * shares={}",
                minted, total_assets, amount, total_shares);
        }
    }
}

proptest! {
    /// Withdrawing more shares than owned silently fails, leaving state unchanged.
    #[test]
    fn withdraw_excess_is_no_op(
        deposit_amount in 1u64..500_000u64,
        extra          in 1u64..u64::MAX,
    ) {
        let mut v = Vault::default();
        let minted = match v.deposit(deposit_amount) {
            Some(s) => s,
            None => return Ok(()),
        };
        let snap_shares = v.total_shares;
        let snap_assets = v.total_assets;

        let excess = minted.saturating_add(extra);
        prop_assert!(v.withdraw(excess).is_none());
        prop_assert_eq!(v.total_shares, snap_shares);
        prop_assert_eq!(v.total_assets, snap_assets);
    }
}

// ── Stateful invariants ──────────────────────────────────────────────────────

proptest! {
    /// total_shares is always the exact sum of all minted shares, regardless of
    /// deposit order or amounts.
    #[test]
    fn total_shares_equals_sum_of_minted(
        amounts in proptest::collection::vec(1u64..200_000u64, 1..15),
    ) {
        let mut v = Vault::default();
        let mut minted_sum: u64 = 0;

        for &a in &amounts {
            if let Some(s) = v.deposit(a) {
                minted_sum = minted_sum.saturating_add(s);
            }
        }
        prop_assert_eq!(v.total_shares, minted_sum,
            "total_shares {} ≠ running mint sum {}", v.total_shares, minted_sum);
    }
}

proptest! {
    /// After all users withdraw every share they minted, total_shares == 0.
    /// total_assets may have up to 1 token of rounding dust per user.
    #[test]
    fn vault_drains_after_all_withdrawals(
        amounts in proptest::collection::vec(1u64..100_000u64, 1..8),
    ) {
        let mut v = Vault::default();
        let mut positions: Vec<u64> = Vec::new();

        for &a in &amounts {
            if let Some(s) = v.deposit(a) {
                positions.push(s);
            }
        }
        for s in &positions {
            v.withdraw(*s);
        }

        prop_assert_eq!(v.total_shares, 0,
            "total_shares {} ≠ 0 after full drain", v.total_shares);
        prop_assert!(v.total_assets <= positions.len() as u64,
            "dust {} > users {}", v.total_assets, positions.len());
    }
}

proptest! {
    /// Share price (assets/shares) never drops when a deposit is made at the
    /// current share price (it can only stay flat or rise due to floor rounding).
    #[test]
    fn deposit_share_price_does_not_decrease(
        seed   in 1u64..500_000u64,
        amount in 1u64..500_000u64,
    ) {
        let mut v = Vault::default();
        v.deposit(seed);
        let price_before = v.share_price_e9();
        v.deposit(amount);
        let price_after = v.share_price_e9();
        prop_assert!(price_after >= price_before,
            "share price fell after deposit: {} → {}", price_before, price_after);
    }
}

proptest! {
    /// A larger deposit mints proportionally more shares (within ±1 rounding).
    #[test]
    fn deposit_proportionality(
        amount       in 2u64..250_000u64,
        total_shares in 1u64..250_000u64,
        total_assets in 1u64..250_000u64,
    ) {
        let mut v1 = Vault { total_shares, total_assets };
        let mut v2 = Vault { total_shares, total_assets };
        if let (Some(s1), Some(s2)) = (v1.deposit(amount), v2.deposit(amount * 2)) {
            prop_assert!(
                s2 >= s1.saturating_mul(2).saturating_sub(1)
                    && s2 <= s1.saturating_mul(2).saturating_add(1),
                "non-proportional: s1={} s2={} expected s2≈2×s1", s1, s2
            );
        }
    }
}
