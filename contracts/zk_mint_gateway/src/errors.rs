//! Gateway errors. Codes start at 200 so they never collide with the
//! verifier's, which propagate through cross-contract calls unchanged.

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 200,
    NotInitialized = 201,
    Unauthorized = 202,
    Paused = 203,
    NotPaused = 204,
    /// The metadata commitment does not match the token parameters supplied.
    CommitmentMismatch = 210,
    /// A token has already been created from this attestation.
    TokenAlreadyCreated = 211,
    /// No token is registered under this commitment.
    TokenNotFound = 212,
    /// The caller is not the issuer of record for this token.
    NotTokenIssuer = 213,
    /// Minting would exceed the supply cap the metadata committed to.
    SupplyCapExceeded = 214,
    /// The issuer has used up their allowance for the current window.
    QuotaExceeded = 215,
    /// Token parameters failed basic structural validation.
    InvalidTokenParams = 216,
    /// The mint amount was zero or negative.
    InvalidAmount = 217,
    /// The token has been administratively frozen.
    TokenFrozen = 218,
}
