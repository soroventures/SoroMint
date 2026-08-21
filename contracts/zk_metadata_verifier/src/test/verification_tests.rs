//! End-to-end `verify_metadata`: the path a real issuer takes.

use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address};

use super::{scenario, START_LEDGER, VALIDITY_WINDOW};
use crate::errors::Error;
use crate::policy::VERDICT_SAFE;

#[test]
fn happy_path_issues_an_attestation() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 1);
    let proof = s.proof(&signals);

    let att = s.h.client.verify_metadata(&issuer, &proof, &signals);

    assert_eq!(att.metadata_commitment, signals.metadata_commitment);
    assert_eq!(att.issuer, issuer);
    assert_eq!(att.risk_score, 42);
    assert_eq!(att.policy_version, 1);
    assert_eq!(att.verified_at, START_LEDGER);
    assert!(!att.consumed);
    assert_eq!(att.consumer, None);
}

#[test]
fn attestation_is_readable_afterwards() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 2);
    s.h.client
        .verify_metadata(&issuer, &s.proof(&signals), &signals);

    let status = s.h.client.attestation_status(&signals.metadata_commitment);
    assert!(status.exists);
    assert!(status.valid);
    assert!(!status.consumed);
    assert!(!status.expired);
    assert_eq!(status.risk_score, 42);
}

#[test]
fn nullifier_is_spent_and_blocks_replay() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 3);
    let proof = s.proof(&signals);

    assert!(!s.h.client.is_nullifier_spent(&signals.nullifier));
    s.h.client.verify_metadata(&issuer, &proof, &signals);
    assert!(s.h.client.is_nullifier_spent(&signals.nullifier));

    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::NullifierAlreadyUsed);
}

#[test]
fn a_second_valid_proof_of_the_same_statement_is_still_blocked() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 4);
    s.h.client
        .verify_metadata(&issuer, &s.proof(&signals), &signals);

    // Groth16 is randomized: a prover can always mint a fresh, equally valid
    // proof of the same claim. Replay protection therefore has to key on the
    // nullifier, not on the proof bytes — this test is what proves it does.
    let inputs = crate::policy::signals_to_public_inputs(&s.h.env, &signals);
    let different_proof = super::fixtures::synth_proof(&s.h.env, &s.trapdoor, &inputs, 4242);
    assert_ne!(different_proof.c, s.proof(&signals).c);

    let err =
        s.h.client
            .try_verify_metadata(&issuer, &different_proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::NullifierAlreadyUsed);
}

#[test]
fn another_issuer_cannot_redeem_someone_elses_proof() {
    let s = scenario();
    let alice = s.h.issuer.clone();
    let mallory = Address::generate(&s.h.env);

    let signals = s.signals(&alice, 5);
    let proof = s.proof(&signals);

    // The proof is valid and unspent. The only thing stopping Mallory is that
    // the circuit committed to Alice's issuer hash.
    let err =
        s.h.client
            .try_verify_metadata(&mallory, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::IssuerBindingMismatch);

    assert!(s
        .h
        .client
        .try_verify_metadata(&alice, &proof, &signals)
        .is_ok());
}

#[test]
fn rejected_verdict_is_refused_even_with_a_valid_proof() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let mut signals = s.signals(&issuer, 6);
    signals.verdict = 0;
    let proof = s.proof(&signals);

    // The proof correctly attests "the classifier said unsafe". Verifying it
    // is not the same as accepting it.
    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::VerdictRejected);
}

#[test]
fn score_above_the_threshold_is_refused() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let mut signals = s.signals(&issuer, 7);
    signals.risk_score = 251; // policy allows 250
    let proof = s.proof(&signals);

    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::RiskScoreTooHigh);
}

#[test]
fn score_exactly_at_the_threshold_is_accepted() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let mut signals = s.signals(&issuer, 8);
    signals.risk_score = 250;
    s.h.client
        .verify_metadata(&issuer, &s.proof(&signals), &signals);
    assert_eq!(
        s.h.client
            .get_attestation(&signals.metadata_commitment)
            .risk_score,
        250
    );
}

#[test]
fn stale_policy_root_is_refused() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let mut signals = s.signals(&issuer, 9);
    signals.policy_root = super::digest32(&s.h.env, 0xEE);
    let proof = s.proof(&signals);

    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::PolicyRootMismatch);
}

#[test]
fn wrong_model_commitment_is_refused() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let mut signals = s.signals(&issuer, 10);
    signals.model_commitment = super::digest32(&s.h.env, 0xDD);
    let proof = s.proof(&signals);

    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::ModelCommitmentMismatch);
}

#[test]
fn expired_proof_is_refused() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 11);
    let proof = s.proof(&signals);

    s.h.env
        .ledger()
        .with_mut(|l| l.sequence_number = signals.expiry_ledger + 1);

    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::ProofExpired);
}

#[test]
fn overlong_validity_window_is_refused() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let mut signals = s.signals(&issuer, 12);
    signals.expiry_ledger = START_LEDGER + VALIDITY_WINDOW + 1;
    let proof = s.proof(&signals);

    // An issuer must not be able to mint a proof that stays redeemable for a
    // year; the policy caps how far ahead `expiry_ledger` may point.
    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::ExpiryTooDistant);
}

#[test]
fn tampering_with_signals_invalidates_the_proof() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 13);
    let proof = s.proof(&signals);

    // Change a field the policy checks do *not* pin, so the failure has to come
    // from the pairing itself.
    let mut tampered = signals.clone();
    tampered.metadata_commitment = super::digest32(&s.h.env, 0x7F);

    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &tampered)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::ProofVerificationFailed);
}

#[test]
fn lowering_the_score_in_the_signals_invalidates_the_proof() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let mut signals = s.signals(&issuer, 14);
    signals.risk_score = 900;
    let proof = s.proof(&signals);

    // A submitter who is over the threshold cannot simply edit the number down:
    // `risk_score` is a public input, so the proof stops verifying.
    let mut cheated = signals.clone();
    cheated.risk_score = 10;

    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &cheated)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::ProofVerificationFailed);
}

#[test]
fn statistics_count_only_what_survives() {
    let s = scenario();
    let issuer = s.h.issuer.clone();

    let good = s.signals(&issuer, 15);
    s.h.client.verify_metadata(&issuer, &s.proof(&good), &good);

    // A failed verification writes nothing at all — not even a rejection
    // counter — because the failing invocation is rolled back wholesale.
    let mut bad = s.signals(&issuer, 16);
    let proof = s.proof(&bad);
    bad.metadata_commitment = super::digest32(&s.h.env, 0x6F);
    assert!(s
        .h
        .client
        .try_verify_metadata(&issuer, &proof, &bad)
        .is_err());

    let stats = s.h.client.get_stats();
    assert_eq!(stats.proofs_verified, 1);
    assert_eq!(stats.attestations_issued, 1);
    assert_eq!(stats.nullifiers_spent, 1);
    assert!(!s.h.client.is_nullifier_spent(&bad.nullifier));
}

#[test]
fn circuit_verification_counter_increments() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    for n in 20..23u8 {
        let sig = s.signals(&issuer, n);
        s.h.client.verify_metadata(&issuer, &s.proof(&sig), &sig);
    }
    assert_eq!(
        s.h.client
            .get_circuit(&super::METADATA_CIRCUIT)
            .verifications,
        3
    );
}

#[test]
fn issuer_counter_increments() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    assert_eq!(s.h.client.issuer_attestation_count(&issuer), 0);
    for n in 30..32u8 {
        let sig = s.signals(&issuer, n);
        s.h.client.verify_metadata(&issuer, &s.proof(&sig), &sig);
    }
    assert_eq!(s.h.client.issuer_attestation_count(&issuer), 2);
}

#[test]
fn pause_blocks_verification() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 40);
    let proof = s.proof(&signals);

    s.h.client.pause(&s.h.admin);
    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::Paused);

    s.h.client.unpause(&s.h.admin);
    assert!(s
        .h
        .client
        .try_verify_metadata(&issuer, &proof, &signals)
        .is_ok());
}

#[test]
fn signal_encoding_matches_the_declared_order() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 41);
    let encoded = s.h.client.encode_signals(&signals);

    assert_eq!(encoded.len(), super::METADATA_ARITY);
    assert_eq!(
        encoded.get_unchecked(0),
        crate::field::u32_to_scalar_bytes(&s.h.env, VERDICT_SAFE)
    );
    assert_eq!(
        encoded.get_unchecked(1),
        crate::field::u32_to_scalar_bytes(&s.h.env, 42)
    );
    assert_eq!(encoded.get_unchecked(2), signals.policy_root);
    assert_eq!(encoded.get_unchecked(3), signals.model_commitment);
    assert_eq!(encoded.get_unchecked(4), signals.metadata_commitment);
    assert_eq!(encoded.get_unchecked(5), signals.issuer_hash);
    assert_eq!(encoded.get_unchecked(6), signals.nullifier);
    assert_eq!(
        encoded.get_unchecked(7),
        crate::field::u32_to_scalar_bytes(&s.h.env, signals.expiry_ledger)
    );
}

#[test]
fn issuer_hash_is_deterministic_and_per_address() {
    let s = scenario();
    let a = Address::generate(&s.h.env);
    let b = Address::generate(&s.h.env);
    assert_eq!(
        s.h.client.compute_issuer_hash(&a),
        s.h.client.compute_issuer_hash(&a)
    );
    assert_ne!(
        s.h.client.compute_issuer_hash(&a),
        s.h.client.compute_issuer_hash(&b)
    );
    // The hash is fed to the circuit as a field element, so it must be
    // canonical or the verifier would reject every honest proof.
    assert!(s
        .h
        .client
        .is_canonical_scalar(&s.h.client.compute_issuer_hash(&a)));
}

#[test]
fn verify_metadata_without_a_policy_fails_cleanly() {
    let h = super::setup();
    let (vk, td) = super::fixtures::synth_vk(&h.env, super::METADATA_ARITY, 0);
    super::register(&h, &super::METADATA_CIRCUIT, &vk, super::METADATA_ARITY);

    let signals = crate::types::MetadataSignals {
        verdict: 1,
        risk_score: 1,
        policy_root: super::digest32(&h.env, 1),
        model_commitment: super::digest32(&h.env, 2),
        metadata_commitment: super::digest32(&h.env, 3),
        issuer_hash: h.client.compute_issuer_hash(&h.issuer),
        nullifier: super::digest32(&h.env, 4),
        expiry_ledger: START_LEDGER + 100,
    };
    let inputs = crate::policy::signals_to_public_inputs(&h.env, &signals);
    let proof = super::fixtures::synth_proof(&h.env, &td, &inputs, 1);

    let err = h
        .client
        .try_verify_metadata(&h.issuer, &proof, &signals)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::PolicyNotFound);
}
