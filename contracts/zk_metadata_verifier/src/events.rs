//! Event emission.
//!
//! Topics follow the repository convention of `(subject, action, [key])` so
//! indexers can subscribe narrowly. Every event that reports a verification
//! carries the circuit id and policy version, because a proof is only
//! meaningful relative to the rules it was checked against — an indexer that
//! records "metadata approved" without recording *which policy approved it*
//! cannot answer the only question that matters after a rule change.

use soroban_sdk::{symbol_short, Address, Env, Symbol};

use crate::types::{Digest, Role};

const VERIFIER: Symbol = symbol_short!("verifier");
const CIRCUIT: Symbol = symbol_short!("circuit");
const POLICY: Symbol = symbol_short!("policy");
const ATTEST: Symbol = symbol_short!("attest");
const NULLIF: Symbol = symbol_short!("nullif");
const ROLE: Symbol = symbol_short!("role");

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

pub fn initialized(env: &Env, admin: &Address) {
    env.events()
        .publish((VERIFIER, symbol_short!("init")), admin.clone());
}

pub fn paused(env: &Env, by: &Address) {
    env.events()
        .publish((VERIFIER, symbol_short!("paused")), by.clone());
}

pub fn unpaused(env: &Env, by: &Address) {
    env.events()
        .publish((VERIFIER, symbol_short!("unpaused")), by.clone());
}

pub fn config_updated(env: &Env, by: &Address) {
    env.events()
        .publish((VERIFIER, symbol_short!("config")), by.clone());
}

// ---------------------------------------------------------------------------
// Roles
// ---------------------------------------------------------------------------

pub fn role_granted(env: &Env, role: Role, who: &Address, by: &Address) {
    env.events().publish(
        (ROLE, symbol_short!("granted"), who.clone()),
        (role, by.clone()),
    );
}

pub fn role_revoked(env: &Env, role: Role, who: &Address, by: &Address) {
    env.events().publish(
        (ROLE, symbol_short!("revoked"), who.clone()),
        (role, by.clone()),
    );
}

pub fn admin_transfer_started(env: &Env, from: &Address, to: &Address) {
    env.events().publish(
        (ROLE, symbol_short!("xfer_req")),
        (from.clone(), to.clone()),
    );
}

pub fn admin_transfer_completed(env: &Env, from: &Address, to: &Address) {
    env.events()
        .publish((ROLE, symbol_short!("xfer_ok")), (from.clone(), to.clone()));
}

// ---------------------------------------------------------------------------
// Circuit registry
// ---------------------------------------------------------------------------

pub fn circuit_registered(
    env: &Env,
    circuit_id: &Symbol,
    digest: &Digest,
    arity: u32,
    by: &Address,
) {
    env.events().publish(
        (CIRCUIT, symbol_short!("register"), circuit_id.clone()),
        (digest.clone(), arity, by.clone()),
    );
}

pub fn rotation_proposed(env: &Env, circuit_id: &Symbol, digest: &Digest, eta: u32, by: &Address) {
    env.events().publish(
        (CIRCUIT, symbol_short!("rot_prop"), circuit_id.clone()),
        (digest.clone(), eta, by.clone()),
    );
}

pub fn rotation_cancelled(env: &Env, circuit_id: &Symbol, by: &Address) {
    env.events().publish(
        (CIRCUIT, symbol_short!("rot_cncl"), circuit_id.clone()),
        by.clone(),
    );
}

pub fn rotation_finalized(
    env: &Env,
    circuit_id: &Symbol,
    old_digest: &Digest,
    new_digest: &Digest,
    revision: u32,
) {
    env.events().publish(
        (CIRCUIT, symbol_short!("rot_done"), circuit_id.clone()),
        (old_digest.clone(), new_digest.clone(), revision),
    );
}

pub fn circuit_enabled(env: &Env, circuit_id: &Symbol, enabled: bool, by: &Address) {
    env.events().publish(
        (CIRCUIT, symbol_short!("enabled"), circuit_id.clone()),
        (enabled, by.clone()),
    );
}

pub fn circuit_frozen(env: &Env, circuit_id: &Symbol, by: &Address) {
    env.events().publish(
        (CIRCUIT, symbol_short!("frozen"), circuit_id.clone()),
        by.clone(),
    );
}

// ---------------------------------------------------------------------------
// Policy
// ---------------------------------------------------------------------------

pub fn policy_published(env: &Env, version: u32, root: &Digest, by: &Address) {
    env.events().publish(
        (POLICY, symbol_short!("publish"), version),
        (root.clone(), by.clone()),
    );
}

pub fn policy_activated(env: &Env, version: u32, previous: u32, by: &Address) {
    env.events().publish(
        (POLICY, symbol_short!("activate"), version),
        (previous, by.clone()),
    );
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

pub fn proof_verified(env: &Env, circuit_id: &Symbol, inputs: u32) {
    env.events().publish(
        (VERIFIER, symbol_short!("proof_ok"), circuit_id.clone()),
        inputs,
    );
}

pub fn batch_verified(env: &Env, circuit_id: &Symbol, size: u32) {
    env.events().publish(
        (VERIFIER, symbol_short!("batch_ok"), circuit_id.clone()),
        size,
    );
}

pub fn nullifier_spent(env: &Env, nullifier: &Digest, issuer: &Address) {
    env.events().publish(
        (NULLIF, symbol_short!("spent"), nullifier.clone()),
        issuer.clone(),
    );
}

pub fn attestation_issued(
    env: &Env,
    commitment: &Digest,
    issuer: &Address,
    risk_score: u32,
    policy_version: u32,
    expires_at: u32,
) {
    env.events().publish(
        (ATTEST, symbol_short!("issued"), commitment.clone()),
        (issuer.clone(), risk_score, policy_version, expires_at),
    );
}

pub fn attestation_consumed(env: &Env, commitment: &Digest, consumer: &Address) {
    env.events().publish(
        (ATTEST, symbol_short!("consumed"), commitment.clone()),
        consumer.clone(),
    );
}

pub fn attestation_revoked(env: &Env, commitment: &Digest, by: &Address, reason: u32) {
    env.events().publish(
        (ATTEST, symbol_short!("revoked"), commitment.clone()),
        (by.clone(), reason),
    );
}

pub fn consumer_registered(env: &Env, consumer: &Address, by: &Address) {
    env.events().publish(
        (ATTEST, symbol_short!("consumer")),
        (consumer.clone(), by.clone()),
    );
}
