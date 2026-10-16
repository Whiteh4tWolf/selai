// Typed wrappers around Tauri commands. The webview is presentation only:
// every authorization decision happens in the Rust core.
import { Channel, invoke } from '@tauri-apps/api/core';

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
  details?: KeyErrorDetails;
}

export interface KeyErrorDetails {
  algorithm: string;
  fingerprintSha256: string;
  pinnedFingerprintSha256?: string | null;
}

export type AuthMethod = 'password' | 'private_key';

export interface HostView {
  id: string;
  label: string;
  groupId: string | null;
  hostname: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  privateKeyPath: string | null;
  connectTimeoutSeconds: number;
  keepAliveSeconds: number;
  terminalType: string;
  notes: string | null;
  credentialConfigured: boolean;
  updatedAtMs: number;
  lastConnectedAtMs: number | null;
  archived: boolean;
}

export interface Page<T> {
  items: T[];
  nextCursor: string | null;
}

export interface HostInput {
  id?: string;
  expectedUpdatedAtMs?: number;
  label: string;
  groupId?: string | null;
  hostname: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  privateKeyPath?: string | null;
  connectTimeoutSeconds: number;
  keepAliveSeconds: number;
  terminalType: string;
  notes?: string | null;
  /** Write-only. Empty or absent keeps the stored value. */
  password?: string;
  /** Write-only. Empty or absent keeps the stored value. */
  keyPassphrase?: string;
}

export interface ObservedKey {
  algorithm: string;
  fingerprintSha256: string;
  publicKeyBase64: string;
}

export interface ProbeView {
  observed: ObservedKey;
  status: 'trusted' | 'unknown' | 'changed';
  pinnedFingerprintSha256: string | null;
}

export type TrustMode = 'first' | 'replace';

export interface TrustView {
  outcome: 'pinned' | 'rotated' | 'unchanged';
  fingerprintSha256: string;
}

export interface GroupView {
  id: string;
  name: string;
  sortOrder: number;
}

export type TerminalEvent =
  | { type: 'data'; data: string }
  | { type: 'exit'; status: number }
  | { type: 'closed'; exitStatus: number | null }
  | { type: 'error'; code: string; message: string };

export function appInfo(): Promise<AppInfo> {
  return invoke<AppInfo>('app_info');
}

export function storageStatus(): Promise<StorageStatus> {
  return invoke<StorageStatus>('storage_status');
}

export function hostList(query?: string): Promise<Page<HostView>> {
  return invoke<Page<HostView>>('host_list', { query: query ?? null });
}

export function hostSave(input: HostInput): Promise<HostView> {
  return invoke<HostView>('host_save', { input });
}

export function hostArchive(id: string): Promise<void> {
  return invoke<void>('host_archive', { id });
}

export function hostProbe(id: string): Promise<ProbeView> {
  return invoke<ProbeView>('host_probe', { id });
}

export function hostTrust(id: string, fingerprintSha256: string, mode: TrustMode): Promise<TrustView> {
  return invoke<TrustView>('host_trust', { id, fingerprintSha256, mode });
}

export function terminalConnect(
  hostId: string,
  cols: number,
  rows: number,
  onEvent: (event: TerminalEvent) => void,
): Promise<string> {
  const channel = new Channel<TerminalEvent>();
  channel.onmessage = onEvent;
  return invoke<string>('terminal_connect', { hostId, cols, rows, onEvent: channel });
}

export function terminalInput(sessionId: string, data: string): Promise<void> {
  return invoke<void>('terminal_input', { sessionId, data });
}

export function terminalResize(sessionId: string, cols: number, rows: number): Promise<void> {
  return invoke<void>('terminal_resize', { sessionId, cols, rows });
}

export function terminalDisconnect(sessionId: string): Promise<void> {
  return invoke<void>('terminal_disconnect', { sessionId });
}

export function describeError(err: unknown): string {
  if (typeof err === 'object' && err !== null && 'code' in err && 'message' in err) {
    const { code, message } = err as CommandError;
    return `${code}: ${message}`;
  }
  return String(err);
}

/** Decodes the base64 payload of a terminal data event. */
export function decodeBase64(data: string): Uint8Array {
  const binary = atob(data);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

export type ProviderProtocolName = 'openai_responses' | 'openai_chat' | 'anthropic_messages';

export interface ToolCallingStatus {
  status: 'verified' | 'failed' | 'not_verified';
  lastCheckedAtMs: number | null;
}

export interface ProviderView {
  id: string;
  name: string;
  protocol: ProviderProtocolName;
  baseUrl: string;
  modelId: string;
  timeoutSeconds: number;
  maxOutputTokens: number;
  keyConfigured: boolean;
  isLoopback: boolean;
  updatedAtMs: number;
  toolCalling: ToolCallingStatus;
}

export interface TemplateView {
  id: string;
  displayName: string;
  protocol: ProviderProtocolName;
  defaultBaseUrl: string;
  note: string;
}

export interface ProviderInput {
  id?: string;
  expectedUpdatedAtMs?: number;
  name: string;
  protocol: ProviderProtocolName;
  baseUrl: string;
  modelId: string;
  timeoutSeconds: number;
  maxOutputTokens: number;
  /** Write-only. Empty or absent keeps the stored key. */
  apiKey?: string;
  clearApiKey?: boolean;
}

export interface ProviderTestView {
  kind: 'connectivity' | 'tool_calling';
  ok: boolean;
  verified: boolean;
  code: string | null;
  message: string;
  latencyMs: number;
  replyPreview: string | null;
}

export function providerTemplates(): Promise<TemplateView[]> {
  return invoke<TemplateView[]>('provider_templates');
}

export function providerList(): Promise<Page<ProviderView>> {
  return invoke<Page<ProviderView>>('provider_list', { cursor: null });
}

export function providerSave(input: ProviderInput): Promise<ProviderView> {
  return invoke<ProviderView>('provider_save', { input });
}

export function providerDelete(id: string): Promise<void> {
  return invoke<void>('provider_delete', { id });
}

export function providerTest(id: string, kind: 'connectivity' | 'tool_calling'): Promise<ProviderTestView> {
  return invoke<ProviderTestView>('provider_test', { id, kind });
}
