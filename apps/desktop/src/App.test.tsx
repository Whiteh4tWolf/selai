import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from './App';
import * as ipc from './ipc';

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    loadAddon() {}
    open() {}
    write() {}
    writeln() {}
    focus() {}
    onData() {
      return { dispose() {} };
    }
    onResize() {
      return { dispose() {} };
    }
    dispose() {}
  },
}));
vi.mock('@xterm/addon-fit', () => ({
  FitAddon: class {
    fit() {}
  },
}));

describe('App shell', () => {
  beforeEach(() => {
    vi.spyOn(ipc, 'appInfo').mockResolvedValue({ name: 'Selai', version: '0.1.0' });
    vi.spyOn(ipc, 'storageStatus').mockResolvedValue({
      schemaVersion: 2,
      journalMode: 'wal',
      interruptedTasksAtStartup: 0,
      interruptedExecutionsAtStartup: 0,
    });
    vi.spyOn(ipc, "hostList").mockResolvedValue({ items: [], nextCursor: null });
    vi.spyOn(ipc, "providerList").mockResolvedValue({ items: [], nextCursor: null });
    vi.spyOn(ipc, "providerTemplates").mockResolvedValue([]);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('shows the navigation and starts on Hosts', async () => {
    render(<App />);
    expect(screen.getByRole('tab', { name: 'Hosts' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: 'Terminal' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'AI Agent' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'Providers' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'History' })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText(/schema v2, wal mode/)).toBeInTheDocument());
  });

  it('switches to the AI Agent and shows the chat composer', () => {
    render(<App />);
    fireEvent.click(screen.getByRole('tab', { name: 'AI Agent' }));
    expect(screen.getByRole('tab', { name: 'AI Agent' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByLabelText('Request')).toBeVisible();
  });

  it('reports a storage failure with its code instead of hiding it', async () => {
    vi.spyOn(ipc, 'storageStatus').mockRejectedValue({ code: 'STORAGE_BUSY', message: 'database is busy' });
    render(<App />);
    await waitFor(() =>
      expect(screen.getByText('Database unavailable: STORAGE_BUSY: database is busy')).toBeInTheDocument(),
    );
  });

  it('connecting again after a failed login reuses the failed tab', async () => {
    const host = {
      id: 'h1',
      label: 'Production Web',
      hostname: 'web.example.com',
      port: 22,
      username: 'deploy',
      authMethod: 'password',
      credentialConfigured: true,
      archived: false,
    } as ipc.HostView;
    vi.spyOn(ipc, 'hostList').mockResolvedValue({ items: [host], nextCursor: null });
    vi.spyOn(ipc, 'terminalConnect').mockRejectedValue({ code: 'SSH_AUTH_FAILED', message: 'authentication failed' });
    render(<App />);

    fireEvent.click(await screen.findByRole('button', { name: 'Connect to Production Web' }));
    expect(await screen.findByText('Authentication failed')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('tab', { name: 'Hosts' }));
    fireEvent.click(screen.getByRole('button', { name: 'Connect to Production Web' }));
    await waitFor(() => expect(ipc.terminalConnect).toHaveBeenCalledTimes(2));
    const tabs = screen.getByRole('tablist', { name: 'Open terminals' });
    expect(within(tabs).getAllByRole('tab')).toHaveLength(1);
  });
});
