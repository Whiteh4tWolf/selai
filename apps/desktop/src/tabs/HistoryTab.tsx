import { useEffect, useState } from 'react';
import {
  describeError,
  historyExport,
  historyPurgeOutput,
  historyQuery,
  hostList,
  preferencesGet,
  preferencesSet,
  taskTimeline,
  type ExecutionView,
  type ExportResult,
  type HistoryFilter,
  type HostView,
} from '../ipc';
import { Drawer } from '../ui/Overlay';
import { ClockIcon } from '../ui/icons';

const STATES = ['completed', 'failed', 'denied', 'cancelled', 'interrupted', 'timed_out'] as const;

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

/** `visible` comes from the app shell. The panel stays mounted while hidden, so it
 *  reloads each time it is shown and picks up runs that finished meanwhile. */
export function HistoryTab({ visible }: { visible: boolean }) {
  const [hosts, setHosts] = useState<HostView[]>([]);
  const [hostId, setHostId] = useState('');
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

  useEffect(() => {
    let active = true;
    hostList(undefined, true)
      .then((page) => {
        if (!active) return;
        setHosts(page.items);
        setHostId((current) => current || page.items[0]?.id || '');
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
    setNotice(null);
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

  // Load the first page when the tab is shown or the host changes. The filter is
  // applied by the Apply button, not on every change.
  useEffect(() => {
    if (visible && hostId) void search(false);
  }, [visible, hostId]);

  function toggleState(state: string) {
    setStates((prev) => (prev.includes(state) ? prev.filter((s) => s !== state) : [...prev, state]));
  }

  async function openTimeline(row: ExecutionView) {
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

  return (
    <section aria-label="AI history" className="page">
      <header className="page-head">
        <div className="grow">
          <h1>History</h1>
          <p>Commands the AI ran on a host, newest first. Archived hosts keep their history.</p>
        </div>
        <button type="button" className="btn" onClick={exportHistory} disabled={!hostId}>
          Export JSON
        </button>
        <button type="button" className="btn btn-ghost" onClick={purge} disabled={!hostId}>
          Clear stored output
        </button>
      </header>

      <div className="card filters-bar">
        <label className="field">
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
              {state.replace('_', ' ')}
            </label>
          ))}
        </fieldset>
        <span className="grow" />
        <button type="button" className="btn btn-primary" onClick={() => search(false)} disabled={!hostId}>
          Apply
        </button>
      </div>

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

      {rows.length === 0 ? (
        <div className="empty">
          <span className="empty-icon">
            <ClockIcon size={26} />
          </span>
          <h2>No commands match</h2>
          <p>Commands the AI Agent runs on this host appear here with their output and exit code.</p>
        </div>
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
                <tr key={row.id} onClick={() => openTimeline(row)} aria-selected={selected?.id === row.id}>
                  <td className="muted" style={{ whiteSpace: 'nowrap' }}>
                    {new Date(row.createdAtMs).toLocaleString()}
                  </td>
                  <td>
                    <code>{row.commandRedacted}</code>
                  </td>
                  <td>
                    <span className="badge" data-tone={stateTone(row.state)}>
                      {row.state.replace('_', ' ')}
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

      <div className="card" style={{ marginTop: '1rem', padding: '0.9rem 1rem' }}>
        <div style={{ display: 'flex', alignItems: 'flex-end', gap: '0.75rem', flexWrap: 'wrap' }}>
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
          <span className="muted">Older output is removed automatically. Command records are always kept.</span>
        </div>
      </div>

      {selected && (
        <Drawer
          title="Execution"
          subtitle={new Date(selected.createdAtMs).toLocaleString()}
          onClose={() => {
            setSelected(null);
            setTimeline(null);
          }}
        >
          <section aria-label="Execution details">
            <pre className="output" style={{ marginTop: 0 }}>
              <code>{selected.commandRedacted}</code>
            </pre>
            <dl className="details-list">
              <dt>State</dt>
              <dd>
                <span className="badge" data-tone={stateTone(selected.state)}>
                  {selected.state.replace('_', ' ')}
                </span>
              </dd>
              <dt>Exit code</dt>
              <dd>{selected.exitCode ?? '—'}</dd>
              <dt>Tool</dt>
              <dd>{selected.tool}</dd>
              <dt>Model</dt>
              <dd>{selected.modelId}</dd>
              <dt>Policy</dt>
              <dd>{selected.policyDecision ?? '—'}</dd>
              <dt>Command digest (SHA-256)</dt>
              <dd>
                <code>{selected.commandSha256}</code>
              </dd>
              <dt>Error</dt>
              <dd>{selected.errorCode ?? '—'}</dd>
            </dl>
            {selected.outputTruncated && (
              <p role="note" className="banner" data-tone="warning">
                The stored output was cut at its limit. It is not the full output.
              </p>
            )}
            {selected.stdoutPreview && (
              <>
                <h3>Output (stored preview)</h3>
                <pre className="output">{selected.stdoutPreview}</pre>
              </>
            )}
            {selected.stderrPreview && (
              <>
                <h3>Errors (stored preview)</h3>
                <pre className="output">{selected.stderrPreview}</pre>
              </>
            )}
            {timeline && (
              <>
                <h3>Task timeline</h3>
                <ol className="timeline">
                  {timeline.map((step) => (
                    <li key={step.id}>
                      <span>
                        <code>{step.commandRedacted}</code> — {step.state.replace('_', ' ')}
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

function stateTone(state: string): string | undefined {
  if (state === 'completed') return 'success';
  if (['failed', 'denied', 'timed_out', 'interrupted'].includes(state)) return 'danger';
  if (state === 'cancelled') return 'warning';
  return undefined;
}
