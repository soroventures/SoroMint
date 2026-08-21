//! Gateway events.

use soroban_sdk::{symbol_short, Address, Env, Symbol};

use crate::types::Digest;

const GATEWAY: Symbol = symbol_short!("gateway");
const TOKEN: Symbol = symbol_short!("token");

pub fn initialized(env: &Env, admin: &Address, verifier: &Address) {
    env.events().publish(
        (GATEWAY, symbol_short!("init")),
        (admin.clone(), verifier.clone()),
    );
}

pub fn config_updated(env: &Env, by: &Address) {
    env.events()
        .publish((GATEWAY, symbol_short!("config")), by.clone());
}

pub fn paused(env: &Env, by: &Address) {
    env.events()
        .publish((GATEWAY, symbol_short!("paused")), by.clone());
}

pub fn unpaused(env: &Env, by: &Address) {
    env.events()
        .publish((GATEWAY, symbol_short!("unpaused")), by.clone());
}

pub fn token_created(env: &Env, commitment: &Digest, issuer: &Address, risk_score: u32) {
    env.events().publish(
        (TOKEN, symbol_short!("created"), commitment.clone()),
        (issuer.clone(), risk_score),
    );
}

pub fn minted(env: &Env, commitment: &Digest, to: &Address, amount: i128, total: i128) {
    env.events().publish(
        (TOKEN, symbol_short!("minted"), commitment.clone()),
        (to.clone(), amount, total),
    );
}

pub fn token_frozen(env: &Env, commitment: &Digest, frozen: bool, by: &Address) {
    env.events().publish(
        (TOKEN, symbol_short!("frozen"), commitment.clone()),
        (frozen, by.clone()),
    );
}
