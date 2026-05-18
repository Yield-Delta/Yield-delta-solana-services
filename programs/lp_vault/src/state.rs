use anchor_lang::prelude::*;

// ---------------------------------------------------------------------------
// LpVaultState
// PDA seeds: ["lp_vault", lp_token_mint]
// ---------------------------------------------------------------------------

/// State for the LP auto-compounding vault.
///
/// Users deposit mock LP tokens and receive vault share tokens.
/// The `simulate_compound` instruction adds auto-compounded fees to
/// `total_lp_tokens`, which increases the share price over time.
#[account]
pub struct LpVaultState {
    /// Vault admin — authorised to call `simulate_compound`.
    pub authority: Pubkey,
    /// The mock LP token mint that users deposit.
    pub lp_token_mint: Pubkey,
    /// The vault share-token SPL mint; mint authority is this PDA.
    pub vault_mint: Pubkey,
    /// The vault's ATA that holds deposited LP tokens.
    pub vault_lp_account: Pubkey,
    /// Total vault share tokens in circulation.
    pub total_shares: u64,
    /// Total LP tokens held by the vault (principal + compounded fees).
    pub total_lp_tokens: u64,
    /// Simulated compounding rate in basis points per compound event.
    pub compound_fee_bps: u16,
    /// Unix timestamp of the last `simulate_compound` call.
    pub last_compound: i64,
    /// Bump seed for this PDA.
    pub bump: u8,
}

impl LpVaultState {
    /// discriminator (8) + 4×Pubkey (128) + 2×u64 (16) + u16 (2) + i64 (8) + u8 (1) + padding (5)
    pub const LEN: usize = 8 + 32 + 32 + 32 + 32 + 8 + 8 + 2 + 8 + 1;
}

// ---------------------------------------------------------------------------
// UserPosition
// PDA seeds: ["user_position", vault_state, user]
// ---------------------------------------------------------------------------

/// Per-user LP vault position.
#[account]
pub struct UserPosition {
    /// Wallet that owns this position.
    pub owner: Pubkey,
    /// The LpVaultState PDA this position belongs to.
    pub vault: Pubkey,
    /// Vault share tokens held by this user.
    pub shares: u64,
    /// Bump seed for this PDA.
    pub bump: u8,
}

impl UserPosition {
    /// discriminator (8) + 2×Pubkey (64) + u64 (8) + u8 (1) = 81 bytes
    pub const LEN: usize = 8 + 32 + 32 + 8 + 1;
}
