use soroban_sdk::{contracttype, Address};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigKey {
    Admin,
    SmtToken,
    Oracle,
    Assets,
    AssetConfig(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Config(ConfigKey),
    UserCollateral(Address, Address),
    UserDebt(Address),
    ReentrancyLock(Address),
    /// Pending flash-liquidation params, keyed by borrower address.
    PendingFlashLiquidate,
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// Per-asset risk parameters stored by the admin.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetConfig {
    /// Max loan-to-value in basis points (e.g. 7000 = 70%).
    pub ltv_bps: u32,
    /// Liquidation triggers when debt / collateral_value exceeds this (e.g. 8000 = 80%).
    pub liquidation_threshold: u32,
    /// Extra collateral paid to liquidator, in bps (e.g. 500 = 5% bonus).
    pub liquidation_bonus: u32,
    /// Whether this asset is accepted as collateral.
    pub is_active: bool,
}

/// Parameters stored in the pool's instance storage before the flash-loan call
/// and read back inside `receive_loan`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FlashLiquidateParams {
    /// The initiator address.
    pub initiator: Address,
    /// Address of the flash-loan provider.
    pub flash_loan: Address,
    /// Address of the lending pool to liquidate from.
    pub lending_pool: Address,
    /// Borrower whose position must be liquidated.
    pub borrower: Address,
    /// Collateral asset to seize.
    pub collateral_asset: Address,
    /// Amount of SMT debt to repay on the borrower's behalf.
    pub repay_amount: i128,
    /// AMM pool used to swap seized collateral -> SMT to repay the flash loan.
    pub amm_pool: Address,
    /// Minimum SMT received from the AMM swap (slippage guard).
    pub min_swap_output: i128,
    /// Maximum staleness accepted for oracle prices, in ledger seconds.
    pub max_price_age_secs: u64,
}
