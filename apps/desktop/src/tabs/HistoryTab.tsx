import { useEffect, useState } from 'react';
import {
  describeError,
  historyExport,
  historyPurgeOutput,
  historyQuery,
  historySummary,
  historyTasks,
  hostList,
  localHost,
  preferencesGet,
  preferencesSet,
  taskTimeline,
  type ExecutionView,
  type ExportResult,
  type HistoryFilter,
  type HostUsageView,
  type HostView,
  type TaskView,
} from '../ipc';
import { formatCompact, formatCount, formatDuration, formatWhen, sumTokens } from '../ui/format';
import { HostAvatar } from '../ui/HostAvatar';
import { Drawer } from '../ui/Overlay';
import { ClockIcon, TerminalIcon } from '../ui/icons';
import { SelaiLogo } from '../ui/SelaiLogo';

const STATES = ['completed', 'failed', 'denied', 'cancelled', 'interrupted', 'timed_out'] as const;

type View = 'tasks' | 'commands';

/** Local calendar date (yyyy-mm-dd) to epoch milliseconds, start or end of day. */
function dayBoundary(date: string, end: boolean): number | null {
  if (!date) return null;
  const [y, m, d] = date.split('-').map(Number);
  if (!y || !m || !d) return null;
  const at = end
    ? new Date(y, m - 1, d, 23, 59, 59, 999)
    : new Date(y, m - 1, d, 0, 0, 0, 0);
  return at.getTime();
}

function stateTone(state: string): string | undefined {
  if (state === 'completed') return 'success';
  if (['failed', 'denied', 'timed_out', 'interrupted'].includes(state)) return 'danger';
  if (state === 'cancelled' || state === 'awaiting_approval') return 'warning';
  return 'accent';
}

function stateText(state: string): string {
  return state.replace(/_/g, ' ');
}

/** `visible` comes from the app shell. The panel stays mounted while hidden, so it
 *  reloads each time it is shown and picks up runs that finished meanwhile. */
export function HistoryTab({ visible }: { visible: boolean }) {
  const [hosts, setHosts] = useState<HostView[]>([]);
  const [hostId, setHostId] = useState('');
  const [view, setView] = useState<View>('tasks');
  const [summary, setSummary] = useState<HostUsageView | null>(null);
  const [tasks, setTasks] = useState<TaskView[]>([]);
  const [tasksCursor, setTasksCursor] = useState<string | null>(null);
  const [openTask, setOpenTask] = useState<TaskView | null>(null);
  const [states, setStates] = useState<string[]>([]);
  const [from, setFrom] = useState('');
  const [to, setTo] = useState('');
  const [rows, setRows] = useState<ExecutionView[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [selected, setSelected] = useState<ExecutionView | null>(null);
  const [timeline, setTimeline] = useState<ExecutionView[] | null>(null);
  const [retention, setRetention] = useState(30);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let active = true;
    Promise.all([localHost().catch(() => null), hostList(undefined, true)])
      .then(([local, page]) => {
        if (!active) return;
        const all = [...page.items, ...(local ? [local] : [])];
        setHosts(all);
        setHostId((current) => current || all[0]?.id || '');
      })
      .catch((err: unknown) => active && setError(describeError(err)));
    preferencesGet()
      .then((prefs) => active && setRetention(prefs.outputRetentionDays))
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  function currentFilter(): HistoryFilter {
    return { states, sinceMs: dayBoundary(from, false), untilMs: dayBoundary(to, true) };
  }

  async function search(append = false) {
    if (!hostId) return;
    setError(null);
    try {
      const page = await historyQuery(hostId, currentFilter(), append ? nextCursor ?? undefined : undefined);
      setRows((prev) => (append ? [...prev, ...page.items] : page.items));
      setNextCursor(page.nextCursor);
      if (!append) {
        setSelected(null);
        setTimeline(null);
      }
    } catch (err) {
      setError(describeError(err));
    }
  }

  async function loadTasks(append = false) {
    if (!hostId) return;
    try {
      const page = await historyTasks(hostId, append ? tasksCursor ?? undefined : undefined);
      setTasks((prev) => (append ? [...prev, ...page.items] : page.items));
      setTasksCursor(page.nextCursor);
    } catch (err) {
      setError(describeError(err));
    }
  }

  async function loadSummary() {
    if (!hostId) return;
    try {
      setSummary(await historySummary(hostId));
    } catch (err) {
      setError(describeError(err));
    }
  }

  // Load when the tab is shown or the host changes. The command filter is
  // applied by the Apply button, not on every change.
  useEffect(() => {
    if (!visible || !hostId) return;
    setNotice(null);
    void Promise.all([loadSummary(), loadTasks(false), search(false)]).finally(() => setLoaded(true));
  }, [visible, hostId]);

  function toggleState(state: string) {
    setStates((prev) => (prev.includes(state) ? prev.filter((s) => s !== state) : [...prev, state]));
  }

  async function openExecution(row: ExecutionView) {
    setSelected(row);
    try {
      setTimeline(await taskTimeline(row.taskId));
    } catch (err) {
      setError(describeError(err));
    }
  }

  async function exportHistory() {
    setError(null);
    try {
      const result: ExportResult = await historyExport(hostId, currentFilter());
      const parts = [`Exported ${result.executions} row(s) to ${result.path}.`];
      if (result.truncatedRows) parts.push('The export was cut at the row limit.');
      if (result.extraRedactions > 0) parts.push(`${result.extraRedactions} secret-like value(s) were redacted again.`);
      setNotice(parts.join(' '));
    } catch (err) {
      setError(describeError(err));
    }
  }

  async function purge() {
    if (!window.confirm('Delete stored command output for this host? Command history stays.')) return;
    try {
      const result = await historyPurgeOutput(hostId);
      setNotice(`Output cleared for ${result.executions} command(s). Command records are kept.`);
      await search(false);
    } catch (err) {
      setError(describeError(err));
    }
  }

  async function saveRetention() {
    try {
      const prefs = await preferencesSet({ outputRetentionDays: retention });
      setRetention(prefs.outputRetentionDays);
      setNotice(`Output is kept for ${prefs.outputRetentionDays} day(s).`);
    } catch (err) {
      setError(describeError(err));
    }
  }

  const host = hosts.find((h) => h.id === hostId);
  const maxTotal = Math.max(1, ...tasks.map((t) => sumTokens(t.inputTokens, t.outputTokens) ?? 0));

  return (
    <section aria-label="AI history" className="page history-page">
      <header className="page-head">
        {host && <HostAvatar label={host.label} />}
        <div className="grow">
          <h1>History</h1>
          <p>What the AI Agent did on a host: requests, commands and token usage. Archived hosts keep their history.</p>
        </div>
        <label className="field host-select">
          Host
          <select value={hostId} onChange={(e) => setHostId(e.target.value)}>
            {hosts.length === 0 && <option value="">No hosts</option>}
            {hosts.map((h) => (
              <option key={h.id} value={h.id}>
                {h.label}
                {h.archived ? ' (archived)' : ''}
              </option>
            ))}
          </select>
        </label>
      </header>

      {notice && (
        <p role="status" className="banner" data-tone="success">
          {notice}
        </p>
      )}
      {error && (
        <p role="alert" className="banner" data-tone="danger">
          {error}
        </p>
      )}

      <div className="stats" role="group" aria-label="Usage summary">
        <Stat label="AI tasks" value={formatCount(summary?.tasks ?? null)} />
        <Stat
          label="Commands"
          value={formatCount(summary?.executions ?? null)}
          sub={summary && summary.failedExecutions > 0 ? `${summary.failedExecutions} failed` : undefined}
          subTone="danger"
        />
        <Stat label="Input tokens" value={formatCount(summary?.inputTokens ?? null)} />
        <Stat label="Output tokens" value={formatCount(summary?.outputTokens ?? null)} />
        <Stat
          label="Last activity"
          value={summary?.lastActivityMs ? new Date(summary.lastActivityMs).toLocaleDateString() : '—'}
          sub={summary?.lastActivityMs ? new Date(summary.lastActivityMs).toLocaleTimeString() : undefined}
        />
      </div>

      <div className="view-bar">
        <div className="segmented" role="group" aria-label="History view">
          <button type="button" aria-pressed={view === 'tasks'} onClick={() => setView('tasks')}>
            Tasks
          </button>
          <button type="button" aria-pressed={view === 'commands'} onClick={() => setView('commands')}>
            Commands
          </button>
        </div>
        <span className="grow" />
        <button type="button" className="btn btn-sm" onClick={exportHistory} disabled={!hostId}>
          Export JSON
        </button>
        <button type="button" className="btn btn-ghost btn-sm" onClick={purge} disabled={!hostId}>
          Clear stored output
        </button>
      </div>

      {view === 'tasks' && (
        <>
          {loaded && tasks.length === 0 ? (
            <Empty text="Tasks you start from the AI Agent on this host appear here with their token usage." />
          ) : (
            <ul className="task-list" aria-label="Tasks">
              {tasks.map((t) => {
                const total = sumTokens(t.inputTokens, t.outputTokens);
                return (
                  <li key={t.id}>
                    <button type="button" className="task-row" onClick={() => setOpenTask(t)}>
                      <span className="task-icon" data-tone={stateTone(t.state)}>
                        <SelaiLogo size={20} />
                      </span>
                      <span className="task-main">
                        <strong>{t.promptPreview ?? 'Request not recorded (task from an older version)'}</strong>
                        <span className="meta">
                          {formatWhen(t.createdAtMs)} · {t.modelId} · {t.executions} command
                          {t.executions === 1 ? '' : 's'}
                          {t.finishedAtMs ? ` · ${formatDuration(t.finishedAtMs - t.createdAtMs)}` : ''}
                        </span>
                      </span>
                      <span
                        className="task-tokens"
                        title={
                          total === null
                            ? 'The provider did not report token usage'
                            : `${formatCount(t.inputTokens)} input · ${formatCount(t.outputTokens)} output tokens`
                        }
                      >
                        <span className="tok">
                          <b>{formatCompact(t.inputTokens)}</b> in
                        </span>
                        <span className="tok">
                          <b>{formatCompact(t.outputTokens)}</b> out
                        </span>
                        <span className="tok-bar" aria-hidden="true">
                          <span style={{ width: `${Math.round(((total ?? 0) / maxTotal) * 100)}%` }} />
                        </span>
                      </span>
                      <span className="badge" data-tone={stateTone(t.state)}>
                        {stateText(t.state)}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          {tasksCursor && (
            <div className="toolbar-row">
              <button type="button" className="btn" onClick={() => loadTasks(true)}>
                Load more
              </button>
            </div>
          )}
        </>
      )}

      {view === 'commands' && (
        <>
          <div className="card filters-bar">
            <label className="field">
              From
              <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
            </label>
            <label className="field">
              To
              <input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
            </label>
            <fieldset className="state-filters">
              <legend>State</legend>
              {STATES.map((state) => (
                <label key={state} className="pill-check">
                  <input type="checkbox" checked={states.includes(state)} onChange={() => toggleState(state)} />
                  {stateText(state)}
                </label>
              ))}
            </fieldset>
            <span className="grow" />
            <button type="button" className="btn btn-primary" onClick={() => search(false)} disabled={!hostId}>
              Apply
            </button>
          </div>

          {rows.length === 0 ? (
            <Empty text="Commands the AI Agent runs on this host appear here with their output and exit code." />
          ) : (
            <div className="card table-card">
              <table className="history-table">
                <thead>
                  <tr>
                    <th scope="col">Time</th>
                    <th scope="col">Command</th>
                    <th scope="col">State</th>
                    <th scope="col">Exit</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.map((row) => (
                    <tr key={row.id} onClick={() => openExecution(row)} aria-selected={selected?.id === row.id}>
                      <td className="muted" style={{ whiteSpace: 'nowrap' }}>
                        {formatWhen(row.createdAtMs)}
                      </td>
                      <td>
                        <code>{row.commandRedacted}</code>
                      </td>
                      <td>
                        <span className="badge" data-tone={stateTone(row.state)}>
                          {stateText(row.state)}
                        </span>
                      </td>
                      <td className="mono">{row.exitCode ?? '—'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {nextCursor && (
            <div className="toolbar-row">
              <button type="button" className="btn" onClick={() => search(true)}>
                Load more
              </button>
            </div>
          )}
        </>
      )}

      <div className="card retention">
        <label className="field" style={{ width: 200 }}>
          Keep output for (days)
          <input
            type="number"
            min={1}
            max={3650}
            value={retention}
            onChange={(e) => setRetention(Number(e.target.value))}
          />
        </label>
        <button type="button" className="btn" onClick={saveRetention}>
          Save retention
        </button>
        <span className="muted">Older output is removed automatically. Command records and token counts are kept.</span>
      </div>

      {openTask && <TaskDrawer task={openTask} onClose={() => setOpenTask(null)} />}

      {selected && (
        <Drawer
          title="Execution"
          subtitle={formatWhen(selected.createdAtMs)}
          onClose={() => {
            setSelected(null);
            setTimeline(null);
          }}
        >
          <section aria-label="Execution details">
            <ExecutionDetails execution={selected} />
            {timeline && (
              <>
                <h3>Task timeline</h3>
                <ol className="timeline">
                  {timeline.map((step) => (
                    <li key={step.id}>
                      <span>
                        <code>{step.commandRedacted}</code> — {stateText(step.state)}
                        {step.exitCode !== null ? `, exit ${step.exitCode}` : ''}
                      </span>
                    </li>
                  ))}
                </ol>
              </>
            )}
          </section>
        </Drawer>
      )}
    </section>
  );
}

function Stat({ label, value, sub, subTone }: { label: string; value: string; sub?: string; subTone?: string }) {
  return (
    <div className="stat card">
      <span className="stat-label">{label}</span>
      <span className="stat-value">{value}</span>
      {sub && (
        <span className="stat-sub" data-tone={subTone}>
          {sub}
        </span>
      )}
    </div>
  );
}

function Empty({ text }: { text: string }) {
  return (
    <div className="empty">
      <span className="empty-icon">
        <ClockIcon size={26} />
      </span>
      <h2>Nothing here yet</h2>
      <p>{text}</p>
    </div>
  );
}

function ExecutionDetails({ execution }: { execution: ExecutionView }) {
  return (
    <>
      <pre className="output" style={{ marginTop: 0 }}>
        <code>{execution.commandRedacted}</code>
      </pre>
      <dl className="details-list">
        <dt>State</dt>
        <dd>
          <span className="badge" data-tone={stateTone(execution.state)}>
            {stateText(execution.state)}
          </span>
        </dd>
        <dt>Exit code</dt>
        <dd>{execution.exitCode ?? '—'}</dd>
        <dt>Tool</dt>
        <dd>{execution.tool}</dd>
        <dt>Model</dt>
        <dd>{execution.modelId}</dd>
        <dt>Policy</dt>
        <dd>{execution.policyDecision ?? '—'}</dd>
        <dt>Command digest (SHA-256)</dt>
        <dd>
          <code>{execution.commandSha256}</code>
        </dd>
        <dt>Error</dt>
        <dd>{execution.errorCode ?? '—'}</dd>
      </dl>
      {execution.outputTruncated && (
        <p role="note" className="banner" data-tone="warning">
          The stored output was cut at its limit. It is not the full output.
        </p>
      )}
      {execution.stdoutPreview && (
        <>
          <h3>Output (stored preview)</h3>
          <pre className="output">{execution.stdoutPreview}</pre>
        </>
      )}
      {execution.stderrPreview && (
        <>
          <h3>Errors (stored preview)</h3>
          <pre className="output">{execution.stderrPreview}</pre>
        </>
      )}
    </>
  );
}

/** One task: the request, its token usage, and every command it ran. */
function TaskDrawer({ task, onClose }: { task: TaskView; onClose: () => void }) {
  const [steps, setSteps] = useState<ExecutionView[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    taskTimeline(task.id)
      .then((rows) => active && setSteps(rows))
      .catch((err: unknown) => active && setError(describeError(err)));
    return () => {
      active = false;
    };
  }, [task.id]);

  const total = sumTokens(task.inputTokens, task.outputTokens);

  return (
    <Drawer title="Task" subtitle={formatWhen(task.createdAtMs)} onClose={onClose}>
      <section aria-label="Task details">
        <p className="task-request">{task.promptPreview ?? 'The request was not recorded for this task.'}</p>
        <div className="token-grid" role="group" aria-label="Token usage">
          <div>
            <span>Input</span>
            <b>{formatCount(task.inputTokens)}</b>
          </div>
          <div>
            <span>Output</span>
            <b>{formatCount(task.outputTokens)}</b>
          </div>
          <div>
            <span>Total</span>
            <b>{formatCount(total)}</b>
          </div>
          <div>
            <span>Model turns</span>
            <b>{task.modelTurns}</b>
          </div>
        </div>
        {total === null && task.modelTurns > 0 && (
          <p className="muted" style={{ marginBottom: '1rem' }}>
            The provider did not report token usage for this task.
          </p>
        )}
        <dl className="details-list">
          <dt>State</dt>
          <dd>
            <span className="badge" data-tone={stateTone(task.state)}>
              {stateText(task.state)}
            </span>
          </dd>
          <dt>Model</dt>
          <dd>{task.modelId}</dd>
          <dt>Duration</dt>
          <dd>{task.finishedAtMs ? formatDuration(task.finishedAtMs - task.createdAtMs) : '—'}</dd>
          <dt>Error</dt>
          <dd>{task.errorCode ?? '—'}</dd>
        </dl>
        <h3>Commands</h3>
        {error && (
          <p role="alert" className="banner" data-tone="danger">
            {error}
          </p>
        )}
        {steps && steps.length === 0 && <p className="muted">This task ran no commands.</p>}
        <ol className="step-list">
          {steps?.map((step) => (
            <li key={step.id} className="step">
              <div className="step-head">
                <TerminalIcon size={13} />
                <code>{step.commandRedacted}</code>
                <span className="grow" />
                <span className="badge" data-tone={stateTone(step.state)}>
                  {stateText(step.state)}
                  {step.exitCode !== null ? ` · exit ${step.exitCode}` : ''}
                </span>
              </div>
              {(step.stdoutPreview || step.stderrPreview) && (
                <details>
                  <summary>Output</summary>
                  {step.stdoutPreview && <pre className="output">{step.stdoutPreview}</pre>}
                  {step.stderrPreview && <pre className="output">{step.stderrPreview}</pre>}
                </details>
              )}
            </li>
          ))}
        </ol>
      </section>
    </Drawer>
  );
}
