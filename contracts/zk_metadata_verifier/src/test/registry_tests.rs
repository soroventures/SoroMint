//! Circuit registry: registration, timelocked rotation, freezing.

use soroban_sdk::{symbol_short, testutils::Address as _, testutils::Ledger as _, Address, String};

use super::fixtures::*;
use super::{register, setup};
use crate::errors::Error;
use crate::types::Role;

#[test]
fn register_then_read_back() {
    let h = setup();
    let (vk, _) = synth_vk(&h.env, 8, 0);
    let id = symbol_short!("meta_v1");
    register(&h, &id, &vk, 8);

    let record = h.client.get_circuit(&id);
    assert_eq!(record.circuit_id, id);
    assert_eq!(record.num_public_inputs, 8);
    assert!(record.enabled);
    assert!(!record.frozen);
    assert_eq!(record.revision, 0);
    assert_eq!(record.verifications, 0);
    assert_eq!(
        record.vk_digest,
        h.client.compute_vk_digest(&vk, &8),
        "the digest an operator computes off chain must match the stored one"
    );
}

#[test]
fn registering_twice_is_rejected() {
    let h = setup();
    let (vk, _) = synth_vk(&h.env, 4, 0);
    let id = symbol_short!("dup");
    register(&h, &id, &vk, 4);

    let err = h
        .client
        .try_register_circuit(&h.admin, &id, &vk, &4, &String::from_str(&h.env, "x"))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::CircuitAlreadyExists);
}

#[test]
fn arity_mismatch_is_rejected_at_registration() {
    let h = setup();
    let (vk, _) = synth_vk(&h.env, 4, 0);
    let err = h
        .client
        .try_register_circuit(
            &h.admin,
            &symbol_short!("bad"),
            &vk,
            &5,
            &String::from_str(&h.env, "x"),
        )
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::MalformedVerifyingKey);
}

#[test]
fn non_circuit_admin_cannot_register() {
    let h = setup();
    let stranger = Address::generate(&h.env);
    let (vk, _) = synth_vk(&h.env, 2, 0);

    let err = h
        .client
        .try_register_circuit(
            &stranger,
            &symbol_short!("nope"),
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
fn delegated_circuit_admin_can_register() {
    let h = setup();
    let delegate = Address::generate(&h.env);
    h.client
        .grant_role(&h.admin, &Role::CircuitAdmin, &delegate);

    let (vk, _) = synth_vk(&h.env, 2, 0);
    h.client.register_circuit(
        &delegate,
        &symbol_short!("deleg"),
        &vk,
        &2,
        &String::from_str(&h.env, "x"),
    );
    assert_eq!(h.client.get_circuit(&symbol_short!("deleg")).revision, 0);
}

#[test]
fn rotation_requires_the_timelock_to_elapse() {
    let h = setup();
    let id = symbol_short!("rot");
    let (vk0, _) = synth_vk(&h.env, 3, 0);
    let (vk1, _) = synth_vk(&h.env, 3, 1);
    register(&h, &id, &vk0, 3);

    let eta = h.client.propose_rotation(&h.admin, &id, &vk1, &3);
    assert!(eta > h.env.ledger().sequence());

    let err = h
        .client
        .try_finalize_rotation(&h.admin, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::RotationTimelockActive);

    h.env.ledger().with_mut(|l| l.sequence_number = eta);
    let new_digest = h.client.finalize_rotation(&h.admin, &id);

    let record = h.client.get_circuit(&id);
    assert_eq!(record.revision, 1);
    assert_eq!(record.vk_digest, new_digest);
    assert_eq!(new_digest, h.client.compute_vk_digest(&vk1, &3));
}

#[test]
fn pending_rotation_publishes_the_incoming_digest() {
    let h = setup();
    let id = symbol_short!("rot2");
    let (vk0, _) = synth_vk(&h.env, 2, 0);
    let (vk1, _) = synth_vk(&h.env, 2, 7);
    register(&h, &id, &vk0, 2);
    h.client.propose_rotation(&h.admin, &id, &vk1, &2);

    // The whole point of the timelock: the incoming key is auditable *before*
    // it becomes live.
    let pending = h.client.get_rotation(&id);
    assert_eq!(pending.vk_digest, h.client.compute_vk_digest(&vk1, &2));
    assert_eq!(pending.proposer, h.admin);
}

#[test]
fn rotation_to_an_identical_key_is_rejected() {
    let h = setup();
    let id = symbol_short!("same");
    let (vk, _) = synth_vk(&h.env, 2, 0);
    register(&h, &id, &vk, 2);

    let err = h
        .client
        .try_propose_rotation(&h.admin, &id, &vk, &2)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::VerifyingKeyUnchanged);
}

#[test]
fn cancelled_rotation_cannot_be_finalized() {
    let h = setup();
    let id = symbol_short!("cancel");
    let (vk0, _) = synth_vk(&h.env, 2, 0);
    let (vk1, _) = synth_vk(&h.env, 2, 4);
    register(&h, &id, &vk0, 2);

    let eta = h.client.propose_rotation(&h.admin, &id, &vk1, &2);
    h.client.cancel_rotation(&h.admin, &id);
    h.env.ledger().with_mut(|l| l.sequence_number = eta);

    let err = h
        .client
        .try_finalize_rotation(&h.admin, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::NoPendingRotation);
}

#[test]
fn cancelling_nothing_is_an_error() {
    let h = setup();
    let id = symbol_short!("nothing");
    let (vk, _) = synth_vk(&h.env, 1, 0);
    register(&h, &id, &vk, 1);

    let err = h
        .client
        .try_cancel_rotation(&h.admin, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::NoPendingRotation);
}

#[test]
fn frozen_circuit_cannot_be_rotated() {
    let h = setup();
    let id = symbol_short!("frozen");
    let (vk0, _) = synth_vk(&h.env, 2, 0);
    let (vk1, _) = synth_vk(&h.env, 2, 9);
    register(&h, &id, &vk0, 2);
    h.client.freeze_circuit(&h.admin, &id);

    assert!(h.client.get_circuit(&id).frozen);
    let err = h
        .client
        .try_propose_rotation(&h.admin, &id, &vk1, &2)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::CircuitFrozen);
}

#[test]
fn freezing_discards_a_pending_rotation() {
    let h = setup();
    let id = symbol_short!("frz2");
    let (vk0, _) = synth_vk(&h.env, 2, 0);
    let (vk1, _) = synth_vk(&h.env, 2, 12);
    register(&h, &id, &vk0, 2);
    let eta = h.client.propose_rotation(&h.admin, &id, &vk1, &2);

    h.client.freeze_circuit(&h.admin, &id);
    h.env.ledger().with_mut(|l| l.sequence_number = eta);

    // Leaving the rotation queued would show operators a finalizable proposal
    // that can never succeed.
    let err = h
        .client
        .try_finalize_rotation(&h.admin, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::NoPendingRotation);
}

#[test]
fn only_admin_can_freeze() {
    let h = setup();
    let id = symbol_short!("frz3");
    let (vk, _) = synth_vk(&h.env, 1, 0);
    register(&h, &id, &vk, 1);

    let delegate = Address::generate(&h.env);
    h.client
        .grant_role(&h.admin, &Role::CircuitAdmin, &delegate);

    // Freezing is irreversible, so it stays with the root key even though the
    // delegate can do everything else to this circuit.
    let err = h
        .client
        .try_freeze_circuit(&delegate, &id)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::Unauthorized);
}

#[test]
fn disabled_circuit_refuses_verification() {
    let h = setup();
    let id = symbol_short!("off");
    let (vk, td) = synth_vk(&h.env, 2, 0);
    register(&h, &id, &vk, 2);

    let inputs = small_inputs(&h.env, 2);
    let proof = synth_proof(&h.env, &td, &inputs, 1);
    assert!(h.client.verify_proof(&id, &proof, &inputs));

    h.client.set_circuit_enabled(&h.admin, &id, &false);
    let err = h
        .client
        .try_verify_proof(&id, &proof, &inputs)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::CircuitDisabled);

    h.client.set_circuit_enabled(&h.admin, &id, &true);
    assert!(h.client.verify_proof(&id, &proof, &inputs));
}

#[test]
fn unknown_circuit_is_reported() {
    let h = setup();
    let err = h
        .client
        .try_get_circuit(&symbol_short!("ghost"))
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::CircuitNotFound);
}

#[test]
fn circuits_are_listed_in_registration_order() {
    let h = setup();
    let (vk, _) = synth_vk(&h.env, 1, 0);
    for id in [symbol_short!("a"), symbol_short!("b"), symbol_short!("c")] {
        register(&h, &id, &vk, 1);
    }
    let listed = h.client.list_circuits();
    assert_eq!(listed.len(), 3);
    assert_eq!(listed.get_unchecked(0), symbol_short!("a"));
    assert_eq!(listed.get_unchecked(2), symbol_short!("c"));
}

#[test]
fn rotation_actually_changes_which_proofs_verify() {
    let h = setup();
    let id = symbol_short!("live");
    let (vk0, td0) = synth_vk(&h.env, 2, 0);
    let (vk1, td1) = synth_vk(&h.env, 2, 21);
    register(&h, &id, &vk0, 2);

    let inputs = small_inputs(&h.env, 2);
    let proof_old = synth_proof(&h.env, &td0, &inputs, 5);
    let proof_new = synth_proof(&h.env, &td1, &inputs, 5);

    assert!(h.client.check_proof(&id, &proof_old, &inputs));
    assert!(!h.client.check_proof(&id, &proof_new, &inputs));

    let eta = h.client.propose_rotation(&h.admin, &id, &vk1, &2);
    h.env.ledger().with_mut(|l| l.sequence_number = eta);
    h.client.finalize_rotation(&h.admin, &id);

    assert!(!h.client.check_proof(&id, &proof_old, &inputs));
    assert!(h.client.check_proof(&id, &proof_new, &inputs));
}
