//! Gateway storage.

use soroban_sdk::{contracttype, Address, Env, Vec};

use crate::errors::Error;
use crate::types::{Digest, GatewayConfig, IssuerQuota, TokenRecord};

pub const BUMP_AMOUNT: u32 = 518_400;
pub const LIFETIME_THRESHOLD: u32 = BUMP_AMOUNT - 86_400;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Initialized,
    Admin,
    Config,
    Paused,
    Token(Digest),
    TokenIndex,
    Quota(Address),
}

pub fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
}

pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Initialized)
}

pub fn mark_initialized(env: &Env) {
    env.storage().instance().set(&DataKey::Initialized, &true);
}

pub fn require_initialized(env: &Env) -> Result<(), Error> {
    if is_initialized(env) {
        Ok(())
    } else {
        Err(Error::NotInitialized)
    }
}

pub fn get_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn require_admin(env: &Env, caller: &Address) -> Result<(), Error> {
    caller.require_auth();
    if &get_admin(env)? == caller {
        Ok(())
    } else {
        Err(Error::Unauthorized)
    }
}

pub fn get_config(env: &Env) -> Result<GatewayConfig, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(Error::NotInitialized)
}

pub fn set_config(env: &Env, config: &GatewayConfig) {
    env.storage().instance().set(&DataKey::Config, config);
}

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::Paused, &paused);
}

pub fn require_not_paused(env: &Env) -> Result<(), Error> {
    if is_paused(env) {
        Err(Error::Paused)
    } else {
        Ok(())
    }
}

pub fn get_token(env: &Env, commitment: &Digest) -> Result<TokenRecord, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Token(commitment.clone()))
        .ok_or(Error::TokenNotFound)
}

pub fn has_token(env: &Env, commitment: &Digest) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Token(commitment.clone()))
}

pub fn set_token(env: &Env, record: &TokenRecord) {
    let key = DataKey::Token(record.metadata_commitment.clone());
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);
}

pub fn token_index(env: &Env) -> Vec<Digest> {
    env.storage()
        .instance()
        .get(&DataKey::TokenIndex)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn push_token_index(env: &Env, commitment: &Digest) {
    let mut index = token_index(env);
    index.push_back(commitment.clone());
    env.storage().instance().set(&DataKey::TokenIndex, &index);
}

pub fn get_quota(env: &Env, issuer: &Address) -> Option<IssuerQuota> {
    env.storage()
        .persistent()
        .get(&DataKey::Quota(issuer.clone()))
}

pub fn set_quota(env: &Env, issuer: &Address, quota: &IssuerQuota) {
    let key = DataKey::Quota(issuer.clone());
    env.storage().persistent().set(&key, quota);
    env.storage()
        .persistent()
        .extend_ttl(&key, LIFETIME_THRESHOLD, BUMP_AMOUNT);
}
