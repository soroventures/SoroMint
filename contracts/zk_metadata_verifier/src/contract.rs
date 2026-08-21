//! The contract surface.
//!
//! Entrypoints are grouped as: lifecycle, roles, circuit registry, policy,
//! verification, attestation, and queries. The verification group is the
//! interesting one; everything else exists to make it governable.

use soroban_sdk::{contract, contractimpl, Address, Env, String, Symbol, Vec};

use crate::access;
use crate::attestation;
use crate::errors::Error;
use crate::events;
use crate::field::checked_scalar;
use crate::groth16::{self, MAX_BATCH_SIZE};
use crate::policy;
use crate::registry;
use crate::storage;
use crate::types::{
    Attestation, AttestationStatus, BatchOutcome, CircuitRecord, Config, Digest, Groth16Proof,
    MetadataSignals, PendingRotation, Policy, PolicyParams, Role, ScalarBytes, Stats, VerifyingKey,
};

/// Default ledgers a verifying-key rotation must age before activation
/// (~1 day at 5-second ledgers).
pub const DEFAULT_ROTATION_TIMELOCK: u32 = 17_280;
/// Default nullifier retention (~30 days).
pub const DEFAULT_NULLIFIER_TTL: u32 = 518_400;
/// Default attestation retention (~7 days).
pub const DEFAULT_ATTESTATION_TTL: u32 = 120_960;

#[contract]
pub struct ZkMetadataVerifier;

#[contractimpl]
impl ZkMetadataVerifier {
    // =======================================================================
    // Lifecycle
    // =======================================================================

    /// One-time setup. `admin` becomes the root administrator.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if storage::is_initialized(&env) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();

        storage::set_admin(&env, &admin);
        storage::set_config(
            &env,
            &Config {
                rotation_timelock: DEFAULT_ROTATION_TIMELOCK,
                nullifier_ttl: DEFAULT_NULLIFIER_TTL,
                attestation_ttl: DEFAULT_ATTESTATION_TTL,
                max_batch_size: MAX_BATCH_SIZE,
                require_issuer_auth: true,
            },
        );
        storage::set_stats(&env, &Stats::new());
        storage::set_paused(&env, false);
        storage::mark_initialized(&env);
        storage::bump_instance(&env);

        events::initialized(&env, &admin);
        Ok(())
    }

    /// Replace the tunable configuration. Admin only.
    ///
    /// Rejects a configuration where nullifiers would expire before the
    /// attestations they protect — that ordering is the invariant the whole
    /// replay defence rests on.
    pub fn set_config(env: Env, caller: Address, config: Config) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        access::require_admin(&env, &caller)?;

        if config.max_batch_size == 0 || config.max_batch_size > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }
        if config.nullifier_ttl <= config.attestation_ttl {
            return Err(Error::InvalidPolicyParameters);
        }

        storage::set_config(&env, &config);
        storage::bump_instance(&env);
        events::config_updated(&env, &caller);
        Ok(())
    }

    /// Halt verification. Holders of `Pauser` or the admin may call this.
    pub fn pause(env: Env, caller: Address) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        access::require_role(&env, &caller, Role::Pauser)?;
        if storage::is_paused(&env) {
            return Err(Error::Paused);
        }
        storage::set_paused(&env, true);
        events::paused(&env, &caller);
        Ok(())
    }

    /// Resume verification. Admin only — deliberately narrower than `pause`,
    /// so an on-call responder can stop the bleeding without also holding the
    /// key that restarts it.
    pub fn unpause(env: Env, caller: Address) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        access::require_admin(&env, &caller)?;
        if !storage::is_paused(&env) {
            return Err(Error::NotPaused);
        }
        storage::set_paused(&env, false);
        events::unpaused(&env, &caller);
        Ok(())
    }

    // =======================================================================
    // Roles
    // =======================================================================

    pub fn grant_role(env: Env, caller: Address, role: Role, who: Address) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        access::grant(&env, &caller, role, &who)
    }

    pub fn revoke_role(env: Env, caller: Address, role: Role, who: Address) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        access::revoke(&env, &caller, role, &who)
    }

    pub fn transfer_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        access::transfer_admin(&env, &caller, &new_admin)
    }

    pub fn accept_admin(env: Env, caller: Address) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        access::accept_admin(&env, &caller)
    }

    pub fn cancel_admin_transfer(env: Env, caller: Address) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        access::cancel_admin_transfer(&env, &caller)
    }

    /// Authorize a lifecycle contract to redeem attestations.
    pub fn register_consumer(env: Env, caller: Address, consumer: Address) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        access::require_admin(&env, &caller)?;
        if storage::has_role(&env, Role::Consumer, &consumer) {
            return Err(Error::ConsumerAlreadyRegistered);
        }
        storage::grant_role(&env, Role::Consumer, &consumer);
        events::consumer_registered(&env, &consumer, &caller);
        Ok(())
    }

    // =======================================================================
    // Circuit registry
    // =======================================================================

    pub fn register_circuit(
        env: Env,
        caller: Address,
        circuit_id: Symbol,
        vk: VerifyingKey,
        num_public_inputs: u32,
        provenance: String,
    ) -> Result<Digest, Error> {
        storage::require_initialized(&env)?;
        storage::bump_instance(&env);
        let record = registry::register_circuit(
            &env,
            &caller,
            circuit_id,
            vk,
            num_public_inputs,
            provenance,
        )?;
        Ok(record.vk_digest)
    }

    pub fn propose_rotation(
        env: Env,
        caller: Address,
        circuit_id: Symbol,
        vk: VerifyingKey,
        num_public_inputs: u32,
    ) -> Result<u32, Error> {
        storage::require_initialized(&env)?;
        storage::bump_instance(&env);
        let rotation =
            registry::propose_rotation(&env, &caller, circuit_id, vk, num_public_inputs)?;
        Ok(rotation.eta)
    }

    pub fn cancel_rotation(env: Env, caller: Address, circuit_id: Symbol) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        registry::cancel_rotation(&env, &caller, circuit_id)
    }

    pub fn finalize_rotation(
        env: Env,
        caller: Address,
        circuit_id: Symbol,
    ) -> Result<Digest, Error> {
        storage::require_initialized(&env)?;
        storage::bump_instance(&env);
        let record = registry::finalize_rotation(&env, &caller, circuit_id)?;
        Ok(record.vk_digest)
    }

    pub fn set_circuit_enabled(
        env: Env,
        caller: Address,
        circuit_id: Symbol,
        enabled: bool,
    ) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        registry::set_circuit_enabled(&env, &caller, circuit_id, enabled)
    }

    pub fn freeze_circuit(env: Env, caller: Address, circuit_id: Symbol) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        registry::freeze_circuit(&env, &caller, circuit_id)
    }

    // =======================================================================
    // Policy
    // =======================================================================

    pub fn publish_policy(env: Env, caller: Address, params: PolicyParams) -> Result<u32, Error> {
        storage::require_initialized(&env)?;
        storage::bump_instance(&env);
        let p = policy::publish_policy(&env, &caller, params)?;
        Ok(p.version)
    }

    pub fn activate_policy(env: Env, caller: Address, version: u32) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        storage::bump_instance(&env);
        policy::activate_policy(&env, &caller, version)?;
        Ok(())
    }

    // =======================================================================
    // Verification
    // =======================================================================

    /// Stateless Groth16 check against a registered circuit.
    ///
    /// Useful for circuits other than the metadata validator, and for clients
    /// that want to dry-run a proof before paying for the stateful path. It
    /// writes nothing, spends no nullifier, and issues no attestation.
    pub fn verify_proof(
        env: Env,
        circuit_id: Symbol,
        proof: Groth16Proof,
        public_inputs: Vec<ScalarBytes>,
    ) -> Result<bool, Error> {
        storage::require_initialized(&env)?;
        storage::require_not_paused(&env)?;

        let record = registry::active_circuit(&env, &circuit_id)?;
        if public_inputs.len() != record.num_public_inputs {
            return Err(Error::PublicInputCountMismatch);
        }

        groth16::verify_proof(&env, &record.vk, &proof, &public_inputs)?;
        events::proof_verified(&env, &circuit_id, public_inputs.len());
        Ok(true)
    }

    /// Non-throwing flavour of [`Self::verify_proof`], for simulation.
    pub fn check_proof(
        env: Env,
        circuit_id: Symbol,
        proof: Groth16Proof,
        public_inputs: Vec<ScalarBytes>,
    ) -> bool {
        match registry::active_circuit(&env, &circuit_id) {
            Ok(record) => {
                record.num_public_inputs == public_inputs.len()
                    && groth16::is_valid_proof(&env, &record.vk, &proof, &public_inputs)
            }
            Err(_) => false,
        }
    }

    /// The main entrypoint: verify an AI safety proof and issue an attestation.
    ///
    /// Ordering is deliberate and each step is a guard against a specific
    /// attack:
    ///
    /// 1. **Policy checks** — cheap integer comparisons, and they bind the
    ///    proof to the rules currently in force. Running them first means a
    ///    proof against a retired policy costs the submitter a few hundred
    ///    instructions rather than a full pairing.
    /// 2. **Nullifier check** — before the pairing, so a replayed proof is
    ///    rejected cheaply.
    /// 3. **Pairing check** — the expensive part, reached only by a submission
    ///    that is fresh and policy-conformant.
    /// 4. **Spend and attest** — state writes last, so a failure anywhere above
    ///    leaves no partial state.
    pub fn verify_metadata(
        env: Env,
        issuer: Address,
        proof: Groth16Proof,
        signals: MetadataSignals,
    ) -> Result<Attestation, Error> {
        storage::require_initialized(&env)?;
        storage::require_not_paused(&env)?;
        storage::bump_instance(&env);

        let config = storage::get_config(&env)?;
        if config.require_issuer_auth {
            issuer.require_auth();
        }

        let active = storage::active_policy(&env)?;
        let mut record = registry::active_circuit(&env, &active.circuit_id)?;

        // (1) policy semantics
        policy::check_signals(&env, &active, &signals, &issuer)?;

        // (2) replay
        attestation::require_unspent(&env, &signals.nullifier)?;

        // (3) the pairing
        let public_inputs = policy::signals_to_public_inputs(&env, &signals);
        if public_inputs.len() != record.num_public_inputs {
            return Err(Error::PublicInputCountMismatch);
        }
        // No bookkeeping on the failure path: returning `Err` rolls the whole
        // invocation back, so a counter bump or event here would vanish with
        // it. See the note on [`Stats`].
        groth16::verify_proof(&env, &record.vk, &proof, &public_inputs)?;

        // (4) state
        attestation::spend(&env, &signals.nullifier, &issuer, &config);
        let att = attestation::issue(
            &env,
            &signals,
            &issuer,
            &active.circuit_id,
            active.version,
            &config,
        );

        registry::note_verification(&env, &mut record);
        storage::with_stats(&env, |s| {
            s.proofs_verified = s.proofs_verified.saturating_add(1)
        });
        events::proof_verified(&env, &active.circuit_id, public_inputs.len());

        Ok(att)
    }

    /// Verify many proofs for one circuit with `n + 3` pairings.
    ///
    /// Stateless, like [`Self::verify_proof`]: batching is an optimization for
    /// bulk auditing, not a path to bulk attestation. Issuing attestations in
    /// bulk would require spending `n` nullifiers atomically, and a single
    /// stale member would revert the whole batch — worse UX than `n` separate
    /// calls, for no saving.
    pub fn verify_batch(
        env: Env,
        circuit_id: Symbol,
        proofs: Vec<Groth16Proof>,
        inputs: Vec<Vec<ScalarBytes>>,
    ) -> Result<bool, Error> {
        storage::require_initialized(&env)?;
        storage::require_not_paused(&env)?;

        let config = storage::get_config(&env)?;
        let record = registry::active_circuit(&env, &circuit_id)?;

        groth16::verify_batch(
            &env,
            &record.vk,
            &record.vk_digest,
            &proofs,
            &inputs,
            config.max_batch_size,
        )?;

        storage::with_stats(&env, |s| {
            s.batches_verified = s.batches_verified.saturating_add(1);
            s.proofs_verified = s.proofs_verified.saturating_add(proofs.len() as u64);
        });
        events::batch_verified(&env, &circuit_id, proofs.len());
        Ok(true)
    }

    /// Run the batch check and, if it fails, attribute the failure.
    ///
    /// Separated from [`Self::verify_batch`] because the fallback pass costs a
    /// full `4n` pairings. A caller that only needs a yes/no should not pay for
    /// blame attribution it will not read.
    pub fn diagnose_batch(
        env: Env,
        circuit_id: Symbol,
        proofs: Vec<Groth16Proof>,
        inputs: Vec<Vec<ScalarBytes>>,
    ) -> Result<BatchOutcome, Error> {
        storage::require_initialized(&env)?;
        let config = storage::get_config(&env)?;
        let record = registry::active_circuit(&env, &circuit_id)?;

        let size = proofs.len();
        let aggregate = groth16::verify_batch(
            &env,
            &record.vk,
            &record.vk_digest,
            &proofs,
            &inputs,
            config.max_batch_size,
        );

        match aggregate {
            Ok(()) => Ok(crate::types::empty_outcome(&env, size, true)),
            Err(Error::EmptyBatch) => Err(Error::EmptyBatch),
            Err(Error::BatchTooLarge) => Err(Error::BatchTooLarge),
            Err(Error::BatchLengthMismatch) => Err(Error::BatchLengthMismatch),
            Err(_) => Ok(BatchOutcome {
                aggregate_ok: false,
                size,
                failed: groth16::locate_batch_failures(&env, &record.vk, &proofs, &inputs),
            }),
        }
    }

    // =======================================================================
    // Attestations
    // =======================================================================

    /// Redeem an attestation. Only registered consumer contracts may call this.
    pub fn consume_attestation(
        env: Env,
        consumer: Address,
        metadata_commitment: Digest,
        issuer: Address,
    ) -> Result<Attestation, Error> {
        storage::require_initialized(&env)?;
        storage::require_not_paused(&env)?;
        consumer.require_auth();

        if !storage::has_role(&env, Role::Consumer, &consumer) {
            return Err(Error::NotAuthorizedConsumer);
        }
        attestation::consume(&env, &consumer, &metadata_commitment, &issuer)
    }

    /// Invalidate an attestation. Policy admin or root admin.
    pub fn revoke_attestation(
        env: Env,
        caller: Address,
        metadata_commitment: Digest,
        reason: u32,
    ) -> Result<(), Error> {
        storage::require_initialized(&env)?;
        access::require_role(&env, &caller, Role::PolicyAdmin)?;
        attestation::revoke(&env, &caller, &metadata_commitment, reason)
    }

    // =======================================================================
    // Queries
    // =======================================================================

    pub fn get_attestation(env: Env, metadata_commitment: Digest) -> Result<Attestation, Error> {
        storage::get_attestation(&env, &metadata_commitment)
    }

    pub fn attestation_status(env: Env, metadata_commitment: Digest) -> AttestationStatus {
        attestation::status(&env, &metadata_commitment)
    }

    pub fn is_nullifier_spent(env: Env, nullifier: Digest) -> bool {
        storage::nullifier_spent_at(&env, &nullifier).is_some()
    }

    pub fn get_circuit(env: Env, circuit_id: Symbol) -> Result<CircuitRecord, Error> {
        storage::get_circuit(&env, &circuit_id)
    }

    pub fn list_circuits(env: Env) -> Vec<Symbol> {
        storage::circuit_index(&env)
    }

    pub fn get_rotation(env: Env, circuit_id: Symbol) -> Result<PendingRotation, Error> {
        storage::get_rotation(&env, &circuit_id)
    }

    pub fn get_policy(env: Env, version: u32) -> Result<Policy, Error> {
        storage::get_policy(&env, version)
    }

    pub fn get_active_policy(env: Env) -> Result<Policy, Error> {
        storage::active_policy(&env)
    }

    pub fn list_policies(env: Env) -> Vec<u32> {
        storage::policy_index(&env)
    }

    pub fn get_config(env: Env) -> Result<Config, Error> {
        storage::get_config(&env)
    }

    pub fn get_stats(env: Env) -> Stats {
        storage::get_stats(&env)
    }

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        storage::get_admin(&env)
    }

    pub fn has_role(env: Env, role: Role, who: Address) -> bool {
        access::check_role(&env, &who, role)
    }

    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    pub fn issuer_attestation_count(env: Env, issuer: Address) -> u64 {
        storage::issuer_count(&env, &issuer)
    }

    /// Recompute the issuer binding hash. Provers call this to learn the value
    /// the circuit must be witnessed with, rather than reimplementing the
    /// domain-separated hash off chain and getting it subtly wrong.
    pub fn compute_issuer_hash(env: Env, issuer: Address) -> Digest {
        policy::issuer_hash(&env, &issuer)
    }

    /// Fingerprint a verifying key without registering it, so an operator can
    /// diff against their ceremony output before proposing a rotation.
    pub fn compute_vk_digest(env: Env, vk: VerifyingKey, num_public_inputs: u32) -> Digest {
        groth16::verifying_key_digest(&env, &vk, num_public_inputs)
    }

    /// Public-input encoding, exposed so clients can confirm the wire order the
    /// contract expects matches the one their prover produced.
    pub fn encode_signals(env: Env, signals: MetadataSignals) -> Vec<ScalarBytes> {
        policy::signals_to_public_inputs(&env, &signals)
    }

    /// True when a 32-byte string is a canonical `Fr` element.
    pub fn is_canonical_scalar(_env: Env, scalar: ScalarBytes) -> bool {
        checked_scalar(&scalar).is_ok()
    }
}
