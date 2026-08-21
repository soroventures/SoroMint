//! Direct tests of the pairing verifier, bypassing the contract surface.

use soroban_sdk::{Env, Vec};

use super::fixtures::*;
use crate::errors::Error;
use crate::groth16::{
    locate_batch_failures, public_input_commitment, validate_verifying_key, verify_batch,
    verify_proof, verifying_key_digest, MAX_PUBLIC_INPUTS,
};
use crate::types::ScalarBytes;

#[test]
fn synthetic_proof_verifies() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 4, 0);
    let inputs = small_inputs(&env, 4);
    let proof = synth_proof(&env, &td, &inputs, 42);

    assert_eq!(verify_proof(&env, &vk, &proof, &inputs), Ok(()));
}

#[test]
fn zero_input_circuit_verifies() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 0, 3);
    let inputs: Vec<ScalarBytes> = Vec::new(&env);
    let proof = synth_proof(&env, &td, &inputs, 9);

    assert_eq!(verify_proof(&env, &vk, &proof, &inputs), Ok(()));
}

#[test]
fn wide_circuit_verifies() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, MAX_PUBLIC_INPUTS, 5);
    let inputs = small_inputs(&env, MAX_PUBLIC_INPUTS);
    let proof = synth_proof(&env, &td, &inputs, 77);

    assert_eq!(verify_proof(&env, &vk, &proof, &inputs), Ok(()));
}

#[test]
fn proof_for_other_inputs_fails() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 4, 0);
    let inputs = small_inputs(&env, 4);
    let proof = synth_proof(&env, &td, &inputs, 42);

    // Same arity, different values: `l` changes, so the exponent no longer
    // cancels. This is the property that makes the fixtures meaningful.
    let mut other: Vec<ScalarBytes> = Vec::new(&env);
    for j in 0..4u32 {
        other.push_back(crate::field::u32_to_scalar_bytes(&env, 2000 + j));
    }

    assert_eq!(
        verify_proof(&env, &vk, &proof, &other),
        Err(Error::ProofVerificationFailed)
    );
}

#[test]
fn proof_against_other_verifying_key_fails() {
    let env = test_env();
    let (_vk_a, td_a) = synth_vk(&env, 3, 0);
    let (vk_b, _td_b) = synth_vk(&env, 3, 1);
    let inputs = small_inputs(&env, 3);
    let proof = synth_proof(&env, &td_a, &inputs, 11);

    assert_eq!(
        verify_proof(&env, &vk_b, &proof, &inputs),
        Err(Error::ProofVerificationFailed)
    );
}

#[test]
fn substituted_a_fails() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 2, 0);
    let inputs = small_inputs(&env, 2);
    let mut proof = synth_proof(&env, &td, &inputs, 5);
    proof.a = other_g1(&env, 1);

    assert_eq!(
        verify_proof(&env, &vk, &proof, &inputs),
        Err(Error::ProofVerificationFailed)
    );
}

#[test]
fn substituted_b_fails() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 2, 0);
    let inputs = small_inputs(&env, 2);
    let mut proof = synth_proof(&env, &td, &inputs, 5);
    proof.b = other_g2(&env, 2);

    assert_eq!(
        verify_proof(&env, &vk, &proof, &inputs),
        Err(Error::ProofVerificationFailed)
    );
}

#[test]
fn substituted_c_fails() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 2, 0);
    let inputs = small_inputs(&env, 2);
    let mut proof = synth_proof(&env, &td, &inputs, 5);
    proof.c = other_g1(&env, 3);

    assert_eq!(
        verify_proof(&env, &vk, &proof, &inputs),
        Err(Error::ProofVerificationFailed)
    );
}

/// An off-curve `A` is refused by the host during deserialization, which it
/// reports by trapping rather than returning. Pinning that as a test documents
/// the boundary: the contract's own guards cover flags, ranges and subgroup
/// membership, and curve membership is the host's job.
#[test]
#[should_panic]
fn off_curve_a_traps_in_host() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 2, 0);
    let inputs = small_inputs(&env, 2);
    let mut proof = synth_proof(&env, &td, &inputs, 5);
    proof.a = corrupt_g1(&env, &proof.a);
    let _ = verify_proof(&env, &vk, &proof, &inputs);
}

#[test]
#[should_panic]
fn off_curve_b_traps_in_host() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 2, 0);
    let inputs = small_inputs(&env, 2);
    let mut proof = synth_proof(&env, &td, &inputs, 5);
    proof.b = corrupt_g2(&env, &proof.b);
    let _ = verify_proof(&env, &vk, &proof, &inputs);
}

#[test]
fn swapping_a_and_c_fails() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 2, 0);
    let inputs = small_inputs(&env, 2);
    let mut proof = synth_proof(&env, &td, &inputs, 5);
    core::mem::swap(&mut proof.a, &mut proof.c);

    assert_eq!(
        verify_proof(&env, &vk, &proof, &inputs),
        Err(Error::ProofVerificationFailed)
    );
}

#[test]
fn wrong_input_count_is_caught_before_pairing() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 4, 0);
    let inputs = small_inputs(&env, 4);
    let proof = synth_proof(&env, &td, &inputs, 1);

    let short = small_inputs(&env, 3);
    assert_eq!(
        verify_proof(&env, &vk, &proof, &short),
        Err(Error::PublicInputCountMismatch)
    );
}

#[test]
fn non_canonical_scalar_is_rejected() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 1, 0);
    let inputs = small_inputs(&env, 1);
    let proof = synth_proof(&env, &td, &inputs, 1);

    let mut bad: Vec<ScalarBytes> = Vec::new(&env);
    bad.push_back(modulus_scalar(&env));

    assert_eq!(
        verify_proof(&env, &vk, &proof, &bad),
        Err(Error::NonCanonicalScalar)
    );
}

#[test]
fn validate_vk_rejects_arity_mismatch() {
    let env = test_env();
    let (vk, _) = synth_vk(&env, 4, 0);
    assert_eq!(
        validate_verifying_key(&env, &vk, 3),
        Err(Error::MalformedVerifyingKey)
    );
    assert_eq!(validate_verifying_key(&env, &vk, 4), Ok(()));
}

#[test]
fn validate_vk_rejects_oversized_arity() {
    let env = test_env();
    let (vk, _) = synth_vk(&env, 2, 0);
    assert_eq!(
        validate_verifying_key(&env, &vk, MAX_PUBLIC_INPUTS + 1),
        Err(Error::UnsupportedArity)
    );
}

#[test]
#[should_panic]
fn validate_vk_rejects_off_curve_point() {
    let env = test_env();
    let (mut vk, _) = synth_vk(&env, 2, 0);
    vk.alpha_g1 = corrupt_g1(&env, &vk.alpha_g1);
    let _ = validate_verifying_key(&env, &vk, 2);
}

#[test]
fn validate_vk_rejects_out_of_range_point() {
    let env = test_env();
    let (mut vk, _) = synth_vk(&env, 2, 0);
    vk.alpha_g1 = soroban_sdk::BytesN::from_array(&env, &[0xffu8; 96]);
    assert_eq!(
        validate_verifying_key(&env, &vk, 2),
        Err(Error::InvalidG1Point)
    );
}

#[test]
fn validate_vk_rejects_infinite_delta() {
    let env = test_env();
    let (mut vk, _) = synth_vk(&env, 2, 0);
    let mut inf = [0u8; 192];
    inf[0] = 0x40;
    vk.delta_g2 = soroban_sdk::BytesN::from_array(&env, &inf);
    assert_eq!(
        validate_verifying_key(&env, &vk, 2),
        Err(Error::PointAtInfinity)
    );
}

#[test]
fn vk_digest_is_stable_and_distinguishing() {
    let env = test_env();
    let (vk_a, _) = synth_vk(&env, 3, 0);
    let (vk_b, _) = synth_vk(&env, 3, 1);

    assert_eq!(
        verifying_key_digest(&env, &vk_a, 3),
        verifying_key_digest(&env, &vk_a, 3)
    );
    assert_ne!(
        verifying_key_digest(&env, &vk_a, 3),
        verifying_key_digest(&env, &vk_b, 3)
    );
    // Arity participates in the digest, so a key cannot be silently reused at a
    // different arity.
    assert_ne!(
        verifying_key_digest(&env, &vk_a, 3),
        verifying_key_digest(&env, &vk_a, 2)
    );
}

#[test]
fn public_input_commitment_matches_manual_msm() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 3, 0);
    let inputs = small_inputs(&env, 3);

    let l = public_input_commitment(&env, &vk, &inputs).unwrap();

    // Recompute from the trapdoor: l = ic₀ + Σ xⱼ·icⱼ, then l·g₁.
    let bls = env.crypto().bls12_381();
    let mut log = td.ic[0].clone();
    for j in 0..3usize {
        let x = crate::field::checked_scalar(&inputs.get_unchecked(j as u32)).unwrap();
        log = bls.fr_add(&log, &bls.fr_mul(&td.ic[j + 1], &x));
    }
    assert_eq!(l.to_bytes(), g1_mul_gen(&env, &log));
}

// ---------------------------------------------------------------------------
// Batch verification
// ---------------------------------------------------------------------------

fn batch_of(
    env: &Env,
    td: &Trapdoor,
    n: u32,
    arity: u32,
) -> (Vec<crate::types::Groth16Proof>, Vec<Vec<ScalarBytes>>) {
    let mut proofs = Vec::new(env);
    let mut inputs = Vec::new(env);
    for i in 0..n {
        let mut xs: Vec<ScalarBytes> = Vec::new(env);
        for j in 0..arity {
            xs.push_back(crate::field::u32_to_scalar_bytes(env, 500 + i * 17 + j));
        }
        proofs.push_back(synth_proof(env, td, &xs, 3 + i));
        inputs.push_back(xs);
    }
    (proofs, inputs)
}

#[test]
fn batch_of_valid_proofs_verifies() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 4, 0);
    let digest = verifying_key_digest(&env, &vk, 4);
    let (proofs, inputs) = batch_of(&env, &td, 8, 4);

    assert_eq!(
        verify_batch(&env, &vk, &digest, &proofs, &inputs, 16),
        Ok(())
    );
}

#[test]
fn single_element_batch_verifies() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 2, 0);
    let digest = verifying_key_digest(&env, &vk, 2);
    let (proofs, inputs) = batch_of(&env, &td, 1, 2);

    assert_eq!(
        verify_batch(&env, &vk, &digest, &proofs, &inputs, 16),
        Ok(())
    );
}

#[test]
fn batch_with_one_bad_proof_fails() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 3, 0);
    let digest = verifying_key_digest(&env, &vk, 3);
    let (mut proofs, inputs) = batch_of(&env, &td, 5, 3);

    // Replace member 2 with a proof for a different statement. It is a
    // perfectly valid proof — just not of the claim its slot advertises.
    let decoy = small_inputs(&env, 3);
    proofs.set(2, synth_proof(&env, &td, &decoy, 99));

    assert_eq!(
        verify_batch(&env, &vk, &digest, &proofs, &inputs, 16),
        Err(Error::BatchVerificationFailed)
    );
}

#[test]
fn batch_failure_is_attributable() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 3, 0);
    let (mut proofs, inputs) = batch_of(&env, &td, 5, 3);
    let decoy = small_inputs(&env, 3);
    proofs.set(3, synth_proof(&env, &td, &decoy, 99));

    let failed = locate_batch_failures(&env, &vk, &proofs, &inputs);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed.get_unchecked(0), 3);
}

#[test]
fn reordering_a_batch_still_verifies() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 2, 0);
    let digest = verifying_key_digest(&env, &vk, 2);
    let (proofs, inputs) = batch_of(&env, &td, 4, 2);

    let mut rp = Vec::new(&env);
    let mut ri = Vec::new(&env);
    for i in (0..4u32).rev() {
        rp.push_back(proofs.get_unchecked(i));
        ri.push_back(inputs.get_unchecked(i));
    }

    // The transcript changes, so every coefficient changes — but each member is
    // still individually valid, so the aggregate holds.
    assert_eq!(verify_batch(&env, &vk, &digest, &rp, &ri, 16), Ok(()));
}

#[test]
fn mismatched_batch_pairing_is_detected() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 2, 0);
    let digest = verifying_key_digest(&env, &vk, 2);
    let (proofs, mut inputs) = batch_of(&env, &td, 3, 2);

    // Pair proof 0 with proof 1's inputs and vice versa. Each proof is valid
    // and each input vector is legitimate; only the pairing is wrong. A naive
    // "sum of proofs, sum of inputs" batcher would accept this.
    let a = inputs.get_unchecked(0);
    let b = inputs.get_unchecked(1);
    inputs.set(0, b);
    inputs.set(1, a);

    assert_eq!(
        verify_batch(&env, &vk, &digest, &proofs, &inputs, 16),
        Err(Error::BatchVerificationFailed)
    );
}

#[test]
fn empty_batch_is_rejected() {
    let env = test_env();
    let (vk, _) = synth_vk(&env, 2, 0);
    let digest = verifying_key_digest(&env, &vk, 2);
    assert_eq!(
        verify_batch(&env, &vk, &digest, &Vec::new(&env), &Vec::new(&env), 16),
        Err(Error::EmptyBatch)
    );
}

#[test]
fn oversized_batch_is_rejected() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 1, 0);
    let digest = verifying_key_digest(&env, &vk, 1);
    let (proofs, inputs) = batch_of(&env, &td, 5, 1);
    assert_eq!(
        verify_batch(&env, &vk, &digest, &proofs, &inputs, 4),
        Err(Error::BatchTooLarge)
    );
}

#[test]
fn batch_length_mismatch_is_rejected() {
    let env = test_env();
    let (vk, td) = synth_vk(&env, 1, 0);
    let digest = verifying_key_digest(&env, &vk, 1);
    let (proofs, inputs) = batch_of(&env, &td, 3, 1);
    let mut short = Vec::new(&env);
    short.push_back(inputs.get_unchecked(0));
    assert_eq!(
        verify_batch(&env, &vk, &digest, &proofs, &short, 16),
        Err(Error::BatchLengthMismatch)
    );
}
