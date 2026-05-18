//! Stateful property-based fuzz tests.
//!
//! Simulates arbitrary sequences of deposit / withdraw / accrue operations
//! against an in-memory vault model and checks that core invariants hold
//! regardless of operation order or input values.
//!
//! Run: cargo test -p yield-vault-core

use proptest::prelude::*;
use yield_vault_core::math::{
    accrue_simple_interest, calculate_assets_for_shares, calculate_shares_to_mint,
};

// ── In-memory vault model ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
struct Vault {
    total_shares: u64,
    total_assets: u64,
}

impl Vault {
    fn deposit(&mut self, amount: u64) -> Option<u64> {
        if amount == 0 { return None; }
        let minted = calculate_shares_to_mint(amount, self.total_shares, self.total_assets).ok()?;
        if minted == 0 { return None; }
        self.total_assets = self.total_assets.checked_add(amount)?;
        self.total_shares = self.total_shares.checked_add(minted)?;
        Some(minted)
    }

    fn withdraw(&mut self, shares: u64) -> Option<u64> {
        if shares == 0 || shares > self.total_shares { return None; }
        let assets = calculate_assets_for_shares(shares, self.total_shares, self.total_assets).ok()?;
        self.total_assets = self.total_assets.checked_sub(assets)?;
        self.total_shares = self.total_shares.checked_sub(shares)?;
        Some(assets)
    }

    fn accrue(&mut self, bps: u16, elapsed_secs: u64) -> Option<u64> {
        let earned = accrue_simple_interest(self.total_assets, bps, elapsed_secs).ok()?;
        self.total_assets = self.total_assets.checked_add(earned)?;
        Some(earned)
    }

    // Share price as a u128 fraction ×10^9 (avoids floats).
    fn share_price_e9(&self) -> u128 {
        if self.total_shares == 0 { return 1_000_000_000; }
        (self.total_assets as u128) * 1_000_000_000 / (self.total_shares as u128)
    }
}

// ── Operation enum for arbitrary sequences ───────────────────────────────────

#[derive(Debug, Clone)]
enum Op {
    Deposit(u64),
    Withdraw(u64), // raw share count
    Accrue { bps: u16, elapsed: u64 },
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (1u64..1_000_000u64).prop_map(Op::Deposit),
        (1u64..1_000_000u64).prop_map(Op::Withdraw),
        (1u16..5_000u16, 1u64..86_400u64).prop_map(|(bps, elapsed)| Op::Accrue { bps, elapsed }),
    ]
}

// ── Invariant checks ─────────────────────────────────────────────────────────

// Invariant: total_assets / total_shares never decreases after an Accrue.
proptest! {
    #[test]
    fn share_price_never_decreases_on_accrue(
        ops in proptest::collection::vec(op_strategy(), 1..50),
    ) {
        let mut vault = Vault::default();
        let mut last_price = vault.share_price_e9();

        for op in &ops {
            match op {
                Op::Deposit(amount) => { vault.deposit(*amount); }
                Op::Withdraw(shares) => { vault.withdraw(*shares); }
                Op::Accrue { bps, elapsed } => {
                    let price_before = vault.share_price_e9();
                    vault.accrue(*bps, *elapsed);
                    let price_after = vault.share_price_e9();
                    prop_assert!(
                        price_after >= price_before,
                        "share price dropped after accrue: {} → {}",
                        price_before, price_after
                    );
                    last_price = price_after;
                }
            }
        }
        let _ = last_price; // suppress unused warning
    }
}

// Invariant: total_shares is always the correct running sum of all minted/burned shares.
proptest! {
    #[test]
    fn total_shares_tracks_mints_and_burns(
        amounts in proptest::collection::vec(1u64..500_000u64, 1..20),
    ) {
        let mut vault = Vault::default();
        let mut user_shares: Vec<u64> = Vec::new();

        // Deposit phase
        for &amount in &amounts {
            if let Some(minted) = vault.deposit(amount) {
                user_shares.push(minted);
            }
        }

        let expected_total: u64 = user_shares.iter().sum();
        prop_assert_eq!(vault.total_shares, expected_total,
            "total_shares {} ≠ sum of positions {}", vault.total_shares, expected_total);

        // Withdraw phase: redeem each user
        for shares in &user_shares {
            vault.withdraw(*shares);
        }

        prop_assert_eq!(vault.total_shares, 0,
            "total_shares {} ≠ 0 after all withdrawals", vault.total_shares);
    }
}

// Invariant: a single deposit followed by immediate withdrawal returns ≤ deposited amount.
proptest! {
    #[test]
    fn deposit_then_withdraw_no_free_money(amount in 1u64..1_000_000_u64) {
        let mut vault = Vault::default();
        if let Some(minted) = vault.deposit(amount) {
            if let Some(redeemed) = vault.withdraw(minted) {
                prop_assert!(redeemed <= amount,
                    "got back {} > deposited {}", redeemed, amount);
            }
        }
    }
}

// After all users fully withdraw, vault holds at most 1 token of dust per user
// (integer rounding can leave a tiny residue in total_assets).
proptest! {
    #[test]
    fn vault_drains_to_dust_after_all_withdrawals(
        amounts in proptest::collection::vec(1u64..100_000u64, 1..10),
    ) {
        let mut vault = Vault::default();
        let mut positions: Vec<u64> = Vec::new();

        for &a in &amounts {
            if let Some(s) = vault.deposit(a) {
                positions.push(s);
            }
        }

        for s in &positions {
            vault.withdraw(*s);
        }

        prop_assert_eq!(vault.total_shares, 0,
            "shares not zero after full drain: {}", vault.total_shares);
        // Rounding residue ≤ number of users (1 token of dust per user at most)
        prop_assert!(vault.total_assets <= positions.len() as u64,
            "total_assets {} too large after full drain (users={})",
            vault.total_assets, positions.len());
    }
}

// Accrual always increases (or maintains) total_assets.
proptest! {
    #[test]
    fn accrue_never_reduces_assets(
        seed_amount in 1u64..1_000_000u64,
        bps         in 0u16..10_000u16,
        elapsed     in 0u64..31_536_000u64,
    ) {
        let mut vault = Vault::default();
        vault.deposit(seed_amount);
        let assets_before = vault.total_assets;
        vault.accrue(bps, elapsed);
        prop_assert!(
            vault.total_assets >= assets_before,
            "assets reduced by accrue: {} → {}",
            assets_before, vault.total_assets
        );
    }
}

// Withdrawing more shares than owned silently fails (no state mutation).
proptest! {
    #[test]
    fn withdraw_excess_shares_is_no_op(
        deposit_amount in 1u64..1_000_000u64,
        extra          in 1u64..u64::MAX,
    ) {
        let mut vault = Vault::default();
        let minted = vault.deposit(deposit_amount).unwrap_or(0);
        prop_assume!(minted > 0);

        let shares_before = vault.total_shares;
        let assets_before = vault.total_assets;

        // Attempt to withdraw more than minted (should be rejected by None)
        let overflow_shares = minted.saturating_add(extra);
        let result = vault.withdraw(overflow_shares);

        prop_assert!(result.is_none(), "should have failed: got {:?}", result);
        prop_assert_eq!(vault.total_shares, shares_before);
        prop_assert_eq!(vault.total_assets, assets_before);
    }
}

// Multi-user: large deposit followed by many small deposits preserves total.
proptest! {
    #[test]
    fn multi_deposit_share_sum_equals_total(
        big_amount  in 1_000u64..1_000_000u64,
        small_count in 1usize..10usize,
        small_amt   in 1u64..1_000u64,
    ) {
        let mut vault = Vault::default();
        let mut total_minted: u64 = 0;

        if let Some(s) = vault.deposit(big_amount) { total_minted = total_minted.saturating_add(s); }
        for _ in 0..small_count {
            if let Some(s) = vault.deposit(small_amt) { total_minted = total_minted.saturating_add(s); }
        }

        prop_assert_eq!(vault.total_shares, total_minted,
            "total_shares {} ≠ running mint sum {}", vault.total_shares, total_minted);
    }
}
