use anchor_lang::prelude::*;

/// Generic per-user position account shared across all Yield Delta vaults.
/// Programs that use this as an on-chain account should apply #[account] locally.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct UserPosition {
    /// The wallet that owns this position.
    pub owner: Pubkey,
    /// The vault PDA this position belongs to.
    pub vault: Pubkey,
    /// Number of share tokens the user currently holds (tracked on-chain for
    /// convenience — the authoritative balance is the SPL token account).
    pub shares: u64,
    /// Bump seed for this PDA.
    pub bump: u8,
}

impl UserPosition {
    /// Discriminator (8) + 2 × Pubkey (64) + u64 (8) + u8 (1) = 81 bytes.
    pub const LEN: usize = 8 + 32 + 32 + 8 + 1;
}
