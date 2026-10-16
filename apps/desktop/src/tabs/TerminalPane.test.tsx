import { act, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { TerminalPane } from './TerminalPane';

const written: unknown[] = [];

vi.mock('@xterm/xterm', () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    loadAddon() {}
    open() {}
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

beforeEach(() => {
  written.length = 0;
});
afterEach(() => vi.restoreAllMocks());

describe('TerminalPane', () => {
  it('writes decoded output bytes to the terminal and reports the session status', async () => {
    let deliver: (event: ipc.TerminalEvent) => void = () => {};
    vi.spyOn(ipc, 'terminalConnect').mockImplementation((_host, _c, _r, onEvent) => {
      deliver = onEvent;
      return Promise.resolve('session-1');
    });
    const { getByText } = render(<TerminalPane hostId="h1" label="web" hidden={false} />);
    await waitFor(() => expect(getByText('web: connected')).toBeInTheDocument());

    act(() => deliver({ type: 'data', data: 'aGk=' })); // "hi"
    const bytes = written.find((w) => w instanceof Uint8Array) as Uint8Array;
    expect(Array.from(bytes)).toEqual([104, 105]);
  });

  it('shows the failure code when the connection is refused', async () => {
    vi.spyOn(ipc, 'terminalConnect').mockRejectedValue({ code: 'SSH_HOSTKEY_UNKNOWN', message: 'host key is not trusted yet' });
    const { getByText } = render(<TerminalPane hostId="h1" label="web" hidden={false} />);
    await waitFor(() => expect(getByText('web: failed')).toBeInTheDocument());
    expect(written.join('')).toContain('[failed] SSH_HOSTKEY_UNKNOWN: host key is not trusted yet');
  });

  it('closes the remote session when the pane unmounts', async () => {
    vi.spyOn(ipc, 'terminalConnect').mockResolvedValue('session-9');
    const disconnect = vi.spyOn(ipc, 'terminalDisconnect').mockResolvedValue();
    const { getByText, unmount } = render(<TerminalPane hostId="h1" label="web" hidden={false} />);
    await waitFor(() => expect(getByText('web: connected')).toBeInTheDocument());
    unmount();
    expect(disconnect).toHaveBeenCalledWith('session-9');
  });
});
