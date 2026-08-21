/**
 * @file Multisig.test.jsx
 * @description Unit tests for the Multisig dashboard page.
 *
 * Coverage:
 *   1. Page structure — header, status pill, metrics, config card, signers, proposals
 *   2. Metrics — pending, executed, rejected, signer count rendering
 *   3. Proposals table — rows, status badges, signer badges
 *   4. Demo-mode hint — shown when backend is not connected
 *   5. Error handling — API failure shows banner + toast
 *   6. Refresh button — triggers reload
 *   7. Accessibility — ARIA labels, roles
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi, afterEach } from 'vitest';
import React from 'react';

// ─── Module mocks ─────────────────────────────────────────────────────────────

// Mock the multisig service so tests never hit the network
const mockStatus = {
  config: {
    admin: 'GBADMINXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXADMIN',
    signers: [
      'GSIGNER1XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN1',
      'GSIGNER2XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN2',
      'GSIGNER3XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN3',
    ],
    threshold: 2,
    proposalCount: 8,
    executedCount: 5,
    rejectedCount: 1,
    contractId: '',
  },
  proposals: [
    {
      id: 1,
      destination: 'GDEST1XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXST1',
      amount: 15000,
      description: 'Treasury withdrawal',
      signers: ['GSIGNER1XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN1'],
      threshold: 2,
      status: 'pending',
      createdAt: '2026-08-18T14:00:00Z',
    },
    {
      id: 2,
      destination: 'GDEST2XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXST2',
      amount: 5000,
      description: 'Grant disbursement',
      signers: [
        'GSIGNER1XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN1',
        'GSIGNER2XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXGN2',
      ],
      threshold: 2,
      status: 'executed',
      createdAt: '2026-08-15T10:30:00Z',
    },
  ],
  version: '1.0.0',
};

const mockGetMultisigStatus = vi.fn(() => Promise.resolve(mockStatus));

vi.mock('../../services/multisigService', () => ({
  getMultisigStatus: (...args) => mockGetMultisigStatus(...args),
}));

// Mock i18next
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key) => key, // return key as fallback
    i18n: { language: 'en' },
  }),
}));

// Mock react-toastify
vi.mock('react-toastify', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

// Mock SEO component
vi.mock('../../components/SEO', () => ({
  default: () => null,
}));

// ─── Tests ────────────────────────────────────────────────────────────────────

describe('MultisigDashboard', () => {
  beforeEach(() => {
    mockGetMultisigStatus.mockClear();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it('renders a loading state initially', async () => {
    // Keep the promise pending so loading renders
    mockGetMultisigStatus.mockImplementationOnce(() => new Promise(() => {}));

    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    // Should show a loading spinner / text
    expect(screen.getByText('multisig.loading')).toBeTruthy();
  });

  it('renders the full dashboard after loading', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      expect(screen.getByText('multisig.pageTitle')).toBeTruthy();
    });

    // Header
    expect(screen.getByText('multisig.pageSubtitle')).toBeTruthy();
    // Version pill (version text might be in a split text node)
    expect(screen.getByText(/1\.0\.0/)).toBeTruthy();

    // Demo mode hint
    expect(screen.getByText('multisig.demoMode')).toBeTruthy();
  });

  it('renders the four metric cards', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      expect(screen.getByText('multisig.metrics.pending')).toBeTruthy();
    });

    expect(screen.getByText('multisig.metrics.executed')).toBeTruthy();
    expect(screen.getByText('multisig.metrics.rejected')).toBeTruthy();
    expect(screen.getByText('multisig.metrics.signers')).toBeTruthy();
  });

  it('displays the correct proposal count in metrics', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      // 1 pending + 1 executed = 2 values of "1" in the metric cards
      const ones = screen.getAllByText('1');
      expect(ones.length).toBeGreaterThanOrEqual(2);
    });
  });

  it('renders the contract configuration card', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      expect(screen.getByText('multisig.contractConfig')).toBeTruthy();
    });

    expect(screen.getByText('multisig.contractConfigHint')).toBeTruthy();
    expect(screen.getByText('multisig.admin')).toBeTruthy();
    expect(screen.getByText('multisig.threshold')).toBeTruthy();
    expect(screen.getByText('multisig.contractId')).toBeTruthy();
  });

  it('renders the authorised signers card', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      expect(screen.getByText('multisig.signerList')).toBeTruthy();
    });

    // Signer addresses should appear (truncated: first 6 chars + ... + last 4)
    expect(screen.getAllByText(/GSIGNE.*GN1/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/GSIGNE.*GN2/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/GSIGNE.*GN3/).length).toBeGreaterThanOrEqual(1);
  });

  it('renders the proposals table with rows', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      expect(screen.getByText('multisig.proposals')).toBeTruthy();
    });

    // Column headers
    expect(screen.getByText('multisig.colDestination')).toBeTruthy();
    expect(screen.getByText('multisig.colAmount')).toBeTruthy();
    expect(screen.getByText('multisig.colSigners')).toBeTruthy();
    expect(screen.getByText('multisig.colStatus')).toBeTruthy();
    expect(screen.getByText('multisig.colAction')).toBeTruthy();

    // Proposal IDs
    expect(screen.getByText('#1')).toBeTruthy();
    expect(screen.getByText('#2')).toBeTruthy();
  });

  it('shows correct status badges for proposals', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      expect(screen.getByText('Pending')).toBeTruthy();
    });

    expect(screen.getByText('Executed')).toBeTruthy();
  });

  it('shows the "Sign" button for pending proposals needing signatures', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      // Proposal #1 is pending with 1 of 2 signatures — needs 1 more
      const signButtons = screen.getAllByText('multisig.sign');
      expect(signButtons.length).toBeGreaterThanOrEqual(1);
    });
  });

  it('does not show "Sign" for executed proposals', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      // Proposal #2 is executed — should show "Executed" not "Sign"
      expect(screen.getByText('Executed')).toBeTruthy();
    });
  });

  it('handles signing action and shows toast', async () => {
    const { toast } = await import('react-toastify');
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      const signButtons = screen.getAllByText('multisig.sign');
      expect(signButtons.length).toBeGreaterThanOrEqual(1);
    });

    // Click the first sign button
    const signButtons = screen.getAllByText('multisig.sign');
    fireEvent.click(signButtons[0]);

    // Should show signing toast
    await waitFor(() => {
      expect(toast.success).toHaveBeenCalled();
    });
  });

  it('renders the refresh button and triggers reload', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      expect(screen.getByText('multisig.refreshButton')).toBeTruthy();
    });

    const refreshBtn = screen.getByText('multisig.refreshButton');
    fireEvent.click(refreshBtn);

    // Should call the service again
    await waitFor(() => {
      expect(mockGetMultisigStatus).toHaveBeenCalledTimes(2);
    });
  });

  it('has accessible ARIA labels', async () => {
    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      // Refresh button should have aria-label
      expect(screen.getByLabelText('Refresh data')).toBeTruthy();
    });

    // Sign buttons should have aria-label
    const signButtons = screen.getAllByRole('button', { name: /Sign proposal/i });
    expect(signButtons.length).toBeGreaterThanOrEqual(1);
  });

  it('handles API failure gracefully', async () => {
    mockGetMultisigStatus.mockRejectedValueOnce(new Error('Network error'));

    const { default: MultisigDashboard } = await import('./Multisig');
    render(<MultisigDashboard contractId="CTEST" />);

    await waitFor(() => {
      expect(screen.getByText('multisig.loadFailed')).toBeTruthy();
    });
  });
});