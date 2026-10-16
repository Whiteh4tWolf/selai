import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { AgentPanel } from './AgentPanel';
import { PROVIDERS_CHANGED } from './ProvidersPanel';

afterEach(() => vi.restoreAllMocks());

const verified: ipc.ProviderView = {
  id: 'p1',
  name: 'local-vllm',
  protocol: 'openai_chat',
  baseUrl: 'http://127.0.0.1:8000/v1',
  modelId: 'qwen-tools',
  timeoutSeconds: 120,
  maxOutputTokens: 4096,
  keyConfigured: false,
  isLoopback: true,
  allowInsecureHttp: false,
  updatedAtMs: 7,
  toolCalling: { status: 'verified', lastCheckedAtMs: 1 },
};

const host: ipc.HostView = {
  id: 'h1',
  label: 'production-web',
  groupId: null,
  hostname: '10.0.0.5',
  port: 22,
  username: 'ops',
  authMethod: 'password',
  privateKeyPath: null,
  connectTimeoutSeconds: 10,
  keepAliveSeconds: 30,
  terminalType: 'xterm-256color',
  notes: null,
  credentialConfigured: true,
  updatedAtMs: 1,
  lastConnectedAtMs: null,
  archived: false,
};

/** Captures the event callback so a test can play the task's events. */
function mockAgent() {
  let emit: ((event: ipc.AgentEvent) => void) | undefined;
  const start = vi.spyOn(ipc, 'agentStart').mockImplementation(async (_h, _p, _r, onEvent) => {
    emit = onEvent;
    return 't1';
  });
  return { start, play: (event: ipc.AgentEvent) => act(() => emit?.(event)) };
}

async function pickProductionWeb() {
  vi.spyOn(ipc, 'providerList').mockResolvedValue({ items: [verified], nextCursor: null });
  vi.spyOn(ipc, 'hostList').mockResolvedValue({ items: [host], nextCursor: null });
  render(<AgentPanel />);
  const textarea = screen.getByLabelText('Request');
  fireEvent.change(textarea, { target: { value: '@prod' } });
  fireEvent.click(await screen.findByRole('button', { name: /production-web/ }));
  await screen.findByLabelText('Remove production-web');
  expect(textarea).toHaveValue('@production-web ');
  return textarea;
}

async function sendRequest(textarea: HTMLElement, text: string) {
  fireEvent.change(textarea, { target: { value: `@production-web ${text}` } });
  await waitFor(() => expect(screen.getByRole('button', { name: 'Send' })).toBeEnabled());
  fireEvent.click(screen.getByRole('button', { name: 'Send' }));
}

describe('AgentPanel', () => {
  it('chooses the host with @ and starts the task on that host and provider', async () => {
    const agent = mockAgent();
    const textarea = await pickProductionWeb();
    await sendRequest(textarea, 'check disk usage');

    await waitFor(() =>
      expect(agent.start).toHaveBeenCalledWith('h1', 'p1', '@production-web check disk usage', expect.any(Function)),
    );
    // The sent message stays in the thread with the mention highlighted.
    expect(await screen.findByText('@production-web', { selector: '.mention' })).toBeInTheDocument();
  });

  it('mentions a label with spaces and binds the task to that host id', async () => {
    const agent = mockAgent();
    vi.spyOn(ipc, 'providerList').mockResolvedValue({ items: [verified], nextCursor: null });
    const list = vi
      .spyOn(ipc, 'hostList')
      .mockResolvedValue({ items: [{ ...host, id: 'h7', label: 'Production Web' }], nextCursor: null });
    render(<AgentPanel />);
    const textarea = screen.getByLabelText('Request');
    fireEvent.change(textarea, { target: { value: 'restart @Production W' } });
    fireEvent.click(await screen.findByRole('button', { name: /Production Web/ }));
    expect(list).toHaveBeenLastCalledWith('Production W');
    expect(textarea).toHaveValue('restart @Production Web ');
    expect(screen.queryByRole('list', { name: 'Matching hosts' })).not.toBeInTheDocument();

    fireEvent.change(textarea, { target: { value: 'restart @Production Web nginx' } });
    fireEvent.keyDown(textarea, { key: 'Enter' });
    await waitFor(() =>
      expect(agent.start).toHaveBeenCalledWith('h7', 'p1', 'restart @Production Web nginx', expect.any(Function)),
    );
  });

  it('deleting the mention text removes the target', async () => {
    const textarea = await pickProductionWeb();
    fireEvent.change(textarea, { target: { value: 'check disk usage' } });
    expect(screen.queryByLabelText('Remove production-web')).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('picks a host from the @ list with the keyboard', async () => {
    vi.spyOn(ipc, 'providerList').mockResolvedValue({ items: [verified], nextCursor: null });
    vi.spyOn(ipc, 'hostList').mockResolvedValue({
      items: [host, { ...host, id: 'h2', label: 'production-db', hostname: '10.0.0.6' }],
      nextCursor: null,
    });
    render(<AgentPanel />);
    const textarea = screen.getByLabelText('Request');
    fireEvent.change(textarea, { target: { value: 'check @prod' } });
    expect(await screen.findByRole('button', { name: /production-db/ })).toBeInTheDocument();

    fireEvent.keyDown(textarea, { key: 'ArrowDown' });
    fireEvent.keyDown(textarea, { key: 'Enter' });

    expect(await screen.findByLabelText('Remove production-db')).toBeInTheDocument();
    expect(textarea).toHaveValue('check @production-db ');
  });

  it('does not allow sending without a host or with an unverified provider', async () => {
    vi.spyOn(ipc, 'providerList').mockResolvedValue({
      items: [{ ...verified, toolCalling: { status: 'not_verified', lastCheckedAtMs: null } }],
      nextCursor: null,
    });
    const openProviders = vi.fn();
    render(<AgentPanel onOpenProviders={openProviders} />);
    expect(await screen.findByText(/No verified provider selected/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Request'), { target: { value: 'check disk usage' } });
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'Open Providers' }));
    expect(openProviders).toHaveBeenCalled();
  });

  it('shows the exact command and sends the operator decision for an approval', async () => {
    const agent = mockAgent();
    const decide = vi.spyOn(ipc, 'approvalDecide').mockResolvedValue(undefined);
    const textarea = await pickProductionWeb();
    await sendRequest(textarea, 'free text');
    await waitFor(() => expect(agent.start).toHaveBeenCalled());

    agent.play({ taskId: 't1', type: 'toolProposed', executionId: 'e1', tool: 'ssh.run_command', command: 'systemctl restart nginx', hostLabel: 'production-web' });
    agent.play({ taskId: 't1', type: 'approvalRequired', approvalId: 'a1', executionId: 'e1', command: 'systemctl restart nginx', expiresAtMs: Date.now() + 300_000 });

    const card = await screen.findByRole('group', { name: 'Approval required' });
    expect(card).toHaveTextContent('systemctl restart nginx');
    expect(card).toHaveTextContent('Run this command on production-web?');

    fireEvent.click(screen.getByRole('button', { name: 'Approve' }));
    await waitFor(() => expect(decide).toHaveBeenCalledWith('a1', true));
    expect(await screen.findByText('Approved. Waiting for the result.')).toBeInTheDocument();
  });

  it('shows why the database refused a decision', async () => {
    const agent = mockAgent();
    vi.spyOn(ipc, 'approvalDecide').mockRejectedValue({ code: 'APPROVAL_EXPIRED', message: 'the approval expired' });
    const textarea = await pickProductionWeb();
    await sendRequest(textarea, 'free text');
    await waitFor(() => expect(agent.start).toHaveBeenCalled());
    agent.play({ taskId: 't1', type: 'approvalRequired', approvalId: 'a1', executionId: 'e1', command: 'id', expiresAtMs: Date.now() + 300_000 });

    fireEvent.click(await screen.findByRole('button', { name: 'Approve' }));
    expect(await screen.findByText('APPROVAL_EXPIRED: the approval expired')).toBeInTheDocument();
  });

  it('cancels a running task with Stop', async () => {
    const agent = mockAgent();
    const cancel = vi.spyOn(ipc, 'agentCancel').mockResolvedValue(true);
    const textarea = await pickProductionWeb();
    await sendRequest(textarea, 'check disk usage');
    await waitFor(() => expect(agent.start).toHaveBeenCalled());

    fireEvent.click(await screen.findByRole('button', { name: 'Stop' }));
    await waitFor(() => expect(cancel).toHaveBeenCalledWith('t1'));
    agent.play({ taskId: 't1', type: 'state', state: 'cancelled', errorCode: null });
    expect(await screen.findByRole('button', { name: 'Send' })).toBeInTheDocument();
  });

  it('asks to trust the server key when a task cannot connect yet, then starts it', async () => {
    const start = vi
      .spyOn(ipc, 'agentStart')
      .mockRejectedValueOnce({
        code: 'SSH_HOSTKEY_UNKNOWN',
        message: 'host key is not trusted yet',
        details: { algorithm: 'ssh-ed25519', fingerprintSha256: 'SHA256:abc' },
      })
      .mockResolvedValueOnce('t2');
    const trust = vi.spyOn(ipc, 'hostTrust').mockResolvedValue({ outcome: 'pinned', fingerprintSha256: 'SHA256:abc' });
    const textarea = await pickProductionWeb();
    await sendRequest(textarea, 'uptime');

    const dialog = await screen.findByRole('dialog', { name: 'Verify the server' });
    expect(dialog).toHaveTextContent('SHA256:abc');
    fireEvent.click(screen.getByRole('button', { name: 'Trust & connect' }));
    await waitFor(() => expect(trust).toHaveBeenCalledWith('h1', 'SHA256:abc', 'first'));
    await waitFor(() => expect(start).toHaveBeenCalledTimes(2));
    expect(start.mock.calls[1]?.slice(0, 3)).toEqual(['h1', 'p1', '@production-web uptime']);
  });

  it('shows a provider saved elsewhere without restarting the tab', async () => {
    const list = vi
      .spyOn(ipc, 'providerList')
      .mockResolvedValueOnce({ items: [], nextCursor: null })
      .mockResolvedValue({ items: [verified], nextCursor: null });
    render(<AgentPanel />);
    expect(await screen.findByText('No providers configured')).toBeInTheDocument();

    act(() => {
      window.dispatchEvent(new Event(PROVIDERS_CHANGED));
    });

    expect(await screen.findByRole('option', { name: /local-vllm/ })).toBeInTheDocument();
    expect(list).toHaveBeenCalledTimes(2);
    // The shown choice must be the selected one, so Send can be enabled.
    expect(screen.getByLabelText('Provider')).toHaveValue('p1');
    expect(screen.queryByText(/No verified provider/)).not.toBeInTheDocument();
  });

  it('verifies the audit chain on demand', async () => {
    vi.spyOn(ipc, 'providerList').mockResolvedValue({ items: [], nextCursor: null });
    vi.spyOn(ipc, 'auditVerify').mockResolvedValue({ ok: true, eventsChecked: 4, brokenAtSeq: null });
    render(<AgentPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Verify audit log' }));
    expect(await screen.findByText('Audit log intact: 4 event(s) checked.')).toBeInTheDocument();
  });
});
