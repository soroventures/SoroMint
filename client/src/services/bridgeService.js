/**
 * @title Bridge Receiver API Service
 * @author SoroMint Team
 * @notice Client-side service for interacting with the bridge relayer API
 *         that fronts the `bridge_receiver` Soroban contract.
 *
 * @dev All functions are async and throw descriptive Error objects on failure
 *      so callers can surface meaningful toast notifications in the UI.
 *
 * Endpoints covered:
 *   GET  /api/bridge/relayer/status   (requires JWT)
 */

const API_BASE = import.meta.env.VITE_API_BASE_URL || 'http://localhost:5000/api';

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * @notice Thin fetch wrapper that throws an Error with the API's error message
 *         on non-2xx responses.
 * @param {string}  path   - Path relative to API_BASE (e.g. '/bridge/relayer/status')
 * @param {object}  [opts] - fetch() options
 * @param {string}  [token] - Optional JWT for Authorization header
 * @returns {Promise<object>} Parsed JSON body
 */
const apiFetch = async (path, opts = {}, token = null) => {
  const headers = {
    'Content-Type': 'application/json',
    ...(opts.headers || {}),
  };

  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  const res = await fetch(`${API_BASE}${path}`, {
    ...opts,
    headers,
  });

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
// Bridge relayer operations
// ─────────────────────────────────────────────────────────────────────────────

/**
 * @notice Fetch the current bridge relayer status and queue metrics.
 *
 * @param {string}  [token]     - Valid JWT (endpoint is authenticated)
 * @param {boolean} [detailed]  - Include full event details in the response
 *
 * @returns {Promise<{
 *   enabled: boolean,
 *   configured: boolean,
 *   direction: 'both'|'soroban-to-evm'|'evm-to-soroban',
 *   queue: { pending: number, processing: number },
 *   stats: {
 *     observed: number,
 *     skipped: number,
 *     relayed: number,
 *     failed: number,
 *     lastObservedAt: string|null,
 *     lastRelayedAt: string|null,
 *     lastError: string|null
 *   },
 *   config: { sorobanAccountId: string, evmBridgeAddress: string }
 * }>}
 */
export const getRelayerStatus = async (token = null, detailed = false) => {
  const qs = detailed ? '?detailed=true' : '';
  const body = await apiFetch(`/bridge/relayer/status${qs}`, {}, token);
  return body?.data ?? body;
};

// ─────────────────────────────────────────────────────────────────────────────
// Default export for convenience
// ─────────────────────────────────────────────────────────────────────────────

export default {
  getRelayerStatus,
};
