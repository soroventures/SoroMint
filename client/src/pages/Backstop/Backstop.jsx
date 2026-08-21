/**
 * @title Backstop / Insurance Fund Dashboard
 * @notice Full-featured UI for monitoring and operating the SoroMint
 *         Backstop (insurance fund) contract.
 *
 * The Backstop collects protocol fees and holds them as a reserve. In the
 * event of an exploit or liquidation shortfall the admin can draw from the
 * fund to cover losses.
 *
 * Layout (responsive):
 *   ┌───────────────────────────────────────┐
 *   │  Page header + status/version pills   │
 *   ├───────────────┬───────────────────────┤
 *   │  Metrics 4-up (balance, deposits,     │
 *   │  withdrawals, fee rate)               │
 *   ├───────────────┴───────────────────────┤
 *   │  Config card  │  Fee calculator card  │
 *   └───────────────┴───────────────────────┘
 */

import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import {
  ShieldCheck,
  RefreshCw,
  Coins,
  TrendingUp,
  TrendingDown,
  Percent,
  Wallet,
  AlertTriangle,
  Info,
  Copy,
  CheckCircle2,
  ArrowRightLeft,
} from 'lucide-react';

import SEO from '../../components/SEO';
import {
  getBackstopStatus,
  calcFee,
} from '../../services/backstopService';

// ─── Constants ────────────────────────────────────────────────────────────────

/** Backstop contract is a v1.0.0 insurance fund (see contracts/backstop). */
const DEFAULT_VERSION = '1.0.0';
const DEFAULT_CONTRACT_ID = 'CBSKTOPVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVSTOP';

// ─── Default demo config (used when backend endpoints are not deployed) ──────

const DEMO_CONFIG = {
  admin: 'GBADMINXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXADMIN',
  token: 'GTOKENXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXTOKEN',
  fee_bps: 500,
  total_deposited: 250000,
  total_withdrawn: 50000,
};
const DEMO_BALANCE = 200000;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/**
 * @notice Truncate a Stellar C-address or G-address to "CABC…WXYZ" form.
 */
const truncateId = (id) => {
  if (!id || id === '—') return '—';
  if (id.length <= 14) return id;
  return `${id.slice(0, 8)}…${id.slice(-6)}`;
};

/**
 * @notice Format an integer amount with grouping separators.
 */
const formatAmount = (amount) => {
  if (amount === null || amount === undefined || Number.isNaN(Number(amount))) {
    return '—';
  }
  return Number(amount).toLocaleString(undefined, { maximumFractionDigits: 0 });
};

/**
 * @notice Convert fee basis points to a human-friendly percent string (e.g.
 *         500 → "5.00%").
 */
const bpsToPercent = (bps) => {
  const pct = (Number(bps) || 0) / 100;
  return `${pct.toFixed(pct % 1 === 0 ? 0 : 2)}%`;
};

// ─── Sub-component: Status pill ───────────────────────────────────────────────

function StatusPill() {
  const { t } = useTranslation();
  return (
    <span
      className="inline-flex items-center gap-1.5 rounded-full border border-green-200 bg-green-100 px-3 py-1 text-xs font-semibold text-green-700 dark:border-green-700/40 dark:bg-green-900/30 dark:text-green-400"
      data-testid="backstop-status-pill"
    >
      <span className="relative flex h-2 w-2">
        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-green-400 opacity-75" />
        <span className="relative inline-flex h-2 w-2 rounded-full bg-green-500" />
      </span>
      {t('backstop.live') || 'Live'}
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

// ─── Sub-component: Config row ────────────────────────────────────────────────

function ConfigRow({ label, value, mono, copyable }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable — ignore */
    }
  };

  return (
    <div className="flex items-center justify-between gap-4 py-2.5">
      <span className="text-sm text-slate-500 dark:text-slate-400">{label}</span>
      <div className="flex items-center gap-2">
        <span
          className={`text-sm font-medium text-slate-900 dark:text-white ${
            mono ? 'font-mono text-stellar-blue' : ''
          }`}
          data-testid={`config-${label.replace(/\s+/g, '-').toLowerCase()}`}
        >
          {value}
        </span>
        {copyable && value !== '—' && (
          <button
            type="button"
            onClick={onCopy}
            className="text-slate-400 transition hover:text-stellar-blue dark:text-slate-500"
            aria-label={t('backstop.copy') || 'Copy to clipboard'}
          >
            {copied ? <CheckCircle2 size={14} /> : <Copy size={14} />}
          </button>
        )}
      </div>
    </div>
  );
}

// ─── Sub-component: Fee calculator ────────────────────────────────────────────

function FeeCalculator({ feeBps }) {
  const { t } = useTranslation();
  const [principal, setPrincipal] = useState('');

  const fee = principal ? calcFee(principal, feeBps) : null;

  return (
    <div className="glass-card">
      <h2 className="mb-4 flex items-center gap-2 text-lg font-semibold text-slate-900 dark:text-white">
        <Percent size={18} className="text-stellar-blue" />
        {t('backstop.feeCalculator') || 'Fee Calculator'}
      </h2>
      <p className="mb-4 text-sm text-slate-500 dark:text-slate-400">
        {t('backstop.feeCalculatorHint') ||
          'Fee = principal × fee_bps ÷ 10,000, matching calc_fee on-chain.'}
      </p>

      <label className="mb-1 block text-sm font-medium text-slate-500 dark:text-slate-400">
        {t('backstop.principalLabel') || 'Principal Amount'}
      </label>
      <input
        type="number"
        min="0"
        inputMode="numeric"
        className="input-field w-full"
        placeholder="e.g. 10000"
        value={principal}
        onChange={(e) => setPrincipal(e.target.value)}
        aria-label={t('backstop.principalLabel') || 'Principal Amount'}
      />

      <div className="mt-4 flex items-center justify-between rounded-2xl border border-black/5 bg-black/5 px-4 py-3 dark:border-white/10 dark:bg-white/5">
        <span className="text-sm text-slate-500 dark:text-slate-400">
          {t('backstop.feeAmount') || 'Fee charged'}
        </span>
        <span
          className="text-xl font-bold tabular-nums text-stellar-blue"
          data-testid="fee-result"
        >
          {fee === null ? '—' : fee.toLocaleString()}
        </span>
      </div>
      <p className="mt-2 text-xs text-slate-400 dark:text-slate-500">
        {t('backstop.rate', { rate: bpsToPercent(feeBps) }) ||
          `Current rate: ${bpsToPercent(feeBps)}`}
      </p>
    </div>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function BackstopDashboard({ contractId = DEFAULT_CONTRACT_ID }) {
  const { t } = useTranslation();

  const [config, setConfig] = useState(null);
  const [balance, setBalance] = useState(null);
  const [version, setVersion] = useState(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState(null);
  const [loadedWithFallback, setLoadedWithFallback] = useState(false);

  const loadStatus = useCallback(
    async (showToast = true) => {
      setIsLoading(true);
      setError(null);
      try {
        const { config: cfg, balance: bal, version: ver } = await getBackstopStatus(
          contractId,
          null,
          { config: DEMO_CONFIG, balance: DEMO_BALANCE, version: DEFAULT_VERSION },
        );

        // If the backend proxy is absent, the service falls back to the demo
        // payload — surface a subtle hint so users know it's demo data.
        const usedFallback =
          cfg.admin === DEMO_CONFIG.admin &&
          cfg.token === DEMO_CONFIG.token &&
          (cfg.feeBps === DEMO_CONFIG.fee_bps || cfg.feeBps === undefined);

        setConfig(cfg);
        setBalance(bal);
        setVersion(ver);
        setLoadedWithFallback(usedFallback);

        if (showToast && usedFallback) {
          toast.info(t('backstop.demoMode') || 'Showing demo data — backend not connected.');
        }
      } catch (err) {
        setError(err.message);
        toast.error(`${t('backstop.loadFailed') || 'Failed to load backstop status'}: ${err.message}`);
      } finally {
        setIsLoading(false);
      }
    },
    [contractId, t],
  );

  useEffect(() => {
    loadStatus(false);
  }, [loadStatus]);

  return (
    <>
      <SEO title={`${t('backstop.pageTitle') || 'Backstop'} | SoroMint`} path="/backstop" />

      {/* Page header */}
      <div className="mb-8 flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-4">
          <div className="rounded-2xl bg-emerald-500 p-3 shadow-lg shadow-emerald-500/30">
            <ShieldCheck className="h-8 w-8 text-white" />
          </div>
          <div>
            <h1 className="text-2xl font-bold tracking-tight text-slate-900 dark:text-white sm:text-3xl">
              {t('backstop.pageTitle') || 'Backstop'}
            </h1>
            <p className="mt-1 text-sm text-slate-500 dark:text-slate-400">
              {t('backstop.pageSubtitle') ||
                'Protocol insurance fund — reserve for exploit & shortfall coverage'}
            </p>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <StatusPill />
          <span className="inline-flex items-center gap-1.5 rounded-full border border-slate-200 bg-slate-100 px-3 py-1 text-xs font-semibold text-slate-600 dark:border-white/10 dark:bg-white/5 dark:text-slate-300">
            <Info size={12} />
            {t('backstop.contractVersion', { version: version || DEFAULT_VERSION }) ||
              `Contract v${version || DEFAULT_VERSION}`}
          </span>
          <button
            type="button"
            onClick={() => loadStatus()}
            disabled={isLoading}
            className="inline-flex items-center gap-2 rounded-xl border border-black/5 bg-white px-4 py-2 text-sm font-medium text-slate-700 shadow-sm transition hover:bg-slate-50 disabled:opacity-50 dark:border-white/10 dark:bg-white/5 dark:text-slate-200 dark:hover:bg-white/10"
            aria-label={t('backstop.refreshButton') || 'Refresh'}
          >
            <RefreshCw size={14} className={isLoading ? 'animate-spin' : ''} />
            {isLoading
              ? t('backstop.loading') || 'Loading…'
              : t('backstop.refreshButton') || 'Refresh'}
          </button>
        </div>
      </div>

      {/* Error banner */}
      {error && (
        <div
          className="mb-6 flex items-center gap-3 rounded-2xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-700/40 dark:bg-red-900/20 dark:text-red-300"
          role="alert"
        >
          <AlertTriangle size={16} />
          {error}
        </div>
      )}

      {/* Demo-mode hint */}
      {loadedWithFallback && !error && (
        <div
          className="mb-6 flex items-center gap-3 rounded-2xl border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-700/40 dark:bg-amber-900/20 dark:text-amber-300"
          data-testid="demo-hint"
        >
          <Info size={16} />
          {t('backstop.demoMode') ||
            'Showing demo data — the backend Soroban RPC proxy is not connected yet.'}
        </div>
      )}

      {/* Metrics 4-up */}
      <div className="mb-8 grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <MetricCard
          label={t('backstop.metrics.balance') || 'Reserve Balance'}
          value={isLoading ? '' : formatAmount(balance)}
          icon={Coins}
          color="bg-emerald-500"
          isLoading={isLoading}
        />
        <MetricCard
          label={t('backstop.metrics.totalDeposited') || 'Total Deposited'}
          value={isLoading ? '' : formatAmount(config?.totalDeposited)}
          icon={TrendingUp}
          color="bg-stellar-blue"
          isLoading={isLoading}
        />
        <MetricCard
          label={t('backstop.metrics.totalWithdrawn') || 'Total Withdrawn'}
          value={isLoading ? '' : formatAmount(config?.totalWithdrawn)}
          icon={TrendingDown}
          color="bg-rose-500"
          isLoading={isLoading}
        />
        <MetricCard
          label={t('backstop.metrics.feeRate') || 'Fee Rate'}
          value={isLoading ? '' : bpsToPercent(config?.feeBps)}
          icon={Percent}
          color="bg-violet-500"
          isLoading={isLoading}
        />
      </div>

      {/* Config + calculator */}
      <div className="grid grid-cols-1 gap-8 lg:grid-cols-2">
        <div className="glass-card">
          <h2 className="mb-2 flex items-center gap-2 text-lg font-semibold text-slate-900 dark:text-white">
            <Wallet size={18} className="text-stellar-blue" />
            {t('backstop.contractConfig') || 'Contract Configuration'}
          </h2>
          <p className="mb-4 text-sm text-slate-500 dark:text-slate-400">
            {t('backstop.contractConfigHint') ||
              'Admin can withdraw funds for coverage and update the fee rate.'}
          </p>

          <div className="divide-y divide-black/5 dark:divide-white/5">
            <ConfigRow
              label={t('backstop.admin') || 'Admin'}
              value={isLoading ? '…' : truncateId(config?.admin)}
              mono
              copyable={!isLoading}
            />
            <ConfigRow
              label={t('backstop.token') || 'Reserve Token'}
              value={isLoading ? '…' : truncateId(config?.token)}
              mono
              copyable={!isLoading}
            />
            <ConfigRow
              label={t('backstop.feeBps') || 'Fee (bps)'}
              value={isLoading ? '…' : `${config?.feeBps ?? 0} bps`}
            />
          </div>
        </div>

        <FeeCalculator feeBps={config?.feeBps ?? 0} />
      </div>

      {/* Operations note */}
      <div
        className="mt-8 flex items-start gap-3 rounded-2xl border border-black/5 bg-black/5 px-4 py-4 text-sm text-slate-500 dark:border-white/10 dark:bg-white/5 dark:text-slate-400"
        data-testid="ops-note"
      >
        <ArrowRightLeft size={16} className="mt-0.5 shrink-0 text-stellar-blue" />
        <p>
          {t('backstop.opsNote') ||
            'Deposits and withdrawals are executed through the Soroban contract by the protocol or the admin wallet and will appear here automatically after the transaction settles.'}
        </p>
      </div>
    </>
  );
}