//! Coverage-guided libfuzzer target for all yield_vault_core math functions.
//!
//! Run (from repo root):
//!   cargo +nightly fuzz run fuzz_math --fuzz-dir programs/yield_vault_core/fuzz
//!
//! The fuzzer will generate structured (u64, u64, u64, u16, u64) tuples and
//! feed them into every math function, looking for panics or assertion failures.
//! Any panic that isn't caught is reported as a finding.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use yield_vault_core::{
    fees::{calculate_management_fee, calculate_performance_fee},
    math::{accrue_simple_interest, apply_bps, calculate_assets_for_shares, calculate_shares_to_mint},
};

#[derive(Arbitrary, Debug)]
struct MathInput {
    amount:       u64,
    total_shares: u64,
    total_assets: u64,
    bps:          u16,
    elapsed:      u64,
    profit:       u64,
}

fuzz_target!(|input: MathInput| {
    // All calls must not panic.  Ok/Err both accepted.

    let _ = calculate_shares_to_mint(input.amount, input.total_shares, input.total_assets);
    let _ = calculate_assets_for_shares(input.amount, input.total_shares, input.total_assets);
    let _ = apply_bps(input.amount, input.bps);
    let _ = accrue_simple_interest(input.amount, input.bps, input.elapsed);
    let _ = calculate_management_fee(input.total_assets, input.elapsed);
    let _ = calculate_performance_fee(input.profit);

    // Invariant: first deposit (empty vault) is always 1:1
    if let Ok(minted) = calculate_shares_to_mint(input.amount, 0, 0) {
        assert_eq!(minted, input.amount, "first deposit invariant violated");
    }

    // Invariant: apply_bps(v, 10_000) == v
    if let Ok(result) = apply_bps(input.amount, 10_000) {
        assert_eq!(result, input.amount, "apply_bps(v, 10_000) identity violated");
    }

    // Invariant: zero elapsed → zero interest
    if let Ok(interest) = accrue_simple_interest(input.amount, input.bps, 0) {
        assert_eq!(interest, 0, "zero-elapsed interest invariant violated");
    }

    // Invariant: redeeming all shares returns total_assets exactly
    if input.total_shares > 0 {
        if let Ok(assets) = calculate_assets_for_shares(
            input.total_shares,
            input.total_shares,
            input.total_assets,
        ) {
            assert_eq!(assets, input.total_assets, "all-shares redemption invariant violated");
        }
    }

    // Invariant: deposit→withdraw roundtrip never gives back more than deposited
    if input.amount > 0 {
        if let Ok(minted) = calculate_shares_to_mint(input.amount, input.total_shares, input.total_assets) {
            if minted > 0 {
                let new_shares = input.total_shares.saturating_add(minted);
                let new_assets = input.total_assets.saturating_add(input.amount);
                if let Ok(redeemed) = calculate_assets_for_shares(minted, new_shares, new_assets) {
                    assert!(
                        redeemed <= input.amount,
                        "roundtrip gave back {} > deposited {}",
                        redeemed, input.amount
                    );
                }
            }
        }
    }
});
