//! AI safety policy management and public-signal semantics.
//!
//! # The split between circuit and policy
//!
//! The circuit proves a *structural* claim: "I ran the committed scoring
//! procedure over metadata whose commitment is `C`, against the rule set whose
//! Merkle root is `R`, using the model committed to by `M`, and it produced
//! verdict `v` with score `s`."
//!
//! It deliberately does **not** hard-code the acceptance threshold. If it did,
//! every threshold tweak would need a new trusted setup. Instead the contract
//! holds the thresholds and checks `s <= max_risk_score` in cheap integer
//! arithmetic, while pinning `R` and `M` so the prover cannot substitute a
//! weaker rule set or a doctored model.
//!
//! That division is what makes the design operable: rules move at governance
//! speed, the proof system moves at ceremony speed.
//!
//! # Public-signal ordering
//!
//! [`signals_to_public_inputs`] is the single place the wire order is defined.
//! It must mirror the circuit's `component main {public [...]}` declaration
//! exactly — a permutation here would verify proofs of a *different statement*
//! than the one the contract believes it is checking, with no visible symptom.

use soroban_sdk::{xdr::ToXdr, Address, Env, Vec};

use crate::errors::Error;
use crate::events;
use crate::field::{u32_to_scalar_bytes, ISSUER_DST};
use crate::storage;
use crate::types::{Digest, MetadataSignals, Policy, PolicyParams, Role, ScalarBytes};

/// Verdict value the circuit emits for "metadata is safe".
pub const VERDICT_SAFE: u32 = 1;

/// Upper bound on `risk_scale`, keeping score arithmetic well away from `u32`
/// overflow and matching the circuit's range-check width.
pub const MAX_RISK_SCALE: u32 = 1_000_000;

/// Publish a new policy version. It is stored but not yet in force.
pub fn publish_policy(env: &Env, caller: &Address, params: PolicyParams) -> Result<Policy, Error> {
    crate::access::require_role(env, caller, Role::PolicyAdmin)?;
    validate_params(env, &params)?;

    // Versions must strictly increase so that an attestation's recorded
    // `policy_version` is a total order — auditors can then say "everything
    // attested under v3 or earlier is suspect" without ambiguity.
    if let Ok(active) = storage::active_policy_version(env) {
        if params.version <= active {
            return Err(Error::PolicyVersionRegression);
        }
    }

    let policy = Policy {
        version: params.version,
        policy_root: params.policy_root.clone(),
        model_commitment: params.model_commitment,
        max_risk_score: params.max_risk_score,
        risk_scale: params.risk_scale,
        max_validity_ledgers: params.max_validity_ledgers,
        circuit_id: params.circuit_id,
        activated_at: 0,
        rulebook_uri: params.rulebook_uri,
    };

    storage::set_policy(env, &policy);
    storage::push_policy_index(env, policy.version);
    events::policy_published(env, policy.version, &params.policy_root, caller);

    Ok(policy)
}

/// Put a published policy into force.
pub fn activate_policy(env: &Env, caller: &Address, version: u32) -> Result<Policy, Error> {
    crate::access::require_role(env, caller, Role::PolicyAdmin)?;

    let mut policy = storage::get_policy(env, version)?;
    // The policy names the circuit its proofs must satisfy; activating a policy
    // that points at a missing or disabled circuit would brick verification.
    crate::registry::active_circuit(env, &policy.circuit_id)?;

    let previous = storage::active_policy_version(env).unwrap_or(0);
    if version <= previous {
        return Err(Error::PolicyVersionRegression);
    }

    policy.activated_at = env.ledger().sequence();
    storage::set_policy(env, &policy);
    storage::set_active_policy_version(env, version);
    events::policy_activated(env, version, previous, caller);

    Ok(policy)
}

/// Reject structurally nonsensical policy parameters at publish time, where the
/// error is cheap, rather than at verification time where it is confusing.
fn validate_params(env: &Env, params: &PolicyParams) -> Result<(), Error> {
    if params.version == 0 {
        return Err(Error::InvalidPolicyParameters);
    }
    if params.risk_scale == 0 || params.risk_scale > MAX_RISK_SCALE {
        return Err(Error::InvalidPolicyParameters);
    }
    if params.max_risk_score > params.risk_scale {
        return Err(Error::InvalidPolicyParameters);
    }
    if params.max_validity_ledgers == 0 {
        return Err(Error::InvalidPolicyParameters);
    }
    // A proof must not outlive the nullifier that prevents its replay.
    let config = storage::get_config(env)?;
    if params.max_validity_ledgers >= config.nullifier_ttl {
        return Err(Error::InvalidPolicyParameters);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Signal encoding
// ---------------------------------------------------------------------------

/// Flatten [`MetadataSignals`] into the circuit's public-input vector.
///
/// **Order is load-bearing.** See the module docs.
pub fn signals_to_public_inputs(env: &Env, signals: &MetadataSignals) -> Vec<ScalarBytes> {
    let mut inputs: Vec<ScalarBytes> = Vec::new(env);
    inputs.push_back(u32_to_scalar_bytes(env, signals.verdict)); //            0
    inputs.push_back(u32_to_scalar_bytes(env, signals.risk_score)); //         1
    inputs.push_back(signals.policy_root.clone()); //                          2
    inputs.push_back(signals.model_commitment.clone()); //                     3
    inputs.push_back(signals.metadata_commitment.clone()); //                  4
    inputs.push_back(signals.issuer_hash.clone()); //                          5
    inputs.push_back(signals.nullifier.clone()); //                            6
    inputs.push_back(u32_to_scalar_bytes(env, signals.expiry_ledger)); //      7
    inputs
}

/// Number of public inputs the metadata circuit exposes.
pub const METADATA_ARITY: u32 = 8;

/// Bind an issuer address to the field element the circuit committed to.
///
/// The circuit cannot parse a Stellar address, so the prover feeds it a hash.
/// The contract recomputes that hash from the *authenticated caller* and
/// requires equality — which is what stops Mallory from replaying Alice's
/// perfectly valid proof under her own account.
///
/// The top byte is cleared for the same reason as in
/// [`crate::field::Transcript::challenge`]: the result must be a canonical
/// field element, and truncation is the cheapest way to guarantee it.
pub fn issuer_hash(env: &Env, issuer: &Address) -> Digest {
    let mut buf = soroban_sdk::Bytes::new(env);
    buf.extend_from_slice(ISSUER_DST);
    buf.append(&issuer.clone().to_xdr(env));
    let digest = env.crypto().sha256(&buf);
    let mut arr = digest.to_array();
    arr[0] = 0;
    soroban_sdk::BytesN::from_array(env, &arr)
}

/// Enforce every policy-level condition on a set of public signals.
///
/// Runs *before* the pairing check. Two reasons for the ordering: a mismatched
/// policy root is a 300-instruction comparison while the pairing is millions,
/// so failing fast is much cheaper; and it keeps the expensive path from being
/// a free denial-of-service amplifier.
pub fn check_signals(
    env: &Env,
    policy: &Policy,
    signals: &MetadataSignals,
    issuer: &Address,
) -> Result<(), Error> {
    if signals.verdict != VERDICT_SAFE {
        return Err(Error::VerdictRejected);
    }
    if signals.policy_root != policy.policy_root {
        return Err(Error::PolicyRootMismatch);
    }
    if signals.model_commitment != policy.model_commitment {
        return Err(Error::ModelCommitmentMismatch);
    }
    if signals.risk_score > policy.max_risk_score {
        return Err(Error::RiskScoreTooHigh);
    }

    let now = env.ledger().sequence();
    if signals.expiry_ledger <= now {
        return Err(Error::ProofExpired);
    }
    if signals.expiry_ledger.saturating_sub(now) > policy.max_validity_ledgers {
        return Err(Error::ExpiryTooDistant);
    }

    if signals.issuer_hash != issuer_hash(env, issuer) {
        return Err(Error::IssuerBindingMismatch);
    }

    Ok(())
}
