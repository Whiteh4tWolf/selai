import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { TerminalPane } from './TerminalPane';

const written: unknown[] = [];
const terminals: { options: { cursorBlink: boolean }; focus: ReturnType<typeof vi.fn> }[] = [];

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    options = { cursorBlink: true };
    cols = 80;
    rows = 24;
    constructor(options: { cursorBlink: boolean }) {
      this.options.cursorBlink = options.cursorBlink;
      terminals.push(this);
    }
    loadAddon() {}
    open() {}
    focus = vi.fn();
    write(data: unknown) {
      written.push(data);
    }
    writeln(text: string) {
      written.push(text);
    }
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

const host = {
  id: 'h1',
  label: 'Production Web',
  hostname: 'web.example.com',
  port: 22,
  username: 'deploy',
} as ipc.HostView;

beforeEach(() => {
  written.length = 0;
  terminals.length = 0;
});
afterEach(() => vi.restoreAllMocks());

describe('TerminalPane', () => {
  it('writes decoded output bytes to the terminal and reports the session status', async () => {
    let deliver: (event: ipc.TerminalEvent) => void = () => {};
    vi.spyOn(ipc, 'terminalConnect').mockImplementation((_host, _c, _r, onEvent) => {
      deliver = onEvent;
      return Promise.resolve('session-1');
    });
    const onStatus = vi.fn();
    render(<TerminalPane host={host} hidden={false} onStatus={onStatus} />);
    expect(screen.getByText('Connecting to Production Web')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText('Connected')).toBeInTheDocument());
    await waitFor(() => expect(onStatus).toHaveBeenLastCalledWith('connected'));

    act(() => deliver({ type: 'data', data: 'aGk=' })); // "hi"
    const bytes = written.find((w) => w instanceof Uint8Array) as Uint8Array;
    expect(Array.from(bytes)).toEqual([104, 105]);
  });

  it('shows a failed login with its code and retries on request', async () => {
    const connect = vi
      .spyOn(ipc, 'terminalConnect')
      .mockRejectedValueOnce({ code: 'SSH_AUTH_FAILED', message: 'the server rejected the credentials' })
      .mockResolvedValueOnce('session-2');
    const onEditHost = vi.fn();
    render(<TerminalPane host={host} hidden={false} onEditHost={onEditHost} />);
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Authentication failed');
    expect(alert).toHaveTextContent('SSH_AUTH_FAILED');

    fireEvent.click(screen.getByRole('button', { name: 'Edit host' }));
    expect(onEditHost).toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Retry' }));
    await waitFor(() => expect(screen.getByText('Connected')).toBeInTheDocument());
    expect(connect).toHaveBeenCalledTimes(2);
  });

  it('asks to trust an unknown server key inside the connect flow, then connects', async () => {
    const connect = vi
      .spyOn(ipc, 'terminalConnect')
      .mockRejectedValueOnce({
        code: 'SSH_HOSTKEY_UNKNOWN',
        message: 'host key is not trusted yet',
        details: { algorithm: 'ssh-ed25519', fingerprintSha256: 'SHA256:abc' },
      })
      .mockResolvedValueOnce('session-3');
    const trust = vi.spyOn(ipc, 'hostTrust').mockResolvedValue({ outcome: 'pinned', fingerprintSha256: 'SHA256:abc' });
    render(<TerminalPane host={host} hidden={false} />);

    const dialog = await screen.findByRole('dialog', { name: 'Verify the server' });
    expect(dialog).toHaveTextContent('SHA256:abc');
    fireEvent.click(screen.getByRole('button', { name: 'Trust & connect' }));

    await waitFor(() => expect(trust).toHaveBeenCalledWith('h1', 'SHA256:abc', 'first'));
    await waitFor(() => expect(screen.getByText('Connected')).toBeInTheDocument());
    expect(connect).toHaveBeenCalledTimes(2);
  });

  it('a changed key needs explicit confirmation before it is replaced', async () => {
    vi.spyOn(ipc, 'terminalConnect').mockResolvedValue('session-4').mockRejectedValueOnce({
      code: 'SSH_HOSTKEY_CHANGED',
      message: 'host key changed',
      details: { algorithm: 'ssh-ed25519', fingerprintSha256: 'SHA256:new', pinnedFingerprintSha256: 'SHA256:old' },
    });
    const trust = vi.spyOn(ipc, 'hostTrust').mockResolvedValue({ outcome: 'rotated', fingerprintSha256: 'SHA256:new' });
    render(<TerminalPane host={host} hidden={false} />);

    const replace = await screen.findByRole('button', { name: 'Replace key & connect' });
    expect(replace).toBeDisabled();
    expect(screen.getByText('SHA256:old')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('checkbox'));
    fireEvent.click(replace);
    await waitFor(() => expect(trust).toHaveBeenCalledWith('h1', 'SHA256:new', 'replace'));
    await waitFor(() => expect(screen.getByText('Connected')).toBeInTheDocument());
  });

  it('declining the key leaves the session closed and sends nothing', async () => {
    vi.spyOn(ipc, 'terminalConnect').mockRejectedValueOnce({
      code: 'SSH_HOSTKEY_UNKNOWN',
      message: 'host key is not trusted yet',
      details: { algorithm: 'ssh-ed25519', fingerprintSha256: 'SHA256:abc' },
    });
    const trust = vi.spyOn(ipc, 'hostTrust');
    render(<TerminalPane host={host} hidden={false} />);
    fireEvent.click(await screen.findByRole('button', { name: 'Cancel' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Server key not trusted');
    expect(trust).not.toHaveBeenCalled();
  });

  it('closes the remote session when the pane unmounts', async () => {
    vi.spyOn(ipc, 'terminalConnect').mockResolvedValue('session-9');
    const disconnect = vi.spyOn(ipc, 'terminalDisconnect').mockResolvedValue();
    const { unmount } = render(<TerminalPane host={host} hidden={false} />);
    await waitFor(() => expect(screen.getByText('Connected')).toBeInTheDocument());
    unmount();
    expect(disconnect).toHaveBeenCalledWith('session-9');
  });

  it('stops cursor blinking and does not steal focus while hidden', async () => {
    vi.spyOn(ipc, 'terminalConnect').mockResolvedValue('session-hidden');
    const { rerender } = render(<TerminalPane host={host} hidden />);
    await waitFor(() => expect(screen.getByText('Connected')).toBeInTheDocument());
    expect(terminals[0]?.options.cursorBlink).toBe(false);
    expect(terminals[0]?.focus).not.toHaveBeenCalled();

    rerender(<TerminalPane host={host} hidden={false} />);
    expect(terminals[0]?.options.cursorBlink).toBe(true);
    expect(terminals[0]?.focus).toHaveBeenCalledTimes(1);
  });
});
