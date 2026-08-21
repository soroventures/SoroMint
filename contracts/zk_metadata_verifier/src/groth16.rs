//! Groth16 verification over BLS12-381, implemented against the Soroban host's
//! native curve functions.
//!
//! # The equation
//!
//! A Groth16 proof `(A, B, C)` for a statement with public inputs
//! `x = (x_1, …, x_n)` verifies iff
//!
//! ```text
//! e(A, B) = e(α, β) · e(L, γ) · e(C, δ)      where  L = IC_0 + Σ_j x_j · IC_j
//! ```
//!
//! The host exposes a *multi-pairing product* check rather than individual
//! pairings, so the equation is rearranged to put everything on one side:
//!
//! ```text
//! e(−A, B) · e(α, β) · e(L, γ) · e(C, δ) = 1
//! ```
//!
//! That is a single `pairing_check` over four pairs — one Miller loop batch and
//! one final exponentiation, instead of four of each. On Soroban this is the
//! difference between a verification that fits comfortably in a transaction's
//! resource budget and one that does not.
//!
//! # What is checked, and why
//!
//! * **Encoding flags.** The host panics on malformed flag bits. A panic aborts
//!   the whole transaction with an opaque error, so flags are pre-validated
//!   here and surfaced as [`Error::InvalidG1Point`] / [`Error::InvalidG2Point`].
//! * **Coordinate range.** Same reasoning: coordinates ≥ `p` are rejected
//!   before they reach the host.
//! * **Subgroup membership.** BLS12-381's G1 and G2 are proper subgroups of
//!   their curve groups. A point on the curve but outside the subgroup breaks
//!   the pairing's bilinearity and lets an attacker forge. The host's
//!   `g1_is_in_subgroup` / `g2_is_in_subgroup` do the cofactor clearing check.
//!   This is the single most commonly omitted check in hand-rolled verifiers.
//! * **Point at infinity.** Degenerate proof elements are refused. An honest
//!   prover hits this with probability ≈ 2⁻²⁵⁵, so completeness is unaffected.
//! * **Scalar canonicality.** See [`crate::field`].
//!
//! Verifying-key points are validated once at registration rather than on every
//! verification, since the registry is governance-controlled and immutable
//! between rotations.
//!
//! # What this module cannot turn into a clean error
//!
//! One check is missing from the list above: **curve membership** (`y² = x³+4`
//! for G1, its Fp2 analogue for G2). The SDK exposes `Fp` addition and negation
//! but no multiplication, so the equation cannot be evaluated here. The host
//! does evaluate it, during deserialization — but it reports failure by
//! *trapping*, and a Soroban guest cannot catch a host trap.
//!
//! The security consequence is nil: an off-curve point is still rejected, and
//! the transaction still fails. The ergonomic consequence is that the caller
//! sees a host error rather than [`Error::InvalidG1Point`]. The pre-checks in
//! this module exist precisely to keep every *other* malformed input on the
//! clean-error path, so a host trap becomes a reliable signal of exactly one
//! condition: a point that is well-formed and in range but not on the curve.

use soroban_sdk::{
    crypto::bls12_381::{Fr, G1Affine, G2Affine},
    Env, Vec,
};

use crate::errors::Error;
use crate::field::{checked_scalar, scalar_zero, Transcript, BATCH_DST, VK_DIGEST_DST};
use crate::types::{Digest, G1Bytes, G2Bytes, Groth16Proof, ScalarBytes, VerifyingKey};

/// The BLS12-381 base field modulus `p`, big-endian, 48 bytes.
pub const FP_MODULUS_BE: [u8; 48] = [
    0x1a, 0x01, 0x11, 0xea, 0x39, 0x7f, 0xe6, 0x9a, 0x4b, 0x1b, 0xa7, 0xb6, 0x43, 0x4b, 0xac, 0xd7,
    0x64, 0x77, 0x4b, 0x84, 0xf3, 0x85, 0x12, 0xbf, 0x67, 0x30, 0xd2, 0xa0, 0xf6, 0xb0, 0xf6, 0x24,
    0x1e, 0xab, 0xff, 0xfe, 0xb1, 0x53, 0xff, 0xff, 0xb9, 0xfe, 0xff, 0xff, 0xff, 0xff, 0xaa, 0xab,
];

/// Compression flag — must always be clear (only uncompressed points).
const FLAG_COMPRESSION: u8 = 0x80;
/// Infinity flag.
const FLAG_INFINITY: u8 = 0x40;
/// Sort flag — must always be clear.
const FLAG_SORT: u8 = 0x20;
/// Mask isolating the flag bits.
const FLAG_MASK: u8 = 0xE0;

/// Largest public-input arity the contract will register a circuit for.
///
/// Each additional input costs one G1 scalar multiplication inside the MSM.
/// Thirty-two is far above what the metadata circuit needs (eight) and still
/// leaves headroom in the CPU budget for the four pairings.
pub const MAX_PUBLIC_INPUTS: u32 = 32;

/// Largest batch the aggregate verifier will accept in one call.
pub const MAX_BATCH_SIZE: u32 = 16;

// ---------------------------------------------------------------------------
// Encoding validation
// ---------------------------------------------------------------------------

/// True when a 48-byte big-endian limb is a valid `Fp` element (`< p`).
///
/// `masked_first` lets the caller strip flag bits from byte 0 before the
/// comparison; `p`'s leading byte is `0x1a`, so the three flag bits never
/// overlap a significant bit of a valid coordinate.
fn fp_in_range(limb: &[u8], masked_first: bool) -> bool {
    debug_assert!(limb.len() == 48);
    let mut i = 0usize;
    while i < 48 {
        let lhs = if i == 0 && masked_first {
            limb[0] & !FLAG_MASK
        } else {
            limb[i]
        };
        if lhs != FP_MODULUS_BE[i] {
            return lhs < FP_MODULUS_BE[i];
        }
        i += 1;
    }
    false
}

/// True when the encoding has the infinity flag set.
pub fn g1_is_infinity(bytes: &G1Bytes) -> bool {
    (bytes.to_array()[0] & FLAG_INFINITY) != 0
}

/// True when the encoding has the infinity flag set.
pub fn g2_is_infinity(bytes: &G2Bytes) -> bool {
    (bytes.to_array()[0] & FLAG_INFINITY) != 0
}

/// Structural validation of a serialized G1 point, before it reaches the host.
///
/// A correctly flagged point at infinity is accepted here; callers that cannot
/// tolerate infinity reject it separately, so that the two failure modes stay
/// distinguishable in the error code.
fn g1_wellformed(bytes: &G1Bytes) -> Result<(), Error> {
    let arr = bytes.to_array();
    let flags = arr[0] & FLAG_MASK;

    if flags & FLAG_COMPRESSION != 0 || flags & FLAG_SORT != 0 {
        return Err(Error::InvalidG1Point);
    }

    if flags & FLAG_INFINITY != 0 {
        // Infinity must be encoded as the flag alone: every other bit zero.
        if arr[0] & !FLAG_MASK != 0 {
            return Err(Error::InvalidG1Point);
        }
        let mut i = 1usize;
        while i < 96 {
            if arr[i] != 0 {
                return Err(Error::InvalidG1Point);
            }
            i += 1;
        }
        return Ok(());
    }

    if !fp_in_range(&arr[0..48], true) || !fp_in_range(&arr[48..96], false) {
        return Err(Error::InvalidG1Point);
    }
    Ok(())
}

/// Structural validation of a serialized G2 point.
fn g2_wellformed(bytes: &G2Bytes) -> Result<(), Error> {
    let arr = bytes.to_array();
    let flags = arr[0] & FLAG_MASK;

    if flags & FLAG_COMPRESSION != 0 || flags & FLAG_SORT != 0 {
        return Err(Error::InvalidG2Point);
    }

    if flags & FLAG_INFINITY != 0 {
        if arr[0] & !FLAG_MASK != 0 {
            return Err(Error::InvalidG2Point);
        }
        let mut i = 1usize;
        while i < 192 {
            if arr[i] != 0 {
                return Err(Error::InvalidG2Point);
            }
            i += 1;
        }
        return Ok(());
    }

    // Four Fp limbs: X_c1, X_c0, Y_c1, Y_c0. Only the first carries flags.
    if !fp_in_range(&arr[0..48], true)
        || !fp_in_range(&arr[48..96], false)
        || !fp_in_range(&arr[96..144], false)
        || !fp_in_range(&arr[144..192], false)
    {
        return Err(Error::InvalidG2Point);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/// Decode a G1 point with full validation, including the subgroup check.
///
/// `allow_infinity` should be `false` for proof elements and `true` only where
/// the identity is a legitimate value (an unused `IC` base, for instance).
pub fn decode_g1(env: &Env, bytes: &G1Bytes, allow_infinity: bool) -> Result<G1Affine, Error> {
    g1_wellformed(bytes)?;
    let infinity = g1_is_infinity(bytes);
    if infinity && !allow_infinity {
        return Err(Error::PointAtInfinity);
    }
    let point = G1Affine::from_bytes(bytes.clone());
    if !infinity && !env.crypto().bls12_381().g1_is_in_subgroup(&point) {
        return Err(Error::InvalidG1Point);
    }
    Ok(point)
}

/// Decode a G2 point with full validation, including the subgroup check.
pub fn decode_g2(env: &Env, bytes: &G2Bytes, allow_infinity: bool) -> Result<G2Affine, Error> {
    g2_wellformed(bytes)?;
    let infinity = g2_is_infinity(bytes);
    if infinity && !allow_infinity {
        return Err(Error::PointAtInfinity);
    }
    let point = G2Affine::from_bytes(bytes.clone());
    if !infinity && !env.crypto().bls12_381().g2_is_in_subgroup(&point) {
        return Err(Error::InvalidG2Point);
    }
    Ok(point)
}

// ---------------------------------------------------------------------------
// Verifying key handling
// ---------------------------------------------------------------------------

/// Validate every point in a verifying key and confirm its arity.
///
/// Run once, at registration. `ic` must hold exactly `num_public_inputs + 1`
/// bases; a mismatch there is the classic way a verifier silently accepts
/// proofs for a different statement than the operator believes.
pub fn validate_verifying_key(
    env: &Env,
    vk: &VerifyingKey,
    num_public_inputs: u32,
) -> Result<(), Error> {
    if num_public_inputs > MAX_PUBLIC_INPUTS {
        return Err(Error::UnsupportedArity);
    }
    if vk.ic.len() != num_public_inputs + 1 {
        return Err(Error::MalformedVerifyingKey);
    }

    // α, β, γ, δ must all be non-degenerate: a δ or γ at infinity would make
    // the corresponding pairing term vanish and neuter the constraint it
    // enforces.
    decode_g1(env, &vk.alpha_g1, false)?;
    decode_g2(env, &vk.beta_g2, false)?;
    decode_g2(env, &vk.gamma_g2, false)?;
    decode_g2(env, &vk.delta_g2, false)?;

    for base in vk.ic.iter() {
        // An IC base may legitimately be the identity when the corresponding
        // public wire is unconstrained in the circuit.
        decode_g1(env, &base, true)?;
    }
    Ok(())
}

/// SHA-256 fingerprint over the canonical serialization of a verifying key.
///
/// Operators compare this against the digest their local `snarkjs zkey export`
/// produces. Because it covers the arity and every point in order, two keys
/// with the same digest are byte-identical.
pub fn verifying_key_digest(env: &Env, vk: &VerifyingKey, num_public_inputs: u32) -> Digest {
    let mut t = Transcript::new(env, VK_DIGEST_DST);
    t.absorb_u32(num_public_inputs);
    t.absorb_n(&vk.alpha_g1);
    t.absorb_n(&vk.beta_g2);
    t.absorb_n(&vk.gamma_g2);
    t.absorb_n(&vk.delta_g2);
    t.absorb_u32(vk.ic.len());
    for base in vk.ic.iter() {
        t.absorb_n(&base);
    }
    t.finalize()
}

// ---------------------------------------------------------------------------
// Public input commitment
// ---------------------------------------------------------------------------

/// Compute `L = IC_0 + Σ_j x_j · IC_j` with a single host MSM.
///
/// The constant base `IC_0` is folded into the MSM with scalar `1` rather than
/// added afterwards; one `g1_msm` of width `n+1` is cheaper than an MSM of
/// width `n` plus a `g1_add`, and it keeps the code path uniform when `n = 0`.
pub fn public_input_commitment(
    env: &Env,
    vk: &VerifyingKey,
    public_inputs: &Vec<ScalarBytes>,
) -> Result<G1Affine, Error> {
    let n = public_inputs.len();
    if vk.ic.len() != n + 1 {
        return Err(Error::PublicInputCountMismatch);
    }

    let mut bases: Vec<G1Affine> = Vec::new(env);
    let mut scalars: Vec<Fr> = Vec::new(env);

    bases.push_back(G1Affine::from_bytes(vk.ic.get_unchecked(0)));
    scalars.push_back(crate::field::scalar_one(env));

    for j in 0..n {
        let scalar = checked_scalar(&public_inputs.get_unchecked(j))?;
        bases.push_back(G1Affine::from_bytes(vk.ic.get_unchecked(j + 1)));
        scalars.push_back(scalar);
    }

    Ok(env.crypto().bls12_381().g1_msm(bases, scalars))
}

// ---------------------------------------------------------------------------
// Single-proof verification
// ---------------------------------------------------------------------------

/// Verify one Groth16 proof. Returns `Ok(())` only when the pairing holds.
///
/// The verifying key is assumed pre-validated (see [`validate_verifying_key`]);
/// the proof is not, and is fully checked here.
pub fn verify_proof(
    env: &Env,
    vk: &VerifyingKey,
    proof: &Groth16Proof,
    public_inputs: &Vec<ScalarBytes>,
) -> Result<(), Error> {
    let a = decode_g1(env, &proof.a, false)?;
    let b = decode_g2(env, &proof.b, false)?;
    let c = decode_g1(env, &proof.c, false)?;

    let l = public_input_commitment(env, vk, public_inputs)?;

    let alpha = G1Affine::from_bytes(vk.alpha_g1.clone());
    let beta = G2Affine::from_bytes(vk.beta_g2.clone());
    let gamma = G2Affine::from_bytes(vk.gamma_g2.clone());
    let delta = G2Affine::from_bytes(vk.delta_g2.clone());

    let mut g1: Vec<G1Affine> = Vec::new(env);
    let mut g2: Vec<G2Affine> = Vec::new(env);

    // e(−A, B) · e(α, β) · e(L, γ) · e(C, δ) == 1
    g1.push_back(-a);
    g2.push_back(b);
    g1.push_back(alpha);
    g2.push_back(beta);
    g1.push_back(l);
    g2.push_back(gamma);
    g1.push_back(c);
    g2.push_back(delta);

    if env.crypto().bls12_381().pairing_check(g1, g2) {
        Ok(())
    } else {
        Err(Error::ProofVerificationFailed)
    }
}

/// Boolean flavour of [`verify_proof`] for read-only queries.
pub fn is_valid_proof(
    env: &Env,
    vk: &VerifyingKey,
    proof: &Groth16Proof,
    public_inputs: &Vec<ScalarBytes>,
) -> bool {
    verify_proof(env, vk, proof, public_inputs).is_ok()
}

// ---------------------------------------------------------------------------
// Batch verification
// ---------------------------------------------------------------------------

/// Verify `n` proofs against one verifying key with `n + 3` pairings instead of
/// `4n`.
///
/// # Construction
///
/// Each proof satisfies `e(−A_i, B_i)·e(α, β)·e(L_i, γ)·e(C_i, δ) = 1`. Raising
/// proof `i`'s equation to a random `r_i` and multiplying them together gives
///
/// ```text
/// ∏_i e(−r_i·A_i, B_i) · e((Σ r_i)·α, β) · e(Σ r_i·L_i, γ) · e(Σ r_i·C_i, δ) = 1
/// ```
///
/// because α, β, γ and δ are shared. If any single proof is invalid, the
/// product is `1` only for a vanishing fraction of coefficient vectors, so a
/// forger who cannot predict `r` cannot pass.
///
/// # Why the coefficients come from a transcript
///
/// `env.prng()` is seeded from ledger state a submitter can observe, and the
/// submitter chooses when to submit. Deriving `r_i` by hashing the entire batch
/// (every proof point and every public input, in order) removes that freedom:
/// changing anything to game the coefficients changes the coefficients.
///
/// # The nested-MSM fold
///
/// `Σ_i r_i·L_i` expands to `Σ_i r_i·(Σ_j x_{i,j}·IC_j)`. Swapping the sums
/// gives `Σ_j IC_j·(Σ_i r_i·x_{i,j})` — one MSM of width `n_inputs + 1` over
/// scalar sums, rather than one MSM per proof. For a 16-proof batch of the
/// 8-input metadata circuit that is 9 scalar multiplications instead of 144.
pub fn verify_batch(
    env: &Env,
    vk: &VerifyingKey,
    vk_digest: &Digest,
    proofs: &Vec<Groth16Proof>,
    inputs: &Vec<Vec<ScalarBytes>>,
    max_batch: u32,
) -> Result<(), Error> {
    let n = proofs.len();
    if n == 0 {
        return Err(Error::EmptyBatch);
    }
    if n > max_batch {
        return Err(Error::BatchTooLarge);
    }
    if inputs.len() != n {
        return Err(Error::BatchLengthMismatch);
    }

    let arity = vk.ic.len();
    let bls = env.crypto().bls12_381();

    // --- transcript over the whole batch -----------------------------------
    let mut transcript = Transcript::new(env, BATCH_DST);
    transcript.absorb_n(vk_digest);
    transcript.absorb_u32(n);
    for i in 0..n {
        let p = proofs.get_unchecked(i);
        transcript.absorb_n(&p.a);
        transcript.absorb_n(&p.b);
        transcript.absorb_n(&p.c);
        let xs = inputs.get_unchecked(i);
        if xs.len() + 1 != arity {
            return Err(Error::PublicInputCountMismatch);
        }
        transcript.absorb_u32(xs.len());
        for j in 0..xs.len() {
            transcript.absorb_n(&xs.get_unchecked(j));
        }
    }

    // --- accumulators -------------------------------------------------------
    let mut pair_g1: Vec<G1Affine> = Vec::new(env);
    let mut pair_g2: Vec<G2Affine> = Vec::new(env);

    // Folded IC coefficients: index 0 is the constant term.
    let mut ic_coeffs: Vec<Fr> = Vec::new(env);
    for _ in 0..arity {
        ic_coeffs.push_back(scalar_zero(env));
    }

    let mut c_bases: Vec<G1Affine> = Vec::new(env);
    let mut c_scalars: Vec<Fr> = Vec::new(env);
    let mut alpha_coeff = scalar_zero(env);

    for i in 0..n {
        let proof = proofs.get_unchecked(i);
        let r = transcript.challenge(i);

        let a = decode_g1(env, &proof.a, false)?;
        let b = decode_g2(env, &proof.b, false)?;
        let c = decode_g1(env, &proof.c, false)?;

        // e(−r_i·A_i, B_i)
        pair_g1.push_back(-bls.g1_mul(&a, &r));
        pair_g2.push_back(b);

        // Σ r_i·C_i, deferred into one MSM.
        c_bases.push_back(c);
        c_scalars.push_back(r.clone());

        // Σ r_i, the α coefficient.
        alpha_coeff = bls.fr_add(&alpha_coeff, &r);

        // Fold this proof's public inputs into the shared IC coefficients.
        let xs = inputs.get_unchecked(i);
        let c0 = ic_coeffs.get_unchecked(0);
        ic_coeffs.set(0, bls.fr_add(&c0, &r));
        for j in 0..xs.len() {
            let x = checked_scalar(&xs.get_unchecked(j))?;
            let term = bls.fr_mul(&r, &x);
            let prev = ic_coeffs.get_unchecked(j + 1);
            ic_coeffs.set(j + 1, bls.fr_add(&prev, &term));
        }
    }

    // Σ r_i·L_i as a single MSM over the IC bases.
    let mut ic_bases: Vec<G1Affine> = Vec::new(env);
    for j in 0..arity {
        ic_bases.push_back(G1Affine::from_bytes(vk.ic.get_unchecked(j)));
    }
    let folded_l = bls.g1_msm(ic_bases, ic_coeffs);

    // e((Σ r_i)·α, β)
    let alpha = G1Affine::from_bytes(vk.alpha_g1.clone());
    pair_g1.push_back(bls.g1_mul(&alpha, &alpha_coeff));
    pair_g2.push_back(G2Affine::from_bytes(vk.beta_g2.clone()));

    // e(Σ r_i·L_i, γ)
    pair_g1.push_back(folded_l);
    pair_g2.push_back(G2Affine::from_bytes(vk.gamma_g2.clone()));

    // e(Σ r_i·C_i, δ)
    pair_g1.push_back(bls.g1_msm(c_bases, c_scalars));
    pair_g2.push_back(G2Affine::from_bytes(vk.delta_g2.clone()));

    if bls.pairing_check(pair_g1, pair_g2) {
        Ok(())
    } else {
        Err(Error::BatchVerificationFailed)
    }
}

/// Identify which members of a failed batch are individually invalid.
///
/// Only worth calling after [`verify_batch`] has already failed — it costs a
/// full `4n` pairings. Exposed so a caller can attribute blame (and charge the
/// right submitter) rather than rejecting an entire batch opaquely.
pub fn locate_batch_failures(
    env: &Env,
    vk: &VerifyingKey,
    proofs: &Vec<Groth16Proof>,
    inputs: &Vec<Vec<ScalarBytes>>,
) -> Vec<u32> {
    let mut failed: Vec<u32> = Vec::new(env);
    let n = proofs.len();
    for i in 0..n {
        let proof = proofs.get_unchecked(i);
        let xs = inputs.get_unchecked(i);
        if verify_proof(env, vk, &proof, &xs).is_err() {
            failed.push_back(i);
        }
    }
    failed
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{BytesN, Env};

    fn g1_infinity(env: &Env) -> G1Bytes {
        let mut buf = [0u8; 96];
        buf[0] = FLAG_INFINITY;
        BytesN::from_array(env, &buf)
    }

    fn g2_infinity(env: &Env) -> G2Bytes {
        let mut buf = [0u8; 192];
        buf[0] = FLAG_INFINITY;
        BytesN::from_array(env, &buf)
    }

    #[test]
    fn infinity_encoding_is_wellformed() {
        let env = Env::default();
        assert!(g1_wellformed(&g1_infinity(&env)).is_ok());
        assert!(g2_wellformed(&g2_infinity(&env)).is_ok());
    }

    #[test]
    fn infinity_with_dirty_payload_is_rejected() {
        let env = Env::default();
        let mut buf = [0u8; 96];
        buf[0] = FLAG_INFINITY;
        buf[50] = 1;
        assert_eq!(
            g1_wellformed(&BytesN::from_array(&env, &buf)),
            Err(Error::InvalidG1Point)
        );
    }

    #[test]
    fn compression_flag_is_rejected() {
        let env = Env::default();
        let mut buf = [0u8; 96];
        buf[0] = FLAG_COMPRESSION;
        assert_eq!(
            g1_wellformed(&BytesN::from_array(&env, &buf)),
            Err(Error::InvalidG1Point)
        );
    }

    #[test]
    fn sort_flag_is_rejected() {
        let env = Env::default();
        let mut buf = [0u8; 192];
        buf[0] = FLAG_SORT;
        assert_eq!(
            g2_wellformed(&BytesN::from_array(&env, &buf)),
            Err(Error::InvalidG2Point)
        );
    }

    #[test]
    fn out_of_range_coordinate_is_rejected() {
        let env = Env::default();
        let buf = [0xffu8; 96];
        assert_eq!(
            g1_wellformed(&BytesN::from_array(&env, &buf)),
            Err(Error::InvalidG1Point)
        );
    }

    #[test]
    fn modulus_minus_one_coordinate_is_in_range() {
        let mut limb = FP_MODULUS_BE;
        limb[47] -= 1;
        assert!(fp_in_range(&limb, false));
    }

    #[test]
    fn modulus_coordinate_is_out_of_range() {
        assert!(!fp_in_range(&FP_MODULUS_BE, false));
    }

    #[test]
    fn decode_rejects_infinity_when_disallowed() {
        let env = Env::default();
        assert_eq!(
            decode_g1(&env, &g1_infinity(&env), false),
            Err(Error::PointAtInfinity)
        );
        assert_eq!(
            decode_g2(&env, &g2_infinity(&env), false),
            Err(Error::PointAtInfinity)
        );
    }

    #[test]
    fn decode_accepts_infinity_when_allowed() {
        let env = Env::default();
        assert!(decode_g1(&env, &g1_infinity(&env), true).is_ok());
    }
}
