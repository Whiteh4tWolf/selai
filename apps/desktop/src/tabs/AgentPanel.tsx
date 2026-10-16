import { Fragment, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import {
  agentCancel,
  agentStart,
  approvalDecide,
  auditVerify,
  describeError,
  hostKeyProblem,
  hostList,
  preferencesGet,
  preferencesSet,
  providerList,
  type HostKeyProblem,
  type HostView,
  type ProviderView,
} from '../ipc';
import { HostAvatar } from '../ui/HostAvatar';
import { HostKeyDialog } from '../ui/HostKeyDialog';
import {
  AlertIcon,
  AtIcon,
  CheckIcon,
  CloseIcon,
  PlusIcon,
  SendIcon,
  ShieldIcon,
  SparkIcon,
  StopIcon,
  TerminalIcon,
} from '../ui/icons';
import { applyEvent, isTerminal, setApprovalState, type TranscriptItem } from './agentTranscript';
import { PROVIDERS_CHANGED } from './ProvidersPanel';

/** The "@query" being typed at the end of the request. Labels may hold spaces. */
const MENTION = /(^|\s)@([^\n@]{0,64})$/;

interface Turn {
  id: number;
  hostId: string;
  hostLabel: string;
  /** The "@Label" text inside the prompt, highlighted in the bubble. */
  mentionText: string | null;
  prompt: string;
  items: TranscriptItem[];
  taskId: string | null;
  running: boolean;
  notice: string | null;
  error: string | null;
}

const SUGGESTIONS = [
  { title: 'Check disk usage', detail: 'and find the largest directories' },
  { title: 'Why is the load high?', detail: 'look at CPU, memory and top processes' },
  { title: 'Is nginx healthy?', detail: 'service status and recent errors' },
  { title: 'Summarise recent logins', detail: 'who connected in the last day' },
];

let nextTurnId = 0;

interface Props {
  onOpenProviders?: () => void;
  onOpenHosts?: () => void;
}

export function AgentPanel({ onOpenProviders, onOpenHosts }: Props = {}) {
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [providerId, setProviderId] = useState('');
  const [host, setHost] = useState<HostView | null>(null);
  const [draftMention, setDraftMention] = useState<string | null>(null);
  const [prompt, setPrompt] = useState('');
  const [mention, setMention] = useState<string | null>(null);
  const [matches, setMatches] = useState<HostView[]>([]);
  const [active, setActive] = useState(0);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [audit, setAudit] = useState<{ ok: boolean; text: string } | null>(null);
  const [keyPrompt, setKeyPrompt] = useState<{ turn: Turn; host: HostView; problem: HostKeyProblem } | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const running = turns.some((t) => t.running);
  const runningTask = turns.find((t) => t.running)?.taskId ?? null;

  useEffect(() => {
    let active = true;
    // Loads the provider list and picks one when the current choice is missing or no
    // longer verified: the remembered provider if still verified, else the first
    // verified one. A missing preference is not an error.
    const load = () =>
      Promise.all([providerList(), Promise.resolve().then(preferencesGet).catch(() => null)])
        .then(([page, prefs]) => {
          if (!active) return;
          setProviders(page.items);
          const isVerified = (id: string | null | undefined) =>
            page.items.some((p) => p.id === id && p.toolCalling.status === 'verified');
          setProviderId((current) => {
            if (isVerified(current)) return current;
            if (isVerified(prefs?.lastProviderId)) return prefs?.lastProviderId ?? '';
            return page.items.find((p) => p.toolCalling.status === 'verified')?.id ?? '';
          });
        })
        .catch((err: unknown) => active && setError(describeError(err)));
    window.addEventListener(PROVIDERS_CHANGED, load);
    void load();
    return () => {
      active = false;
      window.removeEventListener(PROVIDERS_CHANGED, load);
    };
  }, []);

  useEffect(() => {
    if (mention === null) return;
    let active = true;
    hostList(mention.trim() || undefined)
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
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [turns]);

  // A query that already holds a space and matches nothing is plain text, not a mention.
  const pickerOpen = mention !== null && !(matches.length === 0 && mention.includes(' '));

  function onPromptChange(value: string) {
    setPrompt(value);
    if (draftMention && !value.includes(draftMention)) {
      // The operator deleted the mention, so the target goes with it.
      setDraftMention(null);
      setHost(null);
    }
    const m = MENTION.exec(value);
    if (!m || (draftMention && `@${m[2] ?? ''}`.startsWith(draftMention))) {
      setMention(null);
      return;
    }
    setMention(m[2] ?? '');
  }

  function pickHost(h: HostView) {
    const text = `@${h.label}`;
    setHost(h);
    setDraftMention(text);
    setPrompt((p) => p.replace(MENTION, `$1${text} `));
    setMention(null);
    textareaRef.current?.focus();
  }

  function insertAt() {
    onPromptChange(prompt.length === 0 || /\s$/.test(prompt) ? `${prompt}@` : `${prompt} @`);
    textareaRef.current?.focus();
  }

  function clearTarget() {
    if (draftMention) setPrompt((p) => p.replace(draftMention, '').replace(/^\s+/, ''));
    setDraftMention(null);
    setHost(null);
  }

  function onPromptKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (pickerOpen) {
      if (e.key === 'Escape') {
        e.preventDefault();
        setMention(null);
        return;
      }
      if (matches.length > 0) {
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          setActive((i) => (i + 1) % matches.length);
          return;
        }
        if (e.key === 'ArrowUp') {
          e.preventDefault();
          setActive((i) => (i - 1 + matches.length) % matches.length);
          return;
        }
        if ((e.key === 'Enter' && !e.shiftKey) || e.key === 'Tab') {
          e.preventDefault();
          const target = matches[active] ?? matches[0];
          if (target) pickHost(target);
          return;
        }
      }
    }
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void send();
    }
  }

  function chooseProvider(id: string) {
    setProviderId(id);
    // Remember the choice for the next start. A failed save is not shown; the
    // selection still applies to this session.
    preferencesSet({ lastProviderId: id }).catch(() => undefined);
  }

  const provider = providers.find((p) => p.id === providerId);
  const verified = provider?.toolCalling.status === 'verified';
  const canSend = !running && host !== null && verified && prompt.trim().length > 0;

  const updateTurn = (id: number, change: (t: Turn) => Turn) =>
    setTurns((prev) => prev.map((t) => (t.id === id ? change(t) : t)));

  async function startTurn(turn: Turn, target: HostView) {
    updateTurn(turn.id, (t) => ({ ...t, running: true, notice: null, error: null, items: [] }));
    try {
      const id = await agentStart(target.id, providerId, turn.prompt, (event) => {
        updateTurn(turn.id, (t) => ({
          ...t,
          items: applyEvent(t.items, event),
          running: t.running && !isTerminal(event),
        }));
      });
      updateTurn(turn.id, (t) => ({ ...t, taskId: id }));
    } catch (err) {
      const problem = hostKeyProblem(err);
      if (problem) {
        updateTurn(turn.id, (t) => ({ ...t, running: false, notice: 'Waiting for you to approve the server key…' }));
        setKeyPrompt({ turn, host: target, problem });
        return;
      }
      updateTurn(turn.id, (t) => ({ ...t, running: false, error: describeError(err) }));
    }
  }

  async function send() {
    if (!host || !canSend) return;
    nextTurnId += 1;
    const turn: Turn = {
      id: nextTurnId,
      hostId: host.id,
      hostLabel: host.label,
      mentionText: draftMention,
      prompt: prompt.trim(),
      items: [],
      taskId: null,
      running: true,
      notice: null,
      error: null,
    };
    setTurns((prev) => [...prev, turn]);
    setPrompt('');
    setDraftMention(null);
    setMention(null);
    setError(null);
    await startTurn(turn, host);
  }

  async function stop() {
    if (!runningTask) return;
    try {
      await agentCancel(runningTask);
    } catch (err) {
      setError(describeError(err));
    }
  }

  async function decide(approvalId: string, approve: boolean) {
    const update = (state: 'sending' | 'approved' | 'denied' | 'pending', why: string | null = null) =>
      setTurns((prev) => prev.map((t) => ({ ...t, items: setApprovalState(t.items, approvalId, state, why) })));
    update('sending');
    try {
      await approvalDecide(approvalId, approve);
      update(approve ? 'approved' : 'denied');
    } catch (err) {
      // The database refused it (expired or already decided). Show why.
      update('pending', describeError(err));
    }
  }

  async function verifyAudit() {
    try {
      const result = await auditVerify();
      setAudit(
        result.ok
          ? { ok: true, text: `Audit log intact: ${result.eventsChecked} event(s) checked.` }
          : { ok: false, text: `Audit log broken at sequence ${result.brokenAtSeq ?? '?'}.` },
      );
    } catch (err) {
      setAudit({ ok: false, text: describeError(err) });
    }
  }

  return (
    <section aria-label="AI Agent" className="chat">
      <header className="chat-head">
        <SparkIcon size={18} style={{ color: 'var(--accent)' }} />
        <h1>AI Agent</h1>
        <span className="grow" />
        <select
          aria-label="Provider"
          value={providerId}
          onChange={(e) => chooseProvider(e.target.value)}
          disabled={running}
        >
          {providers.length === 0 && <option value="">No providers configured</option>}
          {providers.length > 0 && providerId === '' && (
            <option value="" disabled>
              Choose a provider
            </option>
          )}
          {providers.map((p) => (
            <option key={p.id} value={p.id} disabled={p.toolCalling.status !== 'verified'}>
              {p.name} · {p.modelId}
              {p.toolCalling.status === 'verified' ? '' : ' (not verified)'}
            </option>
          ))}
        </select>
        <button type="button" className="btn btn-ghost btn-sm" onClick={verifyAudit}>
          <ShieldIcon size={14} /> Verify audit log
        </button>
        <button
          type="button"
          className="btn btn-sm"
          onClick={() => setTurns([])}
          disabled={running || turns.length === 0}
        >
          <PlusIcon size={14} /> New chat
        </button>
      </header>

      <div className="chat-scroll" ref={scrollRef}>
        {audit && (
          <div className="thread" style={{ paddingBottom: 0 }}>
            <p role="status" className="banner" data-tone={audit.ok ? 'success' : 'danger'}>
              {audit.ok ? <CheckIcon /> : <AlertIcon />}
              <span className="grow">{audit.text}</span>
            </p>
          </div>
        )}
        {turns.length === 0 ? (
          <div className="chat-empty">
            <span className="empty-icon">
              <SparkIcon size={26} />
            </span>
            <h2>What should we check?</h2>
            <p>
              Type <kbd>@</kbd> to mention a host, then ask in plain language. Read-only diagnostics run
              on their own; any other command waits for your approval.
            </p>
            <div className="suggestions">
              {SUGGESTIONS.map((s) => (
                <button
                  key={s.title}
                  type="button"
                  className="suggestion"
                  onClick={() => {
                    const body = `${s.title} ${s.detail}`;
                    const next = draftMention ? `${draftMention} ${body}` : body;
                    setPrompt(next);
                    textareaRef.current?.focus();
                  }}
                >
                  {s.title}
                  <small>{s.detail}</small>
                </button>
              ))}
            </div>
          </div>
        ) : (
          <ol className="thread" aria-label="Transcript">
            {turns.map((turn) => (
              <Fragment key={turn.id}>
                <li className="turn-user">
                  <div className="bubble">{highlight(turn.prompt, turn.mentionText)}</div>
                </li>
                <li className="turn-ai">
                  <span className="ai-avatar">
                    <SparkIcon size={15} />
                  </span>
                  <div className="ai-body">
                    <div className="ai-meta">
                      <span>Selai</span>
                      <span>·</span>
                      <span>on {turn.hostLabel}</span>
                      {turn.running && <span className="spinner" aria-label="Working" />}
                    </div>
                    {turn.items.map((item, i) => (
                      <TranscriptRow key={i} item={item} items={turn.items} onDecide={decide} />
                    ))}
                    {turn.running && turn.items.length === 0 && <p className="status-line">Thinking…</p>}
                    {turn.notice && <p className="status-line">{turn.notice}</p>}
                    {turn.error && (
                      <p role="alert" className="banner" data-tone="danger">
                        <AlertIcon />
                        <span className="grow">{turn.error}</span>
                      </p>
                    )}
                  </div>
                </li>
              </Fragment>
            ))}
          </ol>
        )}
      </div>

      <div className="composer-wrap">
        {error && (
          <p role="alert" className="banner" data-tone="danger" style={{ maxWidth: 820, margin: '0 auto 0.6rem' }}>
            {error}
          </p>
        )}
        {!verified && (
          <div className="banner" data-tone="warning" style={{ maxWidth: 820, margin: '0 auto 0.6rem' }}>
            <AlertIcon />
            <span className="grow">
              {providers.length === 0
                ? 'Add an AI provider first.'
                : 'No verified provider selected. Run Test on the Providers page to check connection and tool calling.'}
            </span>
            {onOpenProviders && (
              <button type="button" className="btn btn-sm" onClick={onOpenProviders}>
                Open Providers
              </button>
            )}
          </div>
        )}
        <div className="composer">
          {pickerOpen && (
            <ul className="mention-pop" aria-label="Matching hosts">
              <li className="head">Hosts</li>
              {matches.length === 0 && (
                <li className="none">
                  No matching host.{' '}
                  {onOpenHosts && (
                    <button type="button" className="btn btn-ghost btn-sm" onClick={onOpenHosts}>
                      Add a host
                    </button>
                  )}
                </li>
              )}
              {matches.map((h, i) => (
                <li key={h.id} className={i === active ? 'active' : undefined}>
                  <button
                    type="button"
                    tabIndex={-1}
                    aria-current={i === active ? 'true' : undefined}
                    onMouseDown={(e) => e.preventDefault()}
                    onClick={() => pickHost(h)}
                  >
                    <HostAvatar label={h.label} size={14} />
                    <span>
                      {h.label}
                      <small>
                        {h.username}@{h.hostname}
                      </small>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <textarea
            ref={textareaRef}
            aria-label="Request"
            value={prompt}
            onChange={(e) => onPromptChange(e.target.value)}
            onKeyDown={onPromptKeyDown}
            rows={Math.min(8, Math.max(2, prompt.split('\n').length))}
            placeholder={host ? `Ask about ${host.label}…` : 'Type @ to choose a host, then describe what you need'}
          />
          <div className="composer-bar">
            {host ? (
              <span className="target-chip" title={`${host.username}@${host.hostname}`}>
                <TerminalIcon size={13} />
                {host.label}
                <button type="button" aria-label={`Remove ${host.label}`} onClick={clearTarget}>
                  <CloseIcon size={11} />
                </button>
              </span>
            ) : (
              <button type="button" className="btn btn-ghost btn-sm" onClick={insertAt}>
                <AtIcon size={14} /> Mention host
              </button>
            )}
            <span className="grow" />
            {running ? (
              <button type="button" className="btn btn-danger send-btn" aria-label="Stop" title="Stop" onClick={stop} disabled={!runningTask}>
                <StopIcon size={14} />
              </button>
            ) : (
              <button
                type="button"
                className="btn btn-primary send-btn"
                aria-label="Send"
                title="Send (Enter)"
                onClick={() => void send()}
                disabled={!canSend}
              >
                <SendIcon />
              </button>
            )}
          </div>
        </div>
        <p className="composer-note">
          Enter to send · Shift+Enter for a new line · Each message runs as its own task on the mentioned host.
        </p>
      </div>

      {keyPrompt && (
        <HostKeyDialog
          hostId={keyPrompt.host.id}
          hostLabel={keyPrompt.host.label}
          address={`${keyPrompt.host.hostname}:${keyPrompt.host.port}`}
          problem={keyPrompt.problem}
          onTrusted={() => {
            const { turn, host: target } = keyPrompt;
            setKeyPrompt(null);
            void startTurn(turn, target);
          }}
          onCancel={() => {
            updateTurn(keyPrompt.turn.id, (t) => ({
              ...t,
              notice: null,
              error: 'Not started: the server key was not approved, so nothing was sent to the host.',
            }));
            setKeyPrompt(null);
          }}
        />
      )}
    </section>
  );
}

/** Shows the bound "@Label" as a pill inside the sent message. */
function highlight(text: string, mentionText: string | null): ReactNode {
  if (!mentionText) return text;
  const at = text.indexOf(mentionText);
  if (at < 0) return text;
  return (
    <>
      {text.slice(0, at)}
      <span className="mention">{mentionText}</span>
      {text.slice(at + mentionText.length)}
    </>
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
      return <p className="status-line">{item.text}</p>;
    case 'failed':
      return (
        <p role="alert" className="banner" data-tone="danger">
          <AlertIcon />
          <span className="grow">
            {item.message} <small>({item.code})</small>
          </span>
        </p>
      );
    case 'tool': {
      // A command that needs approval is one card: the command, then the decision.
      const approval = items.find(
        (i): i is Extract<TranscriptItem, { kind: 'approval' }> =>
          i.kind === 'approval' && i.executionId === item.executionId,
      );
      if (approval) return <ApprovalCard item={approval} hostLabel={item.hostLabel} status={item} onDecide={onDecide} />;
      return <ToolCard item={item} />;
    }
    case 'approval': {
      // Shown inside its command card when that card exists.
      if (items.some((i) => i.kind === 'tool' && i.executionId === item.executionId)) return null;
      return <ApprovalCard item={item} hostLabel={null} status={null} onDecide={onDecide} />;
    }
  }
}

type ToolItem = Extract<TranscriptItem, { kind: 'tool' }>;
type ApprovalItem = Extract<TranscriptItem, { kind: 'approval' }>;

function StatusBadge({ item }: { item: ToolItem }) {
  const done = item.status === 'completed';
  const bad = ['failed', 'denied', 'timed_out', 'cancelled', 'interrupted'].includes(item.status);
  return (
    <span className="badge" data-tone={done ? 'success' : bad ? 'danger' : 'accent'}>
      {item.status.replace(/_/g, ' ')}
      {item.exitCode !== null ? ` · exit ${item.exitCode}` : ''}
    </span>
  );
}

function ToolCard({ item }: { item: ToolItem }) {
  return (
    <div className="tool-card" aria-label="Command">
      <div className="tool-head">
        <TerminalIcon size={13} />
        <span>{item.hostLabel}</span>
        <span className="grow" />
        <StatusBadge item={item} />
      </div>
      <pre className="tool-cmd">
        <code>{item.command}</code>
      </pre>
    </div>
  );
}

function ApprovalCard({
  item,
  hostLabel,
  status,
  onDecide,
}: {
  item: ApprovalItem;
  hostLabel: string | null;
  status: ToolItem | null;
  onDecide: (approvalId: string, approve: boolean) => void;
}) {
  const expired = Date.now() > item.expiresAtMs;
  const open = item.state === 'pending' && !expired;
  return (
    <div
      className={open || item.state === 'sending' ? 'tool-card approval' : 'tool-card'}
      role="group"
      aria-label="Approval required"
    >
      <div className="tool-head">
        <ShieldIcon size={13} />
        <span>Run this command on {hostLabel ?? 'the host'}?</span>
        <span className="grow" />
        {status && item.state === 'approved' && <StatusBadge item={status} />}
      </div>
      <pre className="tool-cmd">
        <code>{item.command}</code>
      </pre>
      <div className="approval-foot">
        <span className="grow">
          {item.state === 'sending' && 'Sending your decision…'}
          {item.state === 'approved' &&
            (status && status.status !== 'proposed' ? 'Approved by you.' : 'Approved. Waiting for the result.')}
          {item.state === 'denied' && 'Denied. The command will not run.'}
          {item.state === 'pending' &&
            (expired ? 'Expired. The command will not run.' : `Expires at ${new Date(item.expiresAtMs).toLocaleTimeString()}`)}
        </span>
        {item.state === 'pending' && (
          <>
            <button type="button" className="btn btn-sm" disabled={!open} onClick={() => onDecide(item.approvalId, false)}>
              Deny
            </button>
            <button type="button" className="btn btn-primary btn-sm" disabled={!open} onClick={() => onDecide(item.approvalId, true)}>
              Approve
            </button>
          </>
        )}
      </div>
      {item.error && (
        <p role="alert" className="banner" data-tone="danger" style={{ margin: '0 0.75rem 0.75rem' }}>
          {item.error}
        </p>
      )}
    </div>
  );
}
