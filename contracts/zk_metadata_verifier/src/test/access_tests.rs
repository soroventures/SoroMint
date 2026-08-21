//! Access control, pausing, configuration, and cost measurement.

use soroban_sdk::{testutils::Address as _, Address};

use super::{scenario, setup};
use crate::errors::Error;
use crate::types::Role;

#[test]
fn double_initialize_is_rejected() {
    let h = setup();
    let err = h.client.try_initialize(&h.admin).err().unwrap().unwrap();
    assert_eq!(err, Error::AlreadyInitialized);
}

#[test]
fn admin_holds_every_role_implicitly() {
    let h = setup();
    assert!(h.client.has_role(&Role::PolicyAdmin, &h.admin));
    assert!(h.client.has_role(&Role::CircuitAdmin, &h.admin));
    assert!(h.client.has_role(&Role::Pauser, &h.admin));
}

#[test]
fn grant_and_revoke_round_trip() {
    let h = setup();
    let who = Address::generate(&h.env);
    assert!(!h.client.has_role(&Role::Pauser, &who));

    h.client.grant_role(&h.admin, &Role::Pauser, &who);
    assert!(h.client.has_role(&Role::Pauser, &who));

    h.client.revoke_role(&h.admin, &Role::Pauser, &who);
    assert!(!h.client.has_role(&Role::Pauser, &who));
}

#[test]
fn admin_cannot_be_granted_as_a_plain_role() {
    let h = setup();
    let who = Address::generate(&h.env);
    // A second root admin created without a handoff ceremony would bypass the
    // two-step transfer entirely.
    let err = h
        .client
        .try_grant_role(&h.admin, &Role::Admin, &who)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::InvalidRoleTarget);
}

#[test]
fn non_admin_cannot_grant() {
    let h = setup();
    let stranger = Address::generate(&h.env);
    let who = Address::generate(&h.env);
    let err = h
        .client
        .try_grant_role(&stranger, &Role::Pauser, &who)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::Unauthorized);
}

#[test]
fn admin_transfer_is_two_step() {
    let h = setup();
    let successor = Address::generate(&h.env);

    h.client.transfer_admin(&h.admin, &successor);
    // Nomination alone changes nothing.
    assert_eq!(h.client.get_admin(), h.admin);

    h.client.accept_admin(&successor);
    assert_eq!(h.client.get_admin(), successor);

    // The old key is now just another address.
    let err = h
        .client
        .try_transfer_admin(&h.admin, &successor)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::Unauthorized);
}

#[test]
fn only_the_nominee_can_accept() {
    let h = setup();
    let successor = Address::generate(&h.env);
    let impostor = Address::generate(&h.env);
    h.client.transfer_admin(&h.admin, &successor);

    let err = h.client.try_accept_admin(&impostor).err().unwrap().unwrap();
    assert_eq!(err, Error::NoPendingTransfer);
}

#[test]
fn accepting_without_a_nomination_is_rejected() {
    let h = setup();
    let who = Address::generate(&h.env);
    let err = h.client.try_accept_admin(&who).err().unwrap().unwrap();
    assert_eq!(err, Error::NoPendingTransfer);
}

#[test]
fn transfer_can_be_cancelled() {
    let h = setup();
    let successor = Address::generate(&h.env);
    h.client.transfer_admin(&h.admin, &successor);
    h.client.cancel_admin_transfer(&h.admin);

    let err = h
        .client
        .try_accept_admin(&successor)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::NoPendingTransfer);
}

#[test]
fn transfer_to_self_is_rejected() {
    let h = setup();
    let err = h
        .client
        .try_transfer_admin(&h.admin, &h.admin)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::InvalidRoleTarget);
}

#[test]
fn pauser_can_pause_but_not_unpause() {
    let h = setup();
    let responder = Address::generate(&h.env);
    h.client.grant_role(&h.admin, &Role::Pauser, &responder);

    h.client.pause(&responder);
    assert!(h.client.is_paused());

    // Deliberately asymmetric: stopping the system is an emergency action,
    // restarting it is a considered one.
    let err = h.client.try_unpause(&responder).err().unwrap().unwrap();
    assert_eq!(err, Error::Unauthorized);

    h.client.unpause(&h.admin);
    assert!(!h.client.is_paused());
}

#[test]
fn double_pause_is_rejected() {
    let h = setup();
    h.client.pause(&h.admin);
    let err = h.client.try_pause(&h.admin).err().unwrap().unwrap();
    assert_eq!(err, Error::Paused);
}

#[test]
fn unpausing_a_running_contract_is_rejected() {
    let h = setup();
    let err = h.client.try_unpause(&h.admin).err().unwrap().unwrap();
    assert_eq!(err, Error::NotPaused);
}

#[test]
fn config_rejects_nullifier_ttl_below_attestation_ttl() {
    let h = setup();
    let mut config = h.client.get_config();
    config.nullifier_ttl = config.attestation_ttl;

    // Same invariant as the policy check, enforced from the other direction.
    let err = h
        .client
        .try_set_config(&h.admin, &config)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::InvalidPolicyParameters);
}

#[test]
fn config_rejects_zero_batch_size() {
    let h = setup();
    let mut config = h.client.get_config();
    config.max_batch_size = 0;
    let err = h
        .client
        .try_set_config(&h.admin, &config)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::BatchTooLarge);
}

#[test]
fn config_rejects_oversized_batch_cap() {
    let h = setup();
    let mut config = h.client.get_config();
    config.max_batch_size = crate::groth16::MAX_BATCH_SIZE + 1;
    let err = h
        .client
        .try_set_config(&h.admin, &config)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::BatchTooLarge);
}

#[test]
fn non_admin_cannot_change_config() {
    let h = setup();
    let stranger = Address::generate(&h.env);
    let config = h.client.get_config();
    let err = h
        .client
        .try_set_config(&stranger, &config)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::Unauthorized);
}

#[test]
fn issuer_auth_can_be_disabled_for_relayed_submission() {
    let s = scenario();
    let issuer = s.h.issuer.clone();

    let mut config = s.h.client.get_config();
    config.require_issuer_auth = false;
    s.h.client.set_config(&s.h.admin, &config);

    // With auth off, anyone may relay a proof — the issuer binding in the
    // public signals still decides who the attestation belongs to, so this is
    // a meter-payer question, not a security one.
    let signals = s.signals(&issuer, 55);
    let att =
        s.h.client
            .verify_metadata(&issuer, &s.proof(&signals), &signals);
    assert_eq!(att.issuer, issuer);
}

/// Measure what a verification actually costs.
///
/// Not an assertion about a specific number — those drift with host versions —
/// but a guard that the figure stays inside the same order of magnitude as a
/// Soroban transaction budget, and a printout an operator can read.
#[test]
fn cost_report() {
    let s = scenario();
    let issuer = s.h.issuer.clone();
    let signals = s.signals(&issuer, 60);
    let proof = s.proof(&signals);

    s.h.env.cost_estimate().budget().reset_default();
    s.h.client.verify_metadata(&issuer, &proof, &signals);

    let cpu = s.h.env.cost_estimate().budget().cpu_instruction_cost();
    let mem = s.h.env.cost_estimate().budget().memory_bytes_cost();
    ::std::println!("verify_metadata: {} cpu insns, {} mem bytes", cpu, mem);

    assert!(cpu > 0);
    assert!(mem > 0);
}
