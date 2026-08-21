/**
 * @title Multisig API Service
 * @notice Client-side service for interacting with the SoroMint Multisig
 *         contract. Supports two modes:
 *   1. Backend REST API — proxies Soroban contract reads/writes through the
 *      backend (e.g. `/api/multisig/:contractId/proposals`).
 *   2. Fallback demo payloads — when the backend endpoint is not deployed yet,
 *      callers can pass an optional fallback payload so the UI degrades
 *      gracefully.
 *
 * All functions throw descriptive Error objects on failure so callers can
 * surface meaningful toast notifications in the UI.
 */

const API_BASE = import.meta.env.VITE_API_BASE_URL || 'http://localhost:5000/api';

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * @notice Thin fetch wrapper — throws an Error with the API error message on
 *         non-2xx responses.
 */
const apiFetch = async (path, opts = {}, token = null) => {
  const headers = {
    'Content-Type': 'application/json',
    ...(opts.headers || {}),
  };

  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  const res = await fetch(`${API_BASE}${path}`, { ...opts, headers });

  let body;
  try {
    body = await res.json();
  } catch {
    throw new Error(`Server returned a non-JSON response (HTTP ${res.status})`);
  }

  if (!res.ok) {
    const message =
      body?.error || body?.message || `Request failed with status ${res.status}`;
    const err = new Error(message);
    err.status = res.status;
    err.code = body?.code;
    throw err;
  }

  return body;
};

// ─────────────────────────────────────────────────────────────────────────────
// Multisig contract reads
// ─────────────────────────────────────────────────────────────────────────────

/**
 * @notice Normalise a raw MultisigConfig payload into a flat, UI-ready shape.
 *
 * The Soroban `get_config()` call returns:
 *   { admin, signers, threshold, proposal_count, executed_count, rejected_count }
 *
 * @param {object} raw
 * @returns {object} Normalised config.
 */
export const normaliseMultisigConfig = (raw = {}) => {
  const config = raw.config || raw.data || raw;
  return {
    admin: config.admin ?? '—',
    signers: Array.isArray(config.signers) ? config.signers : [],
    threshold: Number(config.threshold ?? config.threshold ?? 0),
    proposalCount: Number(config.proposal_count ?? config.proposalCount ?? 0),
    executedCount: Number(config.executed_count ?? config.executedCount ?? 0),
    rejectedCount: Number(config.rejected_count ?? config.rejectedCount ?? 0),
    contractId: raw.contractId ?? '',
  };
};

/**
 * @notice Normalise a raw proposal list into a clean array.
 * @param {object} raw
 * @returns {Array<object>}
 */
export const normaliseProposals = (raw = {}) => {
  const items = raw.proposals || raw.data || [];
  if (!Array.isArray(items)) return [];
  return items.map((p, i) => ({
    id: p.id ?? i + 1,
    destination: p.destination ?? '—',
    amount: Number(p.amount ?? 0),
    description: p.description ?? '',
    signers: Array.isArray(p.signers) ? p.signers : [],
    threshold: Number(p.threshold ?? 0),
    status: p.status ?? 'pending', // pending | executed | rejected
    createdAt: p.created_at ?? p.createdAt ?? '',
  }));
};

/**
 * @notice Fetch the Multisig configuration (admin, signers, threshold, counts).
 * @param {string} contractId - Stellar C-address
 * @param {string} [token] - JWT
 * @param {object} [fallback] - Optional mock payload
 * @returns {Promise<object>} Normalised MultisigConfig
 */
export const getMultisigConfig = async (contractId, token = null, fallback = null) => {
  if (!contractId) throw new Error('contractId is required');

  try {
    const body = await apiFetch(
      `/multisig/${encodeURIComponent(contractId)}/config`,
      {},
      token,
    );
    return normaliseMultisigConfig(body);
  } catch (err) {
    if (fallback && [404, 501, 502].includes(err.status)) {
      return normaliseMultisigConfig({ config: fallback, contractId });
    }
    throw err;
  }
};

/**
 * @notice Fetch the list of pending / historical proposals.
 * @param {string} contractId - Stellar C-address
 * @param {string} [token] - JWT
 * @param {Array} [fallbackProposals] - Mock proposals when endpoint is missing
 * @returns {Promise<Array<object>>}
 */
export const getProposals = async (contractId, token = null, fallbackProposals = null) => {
  if (!contractId) throw new Error('contractId is required');

  try {
    const body = await apiFetch(
      `/multisig/${encodeURIComponent(contractId)}/proposals`,
      {},
      token,
    );
    return normaliseProposals(body);
  } catch (err) {
    if (fallbackProposals !== null && [404, 501, 502].includes(err.status)) {
      return normaliseProposals({ proposals: fallbackProposals });
    }
    throw err;
  }
};

/**
 * @notice Fetch the contract version string (health ping).
 * @param {string} contractId - Stellar C-address
 * @param {string} [token] - JWT
 * @param {string} [fallbackVersion] - Version string when endpoint is missing
 * @returns {Promise<string>}
 */
export const getMultisigVersion = async (contractId, token = null, fallbackVersion = null) => {
  if (!contractId) throw new Error('contractId is required');

  try {
    const body = await apiFetch(
      `/multisig/${encodeURIComponent(contractId)}/version`,
      {},
      token,
    );
    return String(body.version ?? body.data ?? '');
  } catch (err) {
    if (fallbackVersion !== null && [404, 501, 502].includes(err.status)) {
      return String(fallbackVersion);
    }
    throw err;
  }
};

/**
 * @notice Convenience: fetch everything the dashboard needs in parallel.
 * @param {string} contractId - Stellar C-address
 * @param {string} [token] - JWT
 * @param {object} [fallbacks] - { config, proposals, version }
 * @returns {Promise<{ config: object, proposals: Array, version: string }>}
 */
export const getMultisigStatus = async (contractId, token = null, fallbacks = null) => {
  if (!contractId) throw new Error('contractId is required');

  const [config, proposals, version] = await Promise.all([
    getMultisigConfig(contractId, token, fallbacks?.config ?? null),
    getProposals(contractId, token, fallbacks?.proposals ?? null),
    getMultisigVersion(contractId, token, fallbacks?.version ?? null),
  ]);

  return { config, proposals, version };
};

// ─────────────────────────────────────────────────────────────────────────────
// Mutations (admin / signer — require wallet auth)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * @notice Submit a new proposal to the multisig contract.
 * @param {object} payload
 * @param {string} payload.contractId - Stellar C-address
 * @param {string} payload.destination - Destination G-address
 * @param {number} payload.amount - Amount to transfer
 * @param {string} payload.description - Proposal description
 * @param {string} [token] - JWT
 * @returns {Promise<object>}
 */
export const submitProposal = async (payload, token = null) => {
  const { contractId, destination, amount, description } = payload || {};
  if (!contractId) throw new Error('contractId is required');
  if (!destination) throw new Error('destination address is required');
  if (!amount || Number(amount) <= 0) throw new Error('amount must be positive');

  const body = await apiFetch(
    `/multisig/${encodeURIComponent(contractId)}/proposals`,
    {
      method: 'POST',
      body: JSON.stringify({ destination, amount: Number(amount), description: description || '' }),
    },
    token,
  );
  return body;
};

/**
 * @notice Sign / approve a pending proposal.
 * @param {object} payload
 * @param {string} payload.contractId - Stellar C-address
 * @param {number} payload.proposalId - ID of the proposal to sign
 * @param {string} payload.signer - G-address of the signer
 * @param {string} [token] - JWT
 * @returns {Promise<object>}
 */
export const signProposal = async (payload, token = null) => {
  const { contractId, proposalId, signer } = payload || {};
  if (!contractId) throw new Error('contractId is required');
  if (!proposalId) throw new Error('proposalId is required');
  if (!signer) throw new Error('signer address is required');

  const body = await apiFetch(
    `/multisig/${encodeURIComponent(contractId)}/proposals/${proposalId}/sign`,
    { method: 'POST', body: JSON.stringify({ signer }) },
    token,
  );
  return body;
};

export default {
  normaliseMultisigConfig,
  normaliseProposals,
  getMultisigConfig,
  getProposals,
  getMultisigVersion,
  getMultisigStatus,
  submitProposal,
  signProposal,
};