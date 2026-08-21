//! Gateway tests, driving a real verifier instance end to end.

extern crate std;

use soroban_sdk::{
    symbol_short, testutils::Address as _, testutils::Ledger as _, Address, BytesN, Env, String,
    Symbol, Vec,
};

use soromint_zk_metadata_verifier::{
    Groth16Proof, MetadataSignals, PolicyParams, ScalarBytes, VerifyingKey, ZkMetadataVerifier,
    ZkMetadataVerifierClient,
};

use crate::errors::Error;
use crate::gateway::{ZkMintGateway, ZkMintGatewayClient};
use crate::types::{Digest, TokenParams};

const CIRCUIT: Symbol = symbol_short!("meta_v1");
const ARITY: u32 = 8;
const START_LEDGER: u32 = 100_000;
const VALIDITY: u32 = 17_280;

// ---------------------------------------------------------------------------
// Synthetic Groth16 material
//
// Same construction as the verifier crate's fixtures: pick the discrete logs,
// then solve for `B` so that `−ab + xy + lc + zd ≡ 0`. The resulting proof is
// real as far as the pairing check is concerned.
// ---------------------------------------------------------------------------

const G1_GENERATOR: [u8; 96] = [
    0x17, 0xf1, 0xd3, 0xa7, 0x31, 0x97, 0xd7, 0x94, 0x26, 0x95, 0x63, 0x8c, 0x4f, 0xa9, 0xac, 0x0f,
    0xc3, 0x68, 0x8c, 0x4f, 0x97, 0x74, 0xb9, 0x05, 0xa1, 0x4e, 0x3a, 0x3f, 0x17, 0x1b, 0xac, 0x58,
    0x6c, 0x55, 0xe8, 0x3f, 0xf9, 0x7a, 0x1a, 0xef, 0xfb, 0x3a, 0xf0, 0x0a, 0xdb, 0x22, 0xc6, 0xbb,
    0x08, 0xb3, 0xf4, 0x81, 0xe3, 0xaa, 0xa0, 0xf1, 0xa0, 0x9e, 0x30, 0xed, 0x74, 0x1d, 0x8a, 0xe4,
    0xfc, 0xf5, 0xe0, 0x95, 0xd5, 0xd0, 0x0a, 0xf6, 0x00, 0xdb, 0x18, 0xcb, 0x2c, 0x04, 0xb3, 0xed,
    0xd0, 0x3c, 0xc7, 0x44, 0xa2, 0x88, 0x8a, 0xe4, 0x0c, 0xaa, 0x23, 0x29, 0x46, 0xc5, 0xe7, 0xe1,
];

const G2_GENERATOR: [u8; 192] = [
    0x13, 0xe0, 0x2b, 0x60, 0x52, 0x71, 0x9f, 0x60, 0x7d, 0xac, 0xd3, 0xa0, 0x88, 0x27, 0x4f, 0x65,
    0x59, 0x6b, 0xd0, 0xd0, 0x99, 0x20, 0xb6, 0x1a, 0xb5, 0xda, 0x61, 0xbb, 0xdc, 0x7f, 0x50, 0x49,
    0x33, 0x4c, 0xf1, 0x12, 0x13, 0x94, 0x5d, 0x57, 0xe5, 0xac, 0x7d, 0x05, 0x5d, 0x04, 0x2b, 0x7e,
    0x02, 0x4a, 0xa2, 0xb2, 0xf0, 0x8f, 0x0a, 0x91, 0x26, 0x08, 0x05, 0x27, 0x2d, 0xc5, 0x10, 0x51,
    0xc6, 0xe4, 0x7a, 0xd4, 0xfa, 0x40, 0x3b, 0x02, 0xb4, 0x51, 0x0b, 0x64, 0x7a, 0xe3, 0xd1, 0x77,
    0x0b, 0xac, 0x03, 0x26, 0xa8, 0x05, 0xbb, 0xef, 0xd4, 0x80, 0x56, 0xc8, 0xc1, 0x21, 0xbd, 0xb8,
    0x06, 0x06, 0xc4, 0xa0, 0x2e, 0xa7, 0x34, 0xcc, 0x32, 0xac, 0xd2, 0xb0, 0x2b, 0xc2, 0x8b, 0x99,
    0xcb, 0x3e, 0x28, 0x7e, 0x85, 0xa7, 0x63, 0xaf, 0x26, 0x74, 0x92, 0xab, 0x57, 0x2e, 0x99, 0xab,
    0x3f, 0x37, 0x0d, 0x27, 0x5c, 0xec, 0x1d, 0xa1, 0xaa, 0xa9, 0x07, 0x5f, 0xf0, 0x5f, 0x79, 0xbe,
    0x0c, 0xe5, 0xd5, 0x27, 0x72, 0x7d, 0x6e, 0x11, 0x8c, 0xc9, 0xcd, 0xc6, 0xda, 0x2e, 0x35, 0x1a,
    0xad, 0xfd, 0x9b, 0xaa, 0x8c, 0xbd, 0xd3, 0xa7, 0x6d, 0x42, 0x9a, 0x69, 0x51, 0x60, 0xd1, 0x2c,
    0x92, 0x3a, 0xc9, 0xcc, 0x3b, 0xac, 0xa2, 0x89, 0xe1, 0x93, 0x54, 0x86, 0x08, 0xb8, 0x28, 0x01,
];

use soroban_sdk::crypto::bls12_381::{Fr, G1Affine, G2Affine};

fn fr(env: &Env, v: u32) -> Fr {
    Fr::from_u256(soroban_sdk::U256::from_u32(env, v))
}

fn g1(env: &Env, s: &Fr) -> BytesN<96> {
    env.crypto()
        .bls12_381()
        .g1_mul(&G1Affine::from_array(env, &G1_GENERATOR), s)
        .to_bytes()
}

fn g2(env: &Env, s: &Fr) -> BytesN<192> {
    env.crypto()
        .bls12_381()
        .g2_mul(&G2Affine::from_array(env, &G2_GENERATOR), s)
        .to_bytes()
}

struct Trapdoor {
    alpha: Fr,
    beta: Fr,
    gamma: Fr,
    delta: Fr,
    ic: std::vec::Vec<Fr>,
}

fn synth_vk(env: &Env) -> (VerifyingKey, Trapdoor) {
    let alpha = fr(env, 7);
    let beta = fr(env, 11);
    let gamma = fr(env, 13);
    let delta = fr(env, 17);

    let mut ic_logs = std::vec::Vec::new();
    let mut ic: Vec<BytesN<96>> = Vec::new(env);
    for j in 0..=ARITY {
        let log = fr(env, 101 + j * 3);
        ic.push_back(g1(env, &log));
        ic_logs.push(log);
    }

    (
        VerifyingKey {
            alpha_g1: g1(env, &alpha),
            beta_g2: g2(env, &beta),
            gamma_g2: g2(env, &gamma),
            delta_g2: g2(env, &delta),
            ic,
        },
        Trapdoor {
            alpha,
            beta,
            gamma,
            delta,
            ic: ic_logs,
        },
    )
}

fn synth_proof(env: &Env, td: &Trapdoor, inputs: &Vec<ScalarBytes>) -> Groth16Proof {
    let bls = env.crypto().bls12_381();
    let mut l = td.ic[0].clone();
    for j in 0..inputs.len() {
        let x = Fr::from_bytes(inputs.get_unchecked(j));
        l = bls.fr_add(&l, &bls.fr_mul(&td.ic[(j + 1) as usize], &x));
    }
    let z = fr(env, 31337);
    let b = bls.fr_add(
        &bls.fr_add(&bls.fr_mul(&td.alpha, &td.beta), &bls.fr_mul(&l, &td.gamma)),
        &bls.fr_mul(&z, &td.delta),
    );
    Groth16Proof {
        a: g1(env, &fr(env, 1)),
        b: g2(env, &b),
        c: g1(env, &z),
    }
}

fn digest32(env: &Env, seed: u8) -> Digest {
    let mut arr = [0u8; 32];
    arr[31] = seed;
    arr[30] = seed.wrapping_mul(7);
    arr[17] = seed.wrapping_add(0x5a);
    BytesN::from_array(env, &arr)
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct Rig<'a> {
    env: Env,
    verifier: ZkMetadataVerifierClient<'a>,
    gateway: ZkMintGatewayClient<'a>,
    admin: Address,
    issuer: Address,
    trapdoor: Trapdoor,
    policy_root: Digest,
    model: Digest,
}

fn setup() -> Rig<'static> {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = START_LEDGER);

    let admin = Address::generate(&env);
    let issuer = Address::generate(&env);

    let verifier_id = env.register(ZkMetadataVerifier, ());
    let verifier = ZkMetadataVerifierClient::new(&env, &verifier_id);
    verifier.initialize(&admin);

    let (vk, trapdoor) = synth_vk(&env);
    verifier.register_circuit(
        &admin,
        &CIRCUIT,
        &vk,
        &ARITY,
        &String::from_str(&env, "gateway-test"),
    );

    let policy_root = digest32(&env, 0x11);
    let model = digest32(&env, 0x22);
    let params = PolicyParams {
        version: 1,
        policy_root: policy_root.clone(),
        model_commitment: model.clone(),
        max_risk_score: 250,
        risk_scale: 1000,
        max_validity_ledgers: VALIDITY,
        circuit_id: CIRCUIT,
        rulebook_uri: String::from_str(&env, "ipfs://policy"),
    };
    verifier.publish_policy(&admin, &params);
    verifier.activate_policy(&admin, &1);

    let gateway_id = env.register(ZkMintGateway, ());
    let gateway = ZkMintGatewayClient::new(&env, &gateway_id);
    gateway.initialize(&admin, &verifier_id);
    verifier.register_consumer(&admin, &gateway_id);

    Rig {
        env,
        verifier,
        gateway,
        admin,
        issuer,
        trapdoor,
        policy_root,
        model,
    }
}

impl Rig<'_> {
    fn params(&self, name: &str, symbol: &str) -> TokenParams {
        TokenParams {
            name: String::from_str(&self.env, name),
            symbol: String::from_str(&self.env, symbol),
            decimals: 7,
            supply_cap: 1_000_000,
            metadata_uri: String::from_str(&self.env, "ipfs://token"),
        }
    }

    /// Run the full off-chain-to-on-chain flow and return the commitment.
    fn attest(&self, params: &TokenParams, nonce: u8) -> Digest {
        let commitment = self.gateway.compute_commitment(&self.issuer, params);
        let signals = MetadataSignals {
            verdict: 1,
            risk_score: 42,
            policy_root: self.policy_root.clone(),
            model_commitment: self.model.clone(),
            metadata_commitment: commitment.clone(),
            issuer_hash: self.verifier.compute_issuer_hash(&self.issuer),
            nullifier: digest32(&self.env, 0x80 | nonce),
            expiry_ledger: START_LEDGER + VALIDITY / 2,
        };
        let inputs = self.verifier.encode_signals(&signals);
        let proof = synth_proof(&self.env, &self.trapdoor, &inputs);
        self.verifier
            .verify_metadata(&self.issuer, &proof, &signals);
        commitment
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn full_flow_creates_a_token() {
    let r = setup();
    let params = r.params("Good Token", "GOOD");
    let commitment = r.attest(&params, 1);

    let record = r.gateway.create_token(&r.issuer, &params, &commitment);
    assert_eq!(record.issuer, r.issuer);
    assert_eq!(record.risk_score, 42);
    assert_eq!(record.policy_version, 1);
    assert_eq!(record.minted, 0);
    assert!(!record.frozen);
}

#[test]
fn creation_consumes_the_attestation() {
    let r = setup();
    let params = r.params("Once", "ONCE");
    let commitment = r.attest(&params, 2);

    r.gateway.create_token(&r.issuer, &params, &commitment);
    assert!(r.verifier.attestation_status(&commitment).consumed);

    // The second attempt fails inside the verifier, not here — which is the
    // point: the gateway does not get to decide single-use for itself.
    assert!(r
        .gateway
        .try_create_token(&r.issuer, &params, &commitment)
        .is_err());
}

#[test]
fn creation_without_an_attestation_fails() {
    let r = setup();
    let params = r.params("Unproven", "NOPE");
    let commitment = r.gateway.compute_commitment(&r.issuer, &params);

    assert!(r
        .gateway
        .try_create_token(&r.issuer, &params, &commitment)
        .is_err());
}

#[test]
fn swapped_parameters_are_caught_by_the_commitment() {
    let r = setup();
    let clean = r.params("Clean Token", "CLEAN");
    let commitment = r.attest(&clean, 3);

    // The attestation covers "Clean Token"; deploying something else under it
    // is exactly the substitution the commitment check exists to stop.
    let nasty = r.params("Rug Pull", "SCAM");
    let err = r
        .gateway
        .try_create_token(&r.issuer, &nasty, &commitment)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::CommitmentMismatch);
}

#[test]
fn another_issuer_cannot_use_the_commitment() {
    let r = setup();
    let params = r.params("Mine", "MINE");
    let commitment = r.attest(&params, 4);

    // The commitment binds the issuer, so recomputing it under a different
    // address yields a different value and never matches.
    let mallory = Address::generate(&r.env);
    let err = r
        .gateway
        .try_create_token(&mallory, &params, &commitment)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::CommitmentMismatch);
}

#[test]
fn minting_respects_the_committed_supply_cap() {
    let r = setup();
    let params = r.params("Capped", "CAP");
    let commitment = r.attest(&params, 5);
    r.gateway.create_token(&r.issuer, &params, &commitment);

    assert_eq!(
        r.gateway.mint(&r.issuer, &commitment, &r.issuer, &600_000),
        600_000
    );
    assert_eq!(
        r.gateway.mint(&r.issuer, &commitment, &r.issuer, &400_000),
        1_000_000
    );

    let err = r
        .gateway
        .try_mint(&r.issuer, &commitment, &r.issuer, &1)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::SupplyCapExceeded);
}

#[test]
fn only_the_issuer_can_mint() {
    let r = setup();
    let params = r.params("Owned", "OWN");
    let commitment = r.attest(&params, 6);
    r.gateway.create_token(&r.issuer, &params, &commitment);

    let stranger = Address::generate(&r.env);
    let err = r
        .gateway
        .try_mint(&stranger, &commitment, &stranger, &1)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::NotTokenIssuer);
}

#[test]
fn zero_and_negative_mints_are_rejected() {
    let r = setup();
    let params = r.params("Zero", "ZRO");
    let commitment = r.attest(&params, 7);
    r.gateway.create_token(&r.issuer, &params, &commitment);

    for amount in [0i128, -1i128] {
        let err = r
            .gateway
            .try_mint(&r.issuer, &commitment, &r.issuer, &amount)
            .err()
            .unwrap()
            .unwrap();
        assert_eq!(err, Error::InvalidAmount);
    }
}

#[test]
fn frozen_token_cannot_mint() {
    let r = setup();
    let params = r.params("Frozen", "FRZ");
    let commitment = r.attest(&params, 8);
    r.gateway.create_token(&r.issuer, &params, &commitment);

    r.gateway.set_token_frozen(&r.admin, &commitment, &true);
    let err = r
        .gateway
        .try_mint(&r.issuer, &commitment, &r.issuer, &1)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::TokenFrozen);

    r.gateway.set_token_frozen(&r.admin, &commitment, &false);
    assert_eq!(r.gateway.mint(&r.issuer, &commitment, &r.issuer, &1), 1);
}

#[test]
fn quota_limits_creations_per_window() {
    let r = setup();
    let mut config = r.gateway.get_config();
    config.tokens_per_window = 2;
    r.gateway.set_config(&r.admin, &config);

    for n in 10..12u8 {
        let params = r.params("Quota", "QTA");
        // Distinct URIs give distinct commitments.
        let mut p = params.clone();
        p.metadata_uri = String::from_str(&r.env, if n == 10 { "a" } else { "b" });
        let commitment = r.attest(&p, n);
        r.gateway.create_token(&r.issuer, &p, &commitment);
    }

    let mut p = r.params("Quota", "QTA");
    p.metadata_uri = String::from_str(&r.env, "c");
    let commitment = r.attest(&p, 12);
    let err = r
        .gateway
        .try_create_token(&r.issuer, &p, &commitment)
        .err()
        .unwrap()
        .unwrap();
    assert_eq!(err, Error::QuotaExceeded);
}

#[test]
fn quota_resets_after_the_window() {
    let r = setup();
    let mut config = r.gateway.get_config();
    config.tokens_per_window = 1;
    r.gateway.set_config(&r.admin, &config);

    let mut p1 = r.params("First", "ONE");
    p1.metadata_uri = String::from_str(&r.env, "one");
    let c1 = r.attest(&p1, 20);
    r.gateway.create_token(&r.issuer, &p1, &c1);

    r.env.ledger().with_mut(|l| {
        l.sequence_number = START_LEDGER + config.window_ledgers + 1;
    });

    let mut p2 = r.params("Second", "TWO");
    p2.metadata_uri = String::from_str(&r.env, "two");
    // Re-attest at the new ledger so the proof is fresh.
    let commitment = r.gateway.compute_commitment(&r.issuer, &p2);
    let signals = MetadataSignals {
        verdict: 1,
        risk_score: 42,
        policy_root: r.policy_root.clone(),
        model_commitment: r.model.clone(),
        metadata_commitment: commitment.clone(),
        issuer_hash: r.verifier.compute_issuer_hash(&r.issuer),
        nullifier: digest32(&r.env, 0xAB),
        expiry_ledger: START_LEDGER + config.window_ledgers + 100,
    };
    let inputs = r.verifier.encode_signals(&signals);
    let proof = synth_proof(&r.env, &r.trapdoor, &inputs);
    r.verifier.verify_metadata(&r.issuer, &proof, &signals);

    r.gateway.create_token(&r.issuer, &p2, &commitment);
    assert_eq!(r.gateway.get_quota(&r.issuer).unwrap().used, 1);
}

#[test]
fn invalid_params_are_rejected() {
    let r = setup();
    let mut p = r.params("", "SYM");
    let c = r.gateway.compute_commitment(&r.issuer, &p);
    assert_eq!(
        r.gateway
            .try_create_token(&r.issuer, &p, &c)
            .err()
            .unwrap()
            .unwrap(),
        Error::InvalidTokenParams
    );

    p = r.params("Name", "SYM");
    p.decimals = 19;
    let c = r.gateway.compute_commitment(&r.issuer, &p);
    assert_eq!(
        r.gateway
            .try_create_token(&r.issuer, &p, &c)
            .err()
            .unwrap()
            .unwrap(),
        Error::InvalidTokenParams
    );

    p = r.params("Name", "SYM");
    p.supply_cap = 0;
    let c = r.gateway.compute_commitment(&r.issuer, &p);
    assert_eq!(
        r.gateway
            .try_create_token(&r.issuer, &p, &c)
            .err()
            .unwrap()
            .unwrap(),
        Error::InvalidTokenParams
    );
}

#[test]
fn commitment_is_field_separated() {
    let r = setup();
    // Without length prefixes, ("ab","c") and ("a","bc") would collide and an
    // issuer could shuffle characters between name and symbol after screening.
    let mut a = r.params("ab", "c");
    a.metadata_uri = String::from_str(&r.env, "");
    let mut b = r.params("a", "bc");
    b.metadata_uri = String::from_str(&r.env, "");

    assert_ne!(
        r.gateway.compute_commitment(&r.issuer, &a),
        r.gateway.compute_commitment(&r.issuer, &b)
    );
}

#[test]
fn pause_blocks_creation_and_minting() {
    let r = setup();
    let params = r.params("Pausable", "PAU");
    let commitment = r.attest(&params, 30);
    r.gateway.create_token(&r.issuer, &params, &commitment);

    r.gateway.pause(&r.admin);
    assert_eq!(
        r.gateway
            .try_mint(&r.issuer, &commitment, &r.issuer, &1)
            .err()
            .unwrap()
            .unwrap(),
        Error::Paused
    );

    r.gateway.unpause(&r.admin);
    assert_eq!(r.gateway.mint(&r.issuer, &commitment, &r.issuer, &1), 1);
}

#[test]
fn non_admin_cannot_freeze_or_configure() {
    let r = setup();
    let stranger = Address::generate(&r.env);
    let params = r.params("Guarded", "GRD");
    let commitment = r.attest(&params, 31);
    r.gateway.create_token(&r.issuer, &params, &commitment);

    assert_eq!(
        r.gateway
            .try_set_token_frozen(&stranger, &commitment, &true)
            .err()
            .unwrap()
            .unwrap(),
        Error::Unauthorized
    );

    let config = r.gateway.get_config();
    assert_eq!(
        r.gateway
            .try_set_config(&stranger, &config)
            .err()
            .unwrap()
            .unwrap(),
        Error::Unauthorized
    );
}

#[test]
fn tokens_are_indexed() {
    let r = setup();
    for (n, uri) in [(40u8, "x"), (41u8, "y")] {
        let mut p = r.params("Indexed", "IDX");
        p.metadata_uri = String::from_str(&r.env, uri);
        let c = r.attest(&p, n);
        r.gateway.create_token(&r.issuer, &p, &c);
    }
    assert_eq!(r.gateway.list_tokens().len(), 2);
}

#[test]
fn double_initialize_is_rejected() {
    let r = setup();
    let other = Address::generate(&r.env);
    assert_eq!(
        r.gateway
            .try_initialize(&r.admin, &other)
            .err()
            .unwrap()
            .unwrap(),
        Error::AlreadyInitialized
    );
}
