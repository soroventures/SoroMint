//! Error taxonomy for the ZK metadata verifier.
//!
//! Codes are grouped by subsystem so that off-chain tooling can route failures
//! without string matching:
//!
//! | Range       | Subsystem                          |
//! |-------------|------------------------------------|
//! | `1..=19`    | Lifecycle / access control         |
//! | `20..=39`   | Circuit & verifying-key registry   |
//! | `40..=59`   | Proof encoding & curve validation  |
//! | `60..=79`   | Public-signal / policy semantics   |
//! | `80..=99`   | Replay protection & attestations   |
//! | `100..=119` | Batch verification                 |

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    // ---------------------------------------------------------------- 1..19
    /// `initialize` called on an already-initialized instance.
    AlreadyInitialized = 1,
    /// A state-touching entrypoint was called before `initialize`.
    NotInitialized = 2,
    /// Caller does not hold the role required for this entrypoint.
    Unauthorized = 3,
    /// The contract is paused; only admin recovery paths remain open.
    Paused = 4,
    /// The contract is not paused, so `unpause` is a no-op.
    NotPaused = 5,
    /// Role handoff was attempted to an invalid target.
    InvalidRoleTarget = 6,
    /// Attempted to revoke the last remaining admin.
    LastAdmin = 7,
    /// A pending admin transfer does not exist or belongs to another address.
    NoPendingTransfer = 8,

    // --------------------------------------------------------------- 20..39
    /// No verifying key is registered under this circuit id.
    CircuitNotFound = 20,
    /// A circuit with this id already exists; use `rotate_verifying_key`.
    CircuitAlreadyExists = 21,
    /// The circuit has been frozen and can no longer be rotated.
    CircuitFrozen = 22,
    /// The circuit is registered but administratively disabled.
    CircuitDisabled = 23,
    /// `ic.len()` must equal `num_public_inputs + 1`.
    MalformedVerifyingKey = 24,
    /// The declared public-input arity is outside the supported bounds.
    UnsupportedArity = 25,
    /// Rotation supplied a key identical to the active one.
    VerifyingKeyUnchanged = 26,
    /// The rotation timelock has not yet elapsed.
    RotationTimelockActive = 27,
    /// No verifying-key rotation is currently pending.
    NoPendingRotation = 28,

    // --------------------------------------------------------------- 40..59
    /// A G1 point failed decoding or the subgroup check.
    InvalidG1Point = 40,
    /// A G2 point failed decoding or the subgroup check.
    InvalidG2Point = 41,
    /// A scalar was not a canonical field element (>= r).
    NonCanonicalScalar = 42,
    /// The number of supplied public inputs disagrees with the circuit.
    PublicInputCountMismatch = 43,
    /// The pairing equation did not hold: the proof is invalid.
    ProofVerificationFailed = 44,
    /// A proof component decoded to the point at infinity where forbidden.
    PointAtInfinity = 45,

    // --------------------------------------------------------------- 60..79
    /// No policy has been published yet.
    PolicyNotFound = 60,
    /// The referenced policy version is not the active one.
    PolicyNotActive = 61,
    /// The proof commits to a policy root the contract does not recognise.
    PolicyRootMismatch = 62,
    /// The proof commits to an unapproved scoring-model commitment.
    ModelCommitmentMismatch = 63,
    /// `risk_score` exceeds the policy's tolerated maximum.
    RiskScoreTooHigh = 64,
    /// The circuit's boolean verdict signal was not `1` (safe).
    VerdictRejected = 65,
    /// The attestation's `expiry_ledger` is in the past.
    ProofExpired = 66,
    /// `expiry_ledger` is further out than the policy's maximum validity.
    ExpiryTooDistant = 67,
    /// The proof was bound to a different issuer than the caller.
    IssuerBindingMismatch = 68,
    /// A published policy carried nonsensical parameters.
    InvalidPolicyParameters = 69,
    /// The policy version supplied is not newer than the active one.
    PolicyVersionRegression = 70,

    // --------------------------------------------------------------- 80..99
    /// The nullifier has already been consumed: this is a replay.
    NullifierAlreadyUsed = 80,
    /// No attestation exists for the supplied metadata commitment.
    AttestationNotFound = 81,
    /// The attestation has already been consumed by the mint lifecycle.
    AttestationAlreadyConsumed = 82,
    /// The attestation has passed its expiry ledger.
    AttestationExpired = 83,
    /// Only a registered consumer contract may consume attestations.
    NotAuthorizedConsumer = 84,
    /// The consumer address is already registered.
    ConsumerAlreadyRegistered = 85,
    /// The metadata commitment does not match the attestation on record.
    CommitmentMismatch = 86,

    // ------------------------------------------------------------- 100..119
    /// A batch must contain at least one proof.
    EmptyBatch = 100,
    /// The batch exceeds `MAX_BATCH_SIZE`.
    BatchTooLarge = 101,
    /// Proof and public-input vectors have different lengths.
    BatchLengthMismatch = 102,
    /// Aggregate pairing check failed; at least one proof is invalid.
    BatchVerificationFailed = 103,
}
