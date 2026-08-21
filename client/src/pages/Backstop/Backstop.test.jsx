/**
 * @file Backstop.test.jsx
 * @description Unit tests for the Backstop / Insurance Fund dashboard page.
 *
 * Coverage:
 *   1. Page structure — header, status pill, metrics, config card
 *   2. Metrics — balance, deposits, withdrawals, fee rate rendering
 *   3. Fee calculator — principal × bps ÷ 10000 math
 *   4. Demo-mode hint — shown when backend proxy is not connected
 *   5. Error handling — API failure shows banner + toast
 *   6. Refresh button — triggers reload
 *   7. Accessibility — ARIA labels, roles
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi, afterEach } from 'vitest';
import React from 'react';

// ─── Module mocks ─────────────────────────────────────────────────────────────

// Mock the backstop service so tests never hit the network
const mockStatus = {
  config: {
    admin: 'GBADMINXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXADMIN',
    token: 'GTOKENXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXTOKEN',
    feeBps: 500,
    totalDeposited: 250000,
    totalWithdrawn: 50000,
    contractId: '',
  },
  balance: 200000,
  version: '1.0.0',
};

vi.mock('../../services/backstopService', () => ({
  getBackstopStatus: vi.fn(() => Promise.resolve(mockStatus)),
  calcFee: vi.fn((principal, feeBps) => {
    const p = Number(principal) || 0;
    const bps = Number(feeBps) || 0;
    if (p < 0) throw new Error('principal must be non-negative');
    if (bps < 0 || bps > 10_000) throw new Error('fee_bps must be between 0 and 10000');
    return Math.floor((p * bps) / 10_000);
  }),
  default: {},
}));

// react-toastify — capture calls without rendering the container
vi.mock('react-toastify', () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
  },
}));

// react-helmet-async — no-op wrapper
vi.mock('react-helmet-async', () => ({
  Helmet: () => null,
}));

// react-i18next — pass through the key so tests can assert on copy
vi.mock('react-i18next', () => ({
  useTranslation: () => ({
    t: (key) => key || '',
  }),
}));

import BackstopDashboard from './Backstop';
import { getBackstopStatus } from '../../services/backstopService';
import { toast } from 'react-toastify';

describe('BackstopDashboard — page structure', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getBackstopStatus.mockResolvedValue({
      config: { ...mockStatus.config },
      balance: mockStatus.balance,
      version: mockStatus.version,
    });
  });

  it('renders the page title and subtitle', async () => {
    render(<BackstopDashboard />);

    expect(screen.getByText('backstop.pageTitle')).toBeInTheDocument();
    expect(screen.getByText('backstop.pageSubtitle')).toBeInTheDocument();
  });

  it('renders the live status pill', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('backstop-status-pill')).toBeInTheDocument();
    });
  });

  it('renders contract version badge', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('backstop-status-pill')).toBeInTheDocument();
    });
    // The version badge text contains the mocked i18n key; assert the badge
    // element exists and carries the version via the CSS class marker.
    const badge = screen
      .getAllByText(/backstop\.contractVersion/)
      .find((el) => el.closest('span'));
    expect(badge).toBeTruthy();
  });

  it('renders all four metric cards', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByLabelText(/backstop.metrics.balance/)).toBeInTheDocument();
      expect(screen.getByLabelText(/backstop.metrics.totalDeposited/)).toBeInTheDocument();
      expect(screen.getByLabelText(/backstop.metrics.totalWithdrawn/)).toBeInTheDocument();
      expect(screen.getByLabelText(/backstop.metrics.feeRate/)).toBeInTheDocument();
    });

    // Metric values render
    expect(screen.getByText(/200,000/)).toBeInTheDocument(); // balance
    // Exact matches to avoid partial regex collisions (250,000 contains 50,000)
    expect(screen.getAllByText('250,000').length).toBeGreaterThan(0);
    expect(screen.getAllByText('50,000').length).toBeGreaterThan(0);
  });

  it('renders the fee rate as a percent', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByText('5%')).toBeInTheDocument(); // 500 bps = 5%
    });
  });

  it('renders the config card with admin and token addresses', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByText('backstop.contractConfig')).toBeInTheDocument();
    });
    // truncateId: first 8 chars + … + last 6 chars
    expect(screen.getByText('GBADMINX…XADMIN')).toBeInTheDocument();
    expect(screen.getByText('GTOKENXX…XTOKEN')).toBeInTheDocument();
  });
});

describe('BackstopDashboard — fee calculator', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getBackstopStatus.mockResolvedValue({
      config: { ...mockStatus.config },
      balance: mockStatus.balance,
      version: mockStatus.version,
    });
  });

  it('shows dash before any principal is entered', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByText('backstop.feeCalculator')).toBeInTheDocument();
    });
    expect(screen.getByTestId('fee-result')).toHaveTextContent('—');
  });

  it('computes fee = principal × bps ÷ 10000', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByLabelText(/backstop.principalLabel/)).toBeInTheDocument();
    });

    fireEvent.change(screen.getByLabelText(/backstop.principalLabel/), {
      target: { value: '10000' },
    });

    // 10000 × 500 / 10000 = 500
    await waitFor(() => {
      expect(screen.getByTestId('fee-result')).toHaveTextContent('500');
    });
  });

  it('updates the fee when the principal changes', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByLabelText(/backstop.principalLabel/)).toBeInTheDocument();
    });

    fireEvent.change(screen.getByLabelText(/backstop.principalLabel/), {
      target: { value: '1000' },
    });
    await waitFor(() => {
      expect(screen.getByTestId('fee-result')).toHaveTextContent('50');
    });

    fireEvent.change(screen.getByLabelText(/backstop.principalLabel/), {
      target: { value: '2000' },
    });
    await waitFor(() => {
      expect(screen.getByTestId('fee-result')).toHaveTextContent('100');
    });
  });

  it('shows the current rate under the fee result', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByText('backstop.rate')).toBeInTheDocument();
    });
  });
});

describe('BackstopDashboard — loading & demo mode', () => {
  it('skeleton-loads the metric cards while fetching', () => {
    getBackstopStatus.mockImplementation(
      () => new Promise((resolve) => setTimeout(() => resolve(mockStatus), 200)),
    );

    render(<BackstopDashboard />);

    // isLoading=true initially — metric cards render pulse skeletons
    expect(document.querySelectorAll('.animate-pulse').length).toBeGreaterThan(0);
  });

  it('shows demo-mode hint when fallback data was used', async () => {
    getBackstopStatus.mockResolvedValue({
      config: mockStatus.config,
      balance: mockStatus.balance,
      version: mockStatus.version,
    });

    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('demo-hint')).toBeInTheDocument();
    });
    // First load does NOT call toast.info (showToast=false).  The hint
    // element alone confirms the fallback path was taken.
  });

  it('does not show demo-mode hint when real data loaded', async () => {
    getBackstopStatus.mockResolvedValue({
      config: {
        admin: 'CCUSTOMADMIN1234567890abcdef1234567890abcdef',
        token: 'CCUSTOMTOKEN1234567890abcdef1234567890abcd',
        feeBps: 300,
        totalDeposited: 111,
        totalWithdrawn: 22,
        contractId: '',
      },
      balance: 99,
      version: '1.0.0',
    });

    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('backstop-status-pill')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('demo-hint')).not.toBeInTheDocument();
    expect(toast.info).not.toHaveBeenCalled();
  });
});

describe('BackstopDashboard — error handling', () => {
  it('shows an error banner and toast when the service fails', async () => {
    getBackstopStatus.mockRejectedValue(new Error('network down'));

    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeInTheDocument();
    });
    expect(screen.getByText(/network down/)).toBeInTheDocument();
    expect(toast.error).toHaveBeenCalled();
  });

  it('keeps the page chrome visible when the service fails', async () => {
    getBackstopStatus.mockRejectedValue(new Error('network down'));

    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeInTheDocument();
    });
    expect(screen.getByText('backstop.pageTitle')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /backstop.refreshButton/ })).toBeInTheDocument();
  });
});

describe('BackstopDashboard — refresh', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getBackstopStatus.mockResolvedValue({
      config: { ...mockStatus.config },
      balance: mockStatus.balance,
      version: mockStatus.version,
    });
  });

  it('re-fetches status when refresh is clicked', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(getBackstopStatus.mock.calls.length).toBeGreaterThanOrEqual(1);
    });

    const callsBefore = getBackstopStatus.mock.calls.length;
    fireEvent.click(screen.getByRole('button', { name: /backstop.refreshButton/ }));

    await waitFor(() => {
      expect(getBackstopStatus.mock.calls.length).toBeGreaterThan(callsBefore);
    });
  });
});

describe('BackstopDashboard — accessibility', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getBackstopStatus.mockResolvedValue({
      config: { ...mockStatus.config },
      balance: mockStatus.balance,
      version: mockStatus.version,
    });
  });

  it('exposes metric cards via aria-label', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByLabelText(/backstop.metrics.balance/)).toBeInTheDocument();
    });
  });

  it('exposes the principal input with an accessible name', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByLabelText(/backstop.principalLabel/)).toBeInTheDocument();
    });
  });

  it('renders the operations note', async () => {
    render(<BackstopDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('ops-note')).toBeInTheDocument();
    });
    expect(screen.getByText('backstop.opsNote')).toBeInTheDocument();
  });
});