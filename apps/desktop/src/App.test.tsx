import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { App } from './App';
import * as ipc from './ipc';

const windowApi = vi.hoisted(() => ({
  isTauri: vi.fn(() => false),
  getCurrentWindow: vi.fn(),
}));
vi.mock('@tauri-apps/api/core', async (importOriginal) => ({
  ...await importOriginal<typeof import('@tauri-apps/api/core')>(),
  isTauri: windowApi.isTauri,
}));
vi.mock('@tauri-apps/api/window', async (importOriginal) => ({
  ...await importOriginal<typeof import('@tauri-apps/api/window')>(),
  getCurrentWindow: windowApi.getCurrentWindow,
}));

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    options = { cursorBlink: true };
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
    windowApi.isTauri.mockReturnValue(false);
    windowApi.getCurrentWindow.mockReset();
    window.localStorage.clear();
    vi.stubGlobal('innerWidth', 1200);
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
    vi.unstubAllGlobals();
  });

  it('is a desktop workbench: host explorer on the left, chat on the right, welcome in the middle', async () => {
    render(<App />);
    expect(screen.getByRole('button', { name: 'Hosts' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('button', { name: 'AI Assistant' })).toHaveAttribute('aria-pressed', 'true');
    expect(screen.getByRole('region', { name: 'Hosts' })).toBeVisible();
    expect(screen.getByLabelText('Request')).toBeVisible();
    expect(screen.getByText('Open a terminal on this computer')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTitle(/schema v2, wal mode/)).toBeInTheDocument());
  });

  it('toggles the side panels from the activity bar and the keyboard', () => {
    render(<App />);
    const header = document.querySelector('.workspace-head');
    const activitybar = screen.getByRole('navigation', { name: 'Main' });
    expect(header).toContainElement(screen.getByRole('button', { name: 'AI Assistant' }));
    expect(within(activitybar).queryByRole('button', { name: 'AI Assistant' })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'AI Assistant' }));
    expect(screen.getByRole('button', { name: 'AI Assistant' })).toHaveAttribute('aria-pressed', 'false');
    expect(screen.getByRole('button', { name: 'AI Assistant' })).toHaveAttribute('aria-expanded', 'false');
    expect(screen.getByLabelText('Request')).not.toBeVisible();
    fireEvent.keyDown(window, { key: 'L', ctrlKey: true, shiftKey: true });
    expect(screen.getByLabelText('Request')).toBeVisible();
    expect(screen.getByRole('button', { name: 'AI Assistant' })).toHaveAttribute('aria-expanded', 'true');

    fireEvent.keyDown(window, { key: 'E', ctrlKey: true, shiftKey: true });
    expect(screen.getByRole('button', { name: 'Hosts' })).toHaveAttribute('aria-pressed', 'false');
  });

  it('opens History and Providers as tabs in the middle', async () => {
    render(<App />);
    fireEvent.click(screen.getByRole('button', { name: 'Providers' }));
    const tabs = screen.getByRole('tablist', { name: 'Open tabs' });
    expect(within(tabs).getByRole('tab', { name: 'Providers' })).toHaveAttribute('aria-selected', 'true');
    fireEvent.click(within(tabs).getByRole('button', { name: 'Close Providers' }));
    expect(screen.queryByRole('tablist', { name: 'Open tabs' })).not.toBeInTheDocument();
  });

  it('keeps a compact window clear and opens Providers from the assistant', async () => {
    vi.stubGlobal('innerWidth', 800);
    render(<App />);
    expect(screen.getByLabelText('Request')).not.toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'AI Assistant' }));
    expect(screen.getByLabelText('Request')).toBeVisible();
    fireEvent.click(screen.getByRole('button', { name: 'Open Providers' }));
    expect(await screen.findByRole('tab', { name: 'Providers' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByLabelText('Request')).not.toBeVisible();
  });

  it('honours the saved assistant visibility in a compact window', () => {
    vi.stubGlobal('innerWidth', 800);
    window.localStorage.setItem('selai.chat', 'open');
    render(<App />);
    expect(screen.getByLabelText('Request')).toBeVisible();
  });

  it('allows the assistant to grow while preserving space for the editor', () => {
    vi.stubGlobal('innerWidth', 1200);
    window.localStorage.setItem('selai.chat.width', '900');
    render(<App />);
    const panel = document.querySelector<HTMLElement>('.chatbar');
    expect(panel?.style.width).toBe('608px');
    fireEvent.click(screen.getByRole('button', { name: 'Hosts' }));
    expect(panel?.style.width).toBe('872px');
  });

  it('puts native window controls and a drag region in the app header', () => {
    const minimize = vi.fn().mockResolvedValue(undefined);
    const toggleMaximize = vi.fn().mockResolvedValue(undefined);
    const close = vi.fn().mockResolvedValue(undefined);
    windowApi.isTauri.mockReturnValue(true);
    windowApi.getCurrentWindow.mockReturnValue({ minimize, toggleMaximize, close });
    render(<App />);
    expect(document.querySelector('.workspace-head')).toHaveAttribute('data-tauri-drag-region');
    fireEvent.click(screen.getByRole('button', { name: 'Minimize window' }));
    fireEvent.click(screen.getByRole('button', { name: 'Maximize or restore window' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close window' }));
    expect(minimize).toHaveBeenCalledTimes(1);
    expect(toggleMaximize).toHaveBeenCalledTimes(1);
    expect(close).toHaveBeenCalledTimes(1);
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

    fireEvent.click(screen.getByRole('button', { name: 'Connect to Production Web' }));
    await waitFor(() => expect(ipc.terminalConnect).toHaveBeenCalledTimes(2));
    const tabs = screen.getByRole('tablist', { name: 'Open tabs' });
    expect(within(tabs).getAllByRole('tab')).toHaveLength(1);
  });
});
