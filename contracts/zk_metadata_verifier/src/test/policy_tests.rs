//! Policy publication, activation and parameter validation.

use soroban_sdk::{symbol_short, testutils::Address as _, Address, String};

use super::{
    digest32, register, scenario, setup, METADATA_ARITY, METADATA_CIRCUIT, VALIDITY_WINDOW,
};
use crate::errors::Error;
use crate::types::{PolicyParams, Role};

fn params(env: &soroban_sdk::Env, version: u32) -> PolicyParams {
    PolicyParams {
        version,
        policy_root: digest32(env, 0x11),
        model_commitment: digest32(env, 0x22),
        max_risk_score: 250,
        risk_scale: 1000,
        max_validity_ledgers: VALIDITY_WINDOW,
        circuit_id: METADATA_CIRCUIT,
        rulebook_uri: String::from_str(env, "ipfs://policy"),
    }
}

fn with_circuit() -> super::Harness<'static> {
    let h = setup();
    let (vk, _) = super::fixtures::synth_vk(&h.env, METADATA_ARITY, 0);
    register(&h, &METADATA_CIRCUIT, &vk, METADATA_ARITY);
    h
}

#[test]
fn publish_then_activate() {
    let h = with_circuit();
    let p = params(&h.env, 1);
    h.client.publish_policy(&h.admin, &p);

    // Publishing alone must not change what verification enforces; an operator
    // needs to be able to stage a policy and activate it deliberately.
    assert!(h.client.try_get_active_policy().is_err());

    h.client.activate_policy(&h.admin, &1);
    let active = h.client.get_active_policy();
    assert_eq!(active.version, 1);
    assert_eq!(active.max_risk_score, 250);
    assert_eq!(active.activated_at, super::START_LEDGER);
}

#[test]
fn versions_must_increase() {
    let h = with_circuit();
    h.client.publish_policy(&h.admin, &params(&h.env, 5));
    h.client.activate_policy(&h.admin, &5);

    let err = h
        .client
        .try_publish_policy(&h.admin, &params(&h.env, 4))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::PolicyVersionRegression);
}

#[test]
fn republishing_the_active_version_is_rejected() {
    let h = with_circuit();
    h.client.publish_policy(&h.admin, &params(&h.env, 3));
    h.client.activate_policy(&h.admin, &3);

    let err = h
        .client
        .try_publish_policy(&h.admin, &params(&h.env, 3))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::PolicyVersionRegression);
}

#[test]
fn version_zero_is_rejected() {
    let h = with_circuit();
    let err = h
        .client
        .try_publish_policy(&h.admin, &params(&h.env, 0))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::InvalidPolicyParameters);
}

#[test]
fn threshold_above_the_scale_is_rejected() {
    let h = with_circuit();
    let mut p = params(&h.env, 1);
    p.max_risk_score = 1001;
    let err = h
        .client
        .try_publish_policy(&h.admin, &p)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::InvalidPolicyParameters);
}

#[test]
fn zero_scale_is_rejected() {
    let h = with_circuit();
    let mut p = params(&h.env, 1);
    p.risk_scale = 0;
    p.max_risk_score = 0;
    let err = h
        .client
        .try_publish_policy(&h.admin, &p)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::InvalidPolicyParameters);
}

#[test]
fn validity_window_longer_than_nullifier_retention_is_rejected() {
    let h = with_circuit();
    let mut p = params(&h.env, 1);
    p.max_validity_ledgers = h.client.get_config().nullifier_ttl;

    // If a proof could stay valid longer than its nullifier is retained, the
    // nullifier would expire while the proof was still redeemable — reopening
    // the replay it exists to prevent.
    let err = h
        .client
        .try_publish_policy(&h.admin, &p)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::InvalidPolicyParameters);
}

#[test]
fn zero_validity_window_is_rejected() {
    let h = with_circuit();
    let mut p = params(&h.env, 1);
    p.max_validity_ledgers = 0;
    let err = h
        .client
        .try_publish_policy(&h.admin, &p)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::InvalidPolicyParameters);
}

#[test]
fn activating_an_unpublished_version_is_rejected() {
    let h = with_circuit();
    let err = h
        .client
        .try_activate_policy(&h.admin, &9)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::PolicyNotFound);
}

#[test]
fn activating_a_policy_that_names_a_missing_circuit_is_rejected() {
    let h = setup();
    let mut p = params(&h.env, 1);
    p.circuit_id = symbol_short!("absent");
    h.client.publish_policy(&h.admin, &p);

    let err = h
        .client
        .try_activate_policy(&h.admin, &1)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::CircuitNotFound);
}

#[test]
fn activating_a_policy_that_names_a_disabled_circuit_is_rejected() {
    let h = with_circuit();
    h.client.publish_policy(&h.admin, &params(&h.env, 1));
    h.client
        .set_circuit_enabled(&h.admin, &METADATA_CIRCUIT, &false);

    let err = h
        .client
        .try_activate_policy(&h.admin, &1)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::CircuitDisabled);
}

#[test]
fn non_policy_admin_cannot_publish() {
    let h = with_circuit();
    let stranger = Address::generate(&h.env);
    let err = h
        .client
        .try_publish_policy(&stranger, &params(&h.env, 1))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::Unauthorized);
}

#[test]
fn delegated_policy_admin_can_publish_and_activate() {
    let h = with_circuit();
    let delegate = Address::generate(&h.env);
    h.client.grant_role(&h.admin, &Role::PolicyAdmin, &delegate);

    h.client.publish_policy(&delegate, &params(&h.env, 1));
    h.client.activate_policy(&delegate, &1);
    assert_eq!(h.client.get_active_policy().version, 1);
}

#[test]
fn policy_admin_cannot_touch_the_circuit_registry() {
    let h = with_circuit();
    let delegate = Address::generate(&h.env);
    h.client.grant_role(&h.admin, &Role::PolicyAdmin, &delegate);

    // Separating these two keys is the point: loosening a threshold is
    // recoverable, installing a forged verifying key is not.
    let (vk, _) = super::fixtures::synth_vk(&h.env, 2, 3);
    let err = h
        .client
        .try_register_circuit(
            &delegate,
            &symbol_short!("sneaky"),
            &vk,
            &2,
            &String::from_str(&h.env, "x"),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::Unauthorized);
}

#[test]
fn policies_are_indexed() {
    let h = with_circuit();
    h.client.publish_policy(&h.admin, &params(&h.env, 1));
    h.client.activate_policy(&h.admin, &1);
    h.client.publish_policy(&h.admin, &params(&h.env, 2));

    let list = h.client.list_policies();
    assert_eq!(list.len(), 2);
    assert_eq!(list.get_unchecked(0), 1);
    assert_eq!(list.get_unchecked(1), 2);
}

#[test]
fn superseded_policy_stays_readable() {
    let h = with_circuit();
    h.client.publish_policy(&h.admin, &params(&h.env, 1));
    h.client.activate_policy(&h.admin, &1);

    let mut p2 = params(&h.env, 2);
    p2.max_risk_score = 100;
    h.client.publish_policy(&h.admin, &p2);
    h.client.activate_policy(&h.admin, &2);

    // Attestations record the version they were issued under, so historical
    // policies must remain queryable to interpret them.
    assert_eq!(h.client.get_policy(&1).max_risk_score, 250);
    assert_eq!(h.client.get_active_policy().max_risk_score, 100);
}

#[test]
fn a_new_policy_changes_which_proofs_are_accepted() {
    let s = scenario();
    let issuer = s.h.issuer.clone();

    let signals = s.signals(&issuer, 1);
    let proof = s.proof(&signals);

    // Tighten the threshold below the proof's score, keeping everything else.
    let mut p2 = s.policy.clone();
    p2.version = 2;
    p2.max_risk_score = 10;
    s.h.client.publish_policy(&s.h.admin, &p2);
    s.h.client.activate_policy(&s.h.admin, &2);

    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::RiskScoreTooHigh);
}

#[test]
fn rotating_the_policy_root_invalidates_old_proofs() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 2);
    let proof = s.proof(&signals);

    let mut p2 = s.policy.clone();
    p2.version = 2;
    p2.policy_root = digest32(&s.h.env, 0x99);
    s.h.client.publish_policy(&s.h.admin, &p2);
    s.h.client.activate_policy(&s.h.admin, &2);

    // The rule set moved; a proof that a payload passed the *old* rules is no
    // longer a statement about anything the platform enforces.
    let err =
        s.h.client
            .try_verify_metadata(&issuer, &proof, &signals)
            .err()
            .unwrap()
            .unwrap();
    assert_eq!(err, Error::PolicyRootMismatch);
}
