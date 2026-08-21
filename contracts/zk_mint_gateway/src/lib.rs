#![no_std]
//! # SoroMint ZK Mint Gateway
//!
//! The lifecycle half of the ZK metadata pipeline. Issuers cannot create a
//! token or mint supply through this contract without a valid AI safety
//! attestation from [`soromint_zk_metadata_verifier`].
//!
//! ## Why the gateway is separate from the verifier
//!
//! The verifier answers one question — "is this proof valid under the active
//! policy?" — and nothing else. Keeping token deployment out of it means the
//! cryptographic core can be frozen and audited on its own, while the lifecycle
//! rules (quotas, which factory to call, what counts as a mint) stay mutable.
//!
//! ## The two-call shape
//!
//! `verify_metadata` on the verifier, then `create_token` here. Splitting them
//! is what makes the gas fit: a Groth16 verification costs roughly 56M CPU
//! instructions against Soroban's 100M transaction budget, leaving too little
//! headroom to also deploy a contract in the same invocation. The attestation
//! is the durable handoff between the two transactions.

mod errors;
mod events;
mod gateway;
mod storage;
mod types;
mod verifier;

pub use errors::Error;
pub use gateway::{ZkMintGateway, ZkMintGatewayClient};
pub use types::{GatewayConfig, IssuerQuota, TokenParams, TokenRecord};
pub use verifier::{Attestation, VerifierClient, VerifierInterface};

#[cfg(test)]
mod test;
