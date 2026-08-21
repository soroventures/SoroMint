//! Test fixtures that build *genuine* Groth16 instances.
//!
//! Running a real prover inside a unit test is not practical, but mocking the
//! pairing check would leave the most important code in the crate untested. The
//! way out is to construct a verifying key and proof whose discrete logarithms
//! we know, chosen so that the verification equation holds exactly.
//!
//! Write every group element as a multiple of a generator — `A = a·g₁`,
//! `B = b·g₂`, `α = x·g₁`, `β = y·g₂`, `L = l·g₁`, `γ = c·g₂`, `C = z·g₁`,
//! `δ = d·g₂`. Bilinearity collapses the whole check into one exponent:
//!
//! ```text
//! e(−A,B)·e(α,β)·e(L,γ)·e(C,δ) = e(g₁,g₂)^(−ab + xy + lc + zd)
//! ```
//!
//! which is the identity iff `−ab + xy + lc + zd ≡ 0 (mod r)`. So we pick
//! `x, y, c, d, z` and the `IC` logarithms freely, fix `a = 1`, and *solve* for
//! `b = xy + lc + zd`. The resulting `(A, B, C)` is indistinguishable from a
//! real proof as far as the verifier is concerned: it goes through the same
//! decoding, the same subgroup checks, and the same host `pairing_check`.
//!
//! `l` depends on the public inputs (`l = ic₀ + Σ xⱼ·icⱼ`), so a proof built
//! for one input vector genuinely fails against another — which is exactly the
//! property the negative tests need.

use soroban_sdk::{
    crypto::bls12_381::{Fr, G1Affine, G2Affine},
    BytesN, Env, Vec,
};

use crate::field::checked_scalar;
use crate::types::{G1Bytes, G2Bytes, Groth16Proof, ScalarBytes, VerifyingKey};

/// Uncompressed BLS12-381 G1 generator: `be(X) || be(Y)`.
pub const G1_GENERATOR: [u8; 96] = [
    0x17, 0xf1, 0xd3, 0xa7, 0x31, 0x97, 0xd7, 0x94, 0x26, 0x95, 0x63, 0x8c, 0x4f, 0xa9, 0xac, 0x0f,
    0xc3, 0x68, 0x8c, 0x4f, 0x97, 0x74, 0xb9, 0x05, 0xa1, 0x4e, 0x3a, 0x3f, 0x17, 0x1b, 0xac, 0x58,
    0x6c, 0x55, 0xe8, 0x3f, 0xf9, 0x7a, 0x1a, 0xef, 0xfb, 0x3a, 0xf0, 0x0a, 0xdb, 0x22, 0xc6, 0xbb,
    0x08, 0xb3, 0xf4, 0x81, 0xe3, 0xaa, 0xa0, 0xf1, 0xa0, 0x9e, 0x30, 0xed, 0x74, 0x1d, 0x8a, 0xe4,
    0xfc, 0xf5, 0xe0, 0x95, 0xd5, 0xd0, 0x0a, 0xf6, 0x00, 0xdb, 0x18, 0xcb, 0x2c, 0x04, 0xb3, 0xed,
    0xd0, 0x3c, 0xc7, 0x44, 0xa2, 0x88, 0x8a, 0xe4, 0x0c, 0xaa, 0x23, 0x29, 0x46, 0xc5, 0xe7, 0xe1,
];

/// Uncompressed BLS12-381 G2 generator.
///
/// Soroban's G2 layout is `be(X_c1) || be(X_c0) || be(Y_c1) || be(Y_c0)` — the
/// `c1` component leads. Getting this backwards is the single most common
/// integration bug when porting a snarkjs verifying key, because the resulting
/// point is still on the curve and still in the subgroup; only the pairing
/// disagrees.
pub const G2_GENERATOR: [u8; 192] = [
    // X_c1
    0x13, 0xe0, 0x2b, 0x60, 0x52, 0x71, 0x9f, 0x60, 0x7d, 0xac, 0xd3, 0xa0, 0x88, 0x27, 0x4f, 0x65,
    0x59, 0x6b, 0xd0, 0xd0, 0x99, 0x20, 0xb6, 0x1a, 0xb5, 0xda, 0x61, 0xbb, 0xdc, 0x7f, 0x50, 0x49,
    0x33, 0x4c, 0xf1, 0x12, 0x13, 0x94, 0x5d, 0x57, 0xe5, 0xac, 0x7d, 0x05, 0x5d, 0x04, 0x2b, 0x7e,
    // X_c0
    0x02, 0x4a, 0xa2, 0xb2, 0xf0, 0x8f, 0x0a, 0x91, 0x26, 0x08, 0x05, 0x27, 0x2d, 0xc5, 0x10, 0x51,
    0xc6, 0xe4, 0x7a, 0xd4, 0xfa, 0x40, 0x3b, 0x02, 0xb4, 0x51, 0x0b, 0x64, 0x7a, 0xe3, 0xd1, 0x77,
    0x0b, 0xac, 0x03, 0x26, 0xa8, 0x05, 0xbb, 0xef, 0xd4, 0x80, 0x56, 0xc8, 0xc1, 0x21, 0xbd, 0xb8,
    // Y_c1
    0x06, 0x06, 0xc4, 0xa0, 0x2e, 0xa7, 0x34, 0xcc, 0x32, 0xac, 0xd2, 0xb0, 0x2b, 0xc2, 0x8b, 0x99,
    0xcb, 0x3e, 0x28, 0x7e, 0x85, 0xa7, 0x63, 0xaf, 0x26, 0x74, 0x92, 0xab, 0x57, 0x2e, 0x99, 0xab,
    0x3f, 0x37, 0x0d, 0x27, 0x5c, 0xec, 0x1d, 0xa1, 0xaa, 0xa9, 0x07, 0x5f, 0xf0, 0x5f, 0x79, 0xbe,
    // Y_c0
    0x0c, 0xe5, 0xd5, 0x27, 0x72, 0x7d, 0x6e, 0x11, 0x8c, 0xc9, 0xcd, 0xc6, 0xda, 0x2e, 0x35, 0x1a,
    0xad, 0xfd, 0x9b, 0xaa, 0x8c, 0xbd, 0xd3, 0xa7, 0x6d, 0x42, 0x9a, 0x69, 0x51, 0x60, 0xd1, 0x2c,
    0x92, 0x3a, 0xc9, 0xcc, 0x3b, 0xac, 0xa2, 0x89, 0xe1, 0x93, 0x54, 0x86, 0x08, 0xb8, 0x28, 0x01,
];

/// An `Env` with the metering budget lifted.
///
/// A single Groth16 verification costs several hundred million CPU units —
/// comfortably inside a real Soroban transaction budget once the contract is
/// compiled with `opt-level = "z"` and LTO, but well past the default test
/// harness limit when running an unoptimized debug build through the
/// interpreter. Tests that call the pairing therefore lift the cap; the
/// `cost_report` test measures the real figure separately.
pub fn test_env() -> Env {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();
    env
}

pub fn g1_generator(env: &Env) -> G1Affine {
    G1Affine::from_array(env, &G1_GENERATOR)
}

pub fn g2_generator(env: &Env) -> G2Affine {
    G2Affine::from_array(env, &G2_GENERATOR)
}

/// `scalar · g₁`, serialized.
pub fn g1_mul_gen(env: &Env, scalar: &Fr) -> G1Bytes {
    env.crypto()
        .bls12_381()
        .g1_mul(&g1_generator(env), scalar)
        .to_bytes()
}

/// `scalar · g₂`, serialized.
pub fn g2_mul_gen(env: &Env, scalar: &Fr) -> G2Bytes {
    env.crypto()
        .bls12_381()
        .g2_mul(&g2_generator(env), scalar)
        .to_bytes()
}

pub fn fr(env: &Env, v: u32) -> Fr {
    crate::field::scalar_from_u32(env, v)
}

/// The discrete logarithms behind a synthetic verifying key.
///
/// In a real deployment these are the toxic waste of the trusted setup and are
/// destroyed. Here they are exactly what lets us mint satisfying proofs.
pub struct Trapdoor {
    pub alpha: Fr,
    pub beta: Fr,
    pub gamma: Fr,
    pub delta: Fr,
    /// `ic[j]`'s discrete log; `ic[0]` is the constant term.
    pub ic: std::vec::Vec<Fr>,
}

/// Build a synthetic verifying key for a circuit with `num_inputs` public
/// signals, plus the trapdoor needed to forge satisfying proofs for it.
///
/// `seed` varies the key so tests can prove that a proof for one key fails
/// against another.
pub fn synth_vk(env: &Env, num_inputs: u32, seed: u32) -> (VerifyingKey, Trapdoor) {
    let alpha = fr(env, 7 + seed);
    let beta = fr(env, 11 + seed);
    let gamma = fr(env, 13 + seed);
    let delta = fr(env, 17 + seed);

    let mut ic_logs: std::vec::Vec<Fr> = std::vec::Vec::new();
    let mut ic: Vec<G1Bytes> = Vec::new(env);
    for j in 0..=num_inputs {
        // Any nonzero logs work; these are just distinct and small.
        let log = fr(env, 101 + seed * 31 + j * 3);
        ic.push_back(g1_mul_gen(env, &log));
        ic_logs.push(log);
    }

    let vk = VerifyingKey {
        alpha_g1: g1_mul_gen(env, &alpha),
        beta_g2: g2_mul_gen(env, &beta),
        gamma_g2: g2_mul_gen(env, &gamma),
        delta_g2: g2_mul_gen(env, &delta),
        ic,
    };

    (
        vk,
        Trapdoor {
            alpha,
            beta,
            gamma,
            delta,
            ic: ic_logs,
        },
    )
}

/// Forge a proof that satisfies the verification equation for `public_inputs`.
///
/// `c_log` seeds the `C` component; varying it produces distinct proofs of the
/// same statement, which mirrors how a real prover's randomness works and lets
/// batch tests use non-identical members.
pub fn synth_proof(
    env: &Env,
    trapdoor: &Trapdoor,
    public_inputs: &Vec<ScalarBytes>,
    c_log: u32,
) -> Groth16Proof {
    let bls = env.crypto().bls12_381();

    // l = ic₀ + Σ xⱼ·icⱼ
    let mut l = trapdoor.ic[0].clone();
    for j in 0..public_inputs.len() {
        let x = checked_scalar(&public_inputs.get_unchecked(j)).expect("canonical input");
        let term = bls.fr_mul(&trapdoor.ic[(j + 1) as usize], &x);
        l = bls.fr_add(&l, &term);
    }

    let z = fr(env, c_log);

    // b = xy + lc + zd, with a = 1.
    let xy = bls.fr_mul(&trapdoor.alpha, &trapdoor.beta);
    let lc = bls.fr_mul(&l, &trapdoor.gamma);
    let zd = bls.fr_mul(&z, &trapdoor.delta);
    let b = bls.fr_add(&bls.fr_add(&xy, &lc), &zd);

    Groth16Proof {
        a: g1_mul_gen(env, &crate::field::scalar_one(env)),
        b: g2_mul_gen(env, &b),
        c: g1_mul_gen(env, &z),
    }
}

/// A vector of `n` distinct small public inputs.
pub fn small_inputs(env: &Env, n: u32) -> Vec<ScalarBytes> {
    let mut v: Vec<ScalarBytes> = Vec::new(env);
    for j in 0..n {
        v.push_back(crate::field::u32_to_scalar_bytes(env, 1000 + j));
    }
    v
}

/// Flip one bit of a G1 encoding's `Y` coordinate.
///
/// The result is in range and correctly flagged but no longer satisfies
/// `y² = x³ + 4`, so the host rejects it during deserialization. The host
/// signals that by trapping, not by returning — see the note on host traps in
/// [`crate::groth16`] — so callers must expect a panic, not an `Err`.
pub fn corrupt_g1(env: &Env, point: &G1Bytes) -> G1Bytes {
    let mut arr = point.to_array();
    arr[95] ^= 0x01;
    BytesN::from_array(env, &arr)
}

/// Same, for G2.
pub fn corrupt_g2(env: &Env, point: &G2Bytes) -> G2Bytes {
    let mut arr = point.to_array();
    arr[191] ^= 0x01;
    BytesN::from_array(env, &arr)
}

/// A different, perfectly valid G1 point.
///
/// Substituting this into a proof exercises the pairing check itself rather
/// than the encoding guards: everything decodes, every subgroup check passes,
/// and the equation simply does not hold.
pub fn other_g1(env: &Env, seed: u32) -> G1Bytes {
    g1_mul_gen(env, &fr(env, 900_001 + seed))
}

/// A different, perfectly valid G2 point.
pub fn other_g2(env: &Env, seed: u32) -> G2Bytes {
    g2_mul_gen(env, &fr(env, 800_001 + seed))
}

/// A 32-byte value equal to the field modulus `r` — the canonical
/// non-canonical scalar.
pub fn modulus_scalar(env: &Env) -> ScalarBytes {
    BytesN::from_array(env, &crate::field::FR_MODULUS_BE)
}
