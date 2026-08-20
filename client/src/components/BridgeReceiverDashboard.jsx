import React, { useState, useEffect, useCallback } from 'react';
import {
  ArrowLeftRight,
  RefreshCw,
  AlertCircle,
  Loader2,
  Inbox,
  CheckCheck,
  XCircle,
  Eye,
  SkipForward,
  Wallet,
} from 'lucide-react';
import { toast } from 'react-toastify';
import { getRelayerStatus } from '../services/bridgeService';

// ─── Helpers ──────────────────────────────────────────────────────────────────

const formatTimestamp = (value) => {
  if (!value) return 'Never';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return 'Never';
  return date.toLocaleString();
};

const formatNumber = (value) => Number(value ?? 0).toLocaleString();

const DIRECTION_LABELS = {
  both: 'Two-way',
  'soroban-to-evm': 'Soroban → EVM',
  'evm-to-soroban': 'EVM → Soroban',
};

// ─── Status badge ─────────────────────────────────────────────────────────────

function StatusBadge({ isOk, okLabel, badLabel }) {
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-semibold ${
        isOk
          ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
          : 'bg-red-500/10 text-red-600 dark:text-red-400'
      }`}
    >
      <span
        className={`h-1.5 w-1.5 rounded-full ${
          isOk ? 'bg-emerald-500' : 'bg-red-500'
        }`}
      />
      {isOk ? okLabel : badLabel}
    </span>
  );
}

// ─── Metric card ──────────────────────────────────────────────────────────────

function MetricCard({ Icon, label, value, tone = 'text-stellar-blue' }) {
  return (
    <div className="glass-card flex items-center gap-4 p-5">
      <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl bg-black/5 dark:bg-white/5">
        <Icon size={20} className={tone} />
      </div>
      <div className="min-w-0">
        <p className="truncate text-sm text-slate-500 dark:text-slate-400">
          {label}
        </p>
        <p className="text-2xl font-bold tabular-nums text-slate-900 dark:text-white">
          {formatNumber(value)}
        </p>
      </div>
    </div>
  );
}

// ─── Skeleton card ────────────────────────────────────────────────────────────

function MetricSkeleton() {
  return (
    <div className="glass-card flex animate-pulse items-center gap-4 p-5">
      <div className="h-11 w-11 shrink-0 rounded-xl bg-black/8 dark:bg-white/10" />
      <div className="space-y-2">
        <div className="h-3 w-20 rounded bg-black/5 dark:bg-white/8" />
        <div className="h-6 w-12 rounded bg-black/8 dark:bg-white/10" />
      </div>
    </div>
  );
}

// ─── Main Component ───────────────────────────────────────────────────────────

export default function BridgeReceiverDashboard({ authToken = null }) {
  const [status, setStatus] = useState(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState(null);

  const fetchStatus = useCallback(async () => {
    if (!authToken) return;

    setIsLoading(true);
    setError(null);
    try {
      const data = await getRelayerStatus(authToken);
      setStatus(data);
    } catch (err) {
      setError(err.message || 'Unknown error');
      toast.error(`Could not load bridge status: ${err.message}`);
    } finally {
      setIsLoading(false);
    }
  }, [authToken]);

  useEffect(() => {
    fetchStatus();
  }, [fetchStatus]);

  // ── Derived ────────────────────────────────────────────────────────────────
  const showAuthPrompt = !authToken;
  const showSkeletons = isLoading && !status;
  const showError = !isLoading && Boolean(error) && !status;
  const showContent = Boolean(status) && !showSkeletons;

  const stats = status?.stats ?? {};
  const queue = status?.queue ?? {};

  // ── Render ─────────────────────────────────────────────────────────────────
  return (
    <div className="space-y-8" aria-busy={isLoading}>
      {/* Header */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <div className="flex items-center gap-3">
            <div className="rounded-2xl bg-stellar-blue p-2.5 shadow-lg shadow-blue-500/25">
              <ArrowLeftRight className="h-6 w-6 text-white" />
            </div>
            <h2 className="text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
              Bridge Receiver
            </h2>
          </div>
          <p className="mt-2 ml-[52px] text-sm text-slate-500 dark:text-slate-400">
            Cross-chain mint signals relayed into the bridge_receiver contract
          </p>
        </div>

        {!showAuthPrompt && (
          <button
            onClick={fetchStatus}
            disabled={isLoading}
            aria-label="Refresh bridge status"
            className="btn-primary flex items-center gap-2 disabled:opacity-50"
          >
            {isLoading ? (
              <Loader2 size={16} className="animate-spin" />
            ) : (
              <RefreshCw size={16} />
            )}
            Refresh
          </button>
        )}
      </div>

      {/* Unauthenticated prompt */}
      {showAuthPrompt && (
        <div className="glass-card flex flex-col items-center justify-center gap-4 py-20 text-center">
          <div className="flex h-16 w-16 items-center justify-center rounded-2xl border border-black/5 bg-black/3 dark:border-white/10 dark:bg-white/5">
            <Wallet size={28} className="text-slate-300 dark:text-slate-600" />
          </div>
          <div>
            <p className="text-base font-semibold text-slate-700 dark:text-slate-200">
              Authentication required
            </p>
            <p className="mt-1 max-w-sm text-sm text-slate-400 dark:text-slate-500">
              Connect your wallet and sign in to view live bridge receiver
              metrics.
            </p>
          </div>
        </div>
      )}

      {/* Error state */}
      {showError && (
        <div className="glass-card flex flex-col items-center justify-center gap-4 py-20 text-center">
          <div className="flex h-14 w-14 items-center justify-center rounded-2xl border border-red-200 bg-red-50 dark:border-red-700/40 dark:bg-red-900/20">
            <AlertCircle size={26} className="text-red-400" />
          </div>
          <div>
            <p className="text-base font-semibold text-slate-700 dark:text-slate-200">
              Failed to load bridge status
            </p>
            <p className="mt-1 max-w-xs text-sm text-slate-400 dark:text-slate-500">
              {error}
            </p>
          </div>
          <button onClick={fetchStatus} className="btn-primary flex items-center gap-2">
            <RefreshCw size={15} />
            Try Again
          </button>
        </div>
      )}

      {/* Skeletons */}
      {showSkeletons && (
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {[0, 1, 2, 3].map((i) => (
            <MetricSkeleton key={i} />
          ))}
        </div>
      )}

      {/* Dashboard content */}
      {showContent && (
        <>
          {/* Status summary */}
          <div className="flex flex-wrap items-center gap-3">
            <StatusBadge
              isOk={Boolean(status.enabled)}
              okLabel="Relayer enabled"
              badLabel="Relayer disabled"
            />
            <StatusBadge
              isOk={Boolean(status.configured)}
              okLabel="Configured"
              badLabel="Not configured"
            />
            <span className="inline-flex items-center gap-1.5 rounded-full bg-stellar-blue/10 px-3 py-1 text-xs font-semibold text-stellar-blue">
              <ArrowLeftRight size={12} />
              {DIRECTION_LABELS[status.direction] ?? status.direction}
            </span>
          </div>

          {/* Metrics */}
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <MetricCard Icon={Eye} label="Events observed" value={stats.observed} />
            <MetricCard
              Icon={CheckCheck}
              label="Commands relayed"
              value={stats.relayed}
              tone="text-emerald-500"
            />
            <MetricCard
              Icon={XCircle}
              label="Failed relays"
              value={stats.failed}
              tone="text-red-500"
            />
            <MetricCard
              Icon={Inbox}
              label="Queue pending"
              value={queue.pending}
              tone="text-amber-500"
            />
          </div>

          {/* Activity details */}
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <div className="glass-card space-y-4 p-6">
              <h3 className="flex items-center gap-2 text-base font-semibold text-slate-900 dark:text-white">
                <SkipForward size={16} className="text-stellar-blue" />
                Activity
              </h3>
              <dl className="space-y-3 text-sm">
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">
                    Events skipped
                  </dt>
                  <dd className="font-medium tabular-nums text-slate-900 dark:text-white">
                    {formatNumber(stats.skipped)}
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">
                    Currently processing
                  </dt>
                  <dd className="font-medium tabular-nums text-slate-900 dark:text-white">
                    {formatNumber(queue.processing)}
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">
                    Last event observed
                  </dt>
                  <dd className="font-medium text-slate-900 dark:text-white">
                    {formatTimestamp(stats.lastObservedAt)}
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">
                    Last command relayed
                  </dt>
                  <dd className="font-medium text-slate-900 dark:text-white">
                    {formatTimestamp(stats.lastRelayedAt)}
                  </dd>
                </div>
              </dl>
            </div>

            <div className="glass-card space-y-4 p-6">
              <h3 className="flex items-center gap-2 text-base font-semibold text-slate-900 dark:text-white">
                <AlertCircle size={16} className="text-stellar-blue" />
                Health &amp; configuration
              </h3>
              <dl className="space-y-3 text-sm">
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">
                    Soroban account
                  </dt>
                  <dd className="max-w-[180px] truncate font-mono text-sm text-stellar-blue">
                    {status.config?.sorobanAccountId ?? 'not set'}
                  </dd>
                </div>
                <div className="flex items-center justify-between gap-4">
                  <dt className="text-slate-500 dark:text-slate-400">
                    EVM bridge address
                  </dt>
                  <dd className="max-w-[180px] truncate font-mono text-sm text-stellar-blue">
                    {status.config?.evmBridgeAddress ?? 'not set'}
                  </dd>
                </div>
                <div>
                  <dt className="mb-1 text-slate-500 dark:text-slate-400">
                    Last error
                  </dt>
                  <dd
                    className={`rounded-xl border p-3 text-sm ${
                      stats.lastError
                        ? 'border-red-200 bg-red-50 text-red-600 dark:border-red-700/40 dark:bg-red-900/20 dark:text-red-400'
                        : 'border-black/5 bg-black/3 text-slate-400 dark:border-white/10 dark:bg-white/5 dark:text-slate-500'
                    }`}
                  >
                    {stats.lastError ?? 'No errors recorded'}
                  </dd>
                </div>
              </dl>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
