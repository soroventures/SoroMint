//! Attestation lifecycle: issuance, consumption, expiry, revocation.

use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address};

use super::{scenario, START_LEDGER};
use crate::errors::Error;

#[test]
fn registered_consumer_can_redeem_once() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let gateway = Address::generate(&s.h.env);
    s.h.client.register_consumer(&s.h.admin, &gateway);

    let signals = s.signals(&issuer, 1);
    s.h.client
        .verify_metadata(&issuer, &s.proof(&signals), &signals);

    let consumed =
        s.h.client
            .consume_attestation(&gateway, &signals.metadata_commitment, &issuer);
    assert!(consumed.consumed);
    assert_eq!(consumed.consumer, Some(gateway.clone()));
    assert_eq!(consumed.consumed_at, Some(START_LEDGER));

    // Single-use: one safety proof authorizes one token, not a stream of them.
    let err =
        s.h.client
            .try_consume_attestation(&gateway, &signals.metadata_commitment, &issuer)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::AttestationAlreadyConsumed);
}

#[test]
fn unregistered_caller_cannot_consume() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let stranger = Address::generate(&s.h.env);

    let signals = s.signals(&issuer, 2);
    s.h.client
        .verify_metadata(&issuer, &s.proof(&signals), &signals);

    let err =
        s.h.client
            .try_consume_attestation(&stranger, &signals.metadata_commitment, &issuer)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::NotAuthorizedConsumer);
}

#[test]
fn consuming_under_the_wrong_issuer_is_refused() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let other = Address::generate(&s.h.env);
    let gateway = Address::generate(&s.h.env);
    s.h.client.register_consumer(&s.h.admin, &gateway);

    let signals = s.signals(&issuer, 3);
    s.h.client
        .verify_metadata(&issuer, &s.proof(&signals), &signals);

    let err =
        s.h.client
            .try_consume_attestation(&gateway, &signals.metadata_commitment, &other)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::IssuerBindingMismatch);
}

#[test]
fn expired_attestation_cannot_be_consumed() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let gateway = Address::generate(&s.h.env);
    s.h.client.register_consumer(&s.h.admin, &gateway);

    let signals = s.signals(&issuer, 4);
    let att =
        s.h.client
            .verify_metadata(&issuer, &s.proof(&signals), &signals);

    s.h.env
        .ledger()
        .with_mut(|l| l.sequence_number = att.expires_at + 1);

    let err =
        s.h.client
            .try_consume_attestation(&gateway, &signals.metadata_commitment, &issuer)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::AttestationExpired);
}

#[test]
fn attestation_never_outlives_the_proofs_own_expiry() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 5);
    let att =
        s.h.client
            .verify_metadata(&issuer, &s.proof(&signals), &signals);

    // The retention window is ~7 days and the proof claims ~half a day. An
    // issuer who proved "safe as of ledger N" has said nothing about later
    // ledgers, so the shorter bound wins.
    let config = s.h.client.get_config();
    assert!(signals.expiry_ledger < START_LEDGER + config.attestation_ttl);
    assert_eq!(att.expires_at, signals.expiry_ledger);
}

#[test]
fn retention_caps_a_long_lived_proof() {
    let s = scenario();
    let issuer = s.h.issuer.clone();

    // Shrink retention below the proof's window and re-check which bound binds.
    let mut config = s.h.client.get_config();
    config.attestation_ttl = 100;
    s.h.client.set_config(&s.h.admin, &config);

    let signals = s.signals(&issuer, 6);
    let att =
        s.h.client
            .verify_metadata(&issuer, &s.proof(&signals), &signals);
    assert_eq!(att.expires_at, START_LEDGER + 100);
}

#[test]
fn status_reports_expiry_without_erroring() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 7);
    let att =
        s.h.client
            .verify_metadata(&issuer, &s.proof(&signals), &signals);

    s.h.env
        .ledger()
        .with_mut(|l| l.sequence_number = att.expires_at + 1);

    let status = s.h.client.attestation_status(&signals.metadata_commitment);
    assert!(status.exists);
    assert!(status.expired);
    assert!(!status.valid);
}

#[test]
fn status_of_an_unknown_commitment_is_empty() {
    let s = scenario();
    let status =
        s.h.client
            .attestation_status(&super::digest32(&s.h.env, 0xF1));
    assert!(!status.exists);
    assert!(!status.valid);
    assert_eq!(status.policy_version, 0);
}

#[test]
fn policy_admin_can_revoke() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let gateway = Address::generate(&s.h.env);
    s.h.client.register_consumer(&s.h.admin, &gateway);

    let signals = s.signals(&issuer, 8);
    s.h.client
        .verify_metadata(&issuer, &s.proof(&signals), &signals);

    s.h.client
        .revoke_attestation(&s.h.admin, &signals.metadata_commitment, &7);

    let err =
        s.h.client
            .try_consume_attestation(&gateway, &signals.metadata_commitment, &issuer)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::AttestationNotFound);
}

#[test]
fn revocation_does_not_free_the_nullifier() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 9);
    let proof = s.proof(&signals);
    s.h.client.verify_metadata(&issuer, &proof, &signals);

    s.h.client
        .revoke_attestation(&s.h.admin, &signals.metadata_commitment, &1);

    // Revoking the receipt must not hand back the right to re-submit the proof;
    // otherwise revocation would be trivially undoable by the issuer.
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
fn stranger_cannot_revoke() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let stranger = Address::generate(&s.h.env);
    let signals = s.signals(&issuer, 10);
    s.h.client
        .verify_metadata(&issuer, &s.proof(&signals), &signals);

    let err =
        s.h.client
            .try_revoke_attestation(&stranger, &signals.metadata_commitment, &0)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::Unauthorized);
}

#[test]
fn registering_the_same_consumer_twice_is_rejected() {
    let s = scenario();
    let gateway = Address::generate(&s.h.env);
    s.h.client.register_consumer(&s.h.admin, &gateway);
    let err =
        s.h.client
            .try_register_consumer(&s.h.admin, &gateway)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::ConsumerAlreadyRegistered);
}

#[test]
fn consumption_is_counted() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let gateway = Address::generate(&s.h.env);
    s.h.client.register_consumer(&s.h.admin, &gateway);

    for n in 11..14u8 {
        let sig = s.signals(&issuer, n);
        s.h.client.verify_metadata(&issuer, &s.proof(&sig), &sig);
        s.h.client
            .consume_attestation(&gateway, &sig.metadata_commitment, &issuer);
    }

    let stats = s.h.client.get_stats();
    assert_eq!(stats.attestations_issued, 3);
    assert_eq!(stats.attestations_consumed, 3);
}

#[test]
fn consuming_an_unknown_commitment_is_reported() {
    let s = scenario();
    let gateway = Address::generate(&s.h.env);
    s.h.client.register_consumer(&s.h.admin, &gateway);

    let err =
        s.h.client
            .try_consume_attestation(&gateway, &super::digest32(&s.h.env, 0xC3), &s.h.issuer)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::AttestationNotFound);
}
