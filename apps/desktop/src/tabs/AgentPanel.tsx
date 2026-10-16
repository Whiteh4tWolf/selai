import { useEffect, useState, type KeyboardEvent } from 'react';
import {
  agentCancel,
  agentStart,
  approvalDecide,
  auditVerify,
  describeError,
  historyList,
  hostList,
  preferencesGet,
  preferencesSet,
  providerList,
  type ExecutionView,
  type HostView,
  type ProviderView,
} from '../ipc';
import { applyEvent, isTerminal, setApprovalState, type TranscriptItem } from './agentTranscript';

interface HostChip {
  hostId: string;
  label: string;
}

/** The "@query" being typed at the end of the request, if any. */
const MENTION = /(^|\s)@([^\s@]*)$/;

export function AgentPanel() {
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [providerId, setProviderId] = useState('');
  const [host, setHost] = useState<HostChip | null>(null);
  const [prompt, setPrompt] = useState('');
  const [mention, setMention] = useState<string | null>(null);
  const [matches, setMatches] = useState<HostView[]>([]);
  const [active, setActive] = useState(0);
  const [running, setRunning] = useState(false);
  const [taskId, setTaskId] = useState<string | null>(null);
  const [items, setItems] = useState<TranscriptItem[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [history, setHistory] = useState<ExecutionView[]>([]);
  const [audit, setAudit] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    // The remembered provider wins when it is still verified; otherwise the first
    // verified one. A missing preference is not an error.
    Promise.all([providerList(), Promise.resolve().then(preferencesGet).catch(() => null)])
      .then(([page, prefs]) => {
        if (!active) return;
        setProviders(page.items);
        const usable = (id: string | null | undefined) =>
          page.items.find((p) => p.id === id && p.toolCalling.status === 'verified')?.id;
        const firstVerified = page.items.find((p) => p.toolCalling.status === 'verified')?.id;
        setProviderId((current) => current || usable(prefs?.lastProviderId) || firstVerified || '');
      })
      .catch((err: unknown) => active && setError(describeError(err)));
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (mention === null) return;
    let active = true;
    hostList(mention)
      .then((page) => {
        if (!active) return;
        setMatches(page.items.filter((h) => !h.archived));
        setActive(0);
      })
      .catch(() => active && setMatches([]));
    return () => {
      active = false;
    };
  }, [mention]);

  useEffect(() => {
    if (!host) {
      setHistory([]);
      return;
    }
    // Reload only when the chosen host changes, not on every render.
    refreshHistory(host.hostId);
  }, [host?.hostId]);

  function refreshHistory(hostId: string) {
    historyList(hostId)
      .then((page) => setHistory(page.items))
      .catch((err: unknown) => setError(describeError(err)));
  }

  function onPromptChange(value: string) {
    setPrompt(value);
    const m = MENTION.exec(value);
    setMention(m ? (m[2] ?? '') : null);
  }

  function onPromptKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (mention === null) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      setMention(null);
      return;
    }
    if (matches.length === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      setActive((i) => (i + 1) % matches.length);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActive((i) => (i - 1 + matches.length) % matches.length);
    } else if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      const target = matches[active] ?? matches[0];
      if (target) pickHost(target);
    }
  }

  function pickHost(h: HostView) {
    setHost({ hostId: h.id, label: h.label });
    setPrompt((p) => p.replace(MENTION, '$1'));
    setMention(null);
  }

  function chooseProvider(id: string) {
    setProviderId(id);
    // Remember the choice for the next start. A failed save is not shown; the
    // selection still applies to this session.
    preferencesSet({ lastProviderId: id }).catch(() => undefined);
  }

  const verified = providers.find((p) => p.id === providerId)?.toolCalling.status === 'verified';
  const canSend = !running && host !== null && verified && prompt.trim().length > 0;

  async function send() {
    if (!host || !canSend) return;
    const hostId = host.hostId;
    setItems([]);
    setError(null);
    setAudit(null);
    setRunning(true);
    setTaskId(null);
    try {
      const id = await agentStart(hostId, providerId, prompt.trim(), (event) => {
        setItems((prev) => applyEvent(prev, event));
        if (isTerminal(event)) {
          setRunning(false);
          refreshHistory(hostId);
        }
      });
      setTaskId(id);
      setPrompt('');
    } catch (err) {
      setRunning(false);
      setError(describeError(err));
    }
  }

  async function stop() {
    if (!taskId) return;
    try {
      await agentCancel(taskId);
    } catch (err) {
      setError(describeError(err));
    }
  }

  async function decide(approvalId: string, approve: boolean) {
    setItems((prev) => setApprovalState(prev, approvalId, 'sending'));
    try {
      await approvalDecide(approvalId, approve);
      setItems((prev) => setApprovalState(prev, approvalId, approve ? 'approved' : 'denied'));
    } catch (err) {
      // The database refused it (expired or already decided). Show why.
      setItems((prev) => setApprovalState(prev, approvalId, 'pending', describeError(err)));
    }
  }

  async function verifyAudit() {
    try {
      const result = await auditVerify();
      setAudit(
        result.ok
          ? `Audit log intact: ${result.eventsChecked} event(s) checked.`
          : `Audit log broken at sequence ${result.brokenAtSeq ?? '?'}.`,
      );
    } catch (err) {
      setAudit(describeError(err));
    }
  }

  return (
    <section aria-label="AI">
      <h2>AI</h2>
      <p className="muted">
        Type <kbd>@</kbd> and pick a host. The model can only act on that host. Free-form commands
        wait for your approval.
      </p>

      <div className="agent-composer">
        <div className="chip-row">
          {host ? (
            <span className="chip">
              @{host.label}
              <button type="button" aria-label={`Remove ${host.label}`} onClick={() => setHost(null)}>
                ×
              </button>
            </span>
          ) : (
            <span className="muted">No host selected</span>
          )}
        </div>

        {mention !== null && (
          <ul className="host-picker" aria-label="Matching hosts">
            {matches.length === 0 && <li className="muted">No matching host</li>}
            {matches.map((h, i) => (
              <li key={h.id} className={i === active ? 'active' : undefined}>
                <button
                  type="button"
                  tabIndex={-1}
                  aria-current={i === active ? 'true' : undefined}
                  onClick={() => pickHost(h)}
                >
                  {h.label} <small>{h.hostname}</small>
                </button>
              </li>
            ))}
          </ul>
        )}

        <label>
          Request
          <textarea
            value={prompt}
            onChange={(e) => onPromptChange(e.target.value)}
            onKeyDown={onPromptKeyDown}
            rows={3}
            placeholder="@production-web check disk usage"
            disabled={running}
          />
        </label>

        <label>
          Provider
          <select value={providerId} onChange={(e) => chooseProvider(e.target.value)} disabled={running}>
            {providers.length === 0 && <option value="">No providers configured</option>}
            {providers.map((p) => (
              <option key={p.id} value={p.id} disabled={p.toolCalling.status !== 'verified'}>
                {p.name} ({p.toolCalling.status === 'verified' ? 'tool calling verified' : 'not verified'})
              </option>
            ))}
          </select>
        </label>
        {providers.length > 0 && !verified && (
          <p className="muted">Run the tool calling test for this provider in the list below first.</p>
        )}

        <div className="button-row">
          <button type="button" onClick={send} disabled={!canSend}>
            Send
          </button>
          {running && (
            <button type="button" onClick={stop} disabled={!taskId}>
              Stop
            </button>
          )}
        </div>
      </div>

      {error && <p role="alert">{error}</p>}

      <ol className="transcript" aria-label="Transcript">
        {items.map((item, i) => (
          <li key={i}>
            <TranscriptRow item={item} items={items} onDecide={decide} />
          </li>
        ))}
      </ol>

      {host && (
        <section aria-label="Command history" className="history">
          <h3>Command history: {host.label}</h3>
          {history.length === 0 && <p className="muted">No commands on this host yet.</p>}
          <ul>
            {history.map((row) => (
              <li key={row.id}>
                <code>{row.commandRedacted}</code>{' '}
                <span className="muted">
                  {row.tool.includes('diagnostic') ? 'diagnostic' : 'command'} · {row.state}
                  {row.exitCode !== null ? ` · exit ${row.exitCode}` : ''}
                </span>
              </li>
            ))}
          </ul>
        </section>
      )}

      <div className="button-row">
        <button type="button" onClick={verifyAudit}>
          Verify audit log
        </button>
        {audit && <span role="status">{audit}</span>}
      </div>
    </section>
  );
}

function TranscriptRow({
  item,
  items,
  onDecide,
}: {
  item: TranscriptItem;
  items: TranscriptItem[];
  onDecide: (approvalId: string, approve: boolean) => void;
}) {
  switch (item.kind) {
    case 'text':
      return <p className="reply">{item.text}</p>;
    case 'status':
      return <p className="muted">{item.text}</p>;
    case 'failed':
      return (
        <p role="alert">
          {item.message} <small>({item.code})</small>
        </p>
      );
    case 'tool':
      return (
        <div className="card" aria-label="Command">
          <code>{item.command}</code>
          <div className="muted">
            on {item.hostLabel} · {item.status}
            {item.exitCode !== null ? ` · exit ${item.exitCode}` : ''}
          </div>
        </div>
      );
    case 'approval': {
      const hostLabel = items.find(
        (i) => i.kind === 'tool' && i.executionId === item.executionId,
      );
      const expired = Date.now() > item.expiresAtMs;
      const open = item.state === 'pending' && !expired;
      return (
        <div className="card approval" role="group" aria-label="Approval required">
          <p>
            Run this command on {hostLabel?.kind === 'tool' ? hostLabel.hostLabel : 'the host'}?
          </p>
          <pre>
            <code>{item.command}</code>
          </pre>
          <p className="muted">
            {expired
              ? 'Expired. The command will not run.'
              : `Expires at ${new Date(item.expiresAtMs).toLocaleTimeString()}`}
          </p>
          {item.state === 'sending' && <p className="muted">Sending your decision…</p>}
          {item.state === 'approved' && <p>Approved. Waiting for the result.</p>}
          {item.state === 'denied' && <p>Denied. The command will not run.</p>}
          {item.error && <p role="alert">{item.error}</p>}
          <div className="button-row">
            <button type="button" disabled={!open} onClick={() => onDecide(item.approvalId, true)}>
              Approve
            </button>
            <button type="button" disabled={!open} onClick={() => onDecide(item.approvalId, false)}>
              Deny
            </button>
          </div>
        </div>
      );
    }
  }
}
