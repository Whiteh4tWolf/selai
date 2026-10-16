import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useEffect } from 'react';
import { VaultGate } from './VaultGate';
import * as vault from './vault';
vi.mock('./vault', async (original) => ({ ...await original<typeof import('./vault')>(), vaultStatus: vi.fn(), vaultAction: vi.fn(), vaultLock: vi.fn(), vaultActivity: vi.fn(), vaultFlushComplete: vi.fn() }));
vi.mock('./App', () => ({ WindowControls: () => <span>Window controls</span> }));
const status = (phase: vault.VaultStatus['phase']): vault.VaultStatus => ({ phase, legacyFound: false, timeoutMinutes: 15, activeTasks: 0, terminals: 0, cleanupPending: false, retryAfterMs: 0, flushRequired: false });
describe('Vault gate', () => {
  beforeEach(() => { vi.mocked(vault.vaultStatus).mockResolvedValue(status('locked')); vi.mocked(vault.vaultAction).mockResolvedValue(null); vi.mocked(vault.vaultActivity).mockResolvedValue(); vi.mocked(vault.vaultLock).mockResolvedValue(); });
  afterEach(() => vi.clearAllMocks());
  it('does not mount or fetch workspace data before unlock', async () => {
    const load = vi.fn(); function Workspace() { useEffect(load, []); return <div>Private workspace</div>; }
    render(<VaultGate><Workspace /></VaultGate>);
    expect(await screen.findByText('Unlock Selai')).toBeInTheDocument(); expect(load).not.toHaveBeenCalled();
    vi.mocked(vault.vaultAction).mockImplementation(async () => { vi.mocked(vault.vaultStatus).mockResolvedValue(status('unlocked')); return null; });
    fireEvent.change(screen.getByLabelText('Passphrase'), { target: { value: 'forest river orange telescope' } }); fireEvent.click(screen.getByRole('button', { name: 'Unlock' }));
    expect(await screen.findByText('Private workspace')).toBeVisible(); expect(load).toHaveBeenCalledTimes(1);
  });
  it('requires saving recovery key before opening a new vault', async () => {
    vi.mocked(vault.vaultStatus).mockResolvedValue({ ...status('setup'), legacyFound: true });
    vi.mocked(vault.vaultAction).mockImplementation(async () => { vi.mocked(vault.vaultStatus).mockResolvedValue(status('unlocked')); return 'synthetic-recovery-key'; });
    render(<VaultGate><div>Private workspace</div></VaultGate>);
    expect(await screen.findByText('Protect your workspace')).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('New passphrase'), { target: { value: 'forest river orange telescope' } }); fireEvent.change(screen.getByLabelText('Confirm passphrase'), { target: { value: 'forest river orange telescope' } });
    fireEvent.click(screen.getByRole('button', { name: 'Create encrypted vault' }));
    expect(await screen.findByText('Save your recovery key')).toBeInTheDocument(); expect(screen.queryByText('Private workspace')).not.toBeInTheDocument(); expect(screen.getByRole('button', { name: 'Open workspace' })).toBeDisabled();
    fireEvent.click(screen.getByLabelText('I saved this key somewhere safe.')); fireEvent.click(screen.getByRole('button', { name: 'Open workspace' })); expect(screen.getByText('Private workspace')).toBeVisible();
  });
  it('keeps credentials out of the workspace on a failed unlock', async () => {
    vi.mocked(vault.vaultAction).mockRejectedValue({ code: 'INVALID_INPUT', message: 'Could not unlock the vault.' });
    render(<VaultGate><div>Private workspace</div></VaultGate>); await screen.findByText('Unlock Selai');
    fireEvent.change(screen.getByLabelText('Passphrase'), { target: { value: 'wrong' } }); fireEvent.click(screen.getByRole('button', { name: 'Unlock' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Could not unlock'); expect(screen.queryByText('Private workspace')).not.toBeInTheDocument();
  });
  it('can restore a backup with its recovery key and a new passphrase', async () => {
    vi.mocked(vault.vaultStatus).mockResolvedValue(status('setup'));
    vi.mocked(vault.vaultAction).mockImplementation(async () => {vi.mocked(vault.vaultStatus).mockResolvedValue(status('unlocked'));return 'new-recovery-code';});
    render(<VaultGate><div>Private workspace</div></VaultGate>);
    fireEvent.click(await screen.findByRole('button',{name:'Restore an existing backup'}));
    fireEvent.click(screen.getByRole('button',{name:'Use the backup recovery key instead'}));
    fireEvent.change(screen.getByLabelText('Recovery key'),{target:{value:'saved-backup-recovery-code'}});
    fireEvent.change(screen.getByLabelText('New passphrase'),{target:{value:'forest river orange telescope'}});
    fireEvent.change(screen.getByLabelText('Confirm passphrase'),{target:{value:'forest river orange telescope'}});
    fireEvent.click(screen.getByRole('button',{name:'Choose backup and restore'}));
    expect(await screen.findByText('Save your recovery key')).toBeInTheDocument();
    expect(vault.vaultAction).toHaveBeenCalledWith('restoreRecovery','saved-backup-recovery-code','forest river orange telescope');
    expect(screen.getByLabelText('Recovery key')).toHaveValue('new-recovery-code');
    expect(screen.queryByText('Private workspace')).not.toBeInTheDocument();
  });
  it('hides during drain, then unmounts all workspace state when locked', async () => {
    const dispose=vi.fn();function Workspace(){useEffect(()=>dispose,[]);return <div>Private workspace</div>;}
    vi.mocked(vault.vaultStatus).mockResolvedValue(status('unlocked')); render(<VaultGate><Workspace /></VaultGate>); expect(await screen.findByText('Private workspace')).toBeVisible();
    vi.mocked(vault.vaultStatus).mockResolvedValue(status('locking')); await waitFor(()=>expect(screen.getByText('Private workspace')).not.toBeVisible(),{timeout:1500}); expect(dispose).not.toHaveBeenCalled();
    vi.mocked(vault.vaultStatus).mockResolvedValue(status('locked')); await waitFor(()=>expect(screen.queryByText('Private workspace')).not.toBeInTheDocument(),{timeout:1500}); expect(dispose).toHaveBeenCalledTimes(1);
  });
});
