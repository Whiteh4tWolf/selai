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
  kind: 'ssh',
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

function task(overrides: Partial<ipc.TaskView> = {}): ipc.TaskView {
  return {
    id: 't1',
    hostId: 'h1',
    state: 'completed',
    modelId: 'qwen-tools',
    errorCode: null,
    createdAtMs: Date.UTC(2026, 9, 16, 9, 0),
    finishedAtMs: Date.UTC(2026, 9, 16, 9, 0, 12),
    promptPreview: 'check disk usage on the web node',
    inputTokens: 12345,
    outputTokens: 678,
    modelTurns: 2,
    executions: 1,
    ...overrides,
  };
}

function mockHistory(rows: ipc.ExecutionView[] = [execution()], tasks: ipc.TaskView[] = [task()]) {
  vi.spyOn(ipc, 'hostList').mockResolvedValue({ items: [host, archived], nextCursor: null });
  vi.spyOn(ipc, 'historyTasks').mockResolvedValue({ items: tasks, nextCursor: null });
  vi.spyOn(ipc, 'historySummary').mockResolvedValue({
    tasks: tasks.length,
    executions: rows.length,
    failedExecutions: 1,
    inputTokens: 12345,
    outputTokens: 678,
    lastActivityMs: Date.UTC(2026, 9, 16, 9, 0),
  });
  return vi
    .spyOn(ipc, 'historyQuery')
    .mockResolvedValue({ items: rows, nextCursor: null });
}

/** The command table lives in the Commands view. */
async function showCommands() {
  fireEvent.click(await screen.findByRole('button', { name: 'Commands' }));
  return screen.findByText('df -hP');
}

describe('HistoryTab', () => {
  it('lists the chosen host history, including archived hosts', async () => {
    const query = mockHistory();
    render(<HistoryTab visible />);

    expect(await showCommands()).toBeInTheDocument();
    expect(query).toHaveBeenCalledWith('h1', { states: [] , sinceMs: null, untilMs: null }, undefined);
    expect(screen.getByRole('option', { name: 'retired-db (archived)' })).toBeInTheDocument();
  });

  it('loads again when the tab is shown, so runs finished while hidden appear', async () => {
    const query = mockHistory();
    const { rerender } = render(<HistoryTab visible={false} />);
    await waitFor(() => expect(ipc.hostList).toHaveBeenCalled());
    expect(query).not.toHaveBeenCalled();
    expect(ipc.historyTasks).not.toHaveBeenCalled();

    rerender(<HistoryTab visible />);
    expect(await showCommands()).toBeInTheDocument();
    expect(query).toHaveBeenCalledTimes(1);
    expect(ipc.historyTasks).toHaveBeenCalledTimes(1);
  });

  it('sends the chosen states when the filter is applied', async () => {
    const query = mockHistory();
    render(<HistoryTab visible />);
    await showCommands();

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
    render(<HistoryTab visible />);
    fireEvent.click(await showCommands());

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
    render(<HistoryTab visible />);
    await screen.findByText('check disk usage on the web node');
    fireEvent.click(screen.getByRole('button', { name: 'Export JSON' }));

    expect(await screen.findByRole('status')).toHaveTextContent(
      'Exported 1000 row(s) to /data/exports/selai-history-h1-1.json. The export was cut at the row limit.',
    );
  });

  it('clears stored output only after the operator confirms', async () => {
    mockHistory();
    const purge = vi.spyOn(ipc, 'historyPurgeOutput').mockResolvedValue({ executions: 1 });
    vi.spyOn(window, 'confirm').mockReturnValueOnce(false).mockReturnValueOnce(true);
    render(<HistoryTab visible />);
    await screen.findByText('check disk usage on the web node');

    fireEvent.click(screen.getByRole('button', { name: 'Clear stored output' }));
    expect(purge).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'Clear stored output' }));
    await waitFor(() => expect(purge).toHaveBeenCalledWith('h1'));
  });

  it('opens on tasks with the request, token usage and a host summary', async () => {
    mockHistory();
    render(<HistoryTab visible />);

    expect(await screen.findByText('check disk usage on the web node')).toBeInTheDocument();
    const summary = screen.getByRole('group', { name: 'Usage summary' });
    await waitFor(() => expect(summary).toHaveTextContent('Input tokens12,345'));
    expect(summary).toHaveTextContent('Output tokens678');
    expect(summary).toHaveTextContent('1 failed');
    const tasks = screen.getByRole('list', { name: 'Tasks' });
    expect(tasks).toHaveTextContent('12.3k in');
    expect(tasks).toHaveTextContent('678 out');
  });

  it('shows unknown token counts as unknown, not zero', async () => {
    mockHistory([execution()], [task({ inputTokens: null, outputTokens: null, promptPreview: null })]);
    render(<HistoryTab visible />);
    const tasks = await screen.findByRole('list', { name: 'Tasks' });
    await waitFor(() => expect(tasks).toHaveTextContent('— in'));
    expect(tasks).toHaveTextContent('Request not recorded');
  });

  it('opens a task with its tokens and the commands it ran', async () => {
    mockHistory();
    const timeline = vi
      .spyOn(ipc, 'taskTimeline')
      .mockResolvedValue([execution({ commandRedacted: 'du -sh /var' })]);
    render(<HistoryTab visible />);
    fireEvent.click(await screen.findByText('check disk usage on the web node'));

    expect(timeline).toHaveBeenCalledWith('t1');
    const usage = await screen.findByRole('group', { name: 'Token usage' });
    expect(usage).toHaveTextContent('Input12,345');
    expect(usage).toHaveTextContent('Total13,023');
    expect(await screen.findByText('du -sh /var')).toBeInTheDocument();
  });
});
