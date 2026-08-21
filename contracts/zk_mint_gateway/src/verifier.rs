//! The slice of the verifier's interface the gateway depends on.
//!
//! Declaring it as a `#[contractclient]` trait rather than importing the
//! verifier crate keeps the two contracts decoupled at link time. Importing it
//! would drag the verifier's exported entrypoints into the gateway's wasm and
//! collide on every shared name.
//!
//! [`Attestation`] mirrors the verifier's struct field for field.
//! `#[contracttype]` structs are encoded as maps keyed by field name, so a
//! structurally identical mirror decodes a verifier response correctly. The
//! integration tests assert exactly that by round-tripping a real attestation
//! through this client.

use soroban_sdk::{contractclient, contracttype, Address, BytesN, Env, Symbol};

/// Mirror of `soromint_zk_metadata_verifier::Attestation`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attestation {
    pub metadata_commitment: BytesN<32>,
    pub issuer: Address,
    pub nullifier: BytesN<32>,
    pub policy_version: u32,
    pub circuit_id: Symbol,
    pub risk_score: u32,
    pub verified_at: u32,
    pub expires_at: u32,
    pub consumed: bool,
    pub consumer: Option<Address>,
    pub consumed_at: Option<u32>,
}

#[contractclient(name = "VerifierClient")]
pub trait VerifierInterface {
    /// Redeem an attestation. Fails in the verifier if it is missing, already
    /// consumed, expired, or bound to a different issuer — the gateway does
    /// not re-check any of that, so single-use has exactly one owner.
    fn consume_attestation(
        env: Env,
        consumer: Address,
        metadata_commitment: BytesN<32>,
        issuer: Address,
    ) -> Attestation;
}
