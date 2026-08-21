//! Gateway data model.

use soroban_sdk::{contracttype, Address, BytesN, String};

/// 32-byte commitment, matching the verifier's `Digest`.
pub type Digest = BytesN<32>;

/// The token parameters an issuer commits to inside the circuit.
///
/// These are hashed into `metadata_commitment`; the gateway recomputes that
/// hash and refuses to proceed unless it matches the attestation. Without that
/// step an issuer could prove a harmless payload safe and then deploy a
/// different one.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenParams {
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
    pub supply_cap: i128,
    pub metadata_uri: String,
}

/// A token created through the gateway.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenRecord {
    pub metadata_commitment: Digest,
    pub issuer: Address,
    pub params: TokenParams,
    pub minted: i128,
    pub created_at: u32,
    pub policy_version: u32,
    pub risk_score: u32,
    pub frozen: bool,
}

/// Per-issuer throughput limit.
///
/// A valid proof is not a licence to deploy unlimited tokens; quotas bound the
/// blast radius of a leaked prover key.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssuerQuota {
    pub window_start: u32,
    pub used: u32,
}

/// Gateway configuration.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GatewayConfig {
    /// Address of the deployed `ZkMetadataVerifier`.
    pub verifier: Address,
    /// Tokens an issuer may create per window.
    pub tokens_per_window: u32,
    /// Window length in ledgers.
    pub window_ledgers: u32,
}
