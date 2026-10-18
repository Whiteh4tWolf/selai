import { Channel, invoke } from '@tauri-apps/api/core';
export interface ProxyProfile {
  id: string;
  name: string;
  kind: 'http' | 'socks5';
  hostname: string;
  port: number;
  username: string | null;
  passwordConfigured: boolean;
}
export type ProxyInput = Omit<ProxyProfile, 'id' | 'passwordConfigured'> & {
  id?: string;
  password?: string;
  clearPassword?: boolean;
};
export interface ForwardProfile {
  id: string;
  name: string;
  hostId: string;
  mode: 'local' | 'remote' | 'dynamic';
  bindAddress: string;
  bindPort: number;
  targetHost: string;
  targetPort: number;
}
export interface ForwardView {
  profile: ForwardProfile;
  status: 'stopped' | 'connecting' | 'running' | 'failed';
  actualPort: number;
  connections: number;
  sent: number;
  received: number;
  connectionErrors: number;
  error: string | null;
}
export interface FileEntry {
  name: string;
  path: string;
  kind: 'file' | 'directory' | 'symlink' | 'other';
  size: number;
  modifiedMs: number | null;
  permissions: number | null;
}
export interface DirectoryView {
  path: string;
  parent: string | null;
  entries: FileEntry[];
}
export interface SftpConnection {
  id: string;
  hostId: string;
  home: string;
}
export interface TransferView {
  id: string;
  connectionId: string;
  direction: 'upload' | 'download';
  status: 'queued' | 'scanning' | 'transferring' | 'completed' | 'failed' | 'cancelled';
  currentFile: string;
  destination: string;
  totalBytes: number;
  transferredBytes: number;
  totalFiles: number;
  completedFiles: number;
  skippedFiles: number;
  bytesPerSecond: number;
  error: string | null;
}
export interface TransferInput {
  connectionId: string;
  direction: 'upload' | 'download';
  sources: string[];
  destination: string;
  overwrite: boolean;
}
export const proxyList = () => invoke<ProxyProfile[]>('proxy_list');
export const proxySave = (input: ProxyInput) => invoke<ProxyProfile>('proxy_save', { input });
export const proxyDelete = (id: string) => invoke<void>('proxy_delete', { id });
export const forwardList = () => invoke<ForwardView[]>('forward_list');
export const forwardWatch = (callback: (event: ForwardView) => void) => {
  const onEvent = new Channel<ForwardView>();
  onEvent.onmessage = callback;
  return invoke<ForwardView[]>('forward_watch', { onEvent });
};
export const transferWatch = (callback: (event: TransferView) => void) => {
  const onEvent = new Channel<TransferView>();
  onEvent.onmessage = callback;
  return invoke<TransferView[]>('transfer_watch', { onEvent });
};
export const sftpList = () => invoke<SftpConnection[]>('sftp_list');
export const forwardSave = (profile: ForwardProfile) => invoke<ForwardView>('forward_save', { profile });
export const forwardStop = (id: string) => invoke<void>('forward_stop', { id });
export const forwardDelete = (id: string) => invoke<void>('forward_delete', { id });
export const forwardStart = (id: string, callback: (event: ForwardView) => void) => {
  const onEvent = new Channel<ForwardView>();
  onEvent.onmessage = callback;
  return invoke<ForwardView>('forward_start', { id, onEvent });
};
export const sftpConnect = (hostId: string) => invoke<SftpConnection>('sftp_connect', { hostId });
export const sftpDisconnect = (id: string) => invoke<void>('sftp_disconnect', { id });
export const filesList = (side: 'local' | 'remote', path: string, connectionId?: string) =>
  invoke<DirectoryView>('files_list', { side, path, connectionId: connectionId ?? null });
export const filesAction = (
  side: 'local' | 'remote',
  action: string,
  path: string,
  connectionId?: string,
  newName?: string,
) =>
  invoke<void>('files_action', {
    side,
    action,
    path,
    connectionId: connectionId ?? null,
    newName: newName ?? null,
  });
export const transferList = () => invoke<TransferView[]>('transfer_list');
export const transferClear = () => invoke<string[]>('transfer_clear');
export const transferCancel = (id: string) => invoke<void>('transfer_cancel', { id });
export const transferStart = (input: TransferInput, callback: (event: TransferView) => void) => {
  const onEvent = new Channel<TransferView>();
  onEvent.onmessage = callback;
  return invoke<TransferView>('transfer_start', { input, onEvent });
};
export function bytes(value: number): string {
  if (value < 1024) return `${value} B`;
  const unit = Math.min(3, Math.floor(Math.log(value) / Math.log(1024)));
  return `${(value / 1024 ** unit).toFixed(unit > 1 ? 1 : 0)} ${['B', 'KiB', 'MiB', 'GiB'][unit]}`;
}
