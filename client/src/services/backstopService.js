/**
 * @title Backstop / Insurance Fund API Service
 * @notice Client-side service for interacting with the SoroMint Backstop
 *         (insurance fund) contract.
 *
 * Two integration modes:
 *   1. Backend REST API — proxies Soroban contract reads/writes through the
 *      backend (e.g. `/api/backstop/:contractId/config`). If the backend
 *      endpoint is not deployed yet, callers can pass an optional
 *      `mockConfig` payload so the UI degrades gracefully.
 *   2. Direct Soroban RPC (via `getBackstopStatus`) — read-only health ping that
 *      calls `version()` / `get_config()` on the contract without auth.
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
// Backstop contract reads
// ─────────────────────────────────────────────────────────────────────────────

/**
 * @notice Normalise a raw BackstopConfig payload into a flat, UI-ready shape.
 *
 * The Soroban `get_config()` call returns:
 *   { admin, token, fee_bps, total_deposited, total_withdrawn }
 *
 * @param {object} raw
 * @returns {object} Normalised config with numeric coercion + display helpers.
 */
export const normaliseBackstopConfig = (raw = {}) => {
  const config = raw.config || raw.data || raw;
  return {
    admin: config.admin ?? '—',
    token: config.token ?? '—',
    feeBps: Number(config.fee_bps ?? config.feeBps ?? 0),
    totalDeposited: Number(config.total_deposited ?? config.totalDeposited ?? 0),
    totalWithdrawn: Number(config.total_withdrawn ?? config.totalWithdrawn ?? 0),
    contractId: raw.contractId ?? '',
  };
};

/**
 * @notice Fetch the full Backstop configuration (admin, token, fee rate,
 *         deposit/withdraw totals) from the backend proxy.
 *
 * @param {string} contractId - Stellar C-address of the Backstop contract
 * @param {string} [token] - JWT
 * @param {object} [fallback] - Optional mock payload used when the backend
 *        endpoint is not available (new contract, demo mode).
 * @returns {Promise<object>} Normalised BackstopConfig
 */
export const getBackstopConfig = async (contractId, token = null, fallback = null) => {
  if (!contractId) throw new Error('contractId is required');

  try {
    const body = await apiFetch(
      `/backstop/${encodeURIComponent(contractId)}/config`,
      {},
      token,
    );
    return normaliseBackstopConfig(body);
  } catch (err) {
    if (fallback && (err.status === 404 || err.status === 501 || err.status === 502)) {
      return normaliseBackstopConfig({ config: fallback, contractId });
    }
    throw err;
  }
};

/**
 * @notice Fetch the current on-chain token balance of the Backstop reserve.
 * @param {string} contractId - Stellar C-address
 * @param {string} [token] - JWT
 * @param {number} [fallbackBalance] - Balance to use when the endpoint is missing
 * @returns {Promise<number>}
 */
export const getBackstopBalance = async (contractId, token = null, fallbackBalance = null) => {
  if (!contractId) throw new Error('contractId is required');

  try {
    const body = await apiFetch(
      `/backstop/${encodeURIComponent(contractId)}/balance`,
      {},
      token,
    );
    return Number(body.balance ?? body.data ?? 0);
  } catch (err) {
    if (fallbackBalance !== null && [404, 501, 502].includes(err.status)) {
      return Number(fallbackBalance);
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
export const getBackstopVersion = async (contractId, token = null, fallbackVersion = null) => {
  if (!contractId) throw new Error('contractId is required');

  try {
    const body = await apiFetch(
      `/backstop/${encodeURIComponent(contractId)}/version`,
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
 * @notice Compute the fee for a principal amount at the current fee_bps.
 *         Mirrors `calc_fee(principal)` on the contract: principal * bps / 10000.
 *
 * @param {number} principal - Amount before fee
 * @param {number} feeBps - Basis points (0–10000)
 * @returns {number} Fee amount (truncated, contract-safe)
 */
export const calcFee = (principal, feeBps) => {
  const p = Number(principal) || 0;
  const bps = Number(feeBps) || 0;
  if (p < 0) throw new Error('principal must be non-negative');
  if (bps < 0 || bps > 10_000) throw new Error('fee_bps must be between 0 and 10000');
  return Math.floor((p * bps) / 10_000);
};

/**
 * @notice Convenience: fetch everything the dashboard needs in parallel.
 * @param {string} contractId - Stellar C-address
 * @param {string} [token] - JWT
 * @param {object} [fallbacks] - { config, balance, version } used when the
 *        backend endpoints are not yet deployed.
 * @returns {Promise<{ config: object, balance: number, version: string }>}
 */
export const getBackstopStatus = async (contractId, token = null, fallbacks = null) => {
  if (!contractId) throw new Error('contractId is required');

  const [config, balance, version] = await Promise.all([
    getBackstopConfig(contractId, token, fallbacks?.config ?? null),
    getBackstopBalance(contractId, token, fallbacks?.balance ?? null),
    getBackstopVersion(contractId, token, fallbacks?.version ?? null),
  ]);

  return { config, balance, version };
};

// ─────────────────────────────────────────────────────────────────────────────
// Mutations (admin / protocol — require wallet auth)
// ─────────────────────────────────────────────────────────────────────────────

/**
 * @notice Deposit a fee amount into the backstop reserve. Mirrors
 *         `deposit_fee(from, amount)` on the contract.
 *
 * @param {object} payload
 * @param {string} payload.contractId - Stellar C-address
 * @param {string} payload.from - Wallet G-address performing the deposit
 * @param {number} payload.amount - Amount to deposit
 * @param {string} [token] - JWT
 * @returns {Promise<object>}
 */
export const depositFee = async (payload, token = null) => {
  const { contractId, from, amount } = payload || {};
  if (!contractId) throw new Error('contractId is required');
  if (!from) throw new Error('from address is required');
  if (!amount || Number(amount) <= 0) throw new Error('amount must be positive');

  const body = await apiFetch(
    `/backstop/${encodeURIComponent(contractId)}/deposit`,
    { method: 'POST', body: JSON.stringify({ from, amount: Number(amount) }) },
    token,
  );
  return body;
};

/**
 * @notice Admin withdrawal from the reserve. Mirrors `withdraw(to, amount)`.
 * @param {object} payload
 * @param {string} payload.contractId - Stellar C-address
 * @param {string} payload.to - Destination G-address
 * @param {number} payload.amount - Amount to withdraw
 * @param {string} [token] - JWT
 * @returns {Promise<object>}
 */
export const withdrawFromBackstop = async (payload, token = null) => {
  const { contractId, to, amount } = payload || {};
  if (!contractId) throw new Error('contractId is required');
  if (!to) throw new Error('to address is required');
  if (!amount || Number(amount) <= 0) throw new Error('amount must be positive');

  const body = await apiFetch(
    `/backstop/${encodeURIComponent(contractId)}/withdraw`,
    { method: 'POST', body: JSON.stringify({ to, amount: Number(amount) }) },
    token,
  );
  return body;
};

/**
 * @notice Update the fee rate (admin only). Mirrors `set_fee_bps(bps)`.
 * @param {object} payload
 * @param {string} payload.contractId - Stellar C-address
 * @param {number} payload.feeBps - New basis points (0–10000)
 * @param {string} [token] - JWT
 * @returns {Promise<object>}
 */
export const setFeeBps = async (payload, token = null) => {
  const { contractId, feeBps } = payload || {};
  if (!contractId) throw new Error('contractId is required');
  const bps = Number(feeBps);
  if (Number.isNaN(bps) || bps < 0 || bps > 10_000) {
    throw new Error('fee_bps must be between 0 and 10000');
  }

  const body = await apiFetch(
    `/backstop/${encodeURIComponent(contractId)}/fee`,
    { method: 'PATCH', body: JSON.stringify({ fee_bps: bps }) },
    token,
  );
  return body;
};

export default {
  normaliseBackstopConfig,
  getBackstopConfig,
  getBackstopBalance,
  getBackstopVersion,
  calcFee,
  getBackstopStatus,
  depositFee,
  withdrawFromBackstop,
  setFeeBps,
};