//! On-chain data model for Groth16 verification and AI metadata attestation.
//!
//! ## Encoding conventions
//!
//! All curve points use the **uncompressed** big-endian serialization that the
//! Soroban BLS12-381 host functions expect:
//!
//! * `G1` — 96 bytes: `be(X) || be(Y)`, each coordinate a 48-byte `Fp`.
//! * `G2` — 192 bytes: `be(X_c1) || be(X_c0) || be(Y_c1) || be(Y_c0)`.
//!
//! The top three bits of the first byte are flag bits (compression, infinity,
//! sort). Only the infinity flag may be set, and only on an otherwise-zero
//! encoding.
//!
//! Scalars (`Fr`) are 32-byte big-endian integers that MUST be strictly less
//! than the group order `r`. Non-canonical encodings are rejected rather than
//! silently reduced, because reduction would make proofs malleable: two
//! distinct byte strings would verify against the same statement.

use soroban_sdk::{contracttype, Address, BytesN, Env, String, Symbol, Vec};

/// Serialized G1 point (uncompressed, 96 bytes).
pub type G1Bytes = BytesN<96>;
/// Serialized G2 point (uncompressed, 192 bytes).
pub type G2Bytes = BytesN<192>;
/// Serialized `Fr` scalar (big-endian, 32 bytes).
pub type ScalarBytes = BytesN<32>;
/// A 32-byte domain digest (Poseidon or SHA-256 depending on context).
pub type Digest = BytesN<32>;

// ---------------------------------------------------------------------------
// Proof material
// ---------------------------------------------------------------------------

/// A Groth16 proof: `(A ∈ G1, B ∈ G2, C ∈ G1)`.
///
/// This is the canonical `snarkjs` / `arkworks` triple. Nothing else from the
/// prover is trusted — the statement lives entirely in the public inputs.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Groth16Proof {
    /// `A` — first G1 element of the proof.
    pub a: G1Bytes,
    /// `B` — the single G2 element of the proof.
    pub b: G2Bytes,
    /// `C` — second G1 element of the proof.
    pub c: G1Bytes,
}

/// A Groth16 verifying key for a fixed circuit.
///
/// `ic` holds the public-input commitment bases: `ic[0]` is the constant term
/// and `ic[i]` pairs with public input `i - 1`. Therefore
/// `ic.len() == num_public_inputs + 1` always.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifyingKey {
    /// `α ∈ G1`.
    pub alpha_g1: G1Bytes,
    /// `β ∈ G2`.
    pub beta_g2: G2Bytes,
    /// `γ ∈ G2`.
    pub gamma_g2: G2Bytes,
    /// `δ ∈ G2`.
    pub delta_g2: G2Bytes,
    /// Public input bases `IC[0..=n]`.
    pub ic: Vec<G1Bytes>,
}

/// Registry record wrapping a verifying key with its governance metadata.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CircuitRecord {
    /// Stable identifier, e.g. `metadata_v1`.
    pub circuit_id: Symbol,
    /// The active verifying key.
    pub vk: VerifyingKey,
    /// Number of public inputs the circuit exposes (excludes the `IC[0]` term).
    pub num_public_inputs: u32,
    /// SHA-256 over the canonical VK serialization; the value operators audit.
    pub vk_digest: Digest,
    /// Human-readable provenance string (circuit source commit, ptau ceremony).
    pub provenance: String,
    /// When `false`, verification against this circuit is refused.
    pub enabled: bool,
    /// When `true`, the key can never be rotated again.
    pub frozen: bool,
    /// Monotonic counter incremented on every successful rotation.
    pub revision: u32,
    /// Ledger sequence at which the current key became active.
    pub activated_at: u32,
    /// Total successful verifications served by this circuit.
    pub verifications: u64,
}

/// A verifying-key rotation awaiting its timelock.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingRotation {
    /// Circuit being rotated.
    pub circuit_id: Symbol,
    /// The key that will become active once the timelock elapses.
    pub vk: VerifyingKey,
    /// Arity of the incoming key.
    pub num_public_inputs: u32,
    /// Digest of the incoming key, published up-front for auditors.
    pub vk_digest: Digest,
    /// Earliest ledger at which `finalize_rotation` may be called.
    pub eta: u32,
    /// Who proposed the rotation.
    pub proposer: Address,
}

// ---------------------------------------------------------------------------
// AI safety policy
// ---------------------------------------------------------------------------

/// The on-chain half of the AI safety policy.
///
/// The circuit enforces the *structure* of validation; this record pins the
/// *parameters* that structure is evaluated against. Both halves are bound
/// together by `policy_root`, which the circuit exposes as a public signal.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    /// Monotonically increasing version number.
    pub version: u32,
    /// Merkle root over the policy's rule set: blocklist root, term weights,
    /// character-class tables and thresholds. The circuit proves the metadata
    /// was evaluated against exactly this root.
    pub policy_root: Digest,
    /// Commitment to the scoring model (architecture hash + weight digest +
    /// quantization scheme). Prevents a stale or rogue model from producing
    /// accepted attestations.
    pub model_commitment: Digest,
    /// Inclusive upper bound on `risk_score` for an accepted attestation.
    /// Scores are integers on a 0..=`risk_scale` axis.
    pub max_risk_score: u32,
    /// Denominator for `risk_score`, e.g. `1000` for per-mille granularity.
    pub risk_scale: u32,
    /// Maximum number of ledgers a proof may claim as its validity window.
    pub max_validity_ledgers: u32,
    /// Circuit that proofs must be verified against under this policy.
    pub circuit_id: Symbol,
    /// Ledger at which this policy became active.
    pub activated_at: u32,
    /// Free-form pointer to the published rule text (IPFS CID, URL, ...).
    pub rulebook_uri: String,
}

/// Parameters accepted by `publish_policy`. Split from [`Policy`] so callers
/// cannot forge `activated_at`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyParams {
    pub version: u32,
    pub policy_root: Digest,
    pub model_commitment: Digest,
    pub max_risk_score: u32,
    pub risk_scale: u32,
    pub max_validity_ledgers: u32,
    pub circuit_id: Symbol,
    pub rulebook_uri: String,
}

// ---------------------------------------------------------------------------
// Public signals
// ---------------------------------------------------------------------------

/// The public signals of the `metadata_validator` circuit, in the exact order
/// the circuit declares them.
///
/// ```text
/// signal output verdict;              // 0
/// signal output riskScore;            // 1
/// signal input  policyRoot;           // 2
/// signal input  modelCommitment;      // 3
/// signal input  metadataCommitment;   // 4
/// signal input  issuerHash;           // 5
/// signal input  nullifier;            // 6
/// signal input  expiryLedger;         // 7
/// ```
///
/// Any change to this ordering is a breaking change to the circuit and must be
/// accompanied by a new `circuit_id`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataSignals {
    /// `1` when the circuit concluded the metadata is safe, `0` otherwise.
    /// The contract additionally refuses `0`, so a rejected verdict can never
    /// produce an attestation even if a caller submits a valid proof of it.
    pub verdict: u32,
    /// Aggregate risk score on the policy's `risk_scale`.
    pub risk_score: u32,
    /// Must equal the active policy's `policy_root`.
    pub policy_root: Digest,
    /// Must equal the active policy's `model_commitment`.
    pub model_commitment: Digest,
    /// Poseidon commitment to the token metadata payload.
    pub metadata_commitment: Digest,
    /// `Poseidon(issuer_address_bytes)` — binds the proof to one issuer.
    pub issuer_hash: Digest,
    /// `Poseidon(metadata_commitment, issuer_hash, nonce)` — spent on use.
    pub nullifier: Digest,
    /// Ledger sequence after which this proof is stale.
    pub expiry_ledger: u32,
}

// ---------------------------------------------------------------------------
// Attestations
// ---------------------------------------------------------------------------

/// The receipt written when a metadata proof verifies.
///
/// The mint lifecycle reads this instead of re-running the pairing check, so a
/// single expensive verification amortizes across token creation and the first
/// mint.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attestation {
    /// Poseidon commitment to the metadata that was validated.
    pub metadata_commitment: Digest,
    /// The address the proof was bound to.
    pub issuer: Address,
    /// Nullifier that was spent to create this attestation.
    pub nullifier: Digest,
    /// Policy version in force at verification time.
    pub policy_version: u32,
    /// Circuit the proof was checked against.
    pub circuit_id: Symbol,
    /// Risk score carried by the proof.
    pub risk_score: u32,
    /// Ledger at which the attestation was created.
    pub verified_at: u32,
    /// Ledger after which the attestation may no longer be consumed.
    pub expires_at: u32,
    /// Set once a lifecycle contract has redeemed it.
    pub consumed: bool,
    /// Which contract consumed it, if any.
    pub consumer: Option<Address>,
    /// Ledger at which it was consumed, if it was.
    pub consumed_at: Option<u32>,
}

/// Compact view returned by read-only queries that do not need the full record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestationStatus {
    pub exists: bool,
    pub valid: bool,
    pub consumed: bool,
    pub expired: bool,
    pub risk_score: u32,
    pub policy_version: u32,
}

// ---------------------------------------------------------------------------
// Roles & configuration
// ---------------------------------------------------------------------------

/// Capabilities that can be granted independently of the root admin.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// Full control, including role management and upgrades.
    Admin,
    /// May publish and activate safety policies.
    PolicyAdmin,
    /// May register, rotate, freeze and disable circuits.
    CircuitAdmin,
    /// May pause the contract (but not unpause — that stays with Admin).
    Pauser,
    /// A lifecycle contract permitted to consume attestations.
    Consumer,
}

/// Instance-level configuration, set at initialization and tunable by Admin.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Ledgers a verifying-key rotation must wait before it can be finalized.
    pub rotation_timelock: u32,
    /// How long spent nullifiers are retained before they may be pruned.
    /// Must exceed the maximum proof validity window or replay becomes
    /// possible across the gap.
    pub nullifier_ttl: u32,
    /// Ledgers an unconsumed attestation is retained.
    pub attestation_ttl: u32,
    /// Hard cap on proofs per batch call.
    pub max_batch_size: u32,
    /// When true, `verify_metadata` requires `issuer.require_auth()`.
    pub require_issuer_auth: bool,
}

/// Aggregate counters, useful for dashboards and for detecting anomalies.
///
/// There is deliberately no "rejected" counter. A Soroban invocation that
/// returns an error rolls back every state change it made, so a counter
/// incremented on the failure path would always read zero — and an event
/// emitted there would never be published. Rejection telemetry has to come
/// from the transaction's own error code, which the ledger records anyway.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Stats {
    pub proofs_verified: u64,
    pub attestations_issued: u64,
    pub attestations_consumed: u64,
    pub nullifiers_spent: u64,
    pub batches_verified: u64,
}

impl Stats {
    /// All counters at zero. Equivalent to `Stats::default()`; named for
    /// readability at the initialization call site.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Result of a batch verification, reported per-proof when the aggregate check
/// fails and a fallback individual pass is requested.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchOutcome {
    /// True when the aggregate pairing check succeeded outright.
    pub aggregate_ok: bool,
    /// Number of proofs in the batch.
    pub size: u32,
    /// Indices that failed, populated only when a fallback pass ran.
    pub failed: Vec<u32>,
}

/// Helper: build an empty [`BatchOutcome`].
pub fn empty_outcome(env: &Env, size: u32, aggregate_ok: bool) -> BatchOutcome {
    BatchOutcome {
        aggregate_ok,
        size,
        failed: Vec::new(env),
    }
}
