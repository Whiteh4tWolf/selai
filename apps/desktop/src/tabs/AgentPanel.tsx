import { Fragment, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import {
  agentCancel,
  agentStart,
  approvalDecide,
  auditVerify,
  chatSend,
  describeError,
  hostKeyProblem,
  sudoOptions,
  hostList,
  localHost,
  preferencesGet,
  preferencesSet,
  providerList,
  type HostKeyProblem,
  type SudoAnswer,
  type SudoOptions,
  type HostView,
  type ProviderView,
} from '../ipc';
import { formatCompact, formatCount } from '../ui/format';
import { HostAvatar } from '../ui/HostAvatar';
import { HostKeyDialog } from '../ui/HostKeyDialog';
import { Markdown } from '../ui/Markdown';
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

/** "This computer" when it matches `query`, then the matching SSH hosts. */
async function targetSearch(query: string): Promise<HostView[]> {
  const q = query.trim().toLowerCase();
  const [local, page] = await Promise.all([
    localHost().catch(() => null),
    hostList(query.trim() || undefined),
  ]);
  const localMatches =
    local !== null && (!q || local.label.toLowerCase().includes(q) || 'local'.startsWith(q));
  return [...(localMatches ? [local] : []), ...page.items.filter((h) => !h.archived)];
}

/** The "@query" being typed at the end of the request. Labels may hold spaces. */
const MENTION = /(^|\s)@([^\n@]{0,64})$/;

interface Turn {
  id: number;
  /** Null for plain discussion: no host, no tools, nothing runs anywhere. */
  hostId: string | null;
  hostLabel: string | null;
  /** The "@Label" text inside the prompt, highlighted in the bubble. */
  mentionText: string | null;
  prompt: string;
  items: TranscriptItem[];
  taskId: string | null;
  running: boolean;
  notice: string | null;
  error: string | null;
  inputTokens: number | null;
  outputTokens: number | null;
}

const SUGGESTIONS = [
  { title: 'Check disk usage', detail: 'needs a pinned server', text: 'check disk usage and the largest directories' },
  { title: 'Is nginx healthy?', detail: 'needs a pinned server', text: 'is nginx healthy? check the service and recent errors' },
  { title: 'Explain a config', detail: 'plain discussion, nothing runs', text: 'Explain what this nginx config does:\n' },
  { title: 'Plan a change', detail: 'plain discussion, nothing runs', text: 'Help me plan a zero-downtime PostgreSQL major upgrade.' },
];

let nextTurnId = 0;

interface Props {
  onOpenProviders?: () => void;
  onOpenHosts?: () => void;
}

export function AgentPanel({ onOpenProviders, onOpenHosts }: Props = {}) {
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [providerId, setProviderId] = useState('');
  /** A one-message target from an `@` mention. */
  const [host, setHost] = useState<HostView | null>(null);
  /** The server this chat is pinned to; messages without a mention run there. */
  const [pinnedId, setPinnedId] = useState<string | null>(null);
  const [targets, setTargets] = useState<HostView[]>([]);
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
          const exists = (id: string | null | undefined) => page.items.some((p) => p.id === id);
          // Prefer a provider verified for tool calling (needed for host tasks);
          // any provider can still be used for plain discussion.
          setProviderId((current) => {
            if (isVerified(current)) return current;
            if (isVerified(prefs?.lastProviderId)) return prefs?.lastProviderId ?? '';
            const firstVerified = page.items.find((p) => p.toolCalling.status === 'verified')?.id;
            if (firstVerified) return firstVerified;
            if (exists(current)) return current;
            return page.items[0]?.id ?? '';
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
    let active = true;
    targetSearch('')
      .then((list) => active && setTargets(list))
      .catch(() => undefined);
    preferencesGet()
      .then((prefs) => active && setPinnedId(prefs.pinnedHostId || null))
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (mention === null) return;
    let active = true;
    targetSearch(mention)
      .then((targets) => {
        if (!active) return;
        setMatches(targets);
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
  // The server for this message: a mention, else the pinned server, else none
  // (plain discussion, which any provider can answer). A server needs tool calling.
  const pinnedTarget = targets.find((t) => t.id === pinnedId) ?? null;
  const target = host ?? pinnedTarget;
  const canSend = !running && provider !== undefined && (target === null || verified) && prompt.trim().length > 0;

  function pinServer(id: string) {
    setPinnedId(id || null);
    // Remembered across restarts. A failed save keeps the choice for this session.
    preferencesSet({ pinnedHostId: id }).catch(() => undefined);
  }

  const updateTurn = (id: number, change: (t: Turn) => Turn) =>
    setTurns((prev) => prev.map((t) => (t.id === id ? change(t) : t)));

  async function startTurn(turn: Turn, target: HostView) {
    updateTurn(turn.id, (t) => ({ ...t, running: true, notice: null, error: null, items: [], inputTokens: null, outputTokens: null }));
    try {
      // The new message has no reply yet, so it is not part of its own history.
      const id = await agentStart(target.id, providerId, turn.prompt, conversation(), (event) => {
        updateTurn(turn.id, (t) => ({
          ...t,
          items: applyEvent(t.items, event),
          running: t.running && !isTerminal(event),
          ...(event.type === 'usage' ? { inputTokens: event.inputTokens, outputTokens: event.outputTokens } : {}),
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

  /** Earlier messages as context for plain discussion. Turns with no reply are skipped. */
  function conversation(): { role: 'user' | 'assistant'; content: string }[] {
    const out: { role: 'user' | 'assistant'; content: string }[] = [];
    for (const t of turns) {
      const reply = t.items
        .filter((i): i is Extract<TranscriptItem, { kind: 'text' }> => i.kind === 'text')
        .map((i) => i.text)
        .join('\n')
        .trim();
      if (!reply) continue;
      out.push({ role: 'user', content: t.prompt }, { role: 'assistant', content: reply });
    }
    return out;
  }

  async function startChat(turn: Turn, history: { role: 'user' | 'assistant'; content: string }[]) {
    try {
      const id = await chatSend(providerId, [...history, { role: 'user', content: turn.prompt }], (event) => {
        const asTask = event.type === 'completed' ? ({ type: 'completed', text: '' } as const) : event;
        updateTurn(turn.id, (t) => ({
          ...t,
          items: applyEvent(t.items, asTask),
          running: t.running && !isTerminal(asTask),
          ...(event.type === 'usage' ? { inputTokens: event.inputTokens, outputTokens: event.outputTokens } : {}),
        }));
      });
      updateTurn(turn.id, (t) => ({ ...t, taskId: id }));
    } catch (err) {
      updateTurn(turn.id, (t) => ({ ...t, running: false, error: describeError(err) }));
    }
  }

  async function send() {
    if (!canSend) return;
    nextTurnId += 1;
    const turn: Turn = {
      id: nextTurnId,
      hostId: target?.id ?? null,
      hostLabel: target?.label ?? null,
      mentionText: host ? draftMention : null,
      prompt: prompt.trim(),
      items: [],
      taskId: null,
      running: true,
      notice: null,
      error: null,
      inputTokens: null,
      outputTokens: null,
    };
    const history = conversation();
    const server = target;
    setTurns((prev) => [...prev, turn]);
    setPrompt('');
    // A mention applies to this message only. The pinned server stays.
    setDraftMention(null);
    setHost(null);
    setMention(null);
    setError(null);
    if (server) await startTurn(turn, server);
    else await startChat(turn, history);
  }

  async function stop() {
    if (!runningTask) return;
    try {
      await agentCancel(runningTask);
    } catch (err) {
      setError(describeError(err));
    }
  }

  async function decide(approvalId: string, approve: boolean, sudo?: SudoAnswer) {
    const update = (state: 'sending' | 'approved' | 'denied' | 'pending', why: string | null = null) =>
      setTurns((prev) => prev.map((t) => ({ ...t, items: setApprovalState(t.items, approvalId, state, why) })));
    update('sending');
    try {
      await approvalDecide(approvalId, approve, sudo);
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
          aria-label="Pinned server"
          className="server-select"
          title="Messages without an @ mention run on this server. Plain chat when none is pinned."
          value={pinnedTarget?.id ?? ''}
          onChange={(e) => pinServer(e.target.value)}
        >
          <option value="">Plain chat · no server</option>
          {targets.map((t) => (
            <option key={t.id} value={t.id}>
              {t.kind === 'local' ? 'This computer' : t.label}
            </option>
          ))}
        </select>
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
            <option key={p.id} value={p.id}>
              {p.name} · {p.modelId}
              {p.toolCalling.status === 'verified' ? '' : ' (chat only)'}
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
            <h2>How can I help?</h2>
            <p>
              Ask anything to discuss it. Pin a server in the header to work on it: messages then run
              there, read-only checks run on their own, and any other command waits for your approval.
              Mention a host with <kbd>@</kbd> to use a server for one message only.
            </p>
            <div className="suggestions">
              {SUGGESTIONS.map((s) => (
                <button
                  key={s.title}
                  type="button"
                  className="suggestion"
                  onClick={() => {
                    setPrompt(draftMention ? `${draftMention} ${s.text}` : s.text);
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
                      {turn.hostLabel && (
                        <span className="host-tag" title="Commands in this reply ran on this host">
                          <TerminalIcon size={11} /> {turn.hostLabel}
                        </span>
                      )}
                      {(turn.inputTokens !== null || turn.outputTokens !== null) && (
                        <span
                          className="token-chip"
                          title={`${formatCount(turn.inputTokens)} input · ${formatCount(turn.outputTokens)} output tokens`}
                        >
                          {formatCompact(turn.inputTokens)} in · {formatCompact(turn.outputTokens)} out
                        </span>
                      )}
                    </div>
                    {turn.items.map((item, i) => (
                      <TranscriptRow key={i} item={item} items={turn.items} onDecide={decide} />
                    ))}
                    {turn.running &&
                      !turn.items.some((i) => i.kind === 'text') &&
                      !turn.items.some((i) => i.kind === 'approval' && i.state === 'pending') && (
                      <span className="typing" aria-label="Working">
                        <span />
                        <span />
                        <span />
                      </span>
                    )}
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
        {(providers.length === 0 || (target !== null && !verified)) && (
          <div className="banner" data-tone="warning" style={{ maxWidth: 820, margin: '0 auto 0.6rem' }}>
            <AlertIcon />
            <span className="grow">
              {providers.length === 0
                ? 'Add an AI provider first.'
                : 'Working on a server needs a provider whose tool calling test passed. Run Test on the Providers page, or unpin the server to just chat.'}
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
                      <small>{h.kind === 'local' ? 'This machine · local shell' : `${h.username}@${h.hostname}`}</small>
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
            placeholder={target ? `What should I do on ${target.label}?` : 'Message Selai… (pin a server to work on it)'}
          />
          <div className="composer-bar">
            {host ? (
              <span className="target-chip" title={`${host.username}@${host.hostname}`}>
                <TerminalIcon size={13} />
                {host.label}
                <small>this message</small>
                <button type="button" aria-label={`Remove ${host.label}`} onClick={clearTarget}>
                  <CloseIcon size={11} />
                </button>
              </span>
            ) : pinnedTarget ? (
              <span className="target-chip" data-pinned="true" title="Pinned in the header">
                <TerminalIcon size={13} />
                {pinnedTarget.label}
                <small>pinned</small>
              </span>
            ) : null}
            <button type="button" className="btn btn-ghost btn-sm" onClick={insertAt}>
              <AtIcon size={14} /> {host ? 'Mention another' : 'Mention host'}
            </button>
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
          {target
            ? `Runs on ${target.label}${host ? ' for this message' : ' (pinned)'}. Commands other than read-only checks wait for your approval.`
            : 'Plain chat: nothing runs on any server.'}
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
  onDecide: (approvalId: string, approve: boolean, sudo?: SudoAnswer) => void;
}) {
  switch (item.kind) {
    case 'text':
      return <Markdown text={item.text} />;
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
      return <Activity item={item} />;
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

/** One quiet line per command: what ran and how it ended. Output stays in History. */
function Activity({ item }: { item: ToolItem }) {
  const done = item.status === 'completed';
  const bad = ['failed', 'denied', 'timed_out', 'cancelled', 'interrupted'].includes(item.status);
  const pending = item.status === 'proposed';
  return (
    <div className="activity" aria-label="Command" data-tone={done ? 'success' : bad ? 'danger' : undefined}>
      <span className="mark">
        {pending ? <span className="spinner" style={{ width: 12, height: 12 }} /> : done ? <CheckIcon size={11} /> : bad ? <CloseIcon size={11} /> : <TerminalIcon size={11} />}
      </span>
      <span className="when">{pending ? 'Running' : 'Ran'}</span>
      <code title={item.command}>{item.command}</code>
      {!pending && (
        <span className="when">
          {done ? '' : item.status.replace(/_/g, ' ')}
          {item.exitCode !== null ? `${done ? '' : ' · '}exit ${item.exitCode}` : ''}
        </span>
      )}
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
  onDecide: (approvalId: string, approve: boolean, sudo?: SudoAnswer) => void;
}) {
  const [sudo, setSudo] = useState<SudoAnswer | null>(null);
  const expired = Date.now() > item.expiresAtMs;
  const open = item.state === 'pending' && !expired;
  const asRoot = item.sudo !== null;
  const needsPassword = item.sudo === 'password_required';
  const ready = !needsPassword || sudo !== null;
  return (
    <div
      className={open || item.state === 'sending' ? 'tool-card approval' : 'tool-card'}
      role="group"
      aria-label="Approval required"
    >
      <div className="tool-head">
        <ShieldIcon size={13} />
        <span>
          Run this command{asRoot ? ' as root' : ''} on {hostLabel ?? 'the host'}?
        </span>
        {asRoot && (
          <span className="badge" data-tone="warning">
            root
          </span>
        )}
        <span className="grow" />
        {status && item.state === 'approved' && <StatusBadge item={status} />}
      </div>
      <pre className="tool-cmd">
        <code>{item.command}</code>
      </pre>
      {needsPassword && open && <SudoPassword approvalId={item.approvalId} onChange={setSudo} />}
      {asRoot && !needsPassword && open && (
        <p className="sudo-note">This host's sudo does not ask for a password.</p>
      )}
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
            <button
              type="button"
              className="btn btn-primary btn-sm"
              disabled={!open || !ready}
              onClick={() => onDecide(item.approvalId, true, needsPassword && sudo ? sudo : undefined)}
            >
              {asRoot ? 'Approve as root' : 'Approve'}
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

/**
 * Asks how to answer sudo's password question. The typed password stays in this
 * component until the approval is sent; the model never sees it, and saved
 * passwords never come back to the webview.
 */
function SudoPassword({ approvalId, onChange }: { approvalId: string; onChange: (answer: SudoAnswer | null) => void }) {
  const [options, setOptions] = useState<SudoOptions | null>(null);
  const [source, setSource] = useState<'typed' | 'saved' | 'login'>('typed');
  const [password, setPassword] = useState('');
  const [remember, setRemember] = useState(false);

  // The answer is handed to the card in the same update as each change, not from
  // an effect, so Approve always sees the current choice.
  function answer(next: 'typed' | 'saved' | 'login', typed = password, keep = remember) {
    setSource(next);
    onChange(next === 'typed' ? (typed ? { source: 'typed', password: typed, remember: keep } : null) : { source: next });
  }

  useEffect(() => {
    let active = true;
    sudoOptions(approvalId)
      .then((o) => {
        if (!active) return;
        setOptions(o);
        const first = o.savedPassword ? 'saved' : o.loginPassword ? 'login' : 'typed';
        setSource(first);
        onChange(first === 'typed' ? null : { source: first });
      })
      .catch(() => active && setOptions({ hostLabel: '', savedPassword: false, loginPassword: false }));
    return () => {
      active = false;
    };
    // The choice is made once per approval; onChange is stable enough for that.
  }, [approvalId]);

  return (
    <fieldset className="sudo-box">
      <legend>sudo asks for a password on this host</legend>
      {options?.savedPassword && (
        <label className="check">
          <input type="radio" name={`sudo-${approvalId}`} checked={source === 'saved'} onChange={() => answer('saved')} />
          <span>Use the saved sudo password</span>
        </label>
      )}
      {options?.loginPassword && (
        <label className="check">
          <input type="radio" name={`sudo-${approvalId}`} checked={source === 'login'} onChange={() => answer('login')} />
          <span>Use my SSH login password</span>
        </label>
      )}
      {(options?.savedPassword || options?.loginPassword) && (
        <label className="check">
          <input type="radio" name={`sudo-${approvalId}`} checked={source === 'typed'} onChange={() => answer('typed')} />
          <span>Type a password</span>
        </label>
      )}
      {source === 'typed' && (
        <>
          <input
            type="password"
            autoComplete="off"
            aria-label="Sudo password"
            placeholder="Sudo password"
            value={password}
            onChange={(e) => {
              setPassword(e.target.value);
              answer('typed', e.target.value, remember);
            }}
          />
          <label className="check">
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => {
                setRemember(e.target.checked);
                answer('typed', password, e.target.checked);
              }}
            />
            <span>Remember for this host (saved in the OS keychain)</span>
          </label>
        </>
      )}
      <p className="hint">Sent only to sudo on the host. The AI never sees it.</p>
    </fieldset>
  );
}
