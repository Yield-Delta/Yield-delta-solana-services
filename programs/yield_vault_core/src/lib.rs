/// yield_vault_core — shared library used by every Yield Delta vault program.
///
/// This crate has no Anchor program entrypoint. It is a pure `[lib]` crate
/// that other programs pull in as a path dependency to share:
///   - seed constants
///   - shares / asset math helpers
///   - fee-accrual helpers
///   - common error codes
///   - shared account structs (UserPosition)
pub mod errors;
pub mod fees;
pub mod math;
pub mod seeds;
pub mod state;
