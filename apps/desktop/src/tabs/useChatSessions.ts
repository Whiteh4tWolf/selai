import { useCallback, useEffect, useRef, useState, type SetStateAction } from 'react';
import { chatSessionGet, chatSessionList, chatSessionSave, describeError, type ChatSessionSummary } from '../ipc';
import type { TranscriptItem } from './agentTranscript';
import { registerVaultFlusher } from '../vault';

export interface ChatTurn {
  id: string;
  hostId: string | null;
  hostLabel: string | null;
  showPrompt: boolean;
  taskPrompt: string;
  mentionText: string[] | null;
  prompt: string;
  items: TranscriptItem[];
  taskId: string | null;
  running: boolean;
  notice: string | null;
  error: string | null;
}

interface Session extends ChatSessionSummary {
  turns: ChatTurn[];
  titleEdited: boolean;
  revision: number;
  savedRevision: number;
}

function freshSession(): Session {
  const now = Date.now();
  return { id: crypto.randomUUID(), title: 'New session', turns: [], titleEdited: false,
    createdAtMs: now, updatedAtMs: now, revision: 0, savedRevision: 0 };
}

const object = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null && !Array.isArray(value);
const optionalText = (value: unknown) => value === null || typeof value === 'string';

function validItem(value: unknown): boolean {
  if (!object(value)) return false;
  switch (value.kind) {
    case 'text': case 'status': return typeof value.text === 'string';
    case 'failed': return typeof value.code === 'string' && typeof value.message === 'string';
    case 'tool': return typeof value.executionId === 'string' && typeof value.command === 'string'
      && typeof value.hostLabel === 'string' && typeof value.status === 'string'
      && (value.exitCode === null || typeof value.exitCode === 'number');
    case 'approval': return typeof value.approvalId === 'string' && typeof value.executionId === 'string'
      && typeof value.command === 'string' && typeof value.expiresAtMs === 'number'
      && ['pending', 'sending', 'approved', 'denied'].includes(String(value.state))
      && optionalText(value.error) && [null, 'no_password', 'password_required'].includes(value.sudo as string | null);
    default: return false;
  }
}

/** Reopening history is a view operation. It never resumes commands or approvals. */
export function restoreTranscript(transcript: string): ChatTurn[] {
  const snapshot: unknown = JSON.parse(transcript);
  if (!object(snapshot) || snapshot.version !== 1 || !Array.isArray(snapshot.turns) || snapshot.turns.length > 1000) {
    throw new Error('Unsupported session history.');
  }
  return snapshot.turns.map((value: unknown) => {
    if (!object(value) || typeof value.id !== 'string' || typeof value.prompt !== 'string'
      || typeof value.taskPrompt !== 'string' || typeof value.showPrompt !== 'boolean' || typeof value.running !== 'boolean'
      || !optionalText(value.hostId) || !optionalText(value.hostLabel) || !optionalText(value.taskId)
      || !optionalText(value.notice) || !optionalText(value.error)
      || !(value.mentionText === null || (Array.isArray(value.mentionText) && value.mentionText.every((m) => typeof m === 'string')))
      || !Array.isArray(value.items) || !value.items.every(validItem)) {
      throw new Error('Invalid session history.');
    }
    const turn = value as unknown as ChatTurn;
    const detached = turn.running || turn.items.some((item) =>
      (item.kind === 'approval' && ['pending', 'sending'].includes(item.state)) ||
      (item.kind === 'tool' && ['proposed', 'running'].includes(item.status)));
    return { ...turn, running: false, notice: detached
      ? 'Previous run is no longer attached. Check History for its outcome before retrying commands.' : turn.notice,
      items: turn.items.map((item): TranscriptItem => {
        if (item.kind === 'approval' && ['pending', 'sending'].includes(item.state)) {
          return { ...item, state: 'pending', expiresAtMs: 0, error: null };
        }
        if (item.kind === 'tool' && ['proposed', 'running'].includes(item.status)) {
          return { ...item, status: 'detached' };
        }
        return item;
      }),
    };
  });
}

function fromSaved(saved: ChatSessionSummary & { transcript: string }): Session {
  return { id: saved.id, title: saved.title, createdAtMs: saved.createdAtMs, updatedAtMs: saved.updatedAtMs,
    turns: restoreTranscript(saved.transcript), titleEdited: true, revision: 0, savedRevision: 0 };
}

/** Keeps live sessions in memory and batches SQLite writes to once a second. */
export function useChatSessions() {
  const [initial] = useState(freshSession);
  const [records, setRecords] = useState<Record<string, Session>>({ [initial.id]: initial });
  const recordsRef = useRef(records);
  const [activeId, setActiveId] = useState(initial.id);
  const activeRef = useRef(initial.id);
  const [summaries, setSummaries] = useState<ChatSessionSummary[]>([]);
  const [ready, setReady] = useState(false);
  const [loading, setLoading] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const timers = useRef(new Map<string, number>());
  const chains = useRef(new Map<string, Promise<void>>());
  const inFlight = useRef(new Map<string, number>());
  const failed = useRef(new Map<string, number>());
  const selection = useRef(0);
  const mounted = useRef(true);

  const mutate = useCallback((change: (prev: Record<string, Session>) => Record<string, Session>) => {
    const next = change(recordsRef.current);
    recordsRef.current = next;
    if (mounted.current) setRecords(next);
  }, []);

  const flush = useCallback((id: string) => {
    const timer = timers.current.get(id);
    if (timer !== undefined) window.clearTimeout(timer);
    timers.current.delete(id);
    const record = recordsRef.current[id];
    if (!record || record.revision <= record.savedRevision || (inFlight.current.get(id) ?? -1) >= record.revision) return;
    const revision = record.revision;
    inFlight.current.set(id, revision);
    const input = { id, title: record.title.trim() || 'New session',
      transcript: JSON.stringify({ version: 1, turns: record.turns }) };
    const previous = chains.current.get(id);
    const request = previous ? previous.catch(() => undefined).then(() => chatSessionSave(input)) : chatSessionSave(input);
    const job = request
      .then(() => {
        failed.current.delete(id);
        mutate((prev) => prev[id] ? { ...prev, [id]: { ...prev[id]!, savedRevision: Math.max(prev[id]!.savedRevision, revision) } } : prev);
        if (mounted.current) {
          setSummaries((prev) => [{ id, title: input.title, createdAtMs: record.createdAtMs, updatedAtMs: record.updatedAtMs },
            ...prev.filter((summary) => summary.id !== id)]);
          if (failed.current.size === 0) setSaveError(null);
        }
      })
      .catch((error) => {
        failed.current.set(id, revision);
        if (mounted.current) setSaveError(`Session history could not be saved: ${describeError(error)}`);
      })
      .finally(() => {
        if (inFlight.current.get(id) === revision) inFlight.current.delete(id);
        if (chains.current.get(id) === job) chains.current.delete(id);
      });
    chains.current.set(id, job);
  }, [mutate]);

  useEffect(() => registerVaultFlusher(async () => {
    Object.keys(recordsRef.current).forEach(flush);
    await Promise.all([...chains.current.values()]);
    if (failed.current.size) throw new Error('Session history could not be saved. Retry saving before locking.');
  }), [flush]);

  useEffect(() => {
    mounted.current = true;
    let live = true;
    void (async () => {
      try {
        const list = await chatSessionList();
        if (!live) return;
        setSummaries(list);
        const latest = list[0];
        if (latest && recordsRef.current[activeRef.current]?.revision === 0) {
          const saved = await chatSessionGet(latest.id);
          if (!live || recordsRef.current[activeRef.current]?.revision !== 0) return;
          const record = fromSaved(saved);
          mutate(() => ({ [record.id]: record }));
          activeRef.current = record.id;
          setActiveId(record.id);
        }
      } catch (error) {
        if (live) setLoadError(`Session history could not be loaded: ${describeError(error)}`);
      } finally {
        if (live) setReady(true);
      }
    })();
    const flushAll = () => Object.keys(recordsRef.current).forEach(flush);
    window.addEventListener('pagehide', flushAll);
    return () => {
      live = false;
      mounted.current = false;
      window.removeEventListener('pagehide', flushAll);
      flushAll();
    };
  }, [flush, mutate]);

  useEffect(() => {
    for (const record of Object.values(records)) {
      if (record.revision <= record.savedRevision || timers.current.has(record.id) ||
        (inFlight.current.get(record.id) ?? -1) >= record.revision || failed.current.get(record.id) === record.revision) continue;
      timers.current.set(record.id, window.setTimeout(() => flush(record.id), 1000));
    }
  }, [records, flush]);

  useEffect(() => {
    // Keep at most eight idle transcripts cached. Running tasks and unsaved work
    // stay resident so their event channels retain their original destination.
    const count = Object.keys(records).length;
    if (count <= 8) return;
    const removable = Object.values(records).filter((record) => record.id !== activeId
      && record.revision === record.savedRevision && !record.turns.some((turn) => turn.running || turn.notice))
      .sort((a, b) => a.updatedAtMs - b.updatedAtMs).slice(0, count - 8);
    if (removable.length) mutate((prev) => {
      const next = { ...prev };
      removable.forEach((record) => { delete next[record.id]; });
      return next;
    });
  }, [records, activeId, mutate]);

  const updateTurns = useCallback((id: string, change: SetStateAction<ChatTurn[]>) => {
    mutate((prev) => {
      const record = prev[id];
      if (!record) return prev;
      const turns = typeof change === 'function' ? change(record.turns) : change;
      if (turns === record.turns || (turns.length === record.turns.length && turns.every((turn, i) => turn === record.turns[i]))) return prev;
      const title = !record.titleEdited && record.turns.length === 0 && turns.length
        ? (turns[0]!.taskPrompt || turns[0]!.prompt).replace(/\s+/g, ' ').trim().slice(0, 80) || 'New session' : record.title;
      return { ...prev, [id]: { ...record, title, turns, revision: record.revision + 1, updatedAtMs: Date.now() } };
    });
  }, [mutate]);

  const setTurns = useCallback((change: SetStateAction<ChatTurn[]>) => updateTurns(activeId, change), [activeId, updateTurns]);
  const updateAllTurns = useCallback((change: (turns: ChatTurn[]) => ChatTurn[]) => {
    for (const id of Object.keys(recordsRef.current)) updateTurns(id, change);
  }, [updateTurns]);

  function newSession() {
    flush(activeRef.current);
    selection.current += 1;
    const record = { ...freshSession(), revision: 1 };
    mutate((prev) => ({ ...prev, [record.id]: record }));
    activeRef.current = record.id;
    setActiveId(record.id);
    setLoading(false);
  }

  async function selectSession(id: string) {
    if (id === activeRef.current) return;
    const request = ++selection.current;
    flush(activeRef.current);
    setLoading(true);
    try {
      if (!recordsRef.current[id]) {
        const saved = await chatSessionGet(id);
        if (!mounted.current || request !== selection.current) return;
        const record = fromSaved(saved);
        mutate((prev) => ({ ...prev, [id]: record }));
      }
      activeRef.current = id;
      setActiveId(id);
      setLoadError(null);
    } catch (error) {
      setLoadError(`Session could not be opened: ${describeError(error)}`);
    } finally {
      if (request === selection.current) setLoading(false);
    }
  }

  function rename(title: string) {
    mutate((prev) => {
      const record = prev[activeRef.current]!;
      return { ...prev, [record.id]: { ...record, title: title.slice(0, 120), titleEdited: true,
        revision: record.revision + 1, updatedAtMs: Date.now() } };
    });
  }

  const list = new Map(summaries.map((summary) => [summary.id, { ...summary, running: false }]));
  for (const record of Object.values(records)) {
    list.set(record.id, { id: record.id, title: record.title, createdAtMs: record.createdAtMs,
      updatedAtMs: record.updatedAtMs, running: record.turns.some((turn) => turn.running) });
  }
  return { current: records[activeId]!, turns: records[activeId]!.turns, setTurns, updateAllTurns,
    sessions: [...list.values()].sort((a, b) => b.updatedAtMs - a.updatedAtMs), ready, loading,
    newSession, selectSession, rename, saveError: loadError ?? saveError,
    retrySave: () => {
      failed.current.clear();
      Object.keys(recordsRef.current).forEach(flush);
      void chatSessionList().then((list) => { setSummaries(list); setLoadError(null); })
        .catch((error) => setLoadError(`Session history could not be loaded: ${describeError(error)}`));
    },
  };
}
