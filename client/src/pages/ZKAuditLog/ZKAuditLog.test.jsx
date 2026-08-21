/**
 * @file ZKAuditLog.test.jsx
 * @description Unit tests for the ZK Audit Log dashboard page.
 *
 * Coverage:
 *   1. Page structure — header, ZK badge, metrics
 *   2. Metrics — total, successful, failed, rate rendering
 *   3. Filtering — status filter, date range filter
 *   4. Export CSV — button triggers download
 *   5. Demo-mode hint — shown when fallback data is used
 *   6. Error handling — API failure shows banner + toast
 *   7. Empty state — no logs message
 *   8. Refresh button — triggers reload
 *   9. Accessibility — ARIA labels, roles
 */

import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi, afterEach } from 'vitest';
import React from 'react';

// ─── Module mocks ─────────────────────────────────────────────────────────────

const mockLogs = [
  {
    _id: '1',
    tokenName: 'DemoToken',
    contractId: 'CDEMO000000000000000000000000000000000001',
    status: 'SUCCESS',
    errorMessage: '',
    createdAt: new Date().toISOString(),
  },
  {
    _id: '2',
    tokenName: 'TestAsset',
    contractId: 'CTEST000000000000000000000000000000000002',
    status: 'FAIL',
    errorMessage: 'Insufficient balance',
    createdAt: new Date().toISOString(),
  },
];

vi.mock('../../services/zkAuditLogService', () => ({
  getAuditLogs: vi.fn(() => Promise.resolve(mockLogs)),
  formatLogEntry: vi.fn((entry) => ({
    id: entry._id,
    tokenName: entry.tokenName,
    contractId: entry.contractId,
    status: entry.status,
    errorMessage: entry.errorMessage,
    createdAt: entry.createdAt,
    isSuccess: entry.status === 'SUCCESS',
    isFailure: entry.status === 'FAIL',
  })),
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

import ZKAuditLogDashboard from './ZKAuditLog';
import { getAuditLogs } from '../../services/zkAuditLogService';
import { toast } from 'react-toastify';

describe('ZKAuditLogDashboard — page structure', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getAuditLogs.mockResolvedValue([
      { ...mockLogs[0] },
      { ...mockLogs[1] },
    ]);
  });

  it('renders the page title and subtitle', async () => {
    render(<ZKAuditLogDashboard />);

    expect(screen.getByText('auditLog.pageTitle')).toBeInTheDocument();
    expect(screen.getByText('auditLog.pageSubtitle')).toBeInTheDocument();
  });

  it('renders the ZK badge', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('zk-badge')).toBeInTheDocument();
    });
  });

  it('renders all four metric cards', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByLabelText(/auditLog.metrics.total/)).toBeInTheDocument();
      expect(screen.getByLabelText(/auditLog.metrics.successful/)).toBeInTheDocument();
      expect(screen.getByLabelText(/auditLog.metrics.failed/)).toBeInTheDocument();
      expect(screen.getByLabelText(/auditLog.metrics.rate/)).toBeInTheDocument();
    });

    // Metric values: 2 total, 1 success, 1 fail, 50% rate
    expect(screen.getByText('2')).toBeInTheDocument();
    expect(screen.getByText('1')).toBeInTheDocument();
    expect(screen.getByText('50%')).toBeInTheDocument();
  });

  it('renders the log table after loading', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('logs-table')).toBeInTheDocument();
    });

    // Both log entries should be visible
    expect(screen.getByTestId('log-row-1')).toBeInTheDocument();
    expect(screen.getByTestId('log-row-2')).toBeInTheDocument();
  });

  it('renders status badges for each log entry', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('logs-table')).toBeInTheDocument();
    });

    // SUCCESS and FAIL badges should exist
    expect(screen.getAllByText('SUCCESS').length).toBeGreaterThan(0);
    expect(screen.getAllByText('FAIL').length).toBeGreaterThan(0);
  });
});

describe('ZKAuditLogDashboard — filtering', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getAuditLogs.mockResolvedValue([
      { ...mockLogs[0] },
      { ...mockLogs[1] },
    ]);
  });

  it('shows all logs by default', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('logs-table')).toBeInTheDocument();
    });

    expect(screen.getByTestId('log-row-1')).toBeInTheDocument();
    expect(screen.getByTestId('log-row-2')).toBeInTheDocument();
  });

  it('filters by status (success)', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('filter-status')).toBeInTheDocument();
    });

    fireEvent.change(screen.getByTestId('filter-status'), { target: { value: 'success' } });

    await waitFor(() => {
      // Only the SUCCESS row should be visible
      expect(screen.getByTestId('log-row-1')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('log-row-2')).not.toBeInTheDocument();
  });

  it('filters by status (fail)', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('filter-status')).toBeInTheDocument();
    });

    fireEvent.change(screen.getByTestId('filter-status'), { target: { value: 'fail' } });

    await waitFor(() => {
      expect(screen.getByTestId('log-row-2')).toBeInTheDocument();
    });
    expect(screen.queryByTestId('log-row-1')).not.toBeInTheDocument();
  });

  it('renders filter controls', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('filter-date-from')).toBeInTheDocument();
      expect(screen.getByTestId('filter-date-to')).toBeInTheDocument();
      expect(screen.getByTestId('filter-status')).toBeInTheDocument();
      expect(screen.getByTestId('export-csv-btn')).toBeInTheDocument();
    });
  });
});

describe('ZKAuditLogDashboard — loading & demo mode', () => {
  it('shows skeleton loading while fetching', () => {
    getAuditLogs.mockImplementation(
      () => new Promise((resolve) => setTimeout(() => resolve(mockLogs), 200)),
    );

    render(<ZKAuditLogDashboard />);

    // isLoading=true initially — show loading skeleton for logs table
    expect(screen.getByTestId('logs-loading')).toBeInTheDocument();
  });

  it('shows demo-mode hint when fallback data was used', async () => {
    getAuditLogs.mockResolvedValue(mockLogs);

    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('demo-hint')).toBeInTheDocument();
    });
  });
});

describe('ZKAuditLogDashboard — error handling', () => {
  it('shows an error banner and toast when the service fails', async () => {
    getAuditLogs.mockRejectedValue(new Error('API unreachable'));

    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeInTheDocument();
    });
    expect(screen.getByText(/API unreachable/)).toBeInTheDocument();
    expect(toast.error).toHaveBeenCalled();
  });

  it('keeps the page chrome visible when the service fails', async () => {
    getAuditLogs.mockRejectedValue(new Error('API unreachable'));

    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByRole('alert')).toBeInTheDocument();
    });
    expect(screen.getByText('auditLog.pageTitle')).toBeInTheDocument();
    expect(screen.getByTestId('refresh-btn')).toBeInTheDocument();
  });
});

describe('ZKAuditLogDashboard — empty state', () => {
  it('shows empty state when logs array is empty', async () => {
    getAuditLogs.mockResolvedValue([]);

    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('logs-empty')).toBeInTheDocument();
    });
    expect(screen.getByText('auditLog.noLogs')).toBeInTheDocument();
  });
});

describe('ZKAuditLogDashboard — export', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getAuditLogs.mockResolvedValue([
      { ...mockLogs[0] },
      { ...mockLogs[1] },
    ]);
  });

  it('calls export when export button is clicked', async () => {
    // Mock URL.createObjectURL and document.createElement for download
    const mockCreateObjectURL = vi.fn(() => 'blob:test');
    const mockRevokeObjectURL = vi.fn();
    const mockClick = vi.fn();
    URL.createObjectURL = mockCreateObjectURL;
    URL.revokeObjectURL = mockRevokeObjectURL;
    document.createElement = vi.fn(() => ({
      href: '',
      download: '',
      click: mockClick,
    }));

    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('export-csv-btn')).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTestId('export-csv-btn'));

    expect(mockClick).toHaveBeenCalled();
    expect(toast.success).toHaveBeenCalled();
  });
});

describe('ZKAuditLogDashboard — refresh', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getAuditLogs.mockResolvedValue([
      { ...mockLogs[0] },
      { ...mockLogs[1] },
    ]);
  });

  it('re-fetches logs when refresh is clicked', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(getAuditLogs.mock.calls.length).toBeGreaterThanOrEqual(1);
    });

    const callsBefore = getAuditLogs.mock.calls.length;
    fireEvent.click(screen.getByTestId('refresh-btn'));

    await waitFor(() => {
      expect(getAuditLogs.mock.calls.length).toBeGreaterThan(callsBefore);
    });
  });
});

describe('ZKAuditLogDashboard — accessibility', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    getAuditLogs.mockResolvedValue([
      { ...mockLogs[0] },
      { ...mockLogs[1] },
    ]);
  });

  it('exposes metric cards via aria-label', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByLabelText(/auditLog.metrics.total/)).toBeInTheDocument();
    });
  });

  it('renders the refresh button', async () => {
    render(<ZKAuditLogDashboard />);

    await waitFor(() => {
      expect(screen.getByTestId('refresh-btn')).toBeInTheDocument();
    });
  });
});