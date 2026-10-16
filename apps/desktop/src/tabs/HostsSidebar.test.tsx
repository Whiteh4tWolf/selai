import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { HostsSidebar } from './HostsSidebar';

afterEach(() => vi.restoreAllMocks());

describe('HostsSidebar', () => {
  it('shows only edit and archive actions, and keeps them separate from connecting', async () => {
    vi.spyOn(ipc, 'localHost').mockRejectedValue(new Error('unavailable'));
    vi.spyOn(ipc, 'hostList').mockResolvedValue({ items: [
      { id: 'one', label: 'Production Web', hostname: 'web.example.com', port: 22, username: 'ops',
        authMethod: 'password', credentialConfigured: true, kind: 'ssh' } as ipc.HostView,
    ], nextCursor: null });
    const connect = vi.fn();
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    render(<HostsSidebar onConnect={connect} />);
    expect(await screen.findByRole('button', { name: 'Edit Production Web' })).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Use .* in chat/ })).not.toBeInTheDocument();
    expect(screen.queryByTitle('Use in chat')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Archive Production Web' }));
    expect(window.confirm).toHaveBeenCalledOnce();
    expect(connect).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole('button', { name: 'Connect to Production Web' }));
    expect(connect).toHaveBeenCalledOnce();
  });

  it('filters loaded hosts locally without querying the database for each keystroke', async () => {
    vi.spyOn(ipc, 'localHost').mockRejectedValue(new Error('unavailable'));
    const hosts = [
      { id: 'one', label: 'Production Web', hostname: 'web.example.com', port: 22, username: 'ops', authMethod: 'password', credentialConfigured: true, kind: 'ssh' },
      { id: 'two', label: 'Staging DB', hostname: 'db.example.com', port: 22, username: 'ops', authMethod: 'password', credentialConfigured: true, kind: 'ssh' },
    ] as ipc.HostView[];
    const list = vi.spyOn(ipc, 'hostList').mockResolvedValue({ items: hosts, nextCursor: null });
    render(<HostsSidebar onConnect={vi.fn()} />);

    expect(await screen.findByText('Production Web')).toBeInTheDocument();
    expect(screen.getByText('Staging DB')).toBeInTheDocument();
    const search = screen.getByRole('searchbox', { name: 'Filter hosts' });
    fireEvent.change(search, { target: { value: 'p' } });
    fireEvent.change(search, { target: { value: 'prod' } });
    expect(screen.getByText('Production Web')).toBeInTheDocument();
    expect(screen.queryByText('Staging DB')).not.toBeInTheDocument();
    expect(list).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: 'Refresh hosts' }));
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
  });
});
