use anchor_lang::prelude::*;

// ---------------------------------------------------------------------------
// DeltaNeutralVaultState
// PDA seeds: ["delta_neutral", usdc_mint]
// ---------------------------------------------------------------------------

/// State for the delta-neutral vault.
///
/// Users deposit USDC. The vault tracks a simulated long position (the USDC
/// itself) and a synthetic short hedge (tracked as notional). The AI keeper
/// calls `rebalance_hedge` to restore neutrality when |delta_bps| drifts
/// past `rebalance_threshold_bps`.
#[account]
pub struct DeltaNeutralVaultState {
    /// Vault admin / AI keeper — authorised to call `rebalance_hedge`.
    pub authority: Pubkey,
    /// USDC mint (devnet USDC).
    pub usdc_mint: Pubkey,
    /// Vault share-token SPL mint; mint authority is this PDA.
    pub vault_mint: Pubkey,
    /// Vault's USDC associated token account.
    pub vault_usdc_account: Pubkey,
    /// Total vault share tokens in circulation.
    pub total_shares: u64,
    /// Total USDC under management (6-decimal precision, i.e. lamports of USDC).
    pub total_assets: u64,
    /// Simulated long spot exposure in USDC (= total_assets on devnet).
    pub long_notional: u64,
    /// Simulated short synthetic exposure in USDC (tracks long to stay neutral).
    pub short_notional: u64,
    /// Net delta in basis points; 0 = perfectly neutral, target.
    /// Computed as: (long - short) * 10_000 / long
    pub delta_bps: i64,
    /// Trigger rebalance when |delta_bps| exceeds this value.
    pub rebalance_threshold_bps: u16,
    /// Unix timestamp of the last `rebalance_hedge` call.
    pub last_rebalance: i64,
    /// Bump seed for this PDA.
    pub bump: u8,
}

impl DeltaNeutralVaultState {
    // discriminator(8) + 4×Pubkey(128) + 5×u64(40) + i64(8) + u16(2) + i64(8) + u8(1) = 195
    // use 256 for alignment padding
    pub const LEN: usize = 256;
}

/// Compute net delta in basis points relative to long exposure.
///   if long == 0 → 0
///   else → (long - short) * 10_000 / long
pub fn compute_delta(long: u64, short: u64) -> i64 {
    if long == 0 {
        return 0;
    }
    ((long as i128 - short as i128) * 10_000 / long as i128) as i64
}
