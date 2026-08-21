/**
 * @title ZK Audit Log Dashboard
 * @notice Full-featured dashboard for viewing and exporting Soroban
 *         deployment audit logs with ZK-enabled verification.
 *
 * Layout (responsive):
 *   ┌──────────────────────────────────────────────────┐
 *   │  Page header + ZK badge + refresh button          │
 *   ├──────────────┬──────────────┬──────────────┬──────┤
 *   │  Total logs  │  Successful  │    Failed    │ Rate │
 *   ├──────────────┴──────────────┴──────────────┴──────┤
 *   │  Filter row: date range │ status │ export CSV     │
 *   ├───────────────────────────────────────────────────┤
 *   │  Log table (tokenName, contractId, status,        │
 *   │            errorMessage, createdAt)               │
 *   └───────────────────────────────────────────────────┘
 */

import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import {
  ScrollText,
  RefreshCw,
  CheckCircle2,
  XCircle,
  Download,
  Filter,
  AlertTriangle,
  Search,
  CalendarDays,
  TrendingUp,
  Activity,
  ShieldCheck,
} from 'lucide-react';

import SEO from '../../components/SEO';
import { getAuditLogs, formatLogEntry } from '../../services/zkAuditLogService';

// ─── Constants ────────────────────────────────────────────────────────────────

const STATUS_OPTIONS = ['all', 'success', 'fail'];

// ─── Demo data used when backend endpoints are not deployed ──────────────────

const DEMO_LOGS = [
  {
    _id: '1',
    tokenName: 'DemoToken',
    contractId: 'CDEMO000000000000000000000000000000000001',
    status: 'SUCCESS',
    errorMessage: '',
    createdAt: new Date(Date.now() - 60_000 * 5).toISOString(),
  },
  {
    _id: '2',
    tokenName: 'TestAsset',
    contractId: 'CTEST000000000000000000000000000000000002',
    status: 'FAIL',
    errorMessage: 'Insufficient balance for deployment fee',
    createdAt: new Date(Date.now() - 60_000 * 15).toISOString(),
  },
  {
    _id: '3',
    tokenName: 'MyToken',
    contractId: 'CMYTO000000000000000000000000000000000003',
    status: 'SUCCESS',
    errorMessage: '',
    createdAt: new Date(Date.now() - 60_000 * 30).toISOString(),
  },
  {
    _id: '4',
    tokenName: 'LaunchPad',
    contractId: 'CLAUN000000000000000000000000000000000004',
    status: 'SUCCESS',
    errorMessage: '',
    createdAt: new Date(Date.now() - 60_000 * 60).toISOString(),
  },
  {
    _id: '5',
    tokenName: 'FailedCoin',
    contractId: 'CFAIL000000000000000000000000000000000005',
    status: 'FAIL',
    errorMessage: 'Contract deployment timed out after 30s',
    createdAt: new Date(Date.now() - 60_000 * 120).toISOString(),
  },
];

// ─── Helpers ─────────────────────────────────────────────────────────────────

/**
 * @notice Truncate a Stellar C-address to "CABC…WXYZ" form.
 */
const truncateId = (id) => {
  if (!id || id === '—') return '—';
  if (id.length <= 14) return id;
  return `${id.slice(0, 8)}…${id.slice(-6)}`;
};

/**
 * @notice Format a date string to a locale-friendly display.
 */
const formatDate = (isoString) => {
  if (!isoString || isoString === '—') return '—';
  try {
    const d = new Date(isoString);
    return d.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  } catch {
    return '—';
  }
};

// ─── Sub-component: ZK badge ─────────────────────────────────────────────────

function ZkBadge() {
  const { t } = useTranslation();
  return (
    <span
      className="inline-flex items-center gap-1.5 rounded-full border border-violet-200 bg-violet-100 px-3 py-1 text-xs font-semibold text-violet-700 dark:border-violet-700/40 dark:bg-violet-900/30 dark:text-violet-400"
      data-testid="zk-badge"
    >
      <ShieldCheck size={14} />
      {t('auditLog.zkBadge') || 'ZK Audit Trail'}
    </span>
  );
}

// ─── Sub-component: Metric card ───────────────────────────────────────────────

function MetricCard({ label, value, icon: Icon, color, isLoading }) {
  return (
    <div
      className="glass-card flex flex-col gap-3 !p-5"
      aria-label={`${label}: ${value}`}
    >
      <div className="flex items-center justify-between">
        <span className="text-sm text-slate-500 dark:text-slate-400">{label}</span>
        <div
          className={`flex h-8 w-8 items-center justify-center rounded-xl ${color}`}
        >
          <Icon size={16} className="text-white" />
        </div>
      </div>
      {isLoading ? (
        <div className="h-7 w-20 animate-pulse rounded-lg bg-black/8 dark:bg-white/10" />
      ) : (
        <p className="text-2xl font-bold tabular-nums text-slate-900 dark:text-white">
          {value}
        </p>
      )}
    </div>
  );
}

// ─── Sub-component: Status badge ─────────────────────────────────────────────

function StatusBadge({ status }) {
  if (status === 'SUCCESS') {
    return (
      <span className="inline-flex items-center gap-1 rounded-full bg-green-100 px-2.5 py-0.5 text-xs font-semibold text-green-700 dark:bg-green-900/30 dark:text-green-400">
        <CheckCircle2 size={12} />
        SUCCESS
      </span>
    );
  }
  if (status === 'FAIL') {
    return (
      <span className="inline-flex items-center gap-1 rounded-full bg-red-100 px-2.5 py-0.5 text-xs font-semibold text-red-700 dark:bg-red-900/30 dark:text-red-400">
        <XCircle size={12} />
        FAIL
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-slate-100 px-2.5 py-0.5 text-xs font-semibold text-slate-500 dark:bg-slate-800 dark:text-slate-400">
      {status}
    </span>
  );
}

// ─── Sub-component: Filter row ────────────────────────────────────────────────

function FilterRow({ dateFrom, dateTo, statusFilter, onDateFromChange, onDateToChange, onStatusChange, onExport }) {
  const { t } = useTranslation();

  return (
    <div className="glass-card flex flex-wrap items-end gap-4 !p-4">
      <div className="flex flex-col gap-1">
        <label className="flex items-center gap-1 text-xs font-medium text-slate-500 dark:text-slate-400">
          <CalendarDays size={12} />
          {t('auditLog.from') || 'From'}
        </label>
        <input
          type="date"
          value={dateFrom}
          onChange={(e) => onDateFromChange(e.target.value)}
          className="input-field w-40 rounded-lg border border-black/10 bg-white px-3 py-1.5 text-sm dark:border-white/10 dark:bg-slate-800"
          data-testid="filter-date-from"
        />
      </div>

      <div className="flex flex-col gap-1">
        <label className="flex items-center gap-1 text-xs font-medium text-slate-500 dark:text-slate-400">
          <CalendarDays size={12} />
          {t('auditLog.to') || 'To'}
        </label>
        <input
          type="date"
          value={dateTo}
          onChange={(e) => onDateToChange(e.target.value)}
          className="input-field w-40 rounded-lg border border-black/10 bg-white px-3 py-1.5 text-sm dark:border-white/10 dark:bg-slate-800"
          data-testid="filter-date-to"
        />
      </div>

      <div className="flex flex-col gap-1">
        <label className="flex items-center gap-1 text-xs font-medium text-slate-500 dark:text-slate-400">
          <Filter size={12} />
          {t('auditLog.status') || 'Status'}
        </label>
        <select
          value={statusFilter}
          onChange={(e) => onStatusChange(e.target.value)}
          className="input-field w-36 rounded-lg border border-black/10 bg-white px-3 py-1.5 text-sm dark:border-white/10 dark:bg-slate-800"
          data-testid="filter-status"
        >
          {STATUS_OPTIONS.map((opt) => (
            <option key={opt} value={opt}>
              {opt === 'all' ? (t('auditLog.all') || 'All') : opt === 'success' ? (t('auditLog.success') || 'Success') : (t('auditLog.fail') || 'Fail')}
            </option>
          ))}
        </select>
      </div>

      <button
        type="button"
        onClick={onExport}
        className="inline-flex items-center gap-1.5 rounded-lg border border-violet-200 bg-white px-3 py-1.5 text-sm font-medium text-violet-600 transition hover:bg-violet-50 dark:border-violet-700/40 dark:bg-slate-800 dark:text-violet-400 dark:hover:bg-violet-900/20"
        data-testid="export-csv-btn"
      >
        <Download size={14} />
        {t('auditLog.exportCsv') || 'Export CSV'}
      </button>
    </div>
  );
}

// ─── Sub-component: Log table ─────────────────────────────────────────────────

function LogTable({ logs, isLoading }) {
  const { t } = useTranslation();

  if (isLoading) {
    return (
      <div className="glass-card" data-testid="logs-loading">
        <div className="space-y-4 !p-5">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="flex gap-4">
              <div className="h-5 w-40 animate-pulse rounded bg-black/8 dark:bg-white/10" />
              <div className="h-5 w-60 animate-pulse rounded bg-black/8 dark:bg-white/10" />
              <div className="h-5 w-20 animate-pulse rounded bg-black/8 dark:bg-white/10" />
              <div className="h-5 w-48 animate-pulse rounded bg-black/8 dark:bg-white/10" />
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (!logs || logs.length === 0) {
    return (
      <div className="glass-card flex min-h-[240px] flex-col items-center justify-center !p-5" data-testid="logs-empty">
        <Search size={40} className="mb-3 text-slate-300 dark:text-slate-600" />
        <p className="text-sm text-slate-500 dark:text-slate-400">
          {t('auditLog.noLogs') || 'No audit logs found'}
        </p>
      </div>
    );
  }

  return (
    <div className="glass-card !p-0 overflow-hidden" data-testid="logs-table">
      <div className="overflow-x-auto">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-black/5 dark:border-white/10 text-sm text-slate-500 dark:text-slate-400">
              <th className="px-5 pb-3 pt-4 font-medium">{t('auditLog.tokenName') || 'Token Name'}</th>
              <th className="px-5 pb-3 pt-4 font-medium">{t('auditLog.contractId') || 'Contract ID'}</th>
              <th className="px-5 pb-3 pt-4 font-medium">{t('auditLog.status') || 'Status'}</th>
              <th className="px-5 pb-3 pt-4 font-medium">{t('auditLog.error') || 'Error'}</th>
              <th className="px-5 pb-3 pt-4 font-medium">{t('auditLog.date') || 'Date'}</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-black/5 dark:divide-white/5">
            {logs.map((entry) => (
              <tr
                key={entry.id || entry._id}
                className="group transition-colors hover:bg-black/5 dark:hover:bg-white/5"
                data-testid={`log-row-${entry.id || entry._id}`}
              >
                <td className="px-5 py-3 font-medium text-slate-900 dark:text-white">
                  {entry.tokenName}
                </td>
                <td className="px-5 py-3 font-mono text-sm text-stellar-blue">
                  {truncateId(entry.contractId)}
                </td>
                <td className="px-5 py-3">
                  <StatusBadge status={entry.status} />
                </td>
                <td className="max-w-[200px] truncate px-5 py-3 text-sm text-slate-500 dark:text-slate-400">
                  {entry.errorMessage || '—'}
                </td>
                <td className="px-5 py-3 text-sm text-slate-500 dark:text-slate-400 whitespace-nowrap">
                  {formatDate(entry.createdAt)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ─── Main ZK Audit Log Dashboard Component ───────────────────────────────────

function ZKAuditLogDashboard() {
  const { t } = useTranslation();
  const [logs, setLogs] = useState([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState(null);
  const [isDemo, setIsDemo] = useState(false);

  // Filters
  const [dateFrom, setDateFrom] = useState('');
  const [dateTo, setDateTo] = useState('');
  const [statusFilter, setStatusFilter] = useState('all');

  // ─── Data fetching ─────────────────────────────────────────────────────────

  const fetchLogs = useCallback(async (showToast = false) => {
    setIsLoading(true);
    setError(null);
    try {
      const data = await getAuditLogs(null, DEMO_LOGS);
      setLogs(data);
      // Check if demo data was used
      if (Array.isArray(data) && data.length > 0 && data === DEMO_LOGS) {
        setIsDemo(true);
        if (showToast) {
          toast.info(t('auditLog.demoMode') || 'Running in demo mode — using mock data');
        }
      } else {
        setIsDemo(false);
      }
    } catch (err) {
      setError(err.message);
      setLogs([]);
      if (showToast) {
        toast.error(`${t('auditLog.fetchError') || 'Failed to load audit logs'}: ${err.message}`);
      }
    } finally {
      setIsLoading(false);
    }
  }, [t]);

  useEffect(() => {
    fetchLogs(true);
  }, [fetchLogs]);

  // ─── Filtering ─────────────────────────────────────────────────────────────

  const filteredLogs = logs.filter((entry) => {
    // Status filter
    if (statusFilter === 'success' && entry.status !== 'SUCCESS') return false;
    if (statusFilter === 'fail' && entry.status !== 'FAIL') return false;

    // Date range filter
    if (dateFrom && entry.createdAt) {
      const entryDate = new Date(entry.createdAt);
      const fromDate = new Date(dateFrom);
      if (entryDate < fromDate) return false;
    }
    if (dateTo && entry.createdAt) {
      const entryDate = new Date(entry.createdAt);
      // Add one day to include the end date
      const toDate = new Date(dateTo);
      toDate.setDate(toDate.getDate() + 1);
      if (entryDate >= toDate) return false;
    }

    return true;
  });

  // ─── Metrics ───────────────────────────────────────────────────────────────

  const totalCount = logs.length;
  const successCount = logs.filter((l) => l.status === 'SUCCESS').length;
  const failCount = logs.filter((l) => l.status === 'FAIL').length;
  const successRate = totalCount > 0 ? Math.round((successCount / totalCount) * 100) : 0;

  // ─── Export CSV ────────────────────────────────────────────────────────────

  const handleExport = () => {
    // Build CSV from current logs
    const headers = ['Token Name,Contract ID,Status,Error,Date'];
    const rows = logs.map((entry) => {
      const name = `"${(entry.tokenName || '').replace(/"/g, '""')}"`;
      const cid = `"${(entry.contractId || '').replace(/"/g, '""')}"`;
      const err = `"${(entry.errorMessage || '').replace(/"/g, '""')}"`;
      return `${name},${cid},${entry.status || ''},${err},${entry.createdAt || ''}`;
    });
    const csv = [...headers, ...rows].join('\n');
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8;' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `audit-log-export-${new Date().toISOString().slice(0, 10)}.csv`;
    a.click();
    URL.revokeObjectURL(url);
    toast.success(t('auditLog.exported') || 'Audit log exported as CSV');
  };

  // ─── Render ────────────────────────────────────────────────────────────────

  return (
    <>
      <SEO title={t('auditLog.pageTitle') || 'ZK Audit Log'} />

      {/* Page header */}
      <div className="mb-6 flex items-center justify-between">
        <div>
          <div className="flex items-center gap-3">
            <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
              {t('auditLog.pageTitle') || 'ZK Audit Log'}
            </h1>
            <ZkBadge />
          </div>
          <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
            {t('auditLog.pageSubtitle') || 'Verified deployment event trail with zero-knowledge integrity'}
          </p>
        </div>
        <button
          type="button"
          onClick={() => fetchLogs(true)}
          disabled={isLoading}
          className="inline-flex items-center gap-2 rounded-xl border border-black/10 bg-white px-4 py-2 text-sm font-medium text-slate-700 transition hover:bg-slate-50 disabled:opacity-50 dark:border-white/10 dark:bg-slate-800 dark:text-slate-300 dark:hover:bg-slate-700"
          data-testid="refresh-btn"
        >
          <RefreshCw size={16} className={isLoading ? 'animate-spin' : ''} />
          {t('auditLog.refreshButton') || 'Refresh'}
        </button>
      </div>

      {/* Demo mode hint */}
      {isDemo && (
        <div
          className="mb-4 rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-800/30 dark:bg-amber-900/20 dark:text-amber-400"
          data-testid="demo-hint"
          role="status"
        >
          <span className="flex items-center gap-2">
            <AlertTriangle size={16} />
            {t('auditLog.demoHint') || 'Running in demo mode — backend API not connected. Showing mock data.'}
          </span>
        </div>
      )}

      {/* Error banner */}
      {error && (
        <div
          className="mb-4 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-800/30 dark:bg-red-900/20 dark:text-red-400"
          role="alert"
          data-testid="error-banner"
        >
          <span className="flex items-center gap-2">
            <AlertTriangle size={16} />
            {error}
          </span>
        </div>
      )}

      {/* Metrics cards */}
      <div className="mb-6 grid grid-cols-2 gap-4 lg:grid-cols-4">
        <MetricCard
          label={t('auditLog.metrics.total') || 'Total Events'}
          value={isLoading ? '—' : totalCount.toLocaleString()}
          icon={Activity}
          color="bg-stellar-blue"
          isLoading={isLoading}
        />
        <MetricCard
          label={t('auditLog.metrics.successful') || 'Successful'}
          value={isLoading ? '—' : successCount.toLocaleString()}
          icon={CheckCircle2}
          color="bg-green-500"
          isLoading={isLoading}
        />
        <MetricCard
          label={t('auditLog.metrics.failed') || 'Failed'}
          value={isLoading ? '—' : failCount.toLocaleString()}
          icon={XCircle}
          color="bg-red-500"
          isLoading={isLoading}
        />
        <MetricCard
          label={t('auditLog.metrics.rate') || 'Success Rate'}
          value={isLoading ? '—' : `${successRate}%`}
          icon={TrendingUp}
          color="bg-violet-500"
          isLoading={isLoading}
        />
      </div>

      {/* Filter row */}
      <div className="mb-4">
        <FilterRow
          dateFrom={dateFrom}
          dateTo={dateTo}
          statusFilter={statusFilter}
          onDateFromChange={setDateFrom}
          onDateToChange={setDateTo}
          onStatusChange={setStatusFilter}
          onExport={handleExport}
        />
      </div>

      {/* Log table */}
      <LogTable logs={filteredLogs} isLoading={isLoading} />
    </>
  );
}

export default ZKAuditLogDashboard;