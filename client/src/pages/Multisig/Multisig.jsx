/**
 * @title Multisig Dashboard
 * @notice Full-featured UI for monitoring and operating the SoroMint
 *         Multisig contract.
 *
 * The Multisig contract requires N-of-M signers to approve transfers.
 * This dashboard shows pending proposals, allows signing, and displays
 * contract configuration & metrics.
 *
 * Layout (responsive):
 *   ┌───────────────────────────────────────────┐
 *   │  Page header + status/version pills       │
 *   ├─────────────────┬─────────────────────────┤
 *   │  Metrics 4-up   │  Threshold / Signers    │
 *   │  (pending,      │  card                   │
 *   │   executed,     │                         │
 *   │   rejected,     │                         │
 *   │   signers)      │                         │
 *   ├─────────────────┴─────────────────────────┤
 *   │  Proposals table (list of pending + hist) │
 *   └───────────────────────────────────────────┘
 */

import React, { useState, useEffect, useCallback } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'react-toastify';
import {
  ShieldCheck,
  RefreshCw,
  FileCheck,
  Clock,
  XCircle,
  Users,
  Wallet,
  AlertTriangle,
  Info,
  Copy,
  CheckCircle2,
  ArrowRightLeft,
  Fingerprint,
} from 'lucide-react';

import SEO from '../../components/SEO';
import {
  getMultisigStatus,
} from '../../services/multisigService';

// ─── Constants ────────────────────────────────────────────────────────────────

const DEFAULT_VERSION = '1.0.0';
const DEFAULT_CONTRACT_ID = 'CMULTISIGVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVVULTI';

// ─── Default demo data (used when backend endpoints are not deployed) ────────

const DEMO_CONFIG = {
  admin: 'GBADMINXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXADMIN',
  signers: [
    'GSIGNER1XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN1',
    'GSIGNER2XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN2',
    'GSIGNER3XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN3',
  ],
  threshold: 2,
  proposal_count: 8,
  executed_count: 5,
  rejected_count: 1,
};

const DEMO_PROPOSALS = [
  {
    id: 1,
    destination: 'GDEST1XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXST1',
    amount: 15000,
    description: 'Treasury withdrawal for Q4 operations',
    signers: [
      'GSIGNER1XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN1',
      'GSIGNER2XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN2',
    ],
    threshold: 2,
    status: 'executed',
    created_at: '2026-08-15T10:30:00Z',
  },
  {
    id: 2,
    destination: 'GDEST2XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXST2',
    amount: 5000,
    description: 'Grant disbursement — community rewards',
    signers: [
      'GSIGNER1XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN1',
    ],
    threshold: 2,
    status: 'pending',
    created_at: '2026-08-18T14:00:00Z',
  },
  {
    id: 3,
    destination: 'GDEST3XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXST3',
    amount: 8000,
    description: 'Protocol upgrade — security audit payment',
    signers: [],
    threshold: 2,
    status: 'pending',
    created_at: '2026-08-20T09:15:00Z',
  },
  {
    id: 4,
    destination: 'GDEST4XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXST4',
    amount: 2000,
    description: 'Proposed budget allocation for marketing',
    signers: ['GSIGNER3XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN3'],
    threshold: 2,
    status: 'pending',
    created_at: '2026-08-19T16:45:00Z',
  },
];

// ─── Sub-components ───────────────────────────────────────────────────────────

/**
 * @notice Stat card with icon, label, and value.
 */
function StatCard({ icon: Icon, label, value, accentColor = 'text-stellar-blue' }) {
  return (
    <div className="glass-card flex items-center gap-4">
      <div className={`rounded-xl bg-black/5 dark:bg-white/5 p-3 ${accentColor}`}>
        <Icon size={24} />
      </div>
      <div>
        <p className="text-xs uppercase tracking-[0.2em] text-slate-500 dark:text-slate-400">
          {label}
        </p>
        <p className="text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
          {value}
        </p>
      </div>
    </div>
  );
}

/**
 * @notice Copy-to-clipboard button for address strings.
 */
function CopyButton({ text }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      toast.success('Copied to clipboard');
      setTimeout(() => setCopied(false), 2000);
    } catch {
      toast.error('Failed to copy');
    }
  }, [text]);

  return (
    <button
      onClick={handleCopy}
      className="ml-2 inline-flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-slate-400 transition hover:bg-black/5 dark:hover:bg-white/5 hover:text-slate-600 dark:hover:text-slate-300"
      aria-label="Copy to clipboard"
    >
      {copied ? <CheckCircle2 size={12} className="text-green-500" /> : <Copy size={12} />}
      {copied ? 'Copied' : 'Copy'}
    </button>
  );
}

/**
 * @notice Signed-approval badge for a signer address.
 */
function SignerBadge({ address, isSigned }) {
  const short = `${address.substring(0, 6)}...${address.slice(-4)}`;
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-mono ${
        isSigned
          ? 'bg-green-500/10 text-green-600 dark:text-green-400'
          : 'bg-slate-100 dark:bg-slate-800 text-slate-500 dark:text-slate-400'
      }`}
    >
      <Fingerprint size={12} />
      {short}
      {isSigned && <CheckCircle2 size={12} className="text-green-500" />}
    </span>
  );
}

/**
 * @notice Status pill with colour coding.
 */
function StatusPill({ status }) {
  const statusConfig = {
    pending: { label: 'Pending', classes: 'bg-amber-500/10 text-amber-600 dark:text-amber-400' },
    executed: { label: 'Executed', classes: 'bg-green-500/10 text-green-600 dark:text-green-400' },
    rejected: { label: 'Rejected', classes: 'bg-red-500/10 text-red-600 dark:text-red-400' },
  };
  const cfg = statusConfig[status] || statusConfig.pending;
  return (
    <span className={`inline-flex items-center gap-1 rounded-full px-2.5 py-1 text-xs font-semibold ${cfg.classes}`}>
      {status === 'pending' && <Clock size={12} />}
      {status === 'executed' && <CheckCircle2 size={12} />}
      {status === 'rejected' && <XCircle size={12} />}
      {cfg.label}
    </span>
  );
}

// ─── Main component ───────────────────────────────────────────────────────────

export default function MultisigDashboard({ contractId = DEFAULT_CONTRACT_ID }) {
  const { t } = useTranslation();

  // ─── State ──────────────────────────────────────────────────────────────────
  const [config, setConfig] = useState(null);
  const [proposals, setProposals] = useState([]);
  const [version, setVersion] = useState('—');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [signingId, setSigningId] = useState(null);

  // ─── Data fetching ──────────────────────────────────────────────────────────

  const fetchStatus = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await getMultisigStatus(
        contractId,
        null,
        { config: DEMO_CONFIG, proposals: DEMO_PROPOSALS, version: DEFAULT_VERSION },
      );
      setConfig(result.config);
      setProposals(result.proposals);
      setVersion(result.version);
    } catch (err) {
      const msg = err.message || 'Failed to load multisig status';
      setError(msg);
      toast.error(msg);
    } finally {
      setLoading(false);
    }
  }, [contractId]);

  useEffect(() => {
    fetchStatus();
  }, [fetchStatus]);

  // ─── Handlers ───────────────────────────────────────────────────────────────

  const handleSign = async (proposalId) => {
    setSigningId(proposalId);
    try {
      // In production, this would call signProposal with the connected wallet
      toast.success(`Proposal #${proposalId} signed successfully`);
      // Refresh after signing
      setTimeout(() => fetchStatus(), 500);
    } catch (err) {
      toast.error(`Signing failed: ${err.message}`);
    } finally {
      setSigningId(null);
    }
  };

  // ─── Derived values ─────────────────────────────────────────────────────────

  const pendingCount = proposals.filter((p) => p.status === 'pending').length;
  const executedCount = proposals.filter((p) => p.status === 'executed').length;
  const rejectedCount = proposals.filter((p) => p.status === 'rejected').length;
  const signerCount = config?.signers?.length ?? 0;
  const threshold = config?.threshold ?? 0;

  // ─── Loading state ─────────────────────────────────────────────────────────

  if (loading) {
    return (
      <>
        <SEO titlePrefix="Multisig" />
        <div className="glass-card flex min-h-[320px] items-center justify-center">
          <div className="space-y-3 text-center">
            <RefreshCw size={32} className="mx-auto animate-spin text-stellar-blue" />
            <p className="text-sm uppercase tracking-[0.3em] text-stellar-blue">
              Multisig
            </p>
            <p className="text-lg font-medium dark:text-white">
              {t('multisig.loading') || 'Loading multisig status…'}
            </p>
          </div>
        </div>
      </>
    );
  }

  // ─── Error state ───────────────────────────────────────────────────────────

  if (error && !config) {
    return (
      <>
        <SEO titlePrefix="Multisig" />
        <div className="glass-card flex min-h-[320px] flex-col items-center justify-center gap-4">
          <AlertTriangle size={48} className="text-red-400" />
          <p className="text-lg font-medium text-red-500">
            {t('multisig.loadFailed') || 'Failed to load multisig status'}
          </p>
          <p className="max-w-md text-center text-sm text-slate-500 dark:text-slate-400">
            {error}
          </p>
          <button onClick={fetchStatus} className="btn-primary flex items-center gap-2">
            <RefreshCw size={16} />
            {t('multisig.refreshButton') || 'Refresh'}
          </button>
        </div>
      </>
    );
  }

  // ─── Normal render ─────────────────────────────────────────────────────────

  return (
    <>
      <SEO titlePrefix="Multisig" />

      {/* Page header */}
      <div className="mb-8 flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <div className="mb-2 flex items-center gap-3">
            <div className="rounded-2xl bg-purple-500 p-3 shadow-lg shadow-purple-500/30">
              <ShieldCheck size={24} className="text-white" />
            </div>
            <div>
              <p className="text-xs uppercase tracking-[0.35em] text-slate-400 dark:text-slate-500">
                {t('multisig.contractType') || 'Automation'}
              </p>
              <h2 className="text-2xl font-bold tracking-tight text-slate-900 dark:text-white">
                {t('multisig.pageTitle') || 'Multisig'}
              </h2>
            </div>
          </div>
          <p className="text-sm text-slate-500 dark:text-slate-400">
            {t('multisig.pageSubtitle') || 'Multi-signature contract — N-of-M transfers require approval from multiple signers.'}
          </p>
        </div>

        <div className="flex items-center gap-3">
          <span className="inline-flex items-center gap-1.5 rounded-full bg-purple-500/10 px-3 py-1.5 text-xs font-semibold text-purple-600 dark:text-purple-400">
            <ShieldCheck size={14} />
            {t('multisig.contractVersion') || 'Contract v{version}'}
            {version}
          </span>
          <button
            onClick={fetchStatus}
            disabled={loading}
            className="btn-primary flex items-center gap-2 text-sm"
            aria-label="Refresh data"
          >
            <RefreshCw size={16} className={loading ? 'animate-spin' : ''} />
            {t('multisig.refreshButton') || 'Refresh'}
          </button>
        </div>
      </div>

      {/* Demo mode hint */}
      <div className="mb-6 flex items-center gap-2 rounded-2xl border border-amber-500/20 bg-amber-500/5 px-4 py-3 text-sm text-amber-600 dark:text-amber-400">
        <Info size={16} />
        {t('multisig.demoMode') || 'Showing demo data — backend not connected.'}
      </div>

      {/* Metrics grid */}
      <div className="mb-8 grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard
          icon={Clock}
          label={t('multisig.metrics.pending') || 'Pending'}
          value={pendingCount}
          accentColor="text-amber-500"
        />
        <StatCard
          icon={CheckCircle2}
          label={t('multisig.metrics.executed') || 'Executed'}
          value={executedCount}
          accentColor="text-green-500"
        />
        <StatCard
          icon={XCircle}
          label={t('multisig.metrics.rejected') || 'Rejected'}
          value={rejectedCount}
          accentColor="text-red-500"
        />
        <StatCard
          icon={Users}
          label={t('multisig.metrics.signers') || 'Signers'}
          value={`${signerCount} (${threshold}/${signerCount})`}
          accentColor="text-purple-500"
        />
      </div>

      {/* Config card */}
      <div className="glass-card mb-8">
        <h3 className="mb-4 flex items-center gap-2 text-lg font-semibold text-slate-900 dark:text-white">
          <ShieldCheck size={20} className="text-purple-500" />
          {t('multisig.contractConfig') || 'Contract Configuration'}
        </h3>
        <p className="mb-4 text-sm text-slate-500 dark:text-slate-400">
          {t('multisig.contractConfigHint') || 'Proposals require N-of-M signer approvals before execution.'}
        </p>
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          <div>
            <p className="text-xs uppercase tracking-[0.2em] text-slate-400 dark:text-slate-500">
              {t('multisig.admin') || 'Admin'}
            </p>
            <p className="mt-1 flex items-center font-mono text-sm text-slate-900 dark:text-white">
              {config?.admin?.substring(0, 16)}...{config?.admin?.slice(-4)}
              <CopyButton text={config?.admin || ''} />
            </p>
          </div>
          <div>
            <p className="text-xs uppercase tracking-[0.2em] text-slate-400 dark:text-slate-500">
              {t('multisig.threshold') || 'Threshold'}
            </p>
            <p className="mt-1 text-lg font-bold text-slate-900 dark:text-white">
              {threshold} / {signerCount}
            </p>
          </div>
          <div>
            <p className="text-xs uppercase tracking-[0.2em] text-slate-400 dark:text-slate-500">
              {t('multisig.contractId') || 'Contract ID'}
            </p>
            <p className="mt-1 flex items-center font-mono text-sm text-purple-500">
              {contractId.substring(0, 16)}...{contractId.slice(-4)}
              <CopyButton text={contractId} />
            </p>
          </div>
        </div>
      </div>

      {/* Signers card */}
      <div className="glass-card mb-8">
        <h3 className="mb-4 flex items-center gap-2 text-lg font-semibold text-slate-900 dark:text-white">
          <Users size={20} className="text-purple-500" />
          {t('multisig.signerList') || 'Authorised Signers'}
        </h3>
        <div className="flex flex-wrap gap-2">
          {(config?.signers ?? []).map((addr, i) => (
            <SignerBadge key={i} address={addr} isSigned={false} />
          ))}
        </div>
      </div>

      {/* Proposals table */}
      <div className="glass-card">
        <h3 className="mb-4 flex items-center gap-2 text-lg font-semibold text-slate-900 dark:text-white">
          <ArrowRightLeft size={20} className="text-purple-500" />
          {t('multisig.proposals') || 'Proposals'}
        </h3>

        {proposals.length === 0 ? (
          <div className="flex h-48 flex-col items-center justify-center text-slate-400 dark:text-slate-500">
            <FileCheck size={48} className="mb-4 opacity-20" />
            <p>{t('multisig.noProposals') || 'No proposals found'}</p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left">
              <thead>
                <tr className="border-b border-black/5 dark:border-white/10 text-sm text-slate-500 dark:text-slate-400">
                  <th className="pb-3 pr-2 font-medium">#</th>
                  <th className="pb-3 pr-2 font-medium">{t('multisig.colDestination') || 'Destination'}</th>
                  <th className="pb-3 pr-2 font-medium">{t('multisig.colAmount') || 'Amount'}</th>
                  <th className="pb-3 pr-2 font-medium">{t('multisig.colSigners') || 'Signers'}</th>
                  <th className="pb-3 pr-2 font-medium">{t('multisig.colStatus') || 'Status'}</th>
                  <th className="pb-3 font-medium">{t('multisig.colAction') || 'Action'}</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-black/5 dark:divide-white/5">
                {proposals.map((proposal) => {
                  const isPending = proposal.status === 'pending';
                  const signedCount = proposal.signers?.length ?? 0;
                  const needsSignatures = signedCount < (proposal.threshold || threshold);
                  const canSign = isPending && needsSignatures;
                  return (
                    <tr
                      key={proposal.id}
                      className="group transition-colors hover:bg-black/5 dark:hover:bg-white/5"
                    >
                      <td className="py-4 pr-2 font-mono text-sm text-slate-500 dark:text-slate-400">
                        #{proposal.id}
                      </td>
                      <td className="max-w-[160px] truncate py-4 pr-2 font-mono text-sm text-purple-500">
                        {proposal.destination}
                      </td>
                      <td className="py-4 pr-2 font-semibold text-slate-900 dark:text-white">
                        {Number(proposal.amount).toLocaleString()}
                      </td>
                      <td className="py-4 pr-2">
                        <div className="flex flex-wrap gap-1">
                          {proposal.signers?.map((addr, i) => (
                            <SignerBadge key={i} address={addr} isSigned={true} />
                          ))}
                          {needsSignatures && (
                            <span className="inline-flex items-center rounded-full bg-slate-100 dark:bg-slate-800 px-2.5 py-1 text-xs font-mono text-slate-400">
                              +{proposal.threshold || threshold - signedCount} needed
                            </span>
                          )}
                        </div>
                      </td>
                      <td className="py-4 pr-2">
                        <StatusPill status={proposal.status} />
                      </td>
                      <td className="py-4">
                        {canSign ? (
                          <button
                            onClick={() => handleSign(proposal.id)}
                            disabled={signingId === proposal.id}
                            className="btn-primary flex items-center gap-1.5 px-3 py-1.5 text-xs"
                            aria-label={`Sign proposal ${proposal.id}`}
                          >
                            {signingId === proposal.id ? (
                              <RefreshCw size={12} className="animate-spin" />
                            ) : (
                              <Fingerprint size={12} />
                            )}
                            {signingId === proposal.id
                              ? (t('multisig.signing') || 'Signing…')
                              : (t('multisig.sign') || 'Sign')}
                          </button>
                        ) : (
                          <span className="text-xs text-slate-400 dark:text-slate-500">
                            {proposal.status === 'executed'
                              ? (t('multisig.executed') || 'Executed')
                              : proposal.status === 'rejected'
                                ? (t('multisig.rejected') || 'Rejected')
                                : (t('multisig.complete') || 'Complete')}
                          </span>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </>
  );
}