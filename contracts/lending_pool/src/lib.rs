//! # SoroMint Lending Pool v2

#![no_std]

mod events;
mod liquidation;
mod oracle;
mod reentrancy;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{contract, contractimpl, token, Address, Bytes, Env, String, Vec};
use types::{ConfigKey, DataKey};
pub use types::{AssetConfig, FlashLiquidateParams};

const DEFAULT_MAX_PRICE_AGE: u64 = 3_600;

#[contract]
pub struct LendingPool;

#[contractimpl]
impl LendingPool {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    pub fn initialize(e: Env, admin: Address, smt_token: Address, oracle: Address) {
        if e.storage().instance().has(&DataKey::Config(ConfigKey::Admin)) {
            panic!("already initialized");
        }
        e.storage().instance().set(&DataKey::Config(ConfigKey::Admin), &admin);
        e.storage().instance().set(&DataKey::Config(ConfigKey::SmtToken), &smt_token);
        e.storage().instance().set(&DataKey::Config(ConfigKey::Oracle), &oracle);
        e.storage().instance().set(&DataKey::Config(ConfigKey::Assets), &Vec::<Address>::new(&e));
    }

    // -----------------------------------------------------------------------
    // Admin
    // -----------------------------------------------------------------------

    pub fn set_asset_config(e: Env, asset: Address, config: AssetConfig) {
        let admin: Address = e.storage().instance()
            .get(&DataKey::Config(ConfigKey::Admin)).expect("not initialized");
        admin.require_auth();

        let mut assets: Vec<Address> = e.storage().instance()
            .get(&DataKey::Config(ConfigKey::Assets)).unwrap_or(Vec::new(&e));
        let mut found = false;
        for a in assets.iter() {
            if a == asset { found = true; break; }
        }
        if !found {
            assets.push_back(asset.clone());
            e.storage().instance().set(&DataKey::Config(ConfigKey::Assets), &assets);
        }
        e.storage().instance().set(&DataKey::Config(ConfigKey::AssetConfig(asset)), &config);
    }

    // -----------------------------------------------------------------------
    // Core operations
    // -----------------------------------------------------------------------

    pub fn deposit(e: Env, user: Address, asset: Address, amount: i128) {
        user.require_auth();
        if amount <= 0 { panic!("amount must be positive"); }

        let config: AssetConfig = e.storage().instance()
            .get(&DataKey::Config(ConfigKey::AssetConfig(asset.clone())))
            .expect("asset not supported");
        if !config.is_active { panic!("asset not active"); }

        token::Client::new(&e, &asset).transfer(&user, &e.current_contract_address(), &amount);

        let key = DataKey::UserCollateral(user.clone(), asset.clone());
        let current: i128 = e.storage().persistent().get(&key).unwrap_or(0);
        e.storage().persistent().set(&key, &(current.checked_add(amount).expect("overflow")));

        events::emit_deposit(&e, &user, &asset, amount);
    }

    pub fn withdraw(e: Env, user: Address, asset: Address, amount: i128) {
        user.require_auth();
        if amount <= 0 { panic!("amount must be positive"); }

        let key = DataKey::UserCollateral(user.clone(), asset.clone());
        let current: i128 = e.storage().persistent().get(&key).unwrap_or(0);
        if amount > current { panic!("insufficient collateral balance"); }

        let new_amount = current.checked_sub(amount).expect("underflow");
        e.storage().persistent().set(&key, &new_amount);

        let debt: i128 = e.storage().persistent()
            .get(&DataKey::UserDebt(user.clone())).unwrap_or(0);
        if debt > 0 && liquidation::health_factor(&e, &user) < liquidation::PRICE_SCALE {
            panic!("withdrawal would undercollateralize position");
        }

        token::Client::new(&e, &asset).transfer(&e.current_contract_address(), &user, &amount);
        events::emit_withdraw(&e, &user, &asset, amount);
    }

    pub fn borrow(e: Env, user: Address, amount: i128) {
        user.require_auth();
        if amount <= 0 { panic!("amount must be positive"); }

        let borrow_power = liquidation::ltv_adjusted_collateral(&e, &user);
        let debt_key = DataKey::UserDebt(user.clone());
        let current_debt: i128 = e.storage().persistent().get(&debt_key).unwrap_or(0);
        let new_debt = current_debt.checked_add(amount).expect("debt overflow");
        if new_debt > borrow_power { panic!("insufficient collateral for borrow"); }

        e.storage().persistent().set(&debt_key, &new_debt);

        let smt_token: Address = e.storage().instance()
            .get(&DataKey::Config(ConfigKey::SmtToken)).expect("not initialized");
        token::Client::new(&e, &smt_token).transfer(&e.current_contract_address(), &user, &amount);

        events::emit_borrow(&e, &user, amount);
    }

    pub fn repay(e: Env, user: Address, amount: i128) {
        user.require_auth();
        if amount <= 0 { panic!("amount must be positive"); }

        let smt_token: Address = e.storage().instance()
            .get(&DataKey::Config(ConfigKey::SmtToken)).expect("not initialized");
        let debt_key = DataKey::UserDebt(user.clone());
        let current_debt: i128 = e.storage().persistent().get(&debt_key).unwrap_or(0);
        let repay_amount = amount.min(current_debt);
        if repay_amount == 0 { panic!("no debt to repay"); }

        token::Client::new(&e, &smt_token).transfer(&user, &e.current_contract_address(), &repay_amount);
        e.storage().persistent().set(&debt_key, &(current_debt - repay_amount));

        events::emit_repay(&e, &user, repay_amount);
    }

    // -----------------------------------------------------------------------
    // Standard liquidation
    // -----------------------------------------------------------------------

    /// Direct liquidation — liquidator provides SMT and receives collateral.
    pub fn liquidate(
        e: Env,
        liquidator: Address,
        borrower: Address,
        collateral_asset: Address,
        repay_amount: i128,
    ) {
        liquidator.require_auth();
        let _guard = reentrancy::ReentrancyGuard::lock(&e, &liquidator);
        liquidation::execute_liquidation(
            &e, &liquidator, &borrower, &collateral_asset,
            repay_amount, DEFAULT_MAX_PRICE_AGE,
        );
    }

    // -----------------------------------------------------------------------
    // Flash-loan liquidation
    // -----------------------------------------------------------------------

    /// Store flash-liquidation parameters before calling flash_loan externally.
    ///
    /// Call sequence (no re-entry):
    ///   1. liquidator calls pool.setup_flash_liquidation(params)
    ///   2. liquidator calls flash_loan.flash_loan(borrower=pool, amount)
    ///   3. flash_loan transfers SMT to pool, then calls pool.receive_loan(...)
    ///   4. pool.receive_loan reads params, liquidates, swaps, repays flash loan
    pub fn setup_flash_liquidation(e: Env, params: FlashLiquidateParams) {
        if e.storage().instance().has(&DataKey::PendingFlashLiquidate) {
            panic!("flash liquidation already pending");
        }
        // Pre-flight check
        if !liquidation::is_liquidatable(&e, &params.borrower) {
            panic!("borrower is healthy");
        }
        e.storage().instance().set(&DataKey::PendingFlashLiquidate, &params);
    }

    /// Flash-loan callback. Called by SmtFlashLoanProvider after transferring
    /// `amount` SMT to this contract. Reads pending params, executes the
    /// liquidation, swaps collateral on the AMM, and repays the flash loan.
    pub fn receive_loan(e: Env, provider: Address, amount: i128, fee: i128, _params: Bytes) {
        let pending: FlashLiquidateParams = e
            .storage()
            .instance()
            .get(&DataKey::PendingFlashLiquidate)
            .expect("no pending flash liquidation");

        if amount != pending.repay_amount {
            panic!("flash loan amount mismatch");
        }

        liquidation::execute_flash_receive(
            &e, provider, amount, fee, pending,
        );

        e.storage().instance().remove(&DataKey::PendingFlashLiquidate);
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn is_healthy(e: Env, user: Address) -> bool {
        !liquidation::is_liquidatable(&e, &user)
    }

    pub fn get_health_factor(e: Env, user: Address) -> i128 {
        liquidation::health_factor(&e, &user)
    }

    pub fn get_borrow_power(e: Env, user: Address) -> i128 {
        liquidation::ltv_adjusted_collateral(&e, &user)
    }

    pub fn get_debt(e: Env, user: Address) -> i128 {
        e.storage().persistent().get(&DataKey::UserDebt(user)).unwrap_or(0)
    }

    pub fn get_collateral(e: Env, user: Address, asset: Address) -> i128 {
        e.storage().persistent().get(&DataKey::UserCollateral(user, asset)).unwrap_or(0)
    }

    pub fn get_account_collateral_value(e: Env, user: Address) -> i128 {
        liquidation::threshold_adjusted_collateral(&e, &user)
    }

    pub fn get_smt_token(e: Env) -> Address {
        e.storage().instance()
            .get(&DataKey::Config(ConfigKey::SmtToken)).expect("not initialized")
    }

    pub fn version(_e: Env) -> String { String::from_str(&_e, "2.0.0") }
    pub fn status(_e: Env) -> String { String::from_str(&_e, "alive") }
}

// Re-export for test access
pub use liquidation::{compute_collateral_to_seize, require_fresh_price, PRICE_SCALE};
pub use reentrancy::ReentrancyGuard;
