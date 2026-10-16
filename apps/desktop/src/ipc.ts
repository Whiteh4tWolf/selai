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

export function hostList(query?: string, includeArchived = false): Promise<Page<HostView>> {
  return invoke<Page<HostView>>('host_list', { query: query ?? null, includeArchived });
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

export function isCommandError(err: unknown): err is CommandError {
  return typeof err === 'object' && err !== null && 'code' in err && 'message' in err;
}

/** A refused connect the user can resolve by approving the server key. */
export interface HostKeyProblem {
  kind: 'unknown' | 'changed';
  details: KeyErrorDetails;
}

export function hostKeyProblem(err: unknown): HostKeyProblem | null {
  if (!isCommandError(err) || !err.details) return null;
  if (err.code === 'SSH_HOSTKEY_UNKNOWN') return { kind: 'unknown', details: err.details };
  if (err.code === 'SSH_HOSTKEY_CHANGED') return { kind: 'changed', details: err.details };
  return null;
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
  allowInsecureHttp: boolean;
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
  /** Lets `http://` reach a non-loopback host. The key and prompts then travel unencrypted. */
  allowInsecureHttp?: boolean;
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

// ---- agent (M3) -------------------------------------------------------------

/** One event of a task. Mirrors `agent_service::TaskEventView` in Rust. */
export type TaskEvent =
  | { type: 'state'; state: string; errorCode: string | null }
  | { type: 'textDelta'; text: string }
  | { type: 'toolProposed'; executionId: string; tool: string; command: string; hostLabel: string }
  | { type: 'approvalRequired'; approvalId: string; executionId: string; command: string; expiresAtMs: number }
  | { type: 'toolFinished'; executionId: string; state: string; exitCode: number | null; errorCode: string | null }
  | { type: 'completed'; text: string }
  | { type: 'failed'; code: string; message: string };

export type AgentEvent = TaskEvent & { taskId: string };

export interface ExecutionView {
  id: string;
  taskId: string;
  hostId: string;
  modelId: string;
  tool: string;
  commandRedacted: string;
  commandSha256: string;
  state: string;
  policyDecision: string | null;
  exitCode: number | null;
  errorCode: string | null;
  stdoutPreview: string | null;
  stderrPreview: string | null;
  outputTruncated: boolean;
  createdAtMs: number;
  startedAtMs: number | null;
  finishedAtMs: number | null;
}

export interface AuditVerifyView {
  ok: boolean;
  eventsChecked: number;
  brokenAtSeq: number | null;
}

export function agentStart(
  hostId: string,
  providerId: string,
  prompt: string,
  onEvent: (event: AgentEvent) => void,
): Promise<string> {
  const channel = new Channel<AgentEvent>();
  channel.onmessage = onEvent;
  return invoke<string>('agent_start', { hostId, providerId, prompt, onEvent: channel });
}

export function agentCancel(taskId: string): Promise<boolean> {
  return invoke<boolean>('agent_cancel', { taskId });
}

export function approvalDecide(approvalId: string, approve: boolean): Promise<void> {
  return invoke<void>('approval_decide', { approvalId, approve });
}

export function historyList(hostId: string, cursor?: string): Promise<Page<ExecutionView>> {
  return invoke<Page<ExecutionView>>('history_list', { hostId, cursor: cursor ?? null });
}

export function auditVerify(): Promise<AuditVerifyView> {
  return invoke<AuditVerifyView>('audit_verify');
}

// ---- AI history and preferences (M4) ----------------------------------------

export interface HistoryFilter {
  states: string[];
  sinceMs?: number | null;
  untilMs?: number | null;
}

export interface TaskView {
  id: string;
  hostId: string;
  state: string;
  modelId: string;
  errorCode: string | null;
  createdAtMs: number;
  finishedAtMs: number | null;
}

export interface ExportResult {
  path: string;
  executions: number;
  truncatedRows: boolean;
  extraRedactions: number;
  disclosure: string;
}

export interface PurgeResult {
  executions: number;
}

export interface AiPreferences {
  lastProviderId: string | null;
  outputRetentionDays: number;
}

export function historyQuery(
  hostId: string,
  filter: HistoryFilter,
  cursor?: string,
): Promise<Page<ExecutionView>> {
  return invoke<Page<ExecutionView>>('history_query', { hostId, filter, cursor: cursor ?? null });
}

export function historyTasks(hostId: string, cursor?: string): Promise<Page<TaskView>> {
  return invoke<Page<TaskView>>('history_tasks', { hostId, cursor: cursor ?? null });
}

export function taskTimeline(taskId: string): Promise<ExecutionView[]> {
  return invoke<ExecutionView[]>('task_timeline', { taskId });
}

export function historyExport(hostId: string, filter: HistoryFilter): Promise<ExportResult> {
  return invoke<ExportResult>('history_export', { hostId, filter });
}

export function historyPurgeOutput(hostId: string): Promise<PurgeResult> {
  return invoke<PurgeResult>('history_purge_output', { hostId });
}

export function preferencesGet(): Promise<AiPreferences> {
  return invoke<AiPreferences>('preferences_get');
}

export function preferencesSet(input: Partial<AiPreferences>): Promise<AiPreferences> {
  return invoke<AiPreferences>('preferences_set', { input });
}
