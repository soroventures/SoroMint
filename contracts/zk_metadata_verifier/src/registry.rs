//! Verifying-key registry with timelocked rotation.
//!
//! A Groth16 verifying key is the entire trust anchor of the system: whoever
//! controls it decides what statements are provable. Swapping it must therefore
//! be *observable before it takes effect*, which is what the two-phase
//! propose/finalize flow buys. Between `propose_rotation` and
//! `finalize_rotation` the incoming key's digest is public on chain, giving
//! integrators a window to compare it against the key produced by the trusted
//! setup ceremony and to exit if it does not match.
//!
//! `freeze_circuit` is the terminal state: once frozen, a circuit's key can
//! never change again. That is the right end state for a circuit whose
//! ceremony transcript has been independently verified.

use soroban_sdk::{Address, Env, String, Symbol};

use crate::errors::Error;
use crate::events;
use crate::groth16::{validate_verifying_key, verifying_key_digest};
use crate::storage;
use crate::types::{CircuitRecord, PendingRotation, Role, VerifyingKey};

/// Register a brand-new circuit.
pub fn register_circuit(
    env: &Env,
    caller: &Address,
    circuit_id: Symbol,
    vk: VerifyingKey,
    num_public_inputs: u32,
    provenance: String,
) -> Result<CircuitRecord, Error> {
    crate::access::require_role(env, caller, Role::CircuitAdmin)?;

    if storage::has_circuit(env, &circuit_id) {
        return Err(Error::CircuitAlreadyExists);
    }

    validate_verifying_key(env, &vk, num_public_inputs)?;
    let vk_digest = verifying_key_digest(env, &vk, num_public_inputs);

    let record = CircuitRecord {
        circuit_id: circuit_id.clone(),
        vk,
        num_public_inputs,
        vk_digest: vk_digest.clone(),
        provenance,
        enabled: true,
        frozen: false,
        revision: 0,
        activated_at: env.ledger().sequence(),
        verifications: 0,
    };

    storage::set_circuit(env, &record);
    storage::push_circuit_index(env, &circuit_id);
    events::circuit_registered(env, &circuit_id, &vk_digest, num_public_inputs, caller);

    Ok(record)
}

/// Queue a replacement key. Takes effect no earlier than `now + timelock`.
pub fn propose_rotation(
    env: &Env,
    caller: &Address,
    circuit_id: Symbol,
    vk: VerifyingKey,
    num_public_inputs: u32,
) -> Result<PendingRotation, Error> {
    crate::access::require_role(env, caller, Role::CircuitAdmin)?;

    let record = storage::get_circuit(env, &circuit_id)?;
    if record.frozen {
        return Err(Error::CircuitFrozen);
    }

    validate_verifying_key(env, &vk, num_public_inputs)?;
    let vk_digest = verifying_key_digest(env, &vk, num_public_inputs);
    if vk_digest == record.vk_digest {
        return Err(Error::VerifyingKeyUnchanged);
    }

    let config = storage::get_config(env)?;
    let eta = env
        .ledger()
        .sequence()
        .saturating_add(config.rotation_timelock);

    let rotation = PendingRotation {
        circuit_id: circuit_id.clone(),
        vk,
        num_public_inputs,
        vk_digest: vk_digest.clone(),
        eta,
        proposer: caller.clone(),
    };

    storage::set_rotation(env, &rotation);
    events::rotation_proposed(env, &circuit_id, &vk_digest, eta, caller);

    Ok(rotation)
}

/// Withdraw a queued rotation before it activates.
pub fn cancel_rotation(env: &Env, caller: &Address, circuit_id: Symbol) -> Result<(), Error> {
    crate::access::require_role(env, caller, Role::CircuitAdmin)?;
    // Presence check first so cancelling nothing is an explicit error rather
    // than a silent success that an operator might mistake for a real cancel.
    storage::get_rotation(env, &circuit_id)?;
    storage::clear_rotation(env, &circuit_id);
    events::rotation_cancelled(env, &circuit_id, caller);
    Ok(())
}

/// Activate a queued rotation once its timelock has elapsed.
pub fn finalize_rotation(
    env: &Env,
    caller: &Address,
    circuit_id: Symbol,
) -> Result<CircuitRecord, Error> {
    crate::access::require_role(env, caller, Role::CircuitAdmin)?;

    let rotation = storage::get_rotation(env, &circuit_id)?;
    if env.ledger().sequence() < rotation.eta {
        return Err(Error::RotationTimelockActive);
    }

    let mut record = storage::get_circuit(env, &circuit_id)?;
    if record.frozen {
        return Err(Error::CircuitFrozen);
    }

    let old_digest = record.vk_digest.clone();
    record.vk = rotation.vk;
    record.num_public_inputs = rotation.num_public_inputs;
    record.vk_digest = rotation.vk_digest.clone();
    record.revision = record.revision.saturating_add(1);
    record.activated_at = env.ledger().sequence();

    storage::set_circuit(env, &record);
    storage::clear_rotation(env, &circuit_id);
    events::rotation_finalized(
        env,
        &circuit_id,
        &old_digest,
        &record.vk_digest,
        record.revision,
    );

    Ok(record)
}

/// Enable or disable a circuit without touching its key.
///
/// The escape hatch for "we found a bug in the circuit at 3am": disabling stops
/// new proofs immediately, without needing a replacement key ready.
pub fn set_circuit_enabled(
    env: &Env,
    caller: &Address,
    circuit_id: Symbol,
    enabled: bool,
) -> Result<(), Error> {
    crate::access::require_role(env, caller, Role::CircuitAdmin)?;
    let mut record = storage::get_circuit(env, &circuit_id)?;
    record.enabled = enabled;
    storage::set_circuit(env, &record);
    events::circuit_enabled(env, &circuit_id, enabled, caller);
    Ok(())
}

/// Permanently seal a circuit's verifying key.
pub fn freeze_circuit(env: &Env, caller: &Address, circuit_id: Symbol) -> Result<(), Error> {
    crate::access::require_admin(env, caller)?;
    let mut record = storage::get_circuit(env, &circuit_id)?;
    record.frozen = true;
    storage::set_circuit(env, &record);
    // A queued rotation would otherwise sit there looking finalizable.
    storage::clear_rotation(env, &circuit_id);
    events::circuit_frozen(env, &circuit_id, caller);
    Ok(())
}

/// Fetch a circuit that is registered *and* usable for verification.
pub fn active_circuit(env: &Env, circuit_id: &Symbol) -> Result<CircuitRecord, Error> {
    let record = storage::get_circuit(env, circuit_id)?;
    if !record.enabled {
        return Err(Error::CircuitDisabled);
    }
    Ok(record)
}

/// Record a successful verification against a circuit.
pub fn note_verification(env: &Env, record: &mut CircuitRecord) {
    record.verifications = record.verifications.saturating_add(1);
    storage::set_circuit(env, record);
}
