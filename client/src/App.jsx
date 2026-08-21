import React, { Suspense, lazy, useState, useEffect } from "react";
import axios from "axios";
import { useTranslation } from "react-i18next";
import { toast } from "react-toastify";
import {
  Wallet,
  Coins,
  Plus,
  List,
  ArrowRight,
  ShieldCheck,
  LayoutDashboard,
  BookOpenText,
  Vote,
  ScrollText,
} from "lucide-react";

// UI Components & Stores
import ErrorBoundary from "./components/error-boundary";
import { AppCrashPage, SectionCrashCard } from "./components/error-fallbacks";
import { SkeletonList, SkeletonTokenForm } from "./components/Skeleton";
import { useWalletStore, useTokenStore, useUIStore } from "./store";
import ThemeToggle from "./components/ThemeToggle";

const DeveloperHub = lazy(() => import("./components/developer-hub"));
const VotingDashboard = lazy(() => import("./components/VotingDashboard"));
const ZKAuditLogDashboard = lazy(() => import("./pages/ZKAuditLog/ZKAuditLog"));

const API_BASE = "http://localhost:5000/api";

const views = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "governance", label: "Governance", icon: Vote },
  { id: "developer-hub", label: "Developer Hub", icon: BookOpenText },
  { id: "audit-log", label: "Audit Log", icon: ScrollText },
];

/**
 * AppHeader Component
 */
function AppHeader({ address, onConnect, onDisconnect, activeView, setView }) {
  const { t } = useTranslation();

  return (
    <header className="mb-16 flex flex-col gap-6 lg:flex-row lg:items-center lg:justify-between">
      <div className="flex items-center gap-3">
        <div className="rounded-2xl bg-stellar-blue p-3 shadow-lg shadow-blue-500/30">
          <Coins className="h-8 w-8 text-white" />
        </div>
        <div>
          <p className="text-sm uppercase tracking-[0.35em] text-sky-200/70">
            Platform
          </p>
          <h1 className="text-3xl font-bold tracking-tight text-slate-900 dark:text-white">
            Soro<span className="text-stellar-blue">Mint</span>
          </h1>
        </div>
      </div>

      <div className="flex flex-col gap-4 sm:flex-row sm:items-center">
        <nav className="inline-flex rounded-2xl border border-black/5 bg-black/5 p-1.5 dark:border-white/10 dark:bg-slate-950/70 shadow-lg">
          {views.map((view) => {
            const Icon = view.icon;
            const isActive = activeView === view.id;
            return (
              <button
                key={view.id}
                onClick={() => setView(view.id)}
                className={`inline-flex items-center gap-2 rounded-xl px-4 py-2 text-sm font-medium transition ${
                  isActive
                    ? "bg-white dark:bg-white/10 text-stellar-blue dark:text-white shadow-sm"
                    : "text-slate-500 hover:text-slate-900 dark:text-slate-400 dark:hover:text-white"
                }`}
              >
                <Icon size={16} />
                {t(`nav.${view.id}`) || view.label}
              </button>
            );
          })}
        </nav>

        <div className="flex items-center gap-3">
          <ThemeToggle />
          <button
            onClick={address ? onDisconnect : onConnect}
            className="btn-primary flex items-center gap-2"
          >
            <Wallet size={18} />
            {address
              ? `${address.substring(0, 6)}...${address.slice(-4)}`
              : t("app.connectWallet") || "Connect Wallet"}
          </button>
        </div>
      </div>
    </header>
  );
}

/**
 * MintTokenPanel Component
 */
function MintTokenPanel({ address, onTokenMinted }) {
  const { t } = useTranslation();
  const [isMinting, setIsMinting] = useState(false);
  const [formData, setFormData] = useState({
    name: "",
    symbol: "",
    decimals: 7,
  });

  const handleSubmit = async (e) => {
    e.preventDefault();
    if (!address) {
      toast.warn(t("mint.connectFirst") || "Please connect wallet first");
      return;
    }

    setIsMinting(true);
    try {
      const mockContractId =
        "C" + Math.random().toString(36).substring(2, 10).toUpperCase();
      const resp = await axios.post(`${API_BASE}/tokens`, {
        ...formData,
        contractId: mockContractId,
        ownerPublicKey: address,
      });

      const createdToken = resp.data?.data ?? resp.data;
      if (createdToken) {
        onTokenMinted(createdToken);
        setFormData({ name: "", symbol: "", decimals: 7 });
        toast.success(t("mint.success") || "Token minted successfully");
      }
    } catch (err) {
      toast.error(`${t("mint.failed") || "Minting failed"}: ${err.message}`);
    } finally {
      setIsMinting(false);
    }
  };

  return (
    <section className="lg:col-span-1">
      <div className="glass-card">
        <h2 className="mb-6 flex items-center gap-2 text-xl font-semibold text-slate-900 dark:text-white">
          <Plus size={20} className="text-stellar-blue" />
          {t("mint.title") || "Mint New Token"}
        </h2>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="mb-1 block text-sm font-medium text-slate-500 dark:text-slate-400">
              {t("mint.nameLabel") || "Token Name"}
            </label>
            <input
              type="text"
              placeholder="e.g. My Stellar Asset"
              className="input-field w-full"
              value={formData.name}
              onChange={(e) =>
                setFormData({ ...formData, name: e.target.value })
              }
              required
            />
          </div>
          <div>
            <label className="mb-1 block text-sm font-medium text-slate-500 dark:text-slate-400">
              {t("mint.symbolLabel") || "Symbol"}
            </label>
            <input
              type="text"
              placeholder="e.g. MSA"
              className="input-field w-full"
              value={formData.symbol}
              onChange={(e) =>
                setFormData({ ...formData, symbol: e.target.value })
              }
              required
            />
          </div>
          <div>
            <label className="mb-1 block text-sm font-medium text-slate-500 dark:text-slate-400">
              {t("mint.decimalsLabel") || "Decimals"}
            </label>
            <input
              type="number"
              className="input-field w-full"
              value={formData.decimals}
              onChange={(e) =>
                setFormData({
                  ...formData,
                  decimals: parseInt(e.target.value, 10) || 0,
                })
              }
              required
            />
          </div>
          <button
            type="submit"
            disabled={isMinting || !address}
            className="btn-primary mt-4 flex w-full items-center justify-center gap-2 disabled:opacity-50"
          >
            {isMinting
              ? t("mint.buttonMinting") || "Deploying..."
              : t("mint.buttonMint") || "Mint Token"}
            {!isMinting && <ArrowRight size={18} />}
          </button>
        </form>
      </div>
    </section>
  );
}

/**
 * AssetsPanel Component
 */
function AssetsPanel({ address, tokens, isLoading }) {
  const { t } = useTranslation();

  return (
    <section className="lg:col-span-2">
      <div className="glass-card min-h-[400px]">
        <h2 className="mb-6 flex items-center gap-2 text-xl font-semibold text-slate-900 dark:text-white">
          <List size={20} className="text-stellar-blue" />
          {t("assets.title") || "My Assets"}
        </h2>

        {!address ? (
          <div className="flex h-64 flex-col items-center justify-center text-slate-400 dark:text-slate-500">
            <ShieldCheck size={48} className="mb-4 opacity-20" />
            <p>
              {t("assets.connectWallet") ||
                "Connect your wallet to see your assets"}
            </p>
          </div>
        ) : isLoading ? (
          <SkeletonList count={4} />
        ) : tokens.length === 0 ? (
          <div className="flex h-64 flex-col items-center justify-center text-slate-400 dark:text-slate-500">
            <p>{t("assets.noTokens") || "No tokens minted yet"}</p>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left">
              <thead>
                <tr className="border-b border-black/5 dark:border-white/10 text-sm text-slate-500 dark:text-slate-400">
                  <th className="pb-4 font-medium">
                    {t("assets.name") || "Name"}
                  </th>
                  <th className="pb-4 font-medium">
                    {t("assets.symbol") || "Symbol"}
                  </th>
                  <th className="pb-4 font-medium">
                    {t("assets.contractId") || "Contract ID"}
                  </th>
                  <th className="pb-4 font-medium">
                    {t("assets.decimals") || "Decimals"}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-black/5 dark:divide-white/5">
                {tokens.map((token, index) => (
                  <tr
                    key={index}
                    className="group transition-colors hover:bg-black/5 dark:hover:bg-white/5"
                  >
                    <td className="py-4 font-medium text-slate-900 dark:text-white">
                      {token.name}
                    </td>
                    <td className="py-4 text-slate-600 dark:text-slate-300">
                      {token.symbol}
                    </td>
                    <td className="max-w-[120px] truncate py-4 font-mono text-sm text-stellar-blue">
                      {token.contractId}
                    </td>
                    <td className="py-4 text-slate-500 dark:text-slate-400">
                      {token.decimals}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  );
}

/**
 * Main App Component
 */
function App() {
  const { t } = useTranslation();
  const [activeView, setActiveView] = useState("dashboard");

  // Zustand Hooks
  const { address, setWallet, disconnectWallet } = useWalletStore();
  const { tokens, addToken, isLoading, fetchTokens } = useTokenStore();
  const { initTheme } = useUIStore();

  useEffect(() => {
    initTheme();
    if (address) {
      fetchTokens(address);
    }
  }, [address, fetchTokens, initTheme]);

  const connectWallet = async () => {
    // Mock wallet connection for demo
    const mockAddress =
      "GB" +
      Math.random().toString(36).substring(2, 10).toUpperCase() +
      Math.random().toString(36).substring(2, 10).toUpperCase();
    setWallet(mockAddress);
    toast.success(t("app.walletConnected") || "Wallet connected");
  };

  return (
    <div className="mx-auto max-w-7xl px-4 py-12 sm:px-6 lg:px-8">
      <AppHeader
        address={address}
        onConnect={connectWallet}
        onDisconnect={disconnectWallet}
        activeView={activeView}
        setView={setActiveView}
      />

      {activeView === "governance" ? (
        <Suspense
          fallback={
            <div className="glass-card flex min-h-[320px] items-center justify-center">
              <div className="space-y-3 text-center">
                <p className="text-sm uppercase tracking-[0.3em] text-stellar-blue">
                  Governance
                </p>
                <p className="text-lg font-medium dark:text-white">
                  Loading proposals…
                </p>
              </div>
            </div>
          }
        >
          <ErrorBoundary
            fallbackRender={({ resetErrorBoundary }) => (
              <SectionCrashCard
                title="Governance Unavailable"
                onRetry={resetErrorBoundary}
              />
            )}
          >
            <VotingDashboard address={address} authToken={null} />
          </ErrorBoundary>
        </Suspense>
      ) : activeView === "developer-hub" ? (
        <Suspense
          fallback={
            <div className="glass-card flex min-h-[320px] items-center justify-center">
              <div className="space-y-3 text-center">
                <p className="text-sm uppercase tracking-[0.3em] text-stellar-blue">
                  Developer Hub
                </p>
                <p className="text-lg font-medium dark:text-white">
                  Loading documentation...
                </p>
              </div>
            </div>
          }
        >
          <DeveloperHub />
        </Suspense>
      ) : activeView === "audit-log" ? (
        <Suspense
          fallback={
            <div className="glass-card flex min-h-[320px] items-center justify-center">
              <div className="space-y-3 text-center">
                <p className="text-sm uppercase tracking-[0.3em] text-violet-500">
                  Audit Log
                </p>
                <p className="text-lg font-medium dark:text-white">
                  Loading deployment audit trail…
                </p>
              </div>
            </div>
          }
        >
          <ErrorBoundary
            fallbackRender={({ resetErrorBoundary }) => (
              <SectionCrashCard
                title="Audit Log Unavailable"
                onRetry={resetErrorBoundary}
              />
            )}
          >
            <ZKAuditLogDashboard />
          </ErrorBoundary>
        </Suspense>
      ) : (
        <main className="grid grid-cols-1 gap-8 lg:grid-cols-3">
          <ErrorBoundary
            fallbackRender={({ resetErrorBoundary }) => (
              <SectionCrashCard
                className="lg:col-span-1"
                title="Mint Panel Unavailable"
                onRetry={resetErrorBoundary}
              />
            )}
          >
            <MintTokenPanel address={address} onTokenMinted={addToken} />
          </ErrorBoundary>

          <ErrorBoundary
            fallbackRender={({ resetErrorBoundary }) => (
              <SectionCrashCard
                className="lg:col-span-2"
                title="Assets Panel Unavailable"
                onRetry={resetErrorBoundary}
              />
            )}
          >
            <AssetsPanel
              address={address}
              tokens={tokens}
              isLoading={isLoading}
            />
          </ErrorBoundary>
        </main>
      )}

      <footer className="mt-16 border-t border-black/10 dark:border-white/5 pt-8 text-center text-sm text-slate-400 dark:text-slate-500">
        <p>
          &copy; {new Date().getFullYear()} SoroMint Platform. All rights
          reserved.
        </p>
      </footer>
    </div>
  );
}

export default function AppWithErrorBoundary() {
  return (
    <ErrorBoundary fallback={<AppCrashPage />}>
      <App />
    </ErrorBoundary>
  );
}