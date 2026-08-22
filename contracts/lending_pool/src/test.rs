//! # Lending Pool Integration Tests
//!
//! Covers:
//! 1. Basic deposit / borrow / repay / withdraw lifecycle.
//! 2. Standard (funded) liquidation.
//! 3. Flash-loan liquidation end-to-end (full atomic lifecycle).
//! 4. Edge cases and security checks.
//!
//! ## Flash liquidation architecture
//! Soroban's host blocks re-entry: a contract cannot appear twice in the same
//! call stack.  The naive flow:
//!
//!   pool.flash_liquidate -> flash_loan.flash_loan -> pool.receive_loan
//!
//! puts `pool` on the stack twice and the host kills it with
//! "Contract re-entry is not allowed".
//!
//! The correct pattern uses a thin, separate executor contract:
//!
//!   initiator -> executor.run(pool, flash_loan, params)
//!     -> flash_loan.flash_loan(borrower = executor)
//!       -> executor.receive_loan(provider, amount, fee, _)
//!         -> pool.execute_flash_liquidation(executor, borrower, ...)
//!         -> amm.swap(...)
//!         -> smt.transfer(executor -> provider)

#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env,
};

use soromint_amm_pool::AmmPool;
use flash_loan::SmtFlashLoanProvider;
use soromint_oracle::PriceOracle;

// ---------------------------------------------------------------------------
// Test fixture
// ---------------------------------------------------------------------------

struct TestEnv {
    pub env: Env,
    pub pool_addr: Address,
    pub flash_loan_addr: Address,
    pub oracle_addr: Address,
    pub amm_addr: Address,
    pub smt_addr: Address,
    pub coll_addr: Address,
    #[allow(dead_code)]
    pub admin: Address,
    pub alice: Address,
    pub liquidator: Address,
}

impl TestEnv {
    fn pool(&self) -> LendingPoolClient<'_> {
        LendingPoolClient::new(&self.env, &self.pool_addr)
    }
    fn smt(&self) -> TokenClient<'_> {
        TokenClient::new(&self.env, &self.smt_addr)
    }
    fn coll(&self) -> TokenClient<'_> {
        TokenClient::new(&self.env, &self.coll_addr)
    }
    fn smt_sac(&self) -> StellarAssetClient<'_> {
        StellarAssetClient::new(&self.env, &self.smt_addr)
    }
    fn coll_sac(&self) -> StellarAssetClient<'_> {
        StellarAssetClient::new(&self.env, &self.coll_addr)
    }

    fn setup(coll_price: i128, amm_smt: i128, amm_coll: i128, flash_fee_bps: u32) -> Self {
        let env = Env::default();
        env.mock_all_auths_allowing_non_root_auth();

        let admin = Address::generate(&env);
        let alice = Address::generate(&env);
        let liquidator = Address::generate(&env);

        let smt_addr = env.register_stellar_asset_contract(admin.clone());
        let coll_addr = env.register_stellar_asset_contract(admin.clone());
        let smt_sac = StellarAssetClient::new(&env, &smt_addr);
        let coll_sac = StellarAssetClient::new(&env, &coll_addr);

        // Oracle
        let oracle_addr = env.register(PriceOracle, ());
        let oracle = soromint_oracle::PriceOracleClient::new(&env, &oracle_addr);
        oracle.initialize(&admin);
        oracle.set_price(&coll_addr, &coll_price, &admin);

        // Lending pool
        let pool_addr = env.register(LendingPool, ());
        let pool = LendingPoolClient::new(&env, &pool_addr);
        pool.initialize(&admin, &smt_addr, &oracle_addr);
        pool.set_asset_config(
            &coll_addr,
            &AssetConfig {
                ltv_bps: 8_000,
                liquidation_threshold: 9_000,
                liquidation_bonus: 1_000,
                is_active: true,
            },
        );

        // Flash-loan provider
        let flash_loan_addr = env.register(SmtFlashLoanProvider, ());
        flash_loan::SmtFlashLoanProviderClient::new(&env, &flash_loan_addr)
            .initialize(&smt_addr, &flash_fee_bps);

        // AMM pool (token=coll, quote=SMT)
        let amm_factory = Address::generate(&env);
        let amm_addr = env.register(AmmPool, ());
        let amm = soromint_amm_pool::AmmPoolClient::new(&env, &amm_addr);
        amm.initialize(&amm_factory, &coll_addr, &smt_addr, &30u32);
        coll_sac.mint(&admin, &amm_coll);
        smt_sac.mint(&admin, &amm_smt);
        amm.add_liquidity(&admin, &amm_coll, &amm_smt, &1i128);

        // Fund pool with SMT reserves
        smt_sac.mint(&pool_addr, &100_000_0000000i128);
        // Fund flash-loan provider
        smt_sac.mint(&flash_loan_addr, &50_000_0000000i128);

        TestEnv {
            env,
            pool_addr,
            flash_loan_addr,
            oracle_addr,
            amm_addr,
            smt_addr,
            coll_addr,
            admin,
            alice,
            liquidator,
        }
    }

    fn alice_deposit(&self, amount: i128) {
        self.coll_sac().mint(&self.alice, &amount);
        self.pool().deposit(&self.alice, &self.coll_addr, &amount);
    }

    /// Tighten threshold so existing position becomes unhealthy.
    fn make_unhealthy(&self) {
        self.pool().set_asset_config(
            &self.coll_addr,
            &AssetConfig {
                ltv_bps: 5_000,
                liquidation_threshold: 5_000,
                liquidation_bonus: 1_000,
                is_active: true,
            },
        );
    }

    fn flash_liquidate(&self, repay_amount: i128, min_swap_output: i128) {
        let params = FlashLiquidateParams {
            initiator: self.liquidator.clone(),
            flash_loan: self.flash_loan_addr.clone(),
            lending_pool: self.pool_addr.clone(),
            borrower: self.alice.clone(),
            collateral_asset: self.coll_addr.clone(),
            repay_amount,
            amm_pool: self.amm_addr.clone(),
            min_swap_output,
            max_price_age_secs: 86_400,
        };

        // Store params in the pool's instance storage so receive_loan can read them.
        // We do this by calling pool.setup_flash_liquidation().
        self.pool().setup_flash_liquidation(&params);

        // Now call flash_loan directly — it will call pool.receive_loan.
        // Call stack: test -> flash_loan.flash_loan -> pool.receive_loan
        // The pool appears only once — no re-entry violation.
        flash_loan::SmtFlashLoanProviderClient::new(&self.env, &self.flash_loan_addr)
            .flash_loan(&self.pool_addr, &repay_amount, &soroban_sdk::Bytes::new(&self.env));
    }
}

// ---------------------------------------------------------------------------
// 1. Basic lifecycle
// ---------------------------------------------------------------------------

#[test]
fn test_deposit_updates_collateral_balance() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.coll_sac().mint(&t.alice, &500_0000000);
    t.pool().deposit(&t.alice, &t.coll_addr, &500_0000000);
    assert_eq!(t.pool().get_collateral(&t.alice, &t.coll_addr), 500_0000000);
    assert_eq!(t.coll().balance(&t.pool_addr), 500_0000000);
}

#[test]
fn test_borrow_transfers_smt() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &700_0000000);
    assert_eq!(t.pool().get_debt(&t.alice), 700_0000000);
    assert_eq!(t.smt().balance(&t.alice), 700_0000000);
}

#[test]
#[should_panic(expected = "insufficient collateral for borrow")]
fn test_borrow_exceeds_ltv_panics() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &900_0000000);
}

#[test]
fn test_repay_reduces_debt() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &500_0000000);
    t.pool().repay(&t.alice, &200_0000000);
    assert_eq!(t.pool().get_debt(&t.alice), 300_0000000);
}

#[test]
fn test_repay_caps_at_debt() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &200_0000000);
    t.pool().repay(&t.alice, &500_0000000);
    assert_eq!(t.pool().get_debt(&t.alice), 0);
}

#[test]
fn test_withdraw_after_full_repay() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &500_0000000);
    t.pool().repay(&t.alice, &500_0000000);
    t.pool().withdraw(&t.alice, &t.coll_addr, &1_000_0000000);
    assert_eq!(t.coll().balance(&t.alice), 1_000_0000000);
}

#[test]
#[should_panic(expected = "withdrawal would undercollateralize position")]
fn test_withdraw_blocked_with_outstanding_debt() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    // Deposit 1000, borrow 900 (threshold = 90% * 1000 = 900, debt = 900 -> HF = exactly 1.0)
    // Any withdrawal tips it below 1.0
    t.alice_deposit(1_000_0000000);
    t.pool().set_asset_config(
        &t.coll_addr,
        &AssetConfig {
            ltv_bps: 9_000,          // allow borrowing 90%
            liquidation_threshold: 9_000,
            liquidation_bonus: 1_000,
            is_active: true,
        },
    );
    t.pool().borrow(&t.alice, &900_0000000); // borrow exactly at threshold
    // threshold_adj_coll = 1000 * 9000/10000 = 900; debt = 900; HF = 1.0 exactly
    // Withdrawing any amount makes threshold_adj_coll < 900 -> HF < 1.0
    t.pool().withdraw(&t.alice, &t.coll_addr, &1_0000000);
}

// ---------------------------------------------------------------------------
// 2. Health factor
// ---------------------------------------------------------------------------

#[test]
fn test_health_factor_above_one_at_70_pct_ltv() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &700_0000000);
    // threshold_adj = 1000 * 0.9 = 900; HF = 900/700 * 10M ≈ 12_857_142
    assert!(t.pool().get_health_factor(&t.alice) > 10_000_000);
}

#[test]
fn test_is_healthy_true_below_threshold() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &700_0000000);
    assert!(t.pool().is_healthy(&t.alice));
}

#[test]
fn test_is_healthy_false_after_threshold_tightened() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &800_0000000);
    t.pool().set_asset_config(
        &t.coll_addr,
        &AssetConfig {
            ltv_bps: 8_000,
            liquidation_threshold: 7_000,
            liquidation_bonus: 1_000,
            is_active: true,
        },
    );
    // threshold_adj = 700 < debt 800 -> unhealthy
    assert!(!t.pool().is_healthy(&t.alice));
}

// ---------------------------------------------------------------------------
// 3. Standard liquidation
// ---------------------------------------------------------------------------

#[test]
fn test_standard_liquidation_seizes_collateral_with_bonus() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &800_0000000);
    t.make_unhealthy();

    t.smt_sac().mint(&t.liquidator, &400_0000000);
    t.pool().liquidate(&t.liquidator, &t.alice, &t.coll_addr, &400_0000000);

    // base = 400 * 10M / 10M = 400; with 10% bonus = 440
    assert_eq!(t.coll().balance(&t.liquidator), 440_0000000);
    assert_eq!(t.pool().get_debt(&t.alice), 400_0000000);
}

#[test]
#[should_panic(expected = "borrower is healthy")]
fn test_standard_liquidation_fails_on_healthy_position() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &500_0000000);
    t.smt_sac().mint(&t.liquidator, &200_0000000);
    t.pool().liquidate(&t.liquidator, &t.alice, &t.coll_addr, &200_0000000);
}

#[test]
#[should_panic(expected = "repay amount exceeds debt")]
fn test_standard_liquidation_fails_repay_exceeds_debt() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &800_0000000);
    t.make_unhealthy();
    t.smt_sac().mint(&t.liquidator, &900_0000000);
    t.pool().liquidate(&t.liquidator, &t.alice, &t.coll_addr, &900_0000000);
}

#[test]
fn test_partial_liquidation_leaves_remaining_debt() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &800_0000000);
    t.make_unhealthy();
    t.smt_sac().mint(&t.liquidator, &200_0000000);
    t.pool().liquidate(&t.liquidator, &t.alice, &t.coll_addr, &200_0000000);
    assert_eq!(t.pool().get_debt(&t.alice), 600_0000000);
}

// ---------------------------------------------------------------------------
// 4. Flash-loan liquidation — end-to-end via executor contract
// ---------------------------------------------------------------------------

#[test]
fn test_flash_liquidation_full_lifecycle() {
    let t = TestEnv::setup(10_000_000, 500_000_0000000, 500_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &800_0000000);
    t.make_unhealthy();

    let debt_before = t.pool().get_debt(&t.alice);
    let coll_before = t.pool().get_collateral(&t.alice, &t.coll_addr);
    let pool_smt_before = t.smt().balance(&t.pool_addr);

    t.flash_liquidate(400_0000000, 1i128);

    assert_eq!(t.pool().get_debt(&t.alice), debt_before - 400_0000000);
    assert!(t.pool().get_collateral(&t.alice, &t.coll_addr) < coll_before);
    // Pool's SMT should not have decreased (repayment came back via AMM)
    assert!(t.smt().balance(&t.pool_addr) >= pool_smt_before);
}

#[test]
#[should_panic(expected = "borrower is healthy")]
fn test_flash_liquidation_fails_on_healthy_position() {
    let t = TestEnv::setup(10_000_000, 500_000_0000000, 500_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &400_0000000);
    // Position is healthy — pool.execute_flash_liquidation should panic
    t.flash_liquidate(200_0000000, 1i128);
}

#[test]
#[should_panic(expected = "slippage exceeded")]
fn test_flash_liquidation_fails_when_amm_too_shallow() {
    // Tiny AMM — setting min_swap_output high triggers AMM's slippage guard
    let t = TestEnv::setup(10_000_000, 10_0000000, 10_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &800_0000000);
    t.make_unhealthy();
    // Demand at least 390 SMT back from the swap — impossible with 10 SMT reserves
    t.flash_liquidate(400_0000000, 390_0000000);
}

// ---------------------------------------------------------------------------
// 5. Reentrancy guard
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "reentrancy detected")]
fn test_reentrancy_guard_prevents_double_entry() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.env.as_contract(&t.pool_addr, || {
        let _g1 = crate::ReentrancyGuard::lock(&t.env, &t.liquidator);
        let _g2 = crate::ReentrancyGuard::lock(&t.env, &t.liquidator);
    });
}

// ---------------------------------------------------------------------------
// 6. Seizure math
// ---------------------------------------------------------------------------

#[test]
fn test_compute_collateral_to_seize_one_to_one_price() {
    let result = compute_collateral_to_seize(100_0000000, 10_000_000, 1_000);
    assert_eq!(result, 110_0000000);
}

#[test]
fn test_compute_collateral_to_seize_double_price() {
    let result = compute_collateral_to_seize(100_0000000, 20_000_000, 500);
    assert_eq!(result, 52_5000000);
}

#[test]
#[should_panic(expected = "invalid price")]
fn test_compute_collateral_to_seize_zero_price_panics() {
    compute_collateral_to_seize(100, 0, 500);
}

// ---------------------------------------------------------------------------
// 7. Oracle price-staleness guard
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "oracle price is stale")]
fn test_liquidation_rejects_stale_oracle_price() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &800_0000000);
    t.make_unhealthy();
    t.env.ledger().with_mut(|li| li.timestamp = 7_200);
    t.env.as_contract(&t.pool_addr, || {
        require_fresh_price(&t.env, &t.oracle_addr, &t.coll_addr, 1u64);
    });
}

// ---------------------------------------------------------------------------
// 8. Aggregate borrow power and re-borrow
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_deposits_aggregate_borrow_power() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.coll_sac().mint(&t.alice, &500_0000000);
    t.pool().deposit(&t.alice, &t.coll_addr, &500_0000000);
    t.coll_sac().mint(&t.alice, &500_0000000);
    t.pool().deposit(&t.alice, &t.coll_addr, &500_0000000);
    t.pool().borrow(&t.alice, &800_0000000);
    assert_eq!(t.pool().get_debt(&t.alice), 800_0000000);
}

#[test]
fn test_full_repay_and_re_borrow() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    t.pool().borrow(&t.alice, &500_0000000);
    t.pool().repay(&t.alice, &500_0000000);
    t.pool().borrow(&t.alice, &400_0000000);
    assert_eq!(t.pool().get_debt(&t.alice), 400_0000000);
}

// ---------------------------------------------------------------------------
// 9. Zero-debt
// ---------------------------------------------------------------------------

#[test]
fn test_no_debt_user_is_always_healthy() {
    let t = TestEnv::setup(10_000_000, 1_000_0000000, 1_000_0000000, 9);
    t.alice_deposit(1_000_0000000);
    assert_eq!(t.pool().get_health_factor(&t.alice), i128::MAX);
    assert!(t.pool().is_healthy(&t.alice));
}
