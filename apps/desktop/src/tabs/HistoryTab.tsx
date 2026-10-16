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

export function HistoryTab() {
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

  // Load the first page whenever the host changes.
  useEffect(() => {
    if (hostId) void search(false);
    // The filter is applied by the Apply button, not on every change.
  }, [hostId]);

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
    <section aria-label="AI history">
      <h2>AI history</h2>
      <p className="muted">
        Commands the AI ran on a host, newest first. Archived hosts keep their history.
      </p>

      <div className="agent-composer">
        <label>
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

        <fieldset className="filters">
          <legend>State</legend>
          {STATES.map((state) => (
            <label key={state}>
              <input type="checkbox" checked={states.includes(state)} onChange={() => toggleState(state)} />
              {state.replace('_', ' ')}
            </label>
          ))}
        </fieldset>

        <div className="button-row">
          <label>
            From <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} />
          </label>
          <label>
            To <input type="date" value={to} onChange={(e) => setTo(e.target.value)} />
          </label>
          <button type="button" onClick={() => search(false)} disabled={!hostId}>
            Apply
          </button>
          <button type="button" onClick={exportHistory} disabled={!hostId}>
            Export JSON
          </button>
          <button type="button" onClick={purge} disabled={!hostId}>
            Clear stored output
          </button>
        </div>

        <div className="button-row">
          <label>
            Keep output for (days)
            <input
              type="number"
              min={1}
              max={3650}
              value={retention}
              onChange={(e) => setRetention(Number(e.target.value))}
            />
          </label>
          <button type="button" onClick={saveRetention}>
            Save retention
          </button>
        </div>
      </div>

      {notice && <p role="status">{notice}</p>}
      {error && <p role="alert">{error}</p>}

      {rows.length === 0 ? (
        <p className="muted">No commands match.</p>
      ) : (
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
                <td>{new Date(row.createdAtMs).toLocaleString()}</td>
                <td>
                  <code>{row.commandRedacted}</code>
                </td>
                <td>{row.state.replace('_', ' ')}</td>
                <td>{row.exitCode ?? '—'}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {nextCursor && (
        <button type="button" onClick={() => search(true)}>
          Load more
        </button>
      )}

      {selected && (
        <section aria-label="Execution details" className="card">
          <h3>Execution</h3>
          <dl>
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
            <p role="note">The stored output was cut at its limit. It is not the full output.</p>
          )}
          {selected.stdoutPreview && (
            <>
              <h4>Output (stored preview)</h4>
              <pre>{selected.stdoutPreview}</pre>
            </>
          )}
          {selected.stderrPreview && (
            <>
              <h4>Errors (stored preview)</h4>
              <pre>{selected.stderrPreview}</pre>
            </>
          )}
          {timeline && (
            <>
              <h4>Task timeline</h4>
              <ol>
                {timeline.map((step) => (
                  <li key={step.id}>
                    <code>{step.commandRedacted}</code> — {step.state.replace('_', ' ')}
                    {step.exitCode !== null ? `, exit ${step.exitCode}` : ''}
                  </li>
                ))}
              </ol>
            </>
          )}
        </section>
      )}
    </section>
  );
}
