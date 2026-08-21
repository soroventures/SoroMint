#![no_std]
//! # SoroMint ZK Metadata Verifier
//!
//! On-chain Groth16 verification of off-chain AI metadata safety proofs.
//!
//! ## Why this exists
//!
//! The predecessor contract (`ai_metadata_validator`) evaluated token metadata
//! directly on chain: regex-ish scanning, substring search against a blocklist,
//! character-class tallies, heuristic scoring. Every one of those is linear in
//! the length of a caller-supplied string, executed inside a metered VM, and
//! the *entire rule set has to be public in contract storage* for it to run.
//! That design is expensive, it caps out well below anything resembling real
//! safety screening, and publishing the blocklist hands evasion instructions to
//! exactly the people it is meant to stop.
//!
//! Moving the computation off chain and verifying a proof of it fixes all
//! three at once. The classifier can be arbitrarily complex — its cost lands on
//! the prover, not the ledger. The rules stay private behind a Merkle
//! commitment. And on-chain cost becomes constant: four pairings, regardless of
//! whether the circuit checked ten rules or ten thousand.
//!
//! ## Architecture
//!
//! ```text
//!   off chain                          │  on chain
//!   ─────────────────────────────────  │  ────────────────────────────────
//!   metadata payload                   │
//!        │                             │
//!        ├─► AI classifier ──► score   │
//!        │                             │
//!        ├─► witness builder           │
//!        │        │                    │
//!        │        ▼                    │
//!        │   metadata_validator.circom │
//!        │        │                    │
//!        │        ▼                    │
//!        └─► Groth16 prover ──proof──► │  ZkMetadataVerifier
//!                                      │      ├─ policy checks   (cheap)
//!                                      │      ├─ nullifier check (cheap)
//!                                      │      ├─ pairing check   (≈4 pairings)
//!                                      │      └─ Attestation ──┐
//!                                      │                       ▼
//!                                      │              ZkMintGateway
//!                                      │              ├─ create_token
//!                                      │              └─ mint
//! ```
//!
//! ## Module map
//!
//! | Module              | Responsibility |
//! |---------------------|----------------|
//! | [`groth16`]         | Pairing verification, point decoding, batch folding |
//! | [`field`]           | Scalar canonicality, Fiat–Shamir transcript |
//! | [`registry`]        | Verifying keys and timelocked rotation |
//! | [`policy`]          | Safety thresholds and public-signal semantics |
//! | [`attestation`]     | Nullifiers and verification receipts |
//! | [`access`]          | Role-based control |
//! | [`storage`]         | Keys, durability tiers, TTLs |
//! | [`contract`]        | The entrypoints |
//!
//! ## Trust assumptions
//!
//! 1. **Trusted setup.** Groth16 needs a per-circuit CRS. A participant who
//!    retains the toxic waste can forge proofs for that circuit. The registry's
//!    published VK digests and rotation timelock exist so that a ceremony can
//!    be audited and a suspect key replaced under observation.
//! 2. **Model commitment.** The contract enforces that the prover used the
//!    committed model, not that the committed model is *good*. Model quality is
//!    a governance question, tracked through `Policy::version`.
//! 3. **Issuer binding.** A proof is bound to one issuer address. It says
//!    nothing about who *generated* it, only who may redeem it.

// `std` is linked for tests only. The wasm build stays allocator-free: nothing
// in the contract path needs a heap, and pulling one in would add code size to
// every deployment for the benefit of the test fixtures alone.
#[cfg(test)]
extern crate std;

pub mod access;
pub mod attestation;
pub mod contract;
pub mod errors;
pub mod events;
pub mod field;
pub mod groth16;
pub mod policy;
pub mod registry;
pub mod storage;
pub mod types;

pub use contract::{ZkMetadataVerifier, ZkMetadataVerifierClient};
pub use errors::Error;
pub use types::{
    Attestation, AttestationStatus, BatchOutcome, CircuitRecord, Config, Digest, Groth16Proof,
    MetadataSignals, PendingRotation, Policy, PolicyParams, Role, ScalarBytes, Stats, VerifyingKey,
};

#[cfg(test)]
mod test;
