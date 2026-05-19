//! Invariant and fuzz tests for meta_vault.
//!
//! Covers:
//!   • validate_allocations: constraint checking (stateless)
//!   • Blended APY calculation: weighted average properties (stateless)
//!   • Deposit/accrue/withdraw sequences (stateful)
//!
//! Run: cargo test -p meta-vault

use meta_vault::instructions::initialize::validate_allocations;
use meta_vault::state::{AllocationSlot, MAX_ALLOCATIONS};
use proptest::prelude::*;
use yield_vault_core::math::{accrue_simple_interest, calculate_assets_for_shares, calculate_shares_to_mint};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a single-slot allocation with weight 10_000 (100%).
fn single_slot(strategy_id: u8, apy_bps: u16) -> Vec<AllocationSlot> {
    vec![AllocationSlot { strategy_id, weight_bps: 10_000, simulated_apy_bps: apy_bps }]
}

/// Build an even N-way split over distinct strategy ids 1..=N.
fn even_split(n: usize, apy_bps: u16) -> Vec<AllocationSlot> {
    assert!(n >= 1 && n <= MAX_ALLOCATIONS);
    let per = (10_000u16 / n as u16) as u16;
    let mut slots: Vec<AllocationSlot> = (1..=n as u8)
        .map(|id| AllocationSlot { strategy_id: id, weight_bps: per, simulated_apy_bps: apy_bps })
        .collect();
    // Fix rounding so weights sum exactly to 10_000.
    let residual = 10_000u16 - per * n as u16;
    slots[0].weight_bps += residual;
    slots
}

/// Compute blended bps the same way accrue_returns.rs does.
fn blended_bps(slots: &[AllocationSlot]) -> u16 {
    let numerator: u64 = slots
        .iter()
        .filter(|s| s.strategy_id != 0)
        .map(|s| s.weight_bps as u64 * s.simulated_apy_bps as u64)
        .sum();
    (numerator / 10_000) as u16
}

// ── validate_allocations stateless fuzz ──────────────────────────────────────

#[test]
fn single_slot_full_weight_is_valid() {
    let slots = single_slot(1, 500);
    assert!(validate_allocations(&slots).is_ok());
}

#[test]
fn even_split_all_sizes_are_valid() {
    for n in 1..=MAX_ALLOCATIONS {
        let slots = even_split(n, 800);
        assert!(validate_allocations(&slots).is_ok(), "even split n={} failed", n);
    }
}

#[test]
fn empty_allocation_list_fails() {
    // No active slots — sum == 0 ≠ 10_000.
    let result = validate_allocations(&[]);
    assert!(result.is_err(), "empty allocations should fail (sum=0)");
}

#[test]
fn too_many_allocations_fails() {
    let mut slots = even_split(MAX_ALLOCATIONS, 500);
    slots.push(AllocationSlot { strategy_id: 99, weight_bps: 0, simulated_apy_bps: 0 });
    let result = validate_allocations(&slots);
    assert!(result.is_err(), "more than MAX_ALLOCATIONS should fail");
}

proptest! {
    /// Weight sum ≠ 10_000 always fails validation.
    #[test]
    fn wrong_weight_sum_fails(
        weight in 0u16..9_999u16, // any sum < 10_000
    ) {
        let slots = vec![
            AllocationSlot { strategy_id: 1, weight_bps: weight, simulated_apy_bps: 500 },
        ];
        prop_assert!(validate_allocations(&slots).is_err(),
            "weight {} should fail (expected 10_000)", weight);
    }
}

#[test]
fn duplicate_strategy_ids_fail() {
    let slots = vec![
        AllocationSlot { strategy_id: 1, weight_bps: 5_000, simulated_apy_bps: 500 },
        AllocationSlot { strategy_id: 1, weight_bps: 5_000, simulated_apy_bps: 700 },
    ];
    assert!(validate_allocations(&slots).is_err(), "duplicate strategy_ids should fail");
}

proptest! {
    /// A single allocation with exactly 10_000 weight_bps always passes, regardless
    /// of apy_bps.
    #[test]
    fn single_full_weight_slot_always_valid(
        strategy_id in 1u8..=u8::MAX,
        apy_bps     in 0u16..=u16::MAX,
    ) {
        let slots = vec![AllocationSlot {
            strategy_id,
            weight_bps: 10_000,
            simulated_apy_bps: apy_bps,
        }];
        prop_assert!(validate_allocations(&slots).is_ok());
    }
}

// ── Blended APY stateless invariants ─────────────────────────────────────────

proptest! {
    /// For a single allocation: blended_bps == simulated_apy_bps (identity).
    #[test]
    fn single_slot_blended_equals_apy(apy_bps in 0u16..=u16::MAX) {
        let slots = single_slot(1, apy_bps);
        prop_assert_eq!(blended_bps(&slots), apy_bps);
    }
}

proptest! {
    /// Blended APY is bounded below by min(apy) and above by max(apy) of active slots.
    #[test]
    fn blended_bps_is_within_extremes(
        apy_lo in 0u16..5_000u16,
        apy_hi in 5_000u16..10_000u16,
    ) {
        // Two-way even split: strategy 1 = apy_lo, strategy 2 = apy_hi.
        let slots = vec![
            AllocationSlot { strategy_id: 1, weight_bps: 5_000, simulated_apy_bps: apy_lo },
            AllocationSlot { strategy_id: 2, weight_bps: 5_000, simulated_apy_bps: apy_hi },
        ];
        let blended = blended_bps(&slots);
        prop_assert!(blended >= apy_lo && blended <= apy_hi,
            "blended {} not in [{}, {}]", blended, apy_lo, apy_hi);
    }
}

proptest! {
    /// When all active slots have the same apy_bps, blended == that apy_bps.
    #[test]
    fn uniform_apy_blended_equals_apy(
        n_slots in 1usize..=MAX_ALLOCATIONS,
        apy_bps in 0u16..10_000u16,
    ) {
        let slots = even_split(n_slots, apy_bps);
        let b = blended_bps(&slots);
        // Allow ±1 for integer rounding in the even split.
        prop_assert!(b.saturating_sub(apy_bps) <= 1 && apy_bps.saturating_sub(b) <= 1,
            "uniform apy {}: blended {} differs by more than 1", apy_bps, b);
    }
}

proptest! {
    /// Blended APY increases when apy_bps of one slot increases (others fixed).
    #[test]
    fn blended_monotone_with_apy(
        apy_lo in 0u16..5_000u16,
        apy_hi in 5_000u16..10_000u16,
    ) {
        let lo_slots = single_slot(1, apy_lo);
        let hi_slots = single_slot(1, apy_hi);
        prop_assert!(blended_bps(&lo_slots) <= blended_bps(&hi_slots),
            "blended not monotone: lo={} > hi={}", blended_bps(&lo_slots), blended_bps(&hi_slots));
    }
}

// ── In-memory meta vault model ────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct MetaVault {
    total_shares: u64,
    total_assets: u64,
    accumulated_fees: u64,
    allocations: Vec<AllocationSlot>,
}

impl MetaVault {
    fn new(allocations: Vec<AllocationSlot>) -> Self {
        MetaVault { total_shares: 0, total_assets: 0, accumulated_fees: 0, allocations }
    }

    fn deposit(&mut self, amount: u64) -> Option<u64> {
        if amount == 0 { return None; }
        let net_assets = self.total_assets.saturating_sub(self.accumulated_fees);
        let minted = if self.total_shares == 0 {
            amount
        } else {
            calculate_shares_to_mint(amount, self.total_shares, net_assets).ok()?
        };
        if minted == 0 { return None; }
        self.total_assets = self.total_assets.checked_add(amount)?;
        self.total_shares = self.total_shares.checked_add(minted)?;
        Some(minted)
    }

    fn withdraw(&mut self, shares: u64) -> Option<u64> {
        if shares == 0 || shares > self.total_shares || self.total_shares == 0 { return None; }
        // Mirror the fix: use net_assets (same basis as deposit) to prevent fee siphoning.
        let net_assets = self.total_assets.saturating_sub(self.accumulated_fees);
        let out = calculate_assets_for_shares(shares, self.total_shares, net_assets).ok()?;
        self.total_assets = self.total_assets.checked_sub(out)?;
        self.total_shares = self.total_shares.checked_sub(shares)?;
        Some(out)
    }

    fn accrue(&mut self, elapsed_secs: u64) -> Option<u64> {
        let bps = blended_bps(&self.allocations);
        let earned = accrue_simple_interest(self.total_assets, bps, elapsed_secs).ok()?;
        self.total_assets = self.total_assets.checked_add(earned)?;
        Some(earned)
    }

    fn share_price_e9(&self) -> u128 {
        if self.total_shares == 0 { return 1_000_000_000; }
        (self.total_assets as u128) * 1_000_000_000 / (self.total_shares as u128)
    }
}

// ── Stateful invariants ────────────────────────────────────────────────────────

proptest! {
    /// Share price never decreases after accrue_returns.
    #[test]
    fn share_price_never_decreases_after_accrue(
        seed     in 1u64..1_000_000u64,
        apy_bps  in 0u16..10_000u16,
        elapsed  in 0u64..31_536_000u64,
    ) {
        let mut vault = MetaVault::new(single_slot(1, apy_bps));
        vault.deposit(seed);
        let price_before = vault.share_price_e9();
        vault.accrue(elapsed);
        let price_after = vault.share_price_e9();
        prop_assert!(price_after >= price_before,
            "share price fell after accrue: {} → {}", price_before, price_after);
    }
}

proptest! {
    /// Deposit → accrue → withdraw returns ≥ original deposit (yield positive).
    #[test]
    fn deposit_accrue_withdraw_at_least_principal(
        deposit_amount in 1u64..500_000u64,
        apy_bps        in 1u16..5_000u16,
        elapsed        in 86_400u64..31_536_000u64,
    ) {
        let mut vault = MetaVault::new(single_slot(1, apy_bps));
        if let Some(minted) = vault.deposit(deposit_amount) {
            vault.accrue(elapsed);
            if let Some(returned) = vault.withdraw(minted) {
                prop_assert!(returned >= deposit_amount,
                    "returned {} < deposited {} after accrue (apy={} elapsed={})",
                    returned, deposit_amount, apy_bps, elapsed);
            }
        }
    }
}

proptest! {
    /// No-free-money without accrue: deposit → immediate withdraw ≤ deposited.
    #[test]
    fn no_free_money_without_accrue(amount in 1u64..500_000u64) {
        let mut vault = MetaVault::new(single_slot(1, 500));
        if let Some(minted) = vault.deposit(amount) {
            if let Some(returned) = vault.withdraw(minted) {
                prop_assert!(returned <= amount,
                    "returned {} > deposited {} (no accrue)", returned, amount);
            }
        }
    }
}
