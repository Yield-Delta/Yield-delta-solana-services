/// Canonical PDA seed constants shared across every Yield Delta vault.
///
/// Using a single source of truth prevents seed mismatches between programs
/// and ensures that off-chain clients can derive addresses deterministically.

/// Primary vault state PDA seed.
pub const VAULT_SEED: &[u8] = b"vault";

/// Per-user position PDA seed (combined with the vault PDA key and user key).
pub const USER_POSITION_SEED: &[u8] = b"user_position";

/// Staking vault PDA seed.
pub const STAKING_VAULT_SEED: &[u8] = b"staking_vault";

/// LP vault PDA seed.
pub const LP_VAULT_SEED: &[u8] = b"lp_vault";

/// Delta-neutral vault PDA seed.
pub const DELTA_NEUTRAL_SEED: &[u8] = b"delta_neutral";

/// Meta vault PDA seed.
pub const META_VAULT_SEED: &[u8] = b"meta_vault";

/// Oracle state PDA seed.
pub const ORACLE_SEED: &[u8] = b"oracle";
