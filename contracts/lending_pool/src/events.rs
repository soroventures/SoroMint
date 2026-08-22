use soroban_sdk::{symbol_short, Address, Env, Symbol};

const DEPOSIT: Symbol = symbol_short!("deposit");
const WITHDRAW: Symbol = symbol_short!("withdraw");
const BORROW: Symbol = symbol_short!("borrow");
const REPAY: Symbol = symbol_short!("repay");
const LIQUIDATE: Symbol = symbol_short!("liquid");
const FLASH_LIQ: Symbol = symbol_short!("fl_liq");

pub fn emit_deposit(e: &Env, user: &Address, asset: &Address, amount: i128) {
    e.events().publish((DEPOSIT, user, asset), amount);
}

pub fn emit_withdraw(e: &Env, user: &Address, asset: &Address, amount: i128) {
    e.events().publish((WITHDRAW, user, asset), amount);
}

pub fn emit_borrow(e: &Env, user: &Address, amount: i128) {
    e.events().publish((BORROW, user), amount);
}

pub fn emit_repay(e: &Env, user: &Address, amount: i128) {
    e.events().publish((REPAY, user), amount);
}

pub fn emit_liquidate(
    e: &Env,
    liquidator: &Address,
    borrower: &Address,
    asset: &Address,
    collateral_seized: i128,
) {
    e.events()
        .publish((LIQUIDATE, liquidator, borrower, asset), collateral_seized);
}

/// Emitted at the end of a successful flash-loan liquidation.
///
/// Topics: `(fl_liq, borrower, collateral_asset)`
/// Data:   `(repay_amount, collateral_seized, flash_loan_repaid)`
pub fn emit_flash_liquidate(
    e: &Env,
    borrower: &Address,
    collateral_asset: &Address,
    repay_amount: i128,
    collateral_seized: i128,
    flash_loan_repaid: i128,
) {
    e.events().publish(
        (FLASH_LIQ, borrower, collateral_asset),
        (repay_amount, collateral_seized, flash_loan_repaid),
    );
}
