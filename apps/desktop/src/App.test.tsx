import { fireEvent, render, screen, waitFor } from '@testing-library/react';
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

  it('shows the three tabs and starts on Connections', async () => {
    render(<App />);
    expect(screen.getByRole('tab', { name: 'Connections' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByRole('tab', { name: 'Terminal' })).toBeInTheDocument();
    expect(screen.getByRole('tab', { name: 'AI' })).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText(/schema v2, wal mode/)).toBeInTheDocument());
  });

  it('switches to AI and shows the agent composer', () => {
    render(<App />);
    fireEvent.click(screen.getByRole('tab', { name: 'AI' }));
    expect(screen.getByRole('tab', { name: 'AI' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByLabelText('Request')).toBeVisible();
  });

  it('reports a storage failure with its code instead of hiding it', async () => {
    vi.spyOn(ipc, 'storageStatus').mockRejectedValue({ code: 'STORAGE_BUSY', message: 'database is busy' });
    render(<App />);
    await waitFor(() =>
      expect(screen.getByText('database unavailable: STORAGE_BUSY: database is busy')).toBeInTheDocument(),
    );
  });
});
