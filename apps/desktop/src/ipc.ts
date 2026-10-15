// Typed wrappers around Tauri commands. The webview is presentation only:
// every authorization decision happens in the Rust core.
import { invoke } from '@tauri-apps/api/core';

export interface AppInfo {
  name: string;
  version: string;
}

export interface StorageStatus {
  schemaVersion: number;
  journalMode: string;
  interruptedTasksAtStartup: number;
  interruptedExecutionsAtStartup: number;
}

/** Error shape returned by every failing Rust command. */
export interface CommandError {
  code: string;
  message: string;
}

export function appInfo(): Promise<AppInfo> {
  return invoke<AppInfo>('app_info');
}

export function storageStatus(): Promise<StorageStatus> {
  return invoke<StorageStatus>('storage_status');
}

export function describeError(err: unknown): string {
  if (typeof err === 'object' && err !== null && 'code' in err && 'message' in err) {
    const { code, message } = err as CommandError;
    return `${code}: ${message}`;
  }
  return String(err);
}
