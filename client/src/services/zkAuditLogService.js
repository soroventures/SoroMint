/**
 * @title ZK Audit Log API Service
 * @notice Client-side service for fetching and interacting with Soroban
 *         deployment audit logs from the SoroMint backend.
 *
 * API endpoints:
 *   GET  /api/logs           — list logs for the authenticated user (desc, 50 max)
 *   GET  /api/logs/export    — CSV export with optional date range filter
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
// Public API
// ─────────────────────────────────────────────────────────────────────────────

/**
 * @notice Fetch deployment audit logs for the authenticated user.
 *
 * @param {string}  [token]    — Optional JWT auth token
 * @param {object}  [fallback] — Optional mock data used when the backend
 *                               endpoint is not deployed (demo mode)
 * @returns {Promise<Array<object>>} Array of audit log entries
 *
 * Each entry:
 *   { _id, tokenName, contractId, status: 'SUCCESS'|'FAIL',
 *     errorMessage, createdAt }
 */
export const getAuditLogs = async (token = null, fallback = null) => {
  try {
    const body = await apiFetch('/logs', {}, token);
    return Array.isArray(body) ? body : body?.data || body?.logs || [];
  } catch (err) {
    if (fallback && (err.status === 404 || err.status === 501 || err.status === 502)) {
      return fallback;
    }
    throw err;
  }
};

/**
 * @notice Build a CSV export URL with optional date range parameters.
 *
 * @param {string} [token] — JWT auth token (appended as query param)
 * @param {string} [from]  — ISO date string for start of range
 * @param {string} [to]    — ISO date string for end of range
 * @returns {string} Export URL
 */
export const getExportUrl = (token = null, from = '', to = '') => {
  const params = new URLSearchParams();
  if (token) params.set('token', token);
  if (from) params.set('from', from);
  if (to) params.set('to', to);
  const qs = params.toString();
  return `${API_BASE}/logs/export${qs ? `?${qs}` : ''}`;
};

/**
 * @notice Format a deployment audit log entry for display.
 *
 * @param {object} entry
 * @returns {object} Formatted entry with convenience fields
 */
export const formatLogEntry = (entry = {}) => ({
  id: entry._id || '',
  tokenName: entry.tokenName || '—',
  contractId: entry.contractId || '—',
  status: entry.status || '—',
  errorMessage: entry.errorMessage || '',
  createdAt: entry.createdAt ? new Date(entry.createdAt).toLocaleString() : '—',
  isSuccess: entry.status === 'SUCCESS',
  isFailure: entry.status === 'FAIL',
});