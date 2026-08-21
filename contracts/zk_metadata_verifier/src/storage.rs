//! Storage layout and TTL management.
//!
//! # Durability tiers
//!
//! | Tier         | Holds                                        | Rationale |
//! |--------------|----------------------------------------------|-----------|
//! | Instance     | config, roles, stats, active policy pointer  | Small, read on nearly every call, must live as long as the contract. |
//! | Persistent   | circuits, policies, attestations, nullifiers | Must survive independently; losing a nullifier would re-open a replay window. |
//! | Temporary    | *(unused)*                                   | Deliberately avoided — see below. |
//!
//! Nullifiers are the subtle case. It is tempting to store them as temporary
//! entries so they expire cheaply, but a temporary entry that expires *before*
//! the proof it nullifies does re-opens exactly the replay the nullifier
//! exists to prevent. They therefore live in persistent storage with a TTL that
//! [`crate::types::Config`] forces to exceed the maximum proof validity window.

use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

use crate::errors::Error;
use crate::types::{
    Attestation, CircuitRecord, Config, Digest, PendingRotation, Policy, Role, Stats,
};

/// Instance-storage entries live as long as the contract itself.
pub const INSTANCE_BUMP_AMOUNT: u32 = 518_400; // ~30 days at 5s ledgers
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = INSTANCE_BUMP_AMOUNT - 86_400;

/// Circuits and policies are long-lived governance state.
pub const REGISTRY_BUMP_AMOUNT: u32 = 1_036_800; // ~60 days
pub const REGISTRY_LIFETIME_THRESHOLD: u32 = REGISTRY_BUMP_AMOUNT - 172_800;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Marker written by `initialize`.
    Initialized,
    /// [`Config`].
    Config,
    /// Paused flag.
    Paused,
    /// [`Stats`].
    Stats,
    /// Root administrator address.
    Admin,
    /// Address awaiting a two-step admin handoff.
    PendingAdmin,
    /// `(Role, Address) -> bool` grant.
    RoleGrant(Role, Address),
    /// Number of holders of a role, so the last admin cannot be removed.
    RoleCount(Role),
    /// `Symbol -> CircuitRecord`.
    Circuit(Symbol),
    /// Ordered list of registered circuit ids.
    CircuitIndex,
    /// `Symbol -> PendingRotation`.
    Rotation(Symbol),
    /// `u32 -> Policy`.
    Policy(u32),
    /// Version number of the policy currently in force.
    ActivePolicy,
    /// Ordered list of published policy versions.
    PolicyIndex,
    /// `Digest -> u32` (ledger at which the nullifier was spent).
    Nullifier(Digest),
    /// `Digest -> Attestation`, keyed by metadata commitment.
    Attestation(Digest),
    /// Per-issuer count of attestations issued, for rate observation.
    IssuerCount(Address),
}

// ---------------------------------------------------------------------------
// Instance helpers
// ---------------------------------------------------------------------------

/// Extend the instance TTL. Called at the top of every state-mutating entry.
pub fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
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

pub fn get_config(env: &Env) -> Result<Config, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(Error::NotInitialized)
}

pub fn set_config(env: &Env, config: &Config) {
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

pub fn get_stats(env: &Env) -> Stats {
    env.storage()
        .instance()
        .get(&DataKey::Stats)
        .unwrap_or_default()
}

pub fn set_stats(env: &Env, stats: &Stats) {
    env.storage().instance().set(&DataKey::Stats, stats);
}

/// Apply a mutation to [`Stats`] and write it back.
pub fn with_stats<F: FnOnce(&mut Stats)>(env: &Env, f: F) {
    let mut stats = get_stats(env);
    f(&mut stats);
    set_stats(env, &stats);
}

// ---------------------------------------------------------------------------
// Roles
// ---------------------------------------------------------------------------

pub fn get_admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}

pub fn set_pending_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::PendingAdmin, admin);
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}

pub fn has_role(env: &Env, role: Role, who: &Address) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::RoleGrant(role, who.clone()))
        .unwrap_or(false)
}

pub fn grant_role(env: &Env, role: Role, who: &Address) {
    if has_role(env, role, who) {
        return;
    }
    env.storage()
        .instance()
        .set(&DataKey::RoleGrant(role, who.clone()), &true);
    let count: u32 = role_count(env, role);
    env.storage()
        .instance()
        .set(&DataKey::RoleCount(role), &(count + 1));
}

pub fn revoke_role(env: &Env, role: Role, who: &Address) {
    if !has_role(env, role, who) {
        return;
    }
    env.storage()
        .instance()
        .remove(&DataKey::RoleGrant(role, who.clone()));
    let count: u32 = role_count(env, role);
    env.storage()
        .instance()
        .set(&DataKey::RoleCount(role), &count.saturating_sub(1));
}

pub fn role_count(env: &Env, role: Role) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::RoleCount(role))
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Circuits
// ---------------------------------------------------------------------------

pub fn get_circuit(env: &Env, circuit_id: &Symbol) -> Result<CircuitRecord, Error> {
    let record: CircuitRecord = env
        .storage()
        .persistent()
        .get(&DataKey::Circuit(circuit_id.clone()))
        .ok_or(Error::CircuitNotFound)?;
    env.storage().persistent().extend_ttl(
        &DataKey::Circuit(circuit_id.clone()),
        REGISTRY_LIFETIME_THRESHOLD,
        REGISTRY_BUMP_AMOUNT,
    );
    Ok(record)
}

pub fn has_circuit(env: &Env, circuit_id: &Symbol) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Circuit(circuit_id.clone()))
}

pub fn set_circuit(env: &Env, record: &CircuitRecord) {
    let key = DataKey::Circuit(record.circuit_id.clone());
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, REGISTRY_LIFETIME_THRESHOLD, REGISTRY_BUMP_AMOUNT);
}

pub fn circuit_index(env: &Env) -> Vec<Symbol> {
    env.storage()
        .instance()
        .get(&DataKey::CircuitIndex)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn push_circuit_index(env: &Env, circuit_id: &Symbol) {
    let mut index = circuit_index(env);
    index.push_back(circuit_id.clone());
    env.storage().instance().set(&DataKey::CircuitIndex, &index);
}

pub fn get_rotation(env: &Env, circuit_id: &Symbol) -> Result<PendingRotation, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Rotation(circuit_id.clone()))
        .ok_or(Error::NoPendingRotation)
}

pub fn set_rotation(env: &Env, rotation: &PendingRotation) {
    let key = DataKey::Rotation(rotation.circuit_id.clone());
    env.storage().persistent().set(&key, rotation);
    env.storage()
        .persistent()
        .extend_ttl(&key, REGISTRY_LIFETIME_THRESHOLD, REGISTRY_BUMP_AMOUNT);
}

pub fn clear_rotation(env: &Env, circuit_id: &Symbol) {
    env.storage()
        .persistent()
        .remove(&DataKey::Rotation(circuit_id.clone()));
}

// ---------------------------------------------------------------------------
// Policies
// ---------------------------------------------------------------------------

pub fn get_policy(env: &Env, version: u32) -> Result<Policy, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Policy(version))
        .ok_or(Error::PolicyNotFound)
}

pub fn set_policy(env: &Env, policy: &Policy) {
    let key = DataKey::Policy(policy.version);
    env.storage().persistent().set(&key, policy);
    env.storage()
        .persistent()
        .extend_ttl(&key, REGISTRY_LIFETIME_THRESHOLD, REGISTRY_BUMP_AMOUNT);
}

pub fn active_policy_version(env: &Env) -> Result<u32, Error> {
    env.storage()
        .instance()
        .get(&DataKey::ActivePolicy)
        .ok_or(Error::PolicyNotFound)
}

pub fn set_active_policy_version(env: &Env, version: u32) {
    env.storage()
        .instance()
        .set(&DataKey::ActivePolicy, &version);
}

pub fn active_policy(env: &Env) -> Result<Policy, Error> {
    let version = active_policy_version(env)?;
    get_policy(env, version)
}

pub fn policy_index(env: &Env) -> Vec<u32> {
    env.storage()
        .instance()
        .get(&DataKey::PolicyIndex)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn push_policy_index(env: &Env, version: u32) {
    let mut index = policy_index(env);
    index.push_back(version);
    env.storage().instance().set(&DataKey::PolicyIndex, &index);
}

// ---------------------------------------------------------------------------
// Nullifiers
// ---------------------------------------------------------------------------

pub fn nullifier_spent_at(env: &Env, nullifier: &Digest) -> Option<u32> {
    env.storage()
        .persistent()
        .get(&DataKey::Nullifier(nullifier.clone()))
}

pub fn spend_nullifier(env: &Env, nullifier: &Digest, ttl: u32) {
    let key = DataKey::Nullifier(nullifier.clone());
    env.storage()
        .persistent()
        .set(&key, &env.ledger().sequence());
    let threshold = ttl.saturating_sub(ttl / 8).max(1);
    env.storage().persistent().extend_ttl(&key, threshold, ttl);
}

// ---------------------------------------------------------------------------
// Attestations
// ---------------------------------------------------------------------------

pub fn get_attestation(env: &Env, commitment: &Digest) -> Result<Attestation, Error> {
    env.storage()
        .persistent()
        .get(&DataKey::Attestation(commitment.clone()))
        .ok_or(Error::AttestationNotFound)
}

pub fn find_attestation(env: &Env, commitment: &Digest) -> Option<Attestation> {
    env.storage()
        .persistent()
        .get(&DataKey::Attestation(commitment.clone()))
}

pub fn set_attestation(env: &Env, attestation: &Attestation, ttl: u32) {
    let key = DataKey::Attestation(attestation.metadata_commitment.clone());
    env.storage().persistent().set(&key, attestation);
    let threshold = ttl.saturating_sub(ttl / 8).max(1);
    env.storage().persistent().extend_ttl(&key, threshold, ttl);
}

pub fn remove_attestation(env: &Env, commitment: &Digest) {
    env.storage()
        .persistent()
        .remove(&DataKey::Attestation(commitment.clone()));
}

pub fn bump_issuer_count(env: &Env, issuer: &Address) -> u64 {
    let key = DataKey::IssuerCount(issuer.clone());
    let count: u64 = env.storage().persistent().get(&key).unwrap_or(0);
    let next = count.saturating_add(1);
    env.storage().persistent().set(&key, &next);
    env.storage()
        .persistent()
        .extend_ttl(&key, REGISTRY_LIFETIME_THRESHOLD, REGISTRY_BUMP_AMOUNT);
    next
}

pub fn issuer_count(env: &Env, issuer: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::IssuerCount(issuer.clone()))
        .unwrap_or(0)
}
