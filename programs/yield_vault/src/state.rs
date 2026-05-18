use anchor_lang::prelude::*;

#[account]
pub struct VaultState {
    /// Vault admin — only this key can call privileged instructions in future versions.
    pub authority: Pubkey,
    /// The SPL mint users deposit (e.g. USDC, wSOL).
    pub token_mint: Pubkey,
    /// The share token mint; authority is the vault PDA.
    pub vault_mint: Pubkey,
    /// The ATA owned by the vault PDA that holds deposited tokens.
    pub vault_token_account: Pubkey,
    /// Total share tokens in circulation.
    pub total_shares: u64,
    /// Total deposit tokens held by the vault.
    pub total_assets: u64,
    /// Bump seed for the vault PDA.
    pub bump: u8,
}

impl VaultState {
    /// Discriminator (8) + 4×Pubkey (128) + 2×u64 (16) + u8 (1) + padding (7)
    pub const LEN: usize = 8 + 32 + 32 + 32 + 32 + 8 + 8 + 1;
}

#[account]
pub struct UserPosition {
    /// The wallet that owns this position.
    pub owner: Pubkey,
    /// The vault PDA this position belongs to.
    pub vault: Pubkey,
    /// Number of share tokens the user holds (tracked on-chain for convenience).
    pub shares: u64,
    /// Bump seed for the user-position PDA.
    pub bump: u8,
}

impl UserPosition {
    /// Discriminator (8) + 2×Pubkey (64) + u64 (8) + u8 (1) + padding (7)
    pub const LEN: usize = 8 + 32 + 32 + 8 + 1;
}
