import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { HistoryTab } from './HistoryTab';

afterEach(() => vi.restoreAllMocks());

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

const archived: ipc.HostView = { ...host, id: 'h2', label: 'retired-db', archived: true };

function execution(overrides: Partial<ipc.ExecutionView> = {}): ipc.ExecutionView {
  return {
    id: 'e1',
    taskId: 't1',
    hostId: 'h1',
    modelId: 'qwen-tools',
    tool: 'ssh.run_diagnostic',
    commandRedacted: 'df -hP',
    commandSha256: 'ab'.repeat(32),
    state: 'completed',
    policyDecision: 'allow',
    exitCode: 0,
    errorCode: null,
    stdoutPreview: 'Filesystem  Size  Used',
    stderrPreview: null,
    outputTruncated: false,
    createdAtMs: Date.UTC(2026, 9, 16, 9, 0),
    startedAtMs: null,
    finishedAtMs: null,
    ...overrides,
  };
}

function mockHistory(rows: ipc.ExecutionView[] = [execution()]) {
  vi.spyOn(ipc, 'hostList').mockResolvedValue({ items: [host, archived], nextCursor: null });
  return vi
    .spyOn(ipc, 'historyQuery')
    .mockResolvedValue({ items: rows, nextCursor: null });
}

describe('HistoryTab', () => {
  it('lists the chosen host history, including archived hosts', async () => {
    const query = mockHistory();
    render(<HistoryTab />);

    expect(await screen.findByText('df -hP')).toBeInTheDocument();
    expect(query).toHaveBeenCalledWith('h1', { states: [] , sinceMs: null, untilMs: null }, undefined);
    expect(screen.getByRole('option', { name: 'retired-db (archived)' })).toBeInTheDocument();
  });

  it('sends the chosen states when the filter is applied', async () => {
    const query = mockHistory();
    render(<HistoryTab />);
    await screen.findByText('df -hP');

    fireEvent.click(screen.getByRole('checkbox', { name: 'denied' }));
    fireEvent.click(screen.getByRole('button', { name: 'Apply' }));

    await waitFor(() =>
      expect(query).toHaveBeenLastCalledWith(
        'h1',
        expect.objectContaining({ states: ['denied'] }),
        undefined,
      ),
    );
  });

  it('shows the timeline of the task an execution belongs to', async () => {
    mockHistory();
    const timeline = vi.spyOn(ipc, 'taskTimeline').mockResolvedValue([
      execution({ id: 'e1', commandRedacted: 'uptime' }),
      execution({ id: 'e2', commandRedacted: 'df -hP', state: 'denied' }),
    ]);
    render(<HistoryTab />);
    fireEvent.click(await screen.findByText('df -hP'));

    expect(timeline).toHaveBeenCalledWith('t1');
    expect(await screen.findByText('Task timeline')).toBeInTheDocument();
    expect(screen.getByText(/uptime/)).toBeInTheDocument();
  });

  it('reports where the export went and that it was cut at the row limit', async () => {
    mockHistory();
    vi.spyOn(ipc, 'historyExport').mockResolvedValue({
      path: '/data/exports/selai-history-h1-1.json',
      executions: 1000,
      truncatedRows: true,
      extraRedactions: 0,
      disclosure: 'x',
    });
    render(<HistoryTab />);
    await screen.findByText('df -hP');
    fireEvent.click(screen.getByRole('button', { name: 'Export JSON' }));

    expect(await screen.findByRole('status')).toHaveTextContent(
      'Exported 1000 row(s) to /data/exports/selai-history-h1-1.json. The export was cut at the row limit.',
    );
  });

  it('clears stored output only after the operator confirms', async () => {
    mockHistory();
    const purge = vi.spyOn(ipc, 'historyPurgeOutput').mockResolvedValue({ executions: 1 });
    vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true);
    render(<HistoryTab />);
    await screen.findByText('df -hP');

    fireEvent.click(screen.getByRole('button', { name: 'Clear stored output' }));
    expect(purge).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Clear stored output' }));
    await waitFor(() => expect(purge).toHaveBeenCalledWith('h1'));
  });
});
