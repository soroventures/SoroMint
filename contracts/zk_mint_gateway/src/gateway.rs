//! Token creation and minting, gated on a ZK safety attestation.

use soroban_sdk::{contract, contractimpl, xdr::ToXdr, Address, Bytes, BytesN, Env, Vec};

use crate::errors::Error;
use crate::events;
use crate::storage;
use crate::types::{Digest, GatewayConfig, IssuerQuota, TokenParams, TokenRecord};
use crate::verifier::VerifierClient;

/// Domain tag for the metadata commitment preimage.
const COMMITMENT_DST: &[u8] = b"SOROMINT-ZK-TOKEN-METADATA-V1";

/// Default tokens an issuer may create per window.
pub const DEFAULT_TOKENS_PER_WINDOW: u32 = 5;
/// Default window length (~1 day at 5-second ledgers).
pub const DEFAULT_WINDOW_LEDGERS: u32 = 17_280;

#[contract]
pub struct ZkMintGateway;

#[contractimpl]
impl ZkMintGateway {
    /// One-time setup binding this gateway to a verifier instance.
    pub fn initialize(env: Env, admin: Address, verifier: Address) -> Result<(), Error> {
        if storage::is_initialized(&env) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();

        storage::set_admin(&env, &admin);
        storage::set_config(
            &env,
            &GatewayConfig {
                verifier: verifier.clone(),
                tokens_per_window: DEFAULT_TOKENS_PER_WINDOW,
                window_ledgers: DEFAULT_WINDOW_LEDGERS,
            },
        );
        storage::set_paused(&env, false);
        storage::mark_initialized(&env);
        storage::bump_instance(&env);

        events::initialized(&env, &admin, &verifier);
        Ok(())
    }

    pub fn set_config(env: Env, caller: Address, config: GatewayConfig) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        storage::require_admin(&env, &caller)?;
        if config.window_ledgers == 0 {
            return Err(Error::InvalidTokenParams);
        }
        storage::set_config(&env, &config);
        events::config_updated(&env, &caller);
        Ok(())
    }

    pub fn pause(env: Env, caller: Address) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        storage::require_admin(&env, &caller)?;
        if storage::is_paused(&env) {
            return Err(Error::Paused);
        }
        storage::set_paused(&env, true);
        events::paused(&env, &caller);
        Ok(())
    }

    pub fn unpause(env: Env, caller: Address) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        storage::require_admin(&env, &caller)?;
        if !storage::is_paused(&env) {
            return Err(Error::NotPaused);
        }
        storage::set_paused(&env, false);
        events::unpaused(&env, &caller);
        Ok(())
    }

    /// Create a token by redeeming a safety attestation.
    ///
    /// Three things must line up, and each guards a distinct hole:
    ///
    /// 1. **The commitment matches the parameters.** Recomputing the hash from
    ///    `params` and comparing it to the attestation is what stops an issuer
    ///    from proving a bland payload safe and then deploying a different one.
    /// 2. **The verifier consumes the attestation.** That cross-contract call
    ///    marks it spent, so one proof yields one token.
    /// 3. **The issuer is within quota.** A valid proof is not an unlimited
    ///    deployment licence.
    pub fn create_token(
        env: Env,
        issuer: Address,
        params: TokenParams,
        metadata_commitment: Digest,
    ) -> Result<TokenRecord, Error> {
        storage::require_initialized(&env)?;
        storage::require_not_paused(&env)?;
        issuer.require_auth();
        storage::bump_instance(&env);

        validate_params(&params)?;

        if compute_commitment(&env, &issuer, &params) != metadata_commitment {
            return Err(Error::CommitmentMismatch);
        }
        if storage::has_token(&env, &metadata_commitment) {
            return Err(Error::TokenAlreadyCreated);
        }

        let config = storage::get_config(&env)?;
        charge_quota(&env, &issuer, &config)?;

        // Redeeming through the verifier is what makes the attestation
        // single-use; the gateway never decides that for itself.
        let verifier = VerifierClient::new(&env, &config.verifier);
        let attestation = verifier.consume_attestation(
            &env.current_contract_address(),
            &metadata_commitment,
            &issuer,
        );

        let record = TokenRecord {
            metadata_commitment: metadata_commitment.clone(),
            issuer: issuer.clone(),
            params,
            minted: 0,
            created_at: env.ledger().sequence(),
            policy_version: attestation.policy_version,
            risk_score: attestation.risk_score,
            frozen: false,
        };

        storage::set_token(&env, &record);
        storage::push_token_index(&env, &metadata_commitment);
        events::token_created(&env, &metadata_commitment, &issuer, record.risk_score);

        Ok(record)
    }

    /// Mint supply against an already-created token.
    ///
    /// No proof is needed here: the metadata was screened once at creation and
    /// has not changed. What is enforced is the supply cap the issuer committed
    /// to inside the circuit — so the cap is as unforgeable as the name was.
    pub fn mint(
        env: Env,
        issuer: Address,
        metadata_commitment: Digest,
        to: Address,
        amount: i128,
    ) -> Result<i128, Error> {
        storage::require_initialized(&env)?;
        storage::require_not_paused(&env)?;
        issuer.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut record = storage::get_token(&env, &metadata_commitment)?;
        if record.issuer != issuer {
            return Err(Error::NotTokenIssuer);
        }
        if record.frozen {
            return Err(Error::TokenFrozen);
        }

        let total = record
            .minted
            .checked_add(amount)
            .ok_or(Error::SupplyCapExceeded)?;
        if total > record.params.supply_cap {
            return Err(Error::SupplyCapExceeded);
        }

        record.minted = total;
        storage::set_token(&env, &record);
        events::minted(&env, &metadata_commitment, &to, amount, total);

        Ok(total)
    }

    /// Freeze or unfreeze a token. Admin only.
    pub fn set_token_frozen(
        env: Env,
        caller: Address,
        metadata_commitment: Digest,
        frozen: bool,
    ) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        storage::require_admin(&env, &caller)?;

        let mut record = storage::get_token(&env, &metadata_commitment)?;
        record.frozen = frozen;
        storage::set_token(&env, &record);
        events::token_frozen(&env, &metadata_commitment, frozen, &caller);
        Ok(())
    }

    // ----------------------------------------------------------------- reads

    pub fn get_token(env: Env, metadata_commitment: Digest) -> Result<TokenRecord, Error> {
        storage::get_token(&env, &metadata_commitment)
    }

    pub fn list_tokens(env: Env) -> Vec<Digest> {
        storage::token_index(&env)
    }

    pub fn get_config(env: Env) -> Result<GatewayConfig, Error> {
        storage::get_config(&env)
    }

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env)
    }

    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    pub fn get_quota(env: Env, issuer: Address) -> Option<IssuerQuota> {
        storage::get_quota(&env, &issuer)
    }

    /// Recompute the metadata commitment.
    ///
    /// Provers call this to obtain the exact value to witness the circuit
    /// with, rather than reimplementing the domain-separated encoding off
    /// chain and discovering the mismatch only at redemption time.
    pub fn compute_commitment(env: Env, issuer: Address, params: TokenParams) -> Digest {
        compute_commitment(&env, &issuer, &params)
    }
}

/// `SHA-256(dst || issuer || name || symbol || decimals || cap || uri)`,
/// truncated into the scalar field.
///
/// Length prefixes matter: without them `("ab", "c")` and `("a", "bc")` would
/// hash identically, letting an issuer swap a name fragment into the symbol
/// while keeping the commitment intact.
///
/// The top byte is cleared so the digest is a canonical `Fr` element, matching
/// what the circuit can accept as a public signal.
fn compute_commitment(env: &Env, issuer: &Address, params: &TokenParams) -> Digest {
    let mut buf = Bytes::new(env);
    buf.extend_from_slice(COMMITMENT_DST);
    append_field(&mut buf, &issuer.clone().to_xdr(env));
    append_field(&mut buf, &params.name.clone().to_xdr(env));
    append_field(&mut buf, &params.symbol.clone().to_xdr(env));
    buf.extend_from_array(&params.decimals.to_be_bytes());
    buf.extend_from_array(&params.supply_cap.to_be_bytes());
    append_field(&mut buf, &params.metadata_uri.clone().to_xdr(env));

    let digest = env.crypto().sha256(&buf);
    let mut arr = digest.to_array();
    arr[0] = 0;
    BytesN::from_array(env, &arr)
}

fn append_field(buf: &mut Bytes, field: &Bytes) {
    buf.extend_from_array(&field.len().to_be_bytes());
    buf.append(field);
}

fn validate_params(params: &TokenParams) -> Result<(), Error> {
    if params.name.is_empty() || params.symbol.is_empty() {
        return Err(Error::InvalidTokenParams);
    }
    if params.decimals > 18 {
        return Err(Error::InvalidTokenParams);
    }
    if params.supply_cap <= 0 {
        return Err(Error::InvalidTokenParams);
    }
    Ok(())
}

/// Consume one unit of the issuer's rolling allowance.
fn charge_quota(env: &Env, issuer: &Address, config: &GatewayConfig) -> Result<(), Error> {
    let now = env.ledger().sequence();
    let mut quota = storage::get_quota(env, issuer).unwrap_or(IssuerQuota {
        window_start: now,
        used: 0,
    });

    if now.saturating_sub(quota.window_start) >= config.window_ledgers {
        quota.window_start = now;
        quota.used = 0;
    }
    if quota.used >= config.tokens_per_window {
        return Err(Error::QuotaExceeded);
    }

    quota.used += 1;
    storage::set_quota(env, issuer, &quota);
    Ok(())
}
