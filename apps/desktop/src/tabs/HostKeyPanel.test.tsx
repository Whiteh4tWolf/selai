import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { HostKeyPanel } from './HostKeyPanel';

afterEach(() => vi.restoreAllMocks());

const host = {
  id: 'h1',
  label: 'web',
  hostname: 'web.example.com',
  port: 22,
} as ipc.HostView;

const observed = {
  algorithm: 'ssh-ed25519',
  fingerprintSha256: 'SHA256:new',
  publicKeyBase64: 'AAAA',
};

describe('HostKeyPanel', () => {
  it('first contact shows the fingerprint and trusts the probed key only', async () => {
    vi.spyOn(ipc, 'hostProbe').mockResolvedValue({ observed, status: 'unknown', pinnedFingerprintSha256: null });
    const trust = vi.spyOn(ipc, 'hostTrust').mockResolvedValue({ outcome: 'pinned', fingerprintSha256: 'SHA256:new' });
    render(<HostKeyPanel host={host} onDone={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: 'Read server key' }));
    expect(await screen.findByText('SHA256:new')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'Trust and save' }));
    await waitFor(() => expect(trust).toHaveBeenCalledWith('h1', 'SHA256:new', 'first'));
  });

  it('a changed key cannot be replaced until the operator confirms it out of band', async () => {
    vi.spyOn(ipc, 'hostProbe').mockResolvedValue({
      observed,
      status: 'changed',
      pinnedFingerprintSha256: 'SHA256:old',
    });
    const trust = vi.spyOn(ipc, 'hostTrust').mockResolvedValue({ outcome: 'rotated', fingerprintSha256: 'SHA256:new' });
    render(<HostKeyPanel host={host} onDone={() => {}} />);
    fireEvent.click(screen.getByRole('button', { name: 'Read server key' }));
    const replace = await screen.findByRole('button', { name: 'Replace trusted key' });
    expect(replace).toBeDisabled();
    expect(screen.getByText('SHA256:old')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('checkbox'));
    expect(replace).toBeEnabled();
    fireEvent.click(replace);
    await waitFor(() => expect(trust).toHaveBeenCalledWith('h1', 'SHA256:new', 'replace'));
  });
});
