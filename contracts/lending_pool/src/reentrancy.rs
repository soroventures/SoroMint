//! Reentrancy protection for the lending pool.
//!
//! Flash loans involve outbound cross-contract calls (to the borrower, then
//! potentially back into this pool). We lock on the *contract address* of the
//! caller so that the same initiating contract cannot re-enter.
//!
//! The guard stores a boolean flag in **instance** storage under
//! `DataKey::ReentrancyLock(caller)`.  Because Soroban transactions are
//! atomic, any storage written during a call is visible to subsequent calls
//! within the same transaction, making this pattern sound.

use soroban_sdk::{Address, Env};
use crate::types::DataKey;

pub struct ReentrancyGuard<'env> {
    env: &'env Env,
    key: DataKey,
}

impl<'env> ReentrancyGuard<'env> {
    /// Acquire the lock for `caller`.  Panics if it is already held.
    pub fn lock(env: &'env Env, caller: &Address) -> Self {
        let key = DataKey::ReentrancyLock(caller.clone());
        if env.storage().instance().has(&key) {
            panic!("reentrancy detected");
        }
        env.storage().instance().set(&key, &true);
        ReentrancyGuard { env, key }
    }
}

impl<'env> Drop for ReentrancyGuard<'env> {
    fn drop(&mut self) {
        self.env.storage().instance().remove(&self.key);
    }
}
