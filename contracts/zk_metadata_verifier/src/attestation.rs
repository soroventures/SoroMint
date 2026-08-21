//! Nullifiers and attestation records.
//!
//! # Why a nullifier and not just "hash of the proof"
//!
//! Groth16 proofs are randomized: re-running the prover on identical inputs
//! yields a different `(A, B, C)` every time. Keying replay protection on the
//! proof bytes would therefore protect nothing — an attacker who obtains a
//! witness can mint fresh proofs of the same statement forever.
//!
//! The nullifier is computed *inside the circuit* as
//! `Poseidon(metadataCommitment, issuerHash, nonce)` and exposed as a public
//! signal. Because the circuit constrains it, the prover cannot choose it
//! freely; because it is deterministic in the witness, two proofs of the same
//! statement collide on it. Spending it once closes the replay.
//!
//! # Why attestations are separate from nullifiers
//!
//! A nullifier answers "has this proof been used?". An attestation answers
//! "what was approved, by which rules, and is it still good?". The mint
//! lifecycle needs the second question answered days after the first, and needs
//! it answered cheaply — re-verifying a pairing on every mint would make the
//! design unusable.

use soroban_sdk::{Address, Env, Symbol};

use crate::errors::Error;
use crate::events;
use crate::storage;
use crate::types::{Attestation, AttestationStatus, Config, Digest, MetadataSignals};

/// Reject a nullifier that has already been spent.
pub fn require_unspent(env: &Env, nullifier: &Digest) -> Result<(), Error> {
    if storage::nullifier_spent_at(env, nullifier).is_some() {
        return Err(Error::NullifierAlreadyUsed);
    }
    Ok(())
}

/// Mark a nullifier spent.
///
/// The TTL comes from [`Config::nullifier_ttl`], which policy validation forces
/// to exceed the longest proof validity window. If that invariant were ever
/// broken, an expired nullifier entry would silently re-enable replay of a
/// proof that had not yet expired.
pub fn spend(env: &Env, nullifier: &Digest, issuer: &Address, config: &Config) {
    storage::spend_nullifier(env, nullifier, config.nullifier_ttl);
    storage::with_stats(env, |s| {
        s.nullifiers_spent = s.nullifiers_spent.saturating_add(1)
    });
    events::nullifier_spent(env, nullifier, issuer);
}

/// Build and persist the attestation for a freshly verified proof.
pub fn issue(
    env: &Env,
    signals: &MetadataSignals,
    issuer: &Address,
    circuit_id: &Symbol,
    policy_version: u32,
    config: &Config,
) -> Attestation {
    let now = env.ledger().sequence();

    // The attestation expires at whichever comes first: the proof's own claimed
    // expiry, or the configured retention window. Honouring the proof's expiry
    // matters — an issuer who proved "safe as of ledger N" has not proved
    // anything about ledger N + 100_000.
    let ttl_bound = now.saturating_add(config.attestation_ttl);
    let expires_at = if signals.expiry_ledger < ttl_bound {
        signals.expiry_ledger
    } else {
        ttl_bound
    };

    let attestation = Attestation {
        metadata_commitment: signals.metadata_commitment.clone(),
        issuer: issuer.clone(),
        nullifier: signals.nullifier.clone(),
        policy_version,
        circuit_id: circuit_id.clone(),
        risk_score: signals.risk_score,
        verified_at: now,
        expires_at,
        consumed: false,
        consumer: None,
        consumed_at: None,
    };

    storage::set_attestation(env, &attestation, config.attestation_ttl);
    storage::bump_issuer_count(env, issuer);
    storage::with_stats(env, |s| {
        s.attestations_issued = s.attestations_issued.saturating_add(1)
    });
    events::attestation_issued(
        env,
        &attestation.metadata_commitment,
        issuer,
        attestation.risk_score,
        policy_version,
        expires_at,
    );

    attestation
}

/// Fetch an attestation and assert it is currently redeemable.
pub fn require_valid(env: &Env, commitment: &Digest) -> Result<Attestation, Error> {
    let attestation = storage::get_attestation(env, commitment)?;
    if attestation.consumed {
        return Err(Error::AttestationAlreadyConsumed);
    }
    if env.ledger().sequence() > attestation.expires_at {
        return Err(Error::AttestationExpired);
    }
    Ok(attestation)
}

/// Redeem an attestation on behalf of a lifecycle contract.
///
/// Single-use by construction: the `consumed` flag is set in the same
/// transaction the caller reads it, so a token cannot be created twice off one
/// safety proof.
pub fn consume(
    env: &Env,
    consumer: &Address,
    commitment: &Digest,
    expected_issuer: &Address,
) -> Result<Attestation, Error> {
    let mut attestation = require_valid(env, commitment)?;

    if &attestation.issuer != expected_issuer {
        return Err(Error::IssuerBindingMismatch);
    }

    attestation.consumed = true;
    attestation.consumer = Some(consumer.clone());
    attestation.consumed_at = Some(env.ledger().sequence());

    let config = storage::get_config(env)?;
    storage::set_attestation(env, &attestation, config.attestation_ttl);
    storage::with_stats(env, |s| {
        s.attestations_consumed = s.attestations_consumed.saturating_add(1)
    });
    events::attestation_consumed(env, commitment, consumer);

    Ok(attestation)
}

/// Administratively invalidate an attestation.
///
/// Needed when a policy is retroactively found unsound: the proofs are still
/// mathematically valid, but the statement they prove is no longer one the
/// platform wants to honour.
pub fn revoke(env: &Env, caller: &Address, commitment: &Digest, reason: u32) -> Result<(), Error> {
    storage::get_attestation(env, commitment)?;
    storage::remove_attestation(env, commitment);
    events::attestation_revoked(env, commitment, caller, reason);
    Ok(())
}

/// Non-throwing status view for UIs and off-chain indexers.
pub fn status(env: &Env, commitment: &Digest) -> AttestationStatus {
    match storage::find_attestation(env, commitment) {
        None => AttestationStatus {
            exists: false,
            valid: false,
            consumed: false,
            expired: false,
            risk_score: 0,
            policy_version: 0,
        },
        Some(a) => {
            let expired = env.ledger().sequence() > a.expires_at;
            AttestationStatus {
                exists: true,
                valid: !a.consumed && !expired,
                consumed: a.consumed,
                expired,
                risk_score: a.risk_score,
                policy_version: a.policy_version,
            }
        }
    }
}
