//! # Liquidation Engine
//!
//! Contains all liquidation logic for the lending pool:
//!
//! * **Health-factor calculation** — determines whether a position is
//!   liquidatable.
//! * **Standard liquidation** — a liquidator supplies the repay amount in SMT
//!   directly and receives collateral + bonus.
//! * **Flash-loan liquidation** — an atomic sequence where:
//!   1. The initiator calls `lending_pool.flash_liquidate()`.
//!   2. The pool stores `FlashLiquidateParams` under the initiator's address in
//!      instance storage, then calls `flash_loan.flash_loan(borrower=self, ...)`.
//!   3. The flash-loan provider transfers SMT to the pool and calls back
//!      `lending_pool.receive_loan(provider, amount, fee, empty_bytes)`.
//!   4. `receive_loan` reads the pending params, executes the liquidation,
//!      swaps seized collateral on the AMM, repays the flash loan.
//!   5. Params are cleared from storage on success.
//!
//! ## Price-staleness guard
//! Before executing any liquidation we verify that the oracle price is not
//! older than `max_price_age_secs` via `oracle.is_price_stale()`.
//!
//! ## Decimal convention
//! All prices from the oracle carry **7 decimal places**:
//! `price == 10_000_000` means $1.00 per token unit.
//!
//! Collateral value in "SMT units" = `(amount * price) / 10_000_000`.
//! SMT is treated as $1.00, i.e. debt value == debt amount.

use soroban_sdk::{token::Client as TokenClient, Address, Env, IntoVal, Symbol};

use crate::{
    events,
    oracle::OracleClient,
    types::{AssetConfig, ConfigKey, DataKey, FlashLiquidateParams},
};

/// Fixed-point scale matching oracle 7-decimal prices.
pub const PRICE_SCALE: i128 = 10_000_000;
/// Basis-point denominator.
const BPS: i128 = 10_000;

// ---------------------------------------------------------------------------
// Health factor
// ---------------------------------------------------------------------------

/// Returns the health factor of `user` scaled to 7 decimals.
///
/// A value >= `PRICE_SCALE` (i.e. >= 1.0) means the position is healthy.
/// `i128::MAX` is returned when the user has no debt.
///
/// Formula:
/// ```text
/// health_factor = sum(collateral_i * price_i / PRICE_SCALE * threshold_i / BPS)
///                 ---------------------------------------------------------------
///                                       debt
///                * PRICE_SCALE
/// ```
pub fn health_factor(e: &Env, user: &Address) -> i128 {
    let debt: i128 = e
        .storage()
        .persistent()
        .get(&DataKey::UserDebt(user.clone()))
        .unwrap_or(0);

    if debt == 0 {
        return i128::MAX;
    }

    let threshold_collateral = threshold_adjusted_collateral(e, user);

    // health_factor expressed with PRICE_SCALE precision
    threshold_collateral
        .checked_mul(PRICE_SCALE)
        .expect("health factor numerator overflow")
        .checked_div(debt)
        .expect("health factor division by zero")
}

/// Returns true when `health_factor(user) < PRICE_SCALE`.
pub fn is_liquidatable(e: &Env, user: &Address) -> bool {
    health_factor(e, user) < PRICE_SCALE
}

/// Sum of `(collateral_amount * price / PRICE_SCALE) * threshold_bps / BPS`
/// across all collateral assets held by `user`.
pub fn threshold_adjusted_collateral(e: &Env, user: &Address) -> i128 {
    let oracle_addr: Address = e
        .storage()
        .instance()
        .get(&DataKey::Config(ConfigKey::Oracle))
        .expect("oracle not set");
    let oracle = OracleClient::new(e, &oracle_addr);

    let assets: soroban_sdk::Vec<Address> = e
        .storage()
        .instance()
        .get(&DataKey::Config(ConfigKey::Assets))
        .unwrap_or(soroban_sdk::Vec::new(e));

    let mut total: i128 = 0;
    for asset in assets.iter() {
        let amount: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::UserCollateral(user.clone(), asset.clone()))
            .unwrap_or(0);
        if amount == 0 {
            continue;
        }

        let config: AssetConfig = match e
            .storage()
            .instance()
            .get(&DataKey::Config(ConfigKey::AssetConfig(asset.clone())))
        {
            Some(c) => c,
            None => continue,
        };

        let price = oracle.get_price(&asset);
        // raw collateral value in "SMT units" (7-decimal fixed-point)
        let value = amount
            .checked_mul(price)
            .expect("collateral value mul overflow")
            .checked_div(PRICE_SCALE)
            .expect("collateral value div zero");

        // discount by liquidation threshold
        let adjusted = value
            .checked_mul(config.liquidation_threshold as i128)
            .expect("threshold mul overflow")
            .checked_div(BPS)
            .expect("threshold div zero");

        total = total.checked_add(adjusted).expect("collateral sum overflow");
    }
    total
}

/// LTV-adjusted collateral = borrow power.
pub fn ltv_adjusted_collateral(e: &Env, user: &Address) -> i128 {
    let oracle_addr: Address = e
        .storage()
        .instance()
        .get(&DataKey::Config(ConfigKey::Oracle))
        .expect("oracle not set");
    let oracle = OracleClient::new(e, &oracle_addr);

    let assets: soroban_sdk::Vec<Address> = e
        .storage()
        .instance()
        .get(&DataKey::Config(ConfigKey::Assets))
        .unwrap_or(soroban_sdk::Vec::new(e));

    let mut total: i128 = 0;
    for asset in assets.iter() {
        let amount: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::UserCollateral(user.clone(), asset.clone()))
            .unwrap_or(0);
        if amount == 0 {
            continue;
        }

        let config: AssetConfig = match e
            .storage()
            .instance()
            .get(&DataKey::Config(ConfigKey::AssetConfig(asset.clone())))
        {
            Some(c) => c,
            None => continue,
        };

        let price = oracle.get_price(&asset);
        let value = amount
            .checked_mul(price)
            .expect("ltv value mul overflow")
            .checked_div(PRICE_SCALE)
            .expect("ltv value div zero");

        let adjusted = value
            .checked_mul(config.ltv_bps as i128)
            .expect("ltv mul overflow")
            .checked_div(BPS)
            .expect("ltv div zero");

        total = total.checked_add(adjusted).expect("ltv sum overflow");
    }
    total
}

// ---------------------------------------------------------------------------
// Price-staleness check
// ---------------------------------------------------------------------------

/// Verify that the oracle price for `asset` is not older than
/// `max_age_secs`.  Panics if the price is stale or missing.
pub fn require_fresh_price(e: &Env, oracle_addr: &Address, asset: &Address, max_age_secs: u64) {
    let is_stale: bool = e.invoke_contract(
        oracle_addr,
        &Symbol::new(e, "is_price_stale"),
        soroban_sdk::vec![e, asset.into_val(e), max_age_secs.into_val(e)],
    );
    if is_stale {
        panic!("oracle price is stale");
    }
}

// ---------------------------------------------------------------------------
// Core liquidation — shared by standard and flash paths
// ---------------------------------------------------------------------------

/// Execute the core liquidation logic.
///
/// Preconditions (must be verified by caller):
/// - `smt_repay_amount` of SMT is already in this contract's balance.
/// - The position is unhealthy.
/// - Oracle price is fresh.
///
/// Returns the amount of collateral seized.
fn execute_seizure(
    e: &Env,
    liquidator: &Address,
    borrower: &Address,
    collateral_asset: &Address,
    smt_repay_amount: i128,
) -> i128 {
    let smt_token: Address = e
        .storage()
        .instance()
        .get(&DataKey::Config(ConfigKey::SmtToken))
        .expect("smt token not set");

    let debt_key = DataKey::UserDebt(borrower.clone());
    let current_debt: i128 = e.storage().persistent().get(&debt_key).unwrap_or(0);

    if smt_repay_amount > current_debt {
        panic!("repay amount exceeds debt");
    }

    let config: AssetConfig = e
        .storage()
        .instance()
        .get(&DataKey::Config(ConfigKey::AssetConfig(collateral_asset.clone())))
        .expect("asset not supported");

    let oracle_addr: Address = e
        .storage()
        .instance()
        .get(&DataKey::Config(ConfigKey::Oracle))
        .expect("oracle not set");
    let oracle = OracleClient::new(e, &oracle_addr);
    let price = oracle.get_price(collateral_asset);

    let collateral_to_give = compute_collateral_to_seize(smt_repay_amount, price, config.liquidation_bonus);

    let coll_key = DataKey::UserCollateral(borrower.clone(), collateral_asset.clone());
    let borrower_coll: i128 = e.storage().persistent().get(&coll_key).unwrap_or(0);
    let actual_give = collateral_to_give.min(borrower_coll);

    if actual_give <= 0 {
        panic!("no collateral available to seize");
    }

    // Reduce borrower's debt and collateral.
    e.storage()
        .persistent()
        .set(&debt_key, &(current_debt - smt_repay_amount));
    e.storage()
        .persistent()
        .set(&coll_key, &(borrower_coll - actual_give));

    // Transfer SMT from liquidator (or self) into the pool's balance.
    // On the standard path, we pull from the liquidator.
    // On the flash path, we already hold the SMT — skip the pull.
    // We distinguish via the `liquidator` address: if it's self, skip.
    let self_addr = e.current_contract_address();
    if liquidator != &self_addr {
        let smt = TokenClient::new(e, &smt_token);
        smt.transfer(liquidator, &self_addr, &smt_repay_amount);
    }

    // Transfer seized collateral to liquidator.
    let asset_client = TokenClient::new(e, collateral_asset);
    asset_client.transfer(&self_addr, liquidator, &actual_give);

    events::emit_liquidate(e, liquidator, borrower, collateral_asset, actual_give);

    actual_give
}

// ---------------------------------------------------------------------------
// Standard (funded) liquidation
// ---------------------------------------------------------------------------

/// Execute a standard liquidation where the `liquidator` has already funded
/// themselves and calls the pool directly.
pub fn execute_liquidation(
    e: &Env,
    liquidator: &Address,
    borrower: &Address,
    collateral_asset: &Address,
    repay_amount: i128,
    max_price_age_secs: u64,
) {
    if repay_amount <= 0 {
        panic!("repay amount must be positive");
    }
    if !is_liquidatable(e, borrower) {
        panic!("borrower is healthy");
    }

    let oracle_addr: Address = e
        .storage()
        .instance()
        .get(&DataKey::Config(ConfigKey::Oracle))
        .expect("oracle not set");

    require_fresh_price(e, &oracle_addr, collateral_asset, max_price_age_secs);

    execute_seizure(e, liquidator, borrower, collateral_asset, repay_amount);
}

// ---------------------------------------------------------------------------
// Flash-loan receive handler
// ---------------------------------------------------------------------------

/// Called from `pool.receive_loan` after the flash-loan provider has deposited
/// `amount` SMT into the pool. Executes the liquidation, swaps seized collateral
/// for SMT on the AMM, then repays `amount + fee` to the provider.
pub fn execute_flash_receive(
    e: &Env,
    provider: Address,
    amount: i128,
    fee: i128,
    params: FlashLiquidateParams,
) {
    let self_addr = e.current_contract_address();

    // Validate price freshness
    let oracle_addr: Address = e.storage().instance()
        .get(&DataKey::Config(ConfigKey::Oracle)).expect("oracle not set");
    require_fresh_price(e, &oracle_addr, &params.collateral_asset, params.max_price_age_secs);

    // Verify position is still liquidatable
    if !is_liquidatable(e, &params.borrower) {
        panic!("borrower is healthy");
    }

    // Execute the seizure — pool already holds SMT, so self is liquidator
    let collateral_seized = execute_seizure(
        e, &self_addr, &params.borrower, &params.collateral_asset, params.repay_amount,
    );

    // Swap seized collateral -> SMT on the AMM
    let coll = TokenClient::new(e, &params.collateral_asset);
    coll.approve(
        &self_addr,
        &params.amm_pool,
        &collateral_seized,
        &(e.ledger().sequence().checked_add(1).expect("seq overflow")),
    );

    let _: soroban_sdk::Val = e.invoke_contract(
        &params.amm_pool,
        &Symbol::new(e, "swap"),
        soroban_sdk::vec![
            e,
            self_addr.clone().into_val(e),
            params.collateral_asset.clone().into_val(e),
            collateral_seized.into_val(e),
            params.min_swap_output.into_val(e),
        ],
    );

    // Verify repayment is covered
    let smt_token: Address = e.storage().instance()
        .get(&DataKey::Config(ConfigKey::SmtToken)).expect("not set");
    let smt = TokenClient::new(e, &smt_token);
    let repayment_due = amount.checked_add(fee).expect("overflow");
    if smt.balance(&self_addr) < repayment_due {
        panic!("insufficient SMT to repay flash loan after swap");
    }

    // Repay
    smt.transfer(&self_addr, &provider, &repayment_due);

    events::emit_flash_liquidate(
        e, &params.borrower, &params.collateral_asset,
        params.repay_amount, collateral_seized, repayment_due,
    );
}

// ---------------------------------------------------------------------------
// Math helpers
// ---------------------------------------------------------------------------

/// `repay_amount` is in SMT (7 dec).
/// `price` is oracle price of collateral in SMT (7 dec).
/// `bonus_bps` is the liquidation bonus in basis points.
///
/// Returns the amount of collateral tokens the liquidator receives.
///
/// ```text
/// base = repay_amount * PRICE_SCALE / price
/// result = base * (BPS + bonus_bps) / BPS
/// ```
pub fn compute_collateral_to_seize(repay_amount: i128, price: i128, bonus_bps: u32) -> i128 {
    if price <= 0 {
        panic!("invalid price");
    }
    let base = repay_amount
        .checked_mul(PRICE_SCALE)
        .expect("seize base mul overflow")
        .checked_div(price)
        .expect("seize base div zero");

    base.checked_mul(BPS + bonus_bps as i128)
        .expect("seize bonus mul overflow")
        .checked_div(BPS)
        .expect("seize bonus div zero")
}
