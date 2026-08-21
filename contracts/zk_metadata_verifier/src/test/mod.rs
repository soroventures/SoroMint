//! Test suite.
//!
//! `fixtures` builds real, satisfying Groth16 instances (see its module docs),
//! so every test below exercises the actual host pairing path.

mod fixtures;

mod access_tests;
mod attestation_tests;
mod batch_tests;
mod groth16_tests;
mod policy_tests;
mod registry_tests;
mod verification_tests;

use soroban_sdk::{
    symbol_short, testutils::Address as _, testutils::Ledger as _, Address, BytesN, Env, String,
    Symbol,
};

use crate::contract::{ZkMetadataVerifier, ZkMetadataVerifierClient};
use crate::types::{Digest, Groth16Proof, MetadataSignals, PolicyParams, VerifyingKey};

pub const METADATA_ARITY: u32 = crate::policy::METADATA_ARITY;

/// Ledger the harness starts at.
pub const START_LEDGER: u32 = 100_000;
/// Circuit id used by the metadata scenario.
pub const METADATA_CIRCUIT: Symbol = symbol_short!("meta_v1");
/// Proof validity window used by the metadata scenario (~1 day).
pub const VALIDITY_WINDOW: u32 = 17_280;

/// A deployed verifier plus the addresses used to drive it.
pub struct Harness<'a> {
    pub env: Env,
    pub client: ZkMetadataVerifierClient<'a>,
    /// Kept so tests can register the verifier as another contract's
    /// dependency; read by the gateway crate's integration tests.
    #[allow(dead_code)]
    pub contract_id: Address,
    pub admin: Address,
    pub issuer: Address,
}

pub fn setup() -> Harness<'static> {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);
    // A non-zero starting ledger keeps expiry arithmetic realistic; at
    // sequence 0 every "expires_at" comparison is trivially satisfied and the
    // freshness tests would prove nothing.
    env.ledger().with_mut(|l| l.sequence_number = START_LEDGER);
    let contract_id = env.register(ZkMetadataVerifier, ());
    let client = ZkMetadataVerifierClient::new(&env, &contract_id);
    client.initialize(&admin);
    Harness {
        env,
        client,
        contract_id,
        admin,
        issuer,
    }
}

/// Register `circuit_id` with a synthetic key of the given arity.
pub fn register(h: &Harness, circuit_id: &Symbol, vk: &VerifyingKey, arity: u32) {
    h.client.register_circuit(
        &h.admin,
        circuit_id,
        vk,
        &arity,
        &String::from_str(&h.env, "test-fixture"),
    );
}

/// Publish and activate a policy in one step.
pub fn activate_policy(h: &Harness, params: &PolicyParams) {
    h.client.publish_policy(&h.admin, params);
    h.client.activate_policy(&h.admin, &params.version);
}

// ---------------------------------------------------------------------------
// End-to-end metadata scenario
// ---------------------------------------------------------------------------

/// A 32-byte value that is guaranteed to be a canonical `Fr` element.
///
/// Real commitments come out of Poseidon, which already outputs field
/// elements. Here the top byte is simply zeroed, which is enough for the
/// scalar to be well under `r`.
pub fn digest32(env: &Env, seed: u8) -> Digest {
    let mut arr = [0u8; 32];
    arr[31] = seed;
    arr[30] = seed.wrapping_mul(7);
    arr[17] = seed.wrapping_add(0x5a);
    BytesN::from_array(env, &arr)
}

/// A verifier with the metadata circuit registered and a policy in force.
pub struct Scenario<'a> {
    pub h: Harness<'a>,
    pub trapdoor: fixtures::Trapdoor,
    pub policy: PolicyParams,
}

pub fn scenario() -> Scenario<'static> {
    let h = setup();
    let (vk, trapdoor) = fixtures::synth_vk(&h.env, METADATA_ARITY, 0);
    register(&h, &METADATA_CIRCUIT, &vk, METADATA_ARITY);

    let policy = PolicyParams {
        version: 1,
        policy_root: digest32(&h.env, 0x11),
        model_commitment: digest32(&h.env, 0x22),
        max_risk_score: 250,
        risk_scale: 1000,
        max_validity_ledgers: VALIDITY_WINDOW,
        circuit_id: METADATA_CIRCUIT,
        rulebook_uri: String::from_str(&h.env, "ipfs://policy-v1"),
    };
    activate_policy(&h, &policy);

    Scenario {
        h,
        trapdoor,
        policy,
    }
}

impl Scenario<'_> {
    /// Well-formed signals for `issuer`, accepted by the active policy.
    pub fn signals(&self, issuer: &Address, nonce: u8) -> MetadataSignals {
        MetadataSignals {
            verdict: crate::policy::VERDICT_SAFE,
            risk_score: 42,
            policy_root: self.policy.policy_root.clone(),
            model_commitment: self.policy.model_commitment.clone(),
            metadata_commitment: digest32(&self.h.env, 0x40 | nonce),
            issuer_hash: self.h.client.compute_issuer_hash(issuer),
            nullifier: digest32(&self.h.env, 0x80 | nonce),
            expiry_ledger: START_LEDGER + VALIDITY_WINDOW / 2,
        }
    }

    /// A proof that genuinely satisfies the verification equation for
    /// `signals` under the registered key.
    pub fn proof(&self, signals: &MetadataSignals) -> Groth16Proof {
        let inputs = crate::policy::signals_to_public_inputs(&self.h.env, signals);
        fixtures::synth_proof(&self.h.env, &self.trapdoor, &inputs, 31337)
    }
}
