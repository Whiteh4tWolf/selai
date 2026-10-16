import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { HostsSidebar } from './HostsSidebar';

afterEach(() => vi.restoreAllMocks());

describe('HostsSidebar', () => {
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
