import { invoke } from '@tauri-apps/api/core';

export interface VaultStatus {
  phase: 'setup' | 'locked' | 'unlocked' | 'locking';
  legacyFound: boolean;
  timeoutMinutes: number;
  activeTasks: number;
  terminals: number;
  cleanupPending: boolean;
  retryAfterMs: number;
  flushRequired: boolean;
}
export const vaultStatus = () => invoke<VaultStatus>('vault_status');
export const vaultActivity = () => invoke<void>('vault_activity');
export const vaultLock = (force: boolean) => invoke<void>('vault_lock', { force });
export const vaultFlushComplete = () => invoke<void>('vault_flush_complete');
export const vaultAction = (action: string, passphrase = '', replacement = '', minutes?: number) =>
  invoke<string | null>('vault_action', { action, passphrase, replacement, minutes });

const flushers = new Set<() => Promise<void>>();
export function registerVaultFlusher(flush: () => Promise<void>) { flushers.add(flush); return () => { flushers.delete(flush); }; }
export async function flushVaultViews() { await Promise.all([...flushers].map((flush) => flush())); }
