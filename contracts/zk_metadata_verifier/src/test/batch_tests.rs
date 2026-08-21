//! Batch verification through the contract surface.

use soroban_sdk::{symbol_short, Env, Symbol, Vec};

use super::fixtures::*;
use super::{register, setup, Harness};
use crate::errors::Error;
use crate::types::{Groth16Proof, ScalarBytes};

const ID: Symbol = symbol_short!("batch_v1");
const ARITY: u32 = 4;

fn batch_harness() -> (Harness<'static>, Trapdoor) {
    let h = setup();
    let (vk, td) = synth_vk(&h.env, ARITY, 0);
    register(&h, &ID, &vk, ARITY);
    (h, td)
}

fn make_batch(env: &Env, td: &Trapdoor, n: u32) -> (Vec<Groth16Proof>, Vec<Vec<ScalarBytes>>) {
    let mut proofs = Vec::new(env);
    let mut inputs = Vec::new(env);
    for i in 0..n {
        let mut xs: Vec<ScalarBytes> = Vec::new(env);
        for j in 0..ARITY {
            xs.push_back(crate::field::u32_to_scalar_bytes(env, 7000 + i * 13 + j));
        }
        proofs.push_back(synth_proof(env, td, &xs, 1 + i));
        inputs.push_back(xs);
    }
    (proofs, inputs)
}

#[test]
fn full_batch_verifies() {
    let (h, td) = batch_harness();
    let (proofs, inputs) = make_batch(&h.env, &td, 8);
    assert!(h.client.verify_batch(&ID, &proofs, &inputs));
    assert_eq!(h.client.get_stats().batches_verified, 1);
    assert_eq!(h.client.get_stats().proofs_verified, 8);
}

#[test]
fn batch_at_the_configured_cap_verifies() {
    let (h, td) = batch_harness();
    let cap = h.client.get_config().max_batch_size;
    let (proofs, inputs) = make_batch(&h.env, &td, cap);
    assert!(h.client.verify_batch(&ID, &proofs, &inputs));
}

#[test]
fn batch_above_the_cap_is_rejected() {
    let (h, td) = batch_harness();
    let mut config = h.client.get_config();
    config.max_batch_size = 4;
    h.client.set_config(&h.admin, &config);

    let (proofs, inputs) = make_batch(&h.env, &td, 5);
    let err = h
        .client
        .try_verify_batch(&ID, &proofs, &inputs)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::BatchTooLarge);
}

#[test]
fn one_forged_member_sinks_the_batch() {
    let (h, td) = batch_harness();
    let (mut proofs, inputs) = make_batch(&h.env, &td, 6);
    let mut forged = proofs.get_unchecked(4);
    forged.c = other_g1(&h.env, 5);
    proofs.set(4, forged);

    let err = h
        .client
        .try_verify_batch(&ID, &proofs, &inputs)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::BatchVerificationFailed);
}

#[test]
fn diagnose_pinpoints_the_bad_member() {
    let (h, td) = batch_harness();
    let (mut proofs, inputs) = make_batch(&h.env, &td, 6);
    let mut forged = proofs.get_unchecked(1);
    forged.c = other_g1(&h.env, 9);
    proofs.set(1, forged);

    let outcome = h.client.diagnose_batch(&ID, &proofs, &inputs);
    assert!(!outcome.aggregate_ok);
    assert_eq!(outcome.size, 6);
    assert_eq!(outcome.failed.len(), 1);
    assert_eq!(outcome.failed.get_unchecked(0), 1);
}

#[test]
fn diagnose_on_a_clean_batch_reports_no_failures() {
    let (h, td) = batch_harness();
    let (proofs, inputs) = make_batch(&h.env, &td, 4);
    let outcome = h.client.diagnose_batch(&ID, &proofs, &inputs);
    assert!(outcome.aggregate_ok);
    assert_eq!(outcome.failed.len(), 0);
}

#[test]
fn diagnose_finds_multiple_failures() {
    let (h, td) = batch_harness();
    let (mut proofs, inputs) = make_batch(&h.env, &td, 5);
    for i in [0u32, 3u32] {
        let mut forged = proofs.get_unchecked(i);
        forged.a = other_g1(&h.env, 100 + i);
        proofs.set(i, forged);
    }

    let outcome = h.client.diagnose_batch(&ID, &proofs, &inputs);
    assert_eq!(outcome.failed.len(), 2);
    assert_eq!(outcome.failed.get_unchecked(0), 0);
    assert_eq!(outcome.failed.get_unchecked(1), 3);
}

#[test]
fn batch_is_paused_with_the_contract() {
    let (h, td) = batch_harness();
    let (proofs, inputs) = make_batch(&h.env, &td, 3);
    h.client.pause(&h.admin);
    let err = h
        .client
        .try_verify_batch(&ID, &proofs, &inputs)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::Paused);
}

#[test]
fn batch_against_an_unknown_circuit_is_rejected() {
    let (h, td) = batch_harness();
    let (proofs, inputs) = make_batch(&h.env, &td, 2);
    let err = h
        .client
        .try_verify_batch(&symbol_short!("nope"), &proofs, &inputs)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::CircuitNotFound);
}

#[test]
fn batch_with_wrong_arity_is_rejected() {
    let (h, td) = batch_harness();
    let (proofs, _) = make_batch(&h.env, &td, 2);
    let mut inputs: Vec<Vec<ScalarBytes>> = Vec::new(&h.env);
    for _ in 0..2 {
        inputs.push_back(small_inputs(&h.env, ARITY - 1));
    }
    let err = h
        .client
        .try_verify_batch(&ID, &proofs, &inputs)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::PublicInputCountMismatch);
}
