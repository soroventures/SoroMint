//! Scalar-field helpers for BLS12-381.
//!
//! Two jobs live here:
//!
//! 1. **Canonicality.** The host will happily interpret any 32-byte string as a
//!    scalar, reducing it mod `r`. For *public inputs* that is unsafe: it makes
//!    the statement malleable, since `x` and `x + r` would produce identical
//!    verification results but different transaction payloads. Every scalar
//!    that enters verification passes [`checked_scalar`] first.
//!
//! 2. **Fiat–Shamir.** Batch verification needs random coefficients that the
//!    prover cannot choose. Deriving them from a transcript over the whole
//!    batch (rather than from `env.prng()`) keeps the check sound even against
//!    a submitter who can observe ledger state.

use soroban_sdk::{crypto::bls12_381::Fr, Bytes, BytesN, Env, U256};

use crate::errors::Error;
use crate::types::ScalarBytes;

/// The BLS12-381 subgroup order `r`, big-endian.
///
/// `r = 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001`
pub const FR_MODULUS_BE: [u8; 32] = [
    0x73, 0xed, 0xa7, 0x53, 0x29, 0x9d, 0x7d, 0x48, 0x33, 0x39, 0xd8, 0x08, 0x09, 0xa1, 0xd8, 0x05,
    0x53, 0xbd, 0xa4, 0x02, 0xff, 0xfe, 0x5b, 0xfe, 0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
];

/// Domain separation tag mixed into every batch transcript. Changing this
/// invalidates nothing on chain but prevents challenges from one deployment
/// being replayed as challenges in another.
pub const BATCH_DST: &[u8] = b"SOROMINT-ZK-METADATA-GROTH16-BATCH-V1";

/// Domain separation tag for verifying-key digests.
pub const VK_DIGEST_DST: &[u8] = b"SOROMINT-ZK-METADATA-VK-DIGEST-V1";

/// Domain separation tag for issuer address binding.
pub const ISSUER_DST: &[u8] = b"SOROMINT-ZK-METADATA-ISSUER-V1";

/// Returns `true` when `bytes` encodes a value strictly less than `r`.
///
/// Comparison is a plain big-endian lexicographic walk: the first differing
/// byte decides. Constant-time behaviour is irrelevant here because both
/// operands are public.
pub fn is_canonical(bytes: &ScalarBytes) -> bool {
    let v = bytes.to_array();
    let mut i = 0usize;
    while i < 32 {
        if v[i] != FR_MODULUS_BE[i] {
            return v[i] < FR_MODULUS_BE[i];
        }
        i += 1;
    }
    // Exactly equal to r — not canonical (r ≡ 0, and 0 has the encoding of 0).
    false
}

/// Validate and lift a 32-byte big-endian scalar into an `Fr`.
pub fn checked_scalar(bytes: &ScalarBytes) -> Result<Fr, Error> {
    if !is_canonical(bytes) {
        return Err(Error::NonCanonicalScalar);
    }
    Ok(Fr::from_bytes(bytes.clone()))
}

/// Lift a small unsigned integer into `Fr`. Always canonical.
pub fn scalar_from_u32(env: &Env, value: u32) -> Fr {
    Fr::from_u256(U256::from_u32(env, value))
}

/// Lift a `u64` into `Fr`. Always canonical.
pub fn scalar_from_u64(env: &Env, value: u64) -> Fr {
    // U256::from_u128 is the widest small-int constructor exposed by the SDK.
    Fr::from_u256(U256::from_u128(env, value as u128))
}

/// The additive identity of `Fr`.
pub fn scalar_zero(env: &Env) -> Fr {
    Fr::from_u256(U256::from_u32(env, 0))
}

/// The multiplicative identity of `Fr`.
pub fn scalar_one(env: &Env) -> Fr {
    Fr::from_u256(U256::from_u32(env, 1))
}

/// Encode a `u32` as a 32-byte big-endian scalar. Used when a public signal is
/// semantically an integer (verdict, risk score, ledger number) but must be
/// presented to the pairing check as a field element.
pub fn u32_to_scalar_bytes(env: &Env, value: u32) -> ScalarBytes {
    let mut buf = [0u8; 32];
    buf[28..32].copy_from_slice(&value.to_be_bytes());
    BytesN::from_array(env, &buf)
}

/// A rolling Fiat–Shamir transcript over SHA-256.
///
/// Absorb everything that the challenge must depend on, then `challenge(i)` to
/// squeeze coefficient `i`. Squeezing does not mutate the absorbed state, so
/// coefficients are a deterministic function of the whole batch — reordering
/// the batch changes every coefficient.
pub struct Transcript {
    env: Env,
    buf: Bytes,
}

impl Transcript {
    /// Start a transcript seeded with a domain separation tag.
    pub fn new(env: &Env, dst: &[u8]) -> Self {
        let mut buf = Bytes::new(env);
        buf.extend_from_slice(dst);
        Self {
            env: env.clone(),
            buf,
        }
    }

    /// Absorb raw bytes.
    pub fn absorb_bytes(&mut self, bytes: &Bytes) {
        self.buf.append(bytes);
    }

    /// Absorb a fixed-width byte string.
    pub fn absorb_n<const N: usize>(&mut self, bytes: &BytesN<N>) {
        self.buf.extend_from_array(&bytes.to_array());
    }

    /// Absorb a slice literal (tags, separators).
    pub fn absorb_slice(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Absorb a `u32` in big-endian form.
    pub fn absorb_u32(&mut self, value: u32) {
        self.buf.extend_from_array(&value.to_be_bytes());
    }

    /// Squeeze challenge number `index`.
    ///
    /// The top byte of the SHA-256 output is cleared, bounding the result by
    /// `2^248 < r`. That costs eight bits of challenge entropy and buys an
    /// unconditional canonicality guarantee — a fine trade when the soundness
    /// error is `batch_size / 2^248`.
    pub fn challenge(&self, index: u32) -> Fr {
        let mut material = self.buf.clone();
        material.extend_from_slice(b"|chal|");
        material.extend_from_array(&index.to_be_bytes());
        let digest = self.env.crypto().sha256(&material);
        let mut arr = digest.to_array();
        arr[0] = 0;
        Fr::from_bytes(BytesN::from_array(&self.env, &arr))
    }

    /// Finalize into a plain digest (used for VK fingerprints).
    pub fn finalize(&self) -> BytesN<32> {
        self.env.crypto().sha256(&self.buf).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn zero_is_canonical() {
        let env = Env::default();
        let z = BytesN::from_array(&env, &[0u8; 32]);
        assert!(is_canonical(&z));
    }

    #[test]
    fn modulus_itself_is_not_canonical() {
        let env = Env::default();
        let m = BytesN::from_array(&env, &FR_MODULUS_BE);
        assert!(!is_canonical(&m));
    }

    #[test]
    fn modulus_minus_one_is_canonical() {
        let env = Env::default();
        let mut m = FR_MODULUS_BE;
        m[31] -= 1;
        assert!(is_canonical(&BytesN::from_array(&env, &m)));
    }

    #[test]
    fn all_ones_is_not_canonical() {
        let env = Env::default();
        let m = BytesN::from_array(&env, &[0xffu8; 32]);
        assert!(!is_canonical(&m));
    }

    #[test]
    fn u32_round_trips_through_scalar_bytes() {
        let env = Env::default();
        let b = u32_to_scalar_bytes(&env, 0xDEAD_BEEF);
        let arr = b.to_array();
        assert_eq!(&arr[28..32], &0xDEAD_BEEFu32.to_be_bytes());
        assert!(is_canonical(&b));
    }

    #[test]
    fn challenges_differ_by_index() {
        let env = Env::default();
        let t = Transcript::new(&env, BATCH_DST);
        assert_ne!(t.challenge(0).to_bytes(), t.challenge(1).to_bytes());
    }

    #[test]
    fn challenges_depend_on_absorbed_state() {
        let env = Env::default();
        let mut a = Transcript::new(&env, BATCH_DST);
        let mut b = Transcript::new(&env, BATCH_DST);
        a.absorb_u32(1);
        b.absorb_u32(2);
        assert_ne!(a.challenge(0).to_bytes(), b.challenge(0).to_bytes());
    }

    #[test]
    fn challenges_are_canonical() {
        let env = Env::default();
        let mut t = Transcript::new(&env, BATCH_DST);
        t.absorb_slice(b"some batch material");
        for i in 0..16u32 {
            assert!(is_canonical(&t.challenge(i).to_bytes()));
        }
    }
}
