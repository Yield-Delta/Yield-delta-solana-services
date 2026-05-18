//! Coverage-guided libfuzzer target for stateful vault sequences.
//!
//! Run (from repo root):
//!   cargo +nightly fuzz run fuzz_stateful --fuzz-dir programs/yield_vault_core/fuzz
//!
//! Generates arbitrary sequences of Deposit / Withdraw / Accrue operations and
//! checks that the core vault invariants hold after every step.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use yield_vault_core::math::{
    accrue_simple_interest, calculate_assets_for_shares, calculate_shares_to_mint,
};

// ── In-memory vault (no Anchor accounts) ────────────────────────────────────

#[derive(Default)]
struct Vault {
    total_shares: u64,
    total_assets: u64,
}

impl Vault {
    fn deposit(&mut self, amount: u64) -> Option<u64> {
        if amount == 0 { return None; }
        let minted = calculate_shares_to_mint(amount, self.total_shares, self.total_assets).ok()?;
        if minted == 0 { return None; }
        self.total_shares = self.total_shares.checked_add(minted)?;
        self.total_assets = self.total_assets.checked_add(amount)?;
        Some(minted)
    }

    fn withdraw(&mut self, shares: u64) -> Option<u64> {
        if shares == 0 || shares > self.total_shares { return None; }
        let assets = calculate_assets_for_shares(shares, self.total_shares, self.total_assets).ok()?;
        self.total_shares = self.total_shares.checked_sub(shares)?;
        self.total_assets = self.total_assets.checked_sub(assets)?;
        Some(assets)
    }

    fn accrue(&mut self, bps: u16, elapsed: u64) {
        if let Ok(earned) = accrue_simple_interest(self.total_assets, bps, elapsed) {
            self.total_assets = self.total_assets.saturating_add(earned);
        }
    }

    fn share_price_e9(&self) -> u128 {
        if self.total_shares == 0 { return 1_000_000_000; }
        (self.total_assets as u128) * 1_000_000_000 / (self.total_shares as u128)
    }
}

// ── Fuzz input ───────────────────────────────────────────────────────────────

#[derive(Arbitrary, Debug)]
enum Op {
    Deposit(u32),          // u32 to keep values moderate
    Withdraw(u32),
    Accrue { bps: u16, elapsed_hours: u16 },
}

fuzz_target!(|ops: Vec<Op>| {
    let mut vault = Vault::default();
    let mut running_shares: u64 = 0;
    let mut last_price: u128 = vault.share_price_e9();

    for op in &ops {
        match op {
            Op::Deposit(amount) => {
                let amount = (*amount as u64).max(1);
                if let Some(minted) = vault.deposit(amount) {
                    running_shares = running_shares.saturating_add(minted);
                }
            }
            Op::Withdraw(shares) => {
                let shares = (*shares as u64).min(vault.total_shares);
                if let Some(_assets) = vault.withdraw(shares) {
                    running_shares = running_shares.saturating_sub(shares);
                }
            }
            Op::Accrue { bps, elapsed_hours } => {
                let price_before = vault.share_price_e9();
                vault.accrue(*bps, (*elapsed_hours as u64) * 3_600);
                let price_after = vault.share_price_e9();
                assert!(
                    price_after >= price_before,
                    "share price decreased after accrue: {} → {}",
                    price_before, price_after
                );
                last_price = price_after;
            }
        }

        // Running invariant: tracked shares match vault
        assert_eq!(
            vault.total_shares, running_shares,
            "total_shares {} ≠ running sum {}",
            vault.total_shares, running_shares
        );
    }

    let _ = last_price;
});
