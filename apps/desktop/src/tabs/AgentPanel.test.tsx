import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { AgentPanel } from './AgentPanel';

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
  vi.spyOn(ipc, 'historyList').mockResolvedValue({ items: [], nextCursor: null });
  render(<AgentPanel />);
  const textarea = screen.getByLabelText('Request');
  fireEvent.change(textarea, { target: { value: '@prod' } });
  fireEvent.click(await screen.findByRole('button', { name: /production-web/ }));
  await screen.findByLabelText('Remove production-web');
  return textarea;
}

describe('AgentPanel', () => {
  it('chooses the host with @ and starts the task on that host and provider', async () => {
    const agent = mockAgent();
    const textarea = await pickProductionWeb();
    fireEvent.change(textarea, { target: { value: 'check disk usage' } });

    fireEvent.click(screen.getByRole('button', { name: 'Send' }));

    await waitFor(() => expect(agent.start).toHaveBeenCalledWith('h1', 'p1', 'check disk usage', expect.any(Function)));
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
    expect(textarea).toHaveValue('check ');
  });

  it('does not allow sending without a host or with an unverified provider', async () => {
    vi.spyOn(ipc, 'providerList').mockResolvedValue({
      items: [{ ...verified, toolCalling: { status: 'not_verified', lastCheckedAtMs: null } }],
      nextCursor: null,
    });
    render(<AgentPanel />);
    expect(await screen.findByText(/Run the tool calling test/)).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText('Request'), { target: { value: 'check disk usage' } });
    expect(screen.getByRole('button', { name: 'Send' })).toBeDisabled();
  });

  it('shows the exact command and sends the operator decision for an approval', async () => {
    const agent = mockAgent();
    const decide = vi.spyOn(ipc, 'approvalDecide').mockResolvedValue(undefined);
    const textarea = await pickProductionWeb();
    fireEvent.change(textarea, { target: { value: 'free text' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));
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
    fireEvent.change(textarea, { target: { value: 'free text' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() => expect(agent.start).toHaveBeenCalled());
    agent.play({ taskId: 't1', type: 'approvalRequired', approvalId: 'a1', executionId: 'e1', command: 'id', expiresAtMs: Date.now() + 300_000 });

    fireEvent.click(await screen.findByRole('button', { name: 'Approve' }));
    expect(await screen.findByText('APPROVAL_EXPIRED: the approval expired')).toBeInTheDocument();
  });

  it('cancels a running task with Stop', async () => {
    const agent = mockAgent();
    const cancel = vi.spyOn(ipc, 'agentCancel').mockResolvedValue(true);
    const textarea = await pickProductionWeb();
    fireEvent.change(textarea, { target: { value: 'check disk usage' } });
    fireEvent.click(screen.getByRole('button', { name: 'Send' }));
    await waitFor(() => expect(agent.start).toHaveBeenCalled());

    fireEvent.click(await screen.findByRole('button', { name: 'Stop' }));
    await waitFor(() => expect(cancel).toHaveBeenCalledWith('t1'));
  });

  it('verifies the audit chain on demand', async () => {
    vi.spyOn(ipc, 'providerList').mockResolvedValue({ items: [], nextCursor: null });
    vi.spyOn(ipc, 'auditVerify').mockResolvedValue({ ok: true, eventsChecked: 4, brokenAtSeq: null });
    render(<AgentPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Verify audit log' }));
    expect(await screen.findByText('Audit log intact: 4 event(s) checked.')).toBeInTheDocument();
  });
});
