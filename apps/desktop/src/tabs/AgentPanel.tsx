import { Fragment, memo, useCallback, useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import {
  agentCancel,
  agentStart,
  approvalDecide,
  auditVerify,
  chatSend,
  describeError,
  HOSTS_CHANGED,
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
  type TaskEvent,
} from '../ipc';
import { HostAvatar } from '../ui/HostAvatar';
import { collectPages } from '../ui/collectPages';
import { HostKeyDialog } from '../ui/HostKeyDialog';
import { Markdown } from '../ui/Markdown';
import {
  AlertIcon,
  AtIcon,
  CheckIcon,
  CloseIcon,
  PinIcon,
  PlusIcon,
  SendIcon,
  ShieldIcon,
  StopIcon,
  TerminalIcon,
} from '../ui/icons';
import { SelaiLogo } from '../ui/SelaiLogo';
import { applyEvent, isTerminal, setApprovalState, type TranscriptItem } from './agentTranscript';
import { PROVIDERS_CHANGED } from './ProvidersPanel';

/** Load targets once; the @ picker filters this list without an IPC call per key. */
async function loadTargets(): Promise<HostView[]> {
  const [local, hosts] = await Promise.all([
    localHost().catch(() => null),
    collectPages((cursor) => hostList(undefined, false, cursor)),
  ]);
  return [...(local ? [local] : []), ...hosts.filter((h) => !h.archived)];
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
}

const SUGGESTIONS = [
  { title: 'Check disk usage', text: 'check disk usage and the largest directories' },
  { title: 'Is nginx healthy?', text: 'is nginx healthy? check the service and recent errors' },
  { title: 'Explain a config', text: 'Explain what this nginx config does:\n' },
  { title: 'Plan a change', text: 'Help me plan a zero-downtime PostgreSQL major upgrade.' },
];

let nextTurnId = 0;

interface Props {
  onOpenProviders?: () => void;
  onOpenHosts?: () => void;
  onClose?: () => void;
  /** The host of the terminal in focus. An unpinned chat follows it. */
  activeHost?: HostView | null;
  /** Another view asked to use this host in the chat (pins it). */
  contextRequest?: { host: HostView; n: number } | null;
}

export function AgentPanel({ onOpenProviders, onOpenHosts, onClose, activeHost = null, contextRequest = null }: Props = {}) {
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [providerId, setProviderId] = useState('');
  /** A server mentioned with `@` in the message being written: this message only. */
  const [host, setHost] = useState<HostView | null>(null);
  /**
   * The chat's server: messages without a mention go there, so the operator does not
   * mention it every time. It follows the terminal in focus unless pinned. The model
   * decides per message whether it needs the server at all; a question it can answer
   * directly runs nothing.
   */
  const [contextId, setContextId] = useState<string | null>(null);
  const [pinned, setPinned] = useState(false);
  const [contextMenu, setContextMenu] = useState(false);
  const [targets, setTargets] = useState<HostView[]>([]);
  const [draftMention, setDraftMention] = useState<string | null>(null);
  const [prompt, setPrompt] = useState('');
  const [mention, setMention] = useState<string | null>(null);
  const [active, setActive] = useState(0);
  const [turns, setTurns] = useState<Turn[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [audit, setAudit] = useState<{ ok: boolean; text: string } | null>(null);
  const [keyPrompt, setKeyPrompt] = useState<{ turn: Turn; host: HostView; problem: HostKeyProblem } | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const pendingText = useRef(new Map<number, string>());
  const textFlushTimer = useRef<number | null>(null);

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
    const refreshTargets = () =>
      loadTargets()
        .then((list) => active && setTargets(list))
        .catch(() => undefined);
    refreshTargets();
    window.addEventListener(HOSTS_CHANGED, refreshTargets);
    preferencesGet()
      .then((prefs) => {
        if (!active || !prefs.pinnedHostId) return;
        setContextId(prefs.pinnedHostId);
        setPinned(true);
      })
      .catch(() => undefined);
    return () => {
      active = false;
      window.removeEventListener(HOSTS_CHANGED, refreshTargets);
    };
  }, []);

  const mentionQuery = mention?.trim().toLowerCase();
  const matches = mentionQuery === undefined ? [] : targets.filter((target) =>
    target.label.toLowerCase().includes(mentionQuery) ||
    (target.kind === 'local' && 'local'.startsWith(mentionQuery)));

  useEffect(() => {
    if (!contextMenu) return;
    const close = (e: MouseEvent) => {
      if (!(e.target instanceof Element && e.target.closest('.context-row'))) setContextMenu(false);
    };
    const esc = (e: globalThis.KeyboardEvent) => e.key === 'Escape' && setContextMenu(false);
    document.addEventListener('mousedown', close);
    document.addEventListener('keydown', esc);
    return () => {
      document.removeEventListener('mousedown', close);
      document.removeEventListener('keydown', esc);
    };
  }, [contextMenu]);

  // Follow the terminal in focus, unless the operator pinned a server. Adjusted while
  // rendering, so the chip never shows a stale server for a frame.
  const activeId = activeHost?.id ?? null;
  const [seenActive, setSeenActive] = useState<string | null>(null);
  if (activeId !== seenActive) {
    setSeenActive(activeId);
    if (!pinned && activeId) setContextId(activeId);
  }
  const [seenRequest, setSeenRequest] = useState(0);
  if (contextRequest && contextRequest.n !== seenRequest) {
    setSeenRequest(contextRequest.n);
    setContextId(contextRequest.host.id);
    setPinned(true);
  }
  useEffect(() => {
    // Remember a server pinned from another view.
    if (contextRequest) preferencesSet({ pinnedHostId: contextRequest.host.id }).catch(() => undefined);
  }, [contextRequest]);

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
    setActive(0);
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
  // An archived or deleted server drops out of the list and so out of the chat.
  const contextHost =
    targets.find((t) => t.id === contextId) ??
    (activeHost?.id === contextId ? activeHost : null) ??
    (contextRequest?.host.id === contextId ? contextRequest.host : null);
  // The server for this message: a mention, else the chat's server, else none
  // (plain discussion, which any provider can answer). A server needs tool calling.
  const target = host ?? contextHost;
  const canSend = !running && provider !== undefined && (target === null || verified) && prompt.trim().length > 0;

  /** Pinned servers are remembered across restarts; a failed save keeps the choice for now. */
  function rememberPin(id: string | null) {
    preferencesSet({ pinnedHostId: id ?? '' }).catch(() => undefined);
  }

  /** Choosing a server by hand pins it, so switching terminals does not change it. */
  function chooseContext(id: string | null) {
    setContextMenu(false);
    setContextId(id);
    setPinned(id !== null);
    rememberPin(id);
    textareaRef.current?.focus();
  }

  function togglePin() {
    if (pinned) {
      setPinned(false);
      rememberPin(null);
      // Back to following the terminal in focus.
      if (activeHost) setContextId(activeHost.id);
    } else if (contextId) {
      setPinned(true);
      rememberPin(contextId);
    }
  }

  const updateTurn = (id: number, change: (t: Turn) => Turn) =>
    setTurns((prev) => {
      const index = prev.findIndex((t) => t.id === id);
      if (index < 0) return prev;
      const next = change(prev[index]!);
      if (next === prev[index]) return prev;
      const updated = prev.slice();
      updated[index] = next;
      return updated;
    });

  // Model providers can stream many tiny tokens per second. Render text at most
  // once per short interval, but flush it before approvals and terminal events.
  function flushText() {
    if (textFlushTimer.current !== null) window.clearTimeout(textFlushTimer.current);
    textFlushTimer.current = null;
    if (!pendingText.current.size) return;
    const pending = pendingText.current;
    pendingText.current = new Map();
    setTurns((prev) => prev.map((turn) => {
      const text = pending.get(turn.id);
      return text ? { ...turn, items: applyEvent(turn.items, { type: 'textDelta', text }) } : turn;
    }));
  }

  function displayEvent(id: number, event: TaskEvent) {
    if (event.type === 'textDelta') {
      pendingText.current.set(id, (pendingText.current.get(id) ?? '') + event.text);
      if (pendingText.current.get(id)!.length >= 32_768) flushText();
      else if (textFlushTimer.current === null) textFlushTimer.current = window.setTimeout(flushText, 32);
      return;
    }
    flushText();
    updateTurn(id, (turn) => {
      const items = applyEvent(turn.items, event);
      const running = turn.running && !isTerminal(event);
      return items === turn.items && running === turn.running ? turn : { ...turn, items, running };
    });
  }

  useEffect(() => () => {
    if (textFlushTimer.current !== null) window.clearTimeout(textFlushTimer.current);
    pendingText.current.clear();
  }, []);

  async function startTurn(turn: Turn, target: HostView) {
    updateTurn(turn.id, (t) => ({ ...t, running: true, notice: null, error: null, items: [] }));
    try {
      // The new message has no reply yet, so it is not part of its own history.
      const id = await agentStart(target.id, providerId, turn.prompt, conversation(), (event) => {
        displayEvent(turn.id, event);
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
        displayEvent(turn.id, asTask);
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
    };
    const history = conversation();
    const server = target;
    setTurns((prev) => [...prev, turn]);
    setPrompt('');
    // A mention applies to this message only. The chat's server stays.
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

  const decide = useCallback(async (approvalId: string, approve: boolean, sudo?: SudoAnswer) => {
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
  }, []);

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
      <header className="panel-head">
        <SelaiLogo size={22} />
        <h2>Assistant</h2>
        <span className="grow" />
        <button
          type="button"
          className="icon-btn"
          aria-label="New chat"
          title="New chat"
          onClick={() => setTurns([])}
          disabled={running || turns.length === 0}
        >
          <PlusIcon size={15} />
        </button>
        <button type="button" className="icon-btn" aria-label="Verify audit log" title="Verify audit log" onClick={verifyAudit}>
          <ShieldIcon size={14} />
        </button>
        {onClose && (
          <button type="button" className="icon-btn" aria-label="Close chat" title="Close chat (Ctrl+Shift+L to open)" onClick={onClose}>
            <CloseIcon size={14} />
          </button>
        )}
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
              <SelaiLogo size={44} />
            </span>
            <h2>What are you working on?</h2>
            <p>
              {contextHost
                ? `Ask about ${contextHost.label}, or just discuss. Commands run only when an answer needs them; anything beyond a read-only check waits for your approval.`
                : 'Think through a problem, review a config, or investigate a server. You choose where we work.'}
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
                  <span aria-hidden="true">↗</span>
                </button>
              ))}
            </div>
          </div>
        ) : (
          <ol className="thread" aria-label="Transcript">
            {turns.map((turn) => <TurnEntry key={turn.id} turn={turn} onDecide={decide} />)}
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
                : 'Working on a server needs a provider whose tool calling test passed.'}
            </span>
            {providers.length > 0 && host === null && contextHost && (
              <button type="button" className="btn btn-sm" onClick={() => chooseContext(null)}>
                Chat without server
              </button>
            )}
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
                    <TargetAvatar host={h} />
                    <span>
                      {h.label}
                      <small>{h.kind === 'local' ? 'This machine · local shell' : `${h.username}@${h.hostname}`}</small>
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div className="context-row">
            <span className="context-label">{host ? 'This message' : contextHost ? pinned ? 'Pinned server' : 'Following terminal' : 'Context'}</span>
            {host ? (
              <span className="target-chip" data-kind="mention" title={`${host.username}@${host.hostname}`}>
                <AtIcon size={12} />
                {host.label}
                <small>this message</small>
                <button type="button" aria-label={`Remove ${host.label}`} onClick={clearTarget}>
                  <CloseIcon size={11} />
                </button>
              </span>
            ) : null}
            {contextHost ? (
              <span
                className="target-chip"
                data-muted={host ? 'true' : undefined}
                title={host ? `This message goes to ${host.label} instead` : `${contextHost.username}@${contextHost.hostname}`}
              >
                <button
                  type="button"
                  className="chip-main"
                  aria-label={`Chat server: ${contextHost.label}. Change`}
                  aria-haspopup="menu"
                  aria-expanded={contextMenu}
                  onClick={() => setContextMenu((o) => !o)}
                >
                  <TerminalIcon size={12} />
                  {contextHost.label}
                </button>
                <button
                  type="button"
                  className="chip-pin"
                  aria-pressed={pinned}
                  aria-label={pinned ? `Unpin ${contextHost.label}` : `Pin ${contextHost.label}`}
                  title={pinned ? 'Pinned: stays when you switch terminals. Click to follow the terminal again.' : 'Follows the terminal in focus. Click to keep this server.'}
                  onClick={togglePin}
                >
                  <PinIcon size={12} />
                </button>
                <button type="button" aria-label={`Chat without ${contextHost.label}`} title="Chat without a server" onClick={() => chooseContext(null)}>
                  <CloseIcon size={11} />
                </button>
              </span>
            ) : (
              !host && (
                <button
                  type="button"
                  className="target-chip"
                  data-empty="true"
                  aria-haspopup="menu"
                  aria-expanded={contextMenu}
                  onClick={() => setContextMenu((o) => !o)}
                >
                  <PlusIcon size={12} /> Server
                </button>
              )
            )}
            {contextMenu && (
              <ul className="mention-pop context-pop" role="menu" aria-label="Chat server">
                <li className="head">Chat server</li>
                <li>
                  <button type="button" role="menuitemradio" aria-checked={contextHost === null} onClick={() => chooseContext(null)}>
                    <SelaiLogo size={18} />
                    <span>
                      No server
                      <small>plain chat, nothing runs</small>
                    </span>
                  </button>
                </li>
                {targets.map((t) => (
                  <li key={t.id}>
                    <button type="button" role="menuitemradio" aria-checked={contextHost?.id === t.id} onClick={() => chooseContext(t.id)}>
                      <TargetAvatar host={t} />
                      <span>
                        {t.label}
                        <small>{t.kind === 'local' ? 'local shell' : `${t.username}@${t.hostname}`}</small>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
          <textarea
            ref={textareaRef}
            aria-label="Request"
            value={prompt}
            onChange={(e) => onPromptChange(e.target.value)}
            onKeyDown={onPromptKeyDown}
            rows={Math.min(8, Math.max(2, prompt.split('\n').length))}
            placeholder={target ? `Ask about ${target.label}, or anything…` : 'Ask anything…'}
          />
          <div className="composer-bar">
            <button type="button" className="icon-btn" aria-label="Mention a host" title="Ask about another host for this message (@)" onClick={insertAt}>
              <AtIcon size={14} />
            </button>
            <select
              aria-label="Provider"
              className="model-select"
              value={providerId}
              onChange={(e) => chooseProvider(e.target.value)}
              disabled={running}
            >
              {providers.length === 0 && <option value="">No providers</option>}
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
          {host
            ? `This message goes to ${host.label}. Changes wait for your approval.`
            : contextHost
              ? `Working on ${contextHost.label}: commands run only when needed, and changes wait for your approval.`
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

/** Completed turns keep their identity, so typing or streaming a later turn does
 *  not parse and render all earlier Markdown again. */
const TurnEntry = memo(function TurnEntry({ turn, onDecide }: {
  turn: Turn;
  onDecide: (approvalId: string, approve: boolean, sudo?: SudoAnswer) => void;
}) {
  return (
    <Fragment>
      <li className="turn-user">
        <div className="bubble">{highlight(turn.prompt, turn.mentionText)}</div>
      </li>
      <li className="turn-ai">
        <span className="ai-avatar" role="img" aria-label="Selai assistant">
          <SelaiLogo size={26} />
        </span>
        <div className="ai-body">
          {turn.hostLabel && turn.items.some((i) => i.kind === 'tool' || i.kind === 'approval') && (
            <div className="ai-meta">
              <span className="host-tag" title="Commands in this reply ran on this host">
                <TerminalIcon size={11} /> {turn.hostLabel}
              </span>
            </div>
          )}
          {turn.items.map((item, i) => (
            <TranscriptRow key={i} item={item} items={turn.items} onDecide={onDecide} />
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
  );
});

function TargetAvatar({ host }: { host: HostView }) {
  if (host.kind !== 'local') return <HostAvatar label={host.label} size={13} />;
  return (
    <span className="avatar local-avatar">
      <TerminalIcon size={13} />
    </span>
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
