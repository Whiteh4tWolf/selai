import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { HostImport } from './HostImport';

afterEach(() => vi.restoreAllMocks());

describe('HostImport', () => {
  it('previews a password-manager SSH row and saves only the selected host through host_save', async () => {
    vi.spyOn(ipc, 'hostList').mockResolvedValue({ items: [], nextCursor: null });
    const save = vi.spyOn(ipc, 'hostSave').mockResolvedValue({ id: 'h1' } as ipc.HostView);
    const onImported = vi.fn().mockResolvedValue(undefined);
    render(<HostImport onClose={() => undefined} onImported={onImported} />);

    const csv = 'type,name,login_uri,login_username,login_password,notes\nlogin,Website,https://example.com,web,web-secret,private note\nlogin,Production,ssh://prod.example.com,ops,ssh-secret,private note\n';
    const file = new File([csv], 'bitwarden.csv', { type: 'text/csv' });
    Object.defineProperty(file, 'text', { value: async () => csv });
    fireEvent.change(screen.getByLabelText('Profile export or SSH config file'), { target: { files: [file] } });

    expect(await screen.findByText('ops@prod.example.com')).toBeInTheDocument();
    expect(screen.getByText('Only SSH/SFTP addresses or hostnames are supported.')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Import 1 profile' }));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    expect(save).toHaveBeenCalledWith(expect.objectContaining({
      label: 'Production', hostname: 'prod.example.com', username: 'ops',
      password: 'ssh-secret', notes: null,
    }));
    expect(onImported).toHaveBeenCalledTimes(1);
    expect(await screen.findByText('Imported 1; skipped 1; failed 0.')).toBeInTheDocument();
  });

  it('unlocks KDBX locally through the Rust preview command and imports its SSH entry', async () => {
    const preview = vi.spyOn(ipc, 'kdbxPreview').mockResolvedValue([{
      title: 'Production', url: 'ssh://prod.example.com', username: 'ops', password: 'vault-secret', notes: '',
    }]);
    vi.spyOn(ipc, 'hostList').mockResolvedValue({ items: [], nextCursor: null });
    const save = vi.spyOn(ipc, 'hostSave').mockResolvedValue({ id: 'k1' } as ipc.HostView);
    render(<HostImport onClose={() => undefined} onImported={vi.fn().mockResolvedValue(undefined)} />);

    const file = new File([new Uint8Array([1, 2, 3])], 'passbolt.kdbx');
    fireEvent.change(screen.getByLabelText('Profile export or SSH config file'), { target: { files: [file] } });
    fireEvent.change(screen.getByLabelText('KDBX passphrase'), { target: { value: 'vault-passphrase' } });
    fireEvent.click(screen.getByRole('button', { name: 'Unlock KDBX' }));

    expect(await screen.findByText('ops@prod.example.com')).toBeInTheDocument();
    expect(preview).toHaveBeenCalledWith('AQID', 'vault-passphrase');
    expect(screen.queryByLabelText('KDBX passphrase')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Import 1 profile' }));
    await waitFor(() => expect(save).toHaveBeenCalledWith(expect.objectContaining({
      label: 'Production', hostname: 'prod.example.com', username: 'ops', password: 'vault-secret',
    })));
  });
});
