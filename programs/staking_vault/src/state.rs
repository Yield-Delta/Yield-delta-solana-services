use anchor_lang::prelude::*;

// ---------------------------------------------------------------------------
// StakingVaultState
// PDA seeds: ["staking_vault"]
// ---------------------------------------------------------------------------

/// State for the SOL liquid staking vault.
///
/// Users deposit lamports and receive `stSOL` share tokens.
/// On devnet the staking yield is simulated via the `accrue_yield` instruction.
#[account]
pub struct StakingVaultState {
    /// Vault admin — authorised to call `accrue_yield`.
    pub authority: Pubkey,
    /// The stSOL share-token SPL mint; mint authority is this PDA.
    pub vault_mint: Pubkey,
    /// Total lamports (SOL) currently managed by the vault (principal + accrued yield).
    pub total_sol_staked: u64,
    /// Total stSOL share tokens in circulation.
    pub total_shares: u64,
    /// Simulated annual APY in basis points (e.g. 700 = 7 %).
    pub yield_bps: u16,
    /// Unix timestamp of the last `accrue_yield` call.
    pub last_accrual: i64,
    /// Bump seed for this PDA.
    pub bump: u8,
}

impl StakingVaultState {
    /// discriminator (8) + 2×Pubkey (64) + 2×u64 (16) + u16 (2) + i64 (8) + u8 (1) + padding (5)
    pub const LEN: usize = 8 + 32 + 32 + 8 + 8 + 2 + 8 + 1;
}

// ---------------------------------------------------------------------------
// UserPosition
// PDA seeds: ["user_position", vault_state, user]
// ---------------------------------------------------------------------------

/// Per-user staking position. Tracks how many stSOL shares the user holds.
#[account]
pub struct UserPosition {
    /// Wallet that owns this position.
    pub owner: Pubkey,
    /// The StakingVaultState PDA this position belongs to.
    pub vault: Pubkey,
    /// stSOL shares held by this user.
    pub shares: u64,
    /// Bump seed for this PDA.
    pub bump: u8,
}

impl UserPosition {
    /// discriminator (8) + 2×Pubkey (64) + u64 (8) + u8 (1) = 81 bytes
    pub const LEN: usize = 8 + 32 + 32 + 8 + 1;
}
