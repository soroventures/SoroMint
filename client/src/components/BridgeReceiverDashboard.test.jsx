import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import BridgeReceiverDashboard from './BridgeReceiverDashboard';

const { getRelayerStatus } = vi.hoisted(() => ({
  getRelayerStatus: vi.fn(),
}));

vi.mock('../services/bridgeService', () => ({
  getRelayerStatus,
}));

vi.mock('react-toastify', () => ({
  toast: { error: vi.fn(), success: vi.fn(), warn: vi.fn() },
}));

const mockStatus = {
  enabled: true,
  configured: true,
  direction: 'both',
  queue: { pending: 3, processing: 1 },
  stats: {
    observed: 42,
    skipped: 5,
    relayed: 30,
    failed: 2,
    lastObservedAt: '2026-08-20T10:00:00.000Z',
    lastRelayedAt: '2026-08-20T10:05:00.000Z',
    lastError: null,
  },
  config: {
    sorobanAccountId: 'GA7QYX...',
    evmBridgeAddress: '0xabcd...',
  },
};

describe('BridgeReceiverDashboard', () => {
  beforeEach(() => {
    getRelayerStatus.mockReset();
    vi.spyOn(console, 'error').mockImplementation(() => {});
  });

  afterEach(() => {
    console.error.mockRestore();
  });

  it('prompts for authentication and does not call the API without a token', () => {
    render(<BridgeReceiverDashboard />);

    expect(screen.getByText(/authentication required/i)).toBeInTheDocument();
    expect(getRelayerStatus).not.toHaveBeenCalled();
    expect(screen.queryByRole('button', { name: /refresh bridge status/i })).not.toBeInTheDocument();
  });

  it('shows skeleton loaders while fetching then renders metrics', async () => {
    let resolveFetch;
    getRelayerStatus.mockReturnValue(
      new Promise((resolve) => {
        resolveFetch = resolve;
      })
    );

    render(<BridgeReceiverDashboard authToken="jwt-token" />);

    expect(screen.getByRole('button', { name: /refresh bridge status/i })).toBeDisabled();

    resolveFetch(mockStatus);

    expect(await screen.findByText('Events observed')).toBeInTheDocument();
    expect(screen.getByText('42')).toBeInTheDocument();
    expect(screen.getByText('Commands relayed')).toBeInTheDocument();
    expect(screen.getByText('30')).toBeInTheDocument();
    expect(screen.getByText('Failed relays')).toBeInTheDocument();
    expect(screen.getByText('2')).toBeInTheDocument();
    expect(screen.getByText('Queue pending')).toBeInTheDocument();
    expect(screen.getByText('3')).toBeInTheDocument();
  });

  it('renders relayer health badges and direction', async () => {
    getRelayerStatus.mockResolvedValue(mockStatus);

    render(<BridgeReceiverDashboard authToken="jwt-token" />);

    expect(await screen.findByText('Relayer enabled')).toBeInTheDocument();
    expect(screen.getByText('Configured')).toBeInTheDocument();
    expect(screen.getByText('Two-way')).toBeInTheDocument();
    expect(screen.getByText('No errors recorded')).toBeInTheDocument();
  });

  it('shows an error state with retry when the fetch fails', async () => {
    getRelayerStatus
      .mockRejectedValueOnce(new Error('Request failed with status 401'))
      .mockResolvedValueOnce(mockStatus);

    render(<BridgeReceiverDashboard authToken="jwt-token" />);

    expect(await screen.findByText(/failed to load bridge status/i)).toBeInTheDocument();
    expect(screen.getByText(/request failed with status 401/i)).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /try again/i }));

    await waitFor(() => {
      expect(screen.getByText('Events observed')).toBeInTheDocument();
    });
    expect(getRelayerStatus).toHaveBeenCalledTimes(2);
  });

  it('refetches the status when the refresh button is clicked', async () => {
    getRelayerStatus.mockResolvedValue(mockStatus);

    render(<BridgeReceiverDashboard authToken="jwt-token" />);

    expect(await screen.findByText('Events observed')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /refresh bridge status/i }));

    await waitFor(() => {
      expect(getRelayerStatus).toHaveBeenCalledTimes(2);
    });
  });

  it('renders a disabled state badge when the relayer is disabled', async () => {
    getRelayerStatus.mockResolvedValue({ ...mockStatus, enabled: false });

    render(<BridgeReceiverDashboard authToken="jwt-token" />);

    expect(await screen.findByText('Relayer disabled')).toBeInTheDocument();
  });
});
