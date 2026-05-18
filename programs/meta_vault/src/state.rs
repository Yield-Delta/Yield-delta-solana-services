use anchor_lang::prelude::*;

pub const MAX_ALLOCATIONS: usize = 6;

/// One sub-strategy slot in the meta vault's allocation table.
/// strategy_id == 0 means the slot is empty.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Default)]
pub struct AllocationSlot {
    pub strategy_id: u8,
    pub weight_bps: u16,
    pub simulated_apy_bps: u16,
}

impl AllocationSlot {
    pub const LEN: usize = 1 + 2 + 2; // 5 bytes
}

/// State for the AI auto-allocation meta vault.
/// PDA seeds: ["meta_vault"]
#[account]
pub struct MetaVaultState {
    pub authority: Pubkey,
    pub usdc_mint: Pubkey,
    pub vault_mint: Pubkey,
    pub vault_usdc_account: Pubkey,
    pub total_shares: u64,
    pub total_assets: u64,
    pub fee_bps: u16,
    pub accumulated_fees: u64,
    pub last_fee_accrual: i64,
    pub last_rebalance: i64,
    pub allocations: [AllocationSlot; MAX_ALLOCATIONS],
    pub allocation_count: u8,
    pub high_water_mark: u64,
    pub bump: u8,
}

impl MetaVaultState {
    // discriminator(8) + 4×Pubkey(128) + 2×u64(16) + u16(2) + 2×u64(16)
    // + 2×i64(16) + AllocationSlot×6(30) + u8(1) + u64(8) + u8(1) = 226
    // use 512 for headroom
    pub const LEN: usize = 512;
}
