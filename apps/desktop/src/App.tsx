import { lazy, Suspense, useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { AgentPanel } from './tabs/AgentPanel';
import { HistoryTab } from './tabs/HistoryTab';
import { HostsSidebar } from './tabs/HostsSidebar';
import { ProvidersPanel } from './tabs/ProvidersPanel';
import { appInfo, describeError, localHost, storageStatus, type AppInfo, type HostView } from './ipc';
import type { SessionStatus, TerminalSessionTab } from './tabs/TerminalTab';
import { ClockIcon, CloseIcon, PlugIcon, PlusIcon, ServerIcon, TerminalIcon } from './ui/icons';
import { SelaiLogo } from './ui/SelaiLogo';

// xterm.js is large; load it only when a terminal is first needed.
const TerminalStack = lazy(() => import('./tabs/TerminalTab').then((m) => ({ default: m.TerminalStack })));

/** A tab in the centre area: a terminal, or one of the pages that open as a tab. */
type PageId = 'history' | 'providers';
type EditorTab = ({ kind: 'terminal' } & TerminalSessionTab) | { kind: PageId; key: PageId };

const PAGE_TITLES: Record<PageId, string> = { history: 'History', providers: 'Providers' };

let nextTabNumber = 0;

type Theme = 'dark' | 'light' | 'system';
const THEMES: Theme[] = ['dark', 'light', 'system'];
const THEME_KEY = 'selai.theme';

/** Display preferences are per device, so browser storage is enough. */
function stored(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function store(key: string, value: string) {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Not saved; the choice still applies until the app closes.
  }
}

function storedTheme(): Theme {
  const value = stored(THEME_KEY);
  return THEMES.includes(value as Theme) ? (value as Theme) : 'dark';
}

/** A panel width the operator drags, remembered across starts. */
function usePanelWidth(key: string, initial: number, min: number, max: number) {
  const [width, setWidth] = useState(() => {
    const saved = Number(stored(key));
    return Number.isFinite(saved) && saved >= min && saved <= max ? saved : initial;
  });
  const clamp = useCallback((w: number) => Math.round(Math.min(max, Math.max(min, w))), [min, max]);
  const commit = useCallback((w: number) => store(key, String(w)), [key]);
  const set = (w: number) => {
    const next = clamp(w);
    setWidth(next);
    return next;
  };
  return { width, setWidth: set, commit };
}

function Resizer({
  label,
  width,
  setWidth,
  commit,
  direction,
}: {
  label: string;
  width: number;
  setWidth: (w: number) => number;
  commit: (w: number) => void;
  /** 1 when dragging right widens the panel (left sidebar), -1 for the right panel. */
  direction: 1 | -1;
}) {
  const drag = useRef<{ x: number; start: number; now: number } | null>(null);
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={width}
      tabIndex={0}
      className="resizer"
      onPointerDown={(e) => {
        e.currentTarget.setPointerCapture(e.pointerId);
        drag.current = { x: e.clientX, start: width, now: width };
        document.body.classList.add('resizing');
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (d) d.now = setWidth(d.start + direction * (e.clientX - d.x));
      }}
      onPointerUp={() => {
        if (drag.current) commit(drag.current.now);
        drag.current = null;
        document.body.classList.remove('resizing');
      }}
      onKeyDown={(e) => {
        const step = e.key === 'ArrowRight' ? 16 : e.key === 'ArrowLeft' ? -16 : 0;
        if (!step) return;
        e.preventDefault();
        commit(setWidth(width + direction * step));
      }}
    />
  );
}

export function App() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [storage, setStorage] = useState<{ ok: boolean; text: string }>({ ok: true, text: 'Checking database…' });
  const [tabs, setTabs] = useState<EditorTab[]>([]);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  /** The terminal focused last; the chat follows its host. */
  const [lastTerminal, setLastTerminal] = useState<string | null>(null);
  const [statuses, setStatuses] = useState<Record<string, SessionStatus>>({});
  const statusRef = useRef<Record<string, SessionStatus>>({});
  const [editRequest, setEditRequest] = useState<{ hostId: string; n: number } | null>(null);
  const [chatRequest, setChatRequest] = useState<{ host: HostView; n: number } | null>(null);
  const [theme, setTheme] = useState<Theme>(storedTheme);
  const [sidebarOpen, setSidebarOpen] = useState(() => stored('selai.sidebar') !== 'closed');
  const [chatOpen, setChatOpen] = useState(() => {
    const saved = stored('selai.chat');
    return saved === null ? window.innerWidth > 1100 : saved !== 'closed';
  });
  const side = usePanelWidth('selai.sidebar.width', 264, 220, 420);
  const chat = usePanelWidth('selai.chat.width', 380, 300, 640);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === 'system') delete root.dataset.theme;
    else root.dataset.theme = theme;
    store(THEME_KEY, theme);
  }, [theme]);

  useEffect(() => store('selai.sidebar', sidebarOpen ? 'open' : 'closed'), [sidebarOpen]);
  useEffect(() => store('selai.chat', chatOpen ? 'open' : 'closed'), [chatOpen]);

  // Panel shortcuts, caught before the terminal sees them. Plain Ctrl+B and Ctrl+L
  // belong to the shell, so these use Shift.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (!(e.ctrlKey || e.metaKey) || !e.shiftKey || e.altKey) return;
      const key = e.key.toLowerCase();
      if (key === 'e') {
        e.preventDefault();
        setSidebarOpen((o) => !o);
      } else if (key === 'l') {
        e.preventDefault();
        setChatOpen(true);
        window.setTimeout(() => document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Request"]')?.focus(), 0);
      }
    }
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, []);

  useEffect(() => {
    let active = true;
    appInfo()
      .then((value) => active && setInfo(value))
      .catch(() => active && setInfo(null));
    storageStatus()
      .then((status) => {
        if (!active) return;
        const interrupted =
          status.interruptedTasksAtStartup > 0
            ? `, ${status.interruptedTasksAtStartup} task(s) marked interrupted at startup`
            : '';
        setStorage({ ok: true, text: `Database schema v${status.schemaVersion}, ${status.journalMode} mode${interrupted}` });
      })
      .catch((err: unknown) => active && setStorage({ ok: false, text: `Database unavailable: ${describeError(err)}` }));
    return () => {
      active = false;
    };
  }, []);

  const terminals = tabs.filter((t): t is Extract<EditorTab, { kind: 'terminal' }> => t.kind === 'terminal');

  function activate(key: string) {
    setActiveKey(key);
    if (tabs.some((t) => t.key === key && t.kind === 'terminal')) setLastTerminal(key);
  }

  function openTerminal(host: HostView) {
    nextTabNumber += 1;
    const key = `term-${nextTabNumber}`;
    // A tab for this host that is not connected (failed login, refused key, ended
    // session) is replaced in place instead of piling up a second tab.
    // Statuses are read from the ref: a click can land before the re-render that
    // follows a status change.
    const tab: EditorTab = { kind: 'terminal', key, host };
    setTabs((prev) => {
      const stale = prev.find(
        (t) =>
          t.kind === 'terminal' &&
          t.host.id === host.id &&
          ['failed', 'closed', 'hostkey'].includes(statusRef.current[t.key] ?? ''),
      );
      return stale ? prev.map((t) => (t.key === stale.key ? tab : t)) : [...prev, tab];
    });
    setActiveKey(key);
    setLastTerminal(key);
  }

  function openPage(page: PageId) {
    setTabs((prev) => (prev.some((t) => t.key === page) ? prev : [...prev, { kind: page, key: page }]));
    setActiveKey(page);
  }

  function closeTab(key: string) {
    const index = tabs.findIndex((t) => t.key === key);
    const next = tabs.filter((t) => t.key !== key);
    setTabs(next);
    setStatuses((prev) => Object.fromEntries(Object.entries(prev).filter(([k]) => k !== key)));
    if (activeKey === key) {
      const neighbour = next[Math.min(index, next.length - 1)] ?? null;
      setActiveKey(neighbour?.key ?? null);
      if (neighbour?.kind === 'terminal') setLastTerminal(neighbour.key);
    }
    if (lastTerminal === key) {
      setLastTerminal(next.find((t) => t.kind === 'terminal')?.key ?? null);
    }
  }

  const onStatus = useCallback((key: string, status: SessionStatus) => {
    statusRef.current = { ...statusRef.current, [key]: status };
    setStatuses((prev) => (prev[key] === status ? prev : { ...prev, [key]: status }));
  }, []);

  function editHost(hostId: string) {
    setEditRequest((prev) => ({ hostId, n: (prev?.n ?? 0) + 1 }));
    setSidebarOpen(true);
  }

  function askAi(host: HostView) {
    setChatRequest((prev) => ({ host, n: (prev?.n ?? 0) + 1 }));
    setChatOpen(true);
  }

  function openLocal() {
    localHost()
      .then(openTerminal)
      .catch(() => setSidebarOpen(true));
  }

  const active = tabs.find((t) => t.key === activeKey) ?? null;
  const focusedTerminal = terminals.find((t) => t.key === lastTerminal) ?? null;
  const connectedSessions = Object.values(statuses).filter((status) => status === 'connected').length;
  const locationTitle = active?.kind === 'terminal' ? active.host.label : active ? PAGE_TITLES[active.kind] : 'Overview';
  // Status per host for the explorer: the best state among its open terminals.
  const sessions: Record<string, SessionStatus> = {};
  for (const t of terminals) {
    const s = statuses[t.key] ?? 'connecting';
    if (sessions[t.host.id] !== 'connected') sessions[t.host.id] = s;
  }

  const activity = (
    label: string,
    icon: ReactNode,
    pressed: boolean,
    onClick: () => void,
    shortcut?: string,
  ) => (
    <button
      type="button"
      className="activity-btn"
      aria-label={label}
      aria-pressed={pressed}
      title={shortcut ? `${label} (${shortcut})` : label}
      onClick={onClick}
    >
      {icon}
      <span className="activity-label">{label}</span>
    </button>
  );

  return (
    <div className="workbench">
      <header className="workspace-head">
        <div className="workspace-brand" role="img" aria-label="Selai">
          <SelaiLogo size={28} />
        </div>
        <span className="workspace-divider" aria-hidden="true" />
        <span className="workspace-title">Workspace</span>
        <span className="workspace-location">/ {locationTitle}</span>
        <span className="grow" />
        <span className="workspace-session-count">
          <span className="dot" data-status={connectedSessions > 0 ? 'ok' : undefined} />
          {connectedSessions} connected
        </span>
      </header>
      <div className="workbench-main">
        <nav className="activitybar" aria-label="Main">
          {activity('Hosts', <ServerIcon size={20} />, sidebarOpen, () => setSidebarOpen((o) => !o), 'Ctrl+Shift+E')}
          {activity('History', <ClockIcon size={20} />, active?.kind === 'history', () => openPage('history'))}
          {activity('Providers', <PlugIcon size={20} />, active?.kind === 'providers', () => openPage('providers'))}
          <span className="grow" />
          {activity('Assistant', <SelaiLogo size={26} />, chatOpen, () => setChatOpen((o) => !o), 'Ctrl+Shift+L')}
        </nav>

        <aside className="sidebar" hidden={!sidebarOpen} style={{ width: side.width }}>
          <HostsSidebar onConnect={openTerminal} onAskAi={askAi} sessions={sessions} editRequest={editRequest} />
        </aside>
        {sidebarOpen && <Resizer label="Resize sidebar" direction={1} {...side} />}

        <main className="editor">
          {tabs.length > 0 && (
            <div role="tablist" aria-label="Open tabs" className="editor-tabs">
              {tabs.map((tab) => {
                const title = tab.kind === 'terminal' ? tab.host.label : PAGE_TITLES[tab.kind];
                return (
                  <span key={tab.key} className="editor-tab" data-active={tab.key === activeKey}>
                    <button
                      type="button"
                      role="tab"
                      aria-selected={tab.key === activeKey}
                      onClick={() => activate(tab.key)}
                      onAuxClick={(e) => e.button === 1 && closeTab(tab.key)}
                      title={tab.kind === 'terminal' ? `${tab.host.username}@${tab.host.hostname}` : title}
                    >
                      {tab.kind === 'terminal' ? (
                        <span className="dot" data-status={statuses[tab.key] ?? 'connecting'} />
                      ) : tab.kind === 'history' ? (
                        <ClockIcon size={13} />
                      ) : (
                        <PlugIcon size={13} />
                      )}
                      <span>{title}</span>
                    </button>
                    <button type="button" className="icon-btn" aria-label={`Close ${title}`} onClick={() => closeTab(tab.key)}>
                      <CloseIcon size={12} />
                    </button>
                  </span>
                );
              })}
              <button type="button" className="icon-btn add" aria-label="New local terminal" title="New local terminal" onClick={openLocal}>
                <PlusIcon size={14} />
              </button>
            </div>
          )}
          <div className="editor-body">
            {tabs.length === 0 && (
              <Welcome
                onLocal={openLocal}
                onHosts={() => setSidebarOpen(true)}
                onChat={() => setChatOpen(true)}
                onProviders={() => openPage('providers')}
              />
            )}
            {terminals.length > 0 && (
              <Suspense fallback={<div className="welcome"><span className="spinner spinner-lg" /></div>}>
                <TerminalStack
                  tabs={terminals}
                  activeKey={activeKey}
                  visible={active?.kind === 'terminal'}
                  onClose={closeTab}
                  onStatus={onStatus}
                  onEditHost={editHost}
                />
              </Suspense>
            )}
            {tabs.some((t) => t.kind === 'history') && (
              <div className="editor-page" hidden={active?.kind !== 'history'}>
                <HistoryTab visible={active?.kind === 'history'} />
              </div>
            )}
            {tabs.some((t) => t.kind === 'providers') && (
              <div className="editor-page" hidden={active?.kind !== 'providers'}>
                <ProvidersPanel />
              </div>
            )}
          </div>
        </main>

        {chatOpen && <Resizer label="Resize chat" direction={-1} {...chat} />}
        <aside className="chatbar" hidden={!chatOpen} style={{ width: chat.width }}>
          <AgentPanel
            activeHost={focusedTerminal?.host ?? null}
            contextRequest={chatRequest}
            onOpenProviders={() => {
              openPage('providers');
              if (window.innerWidth <= 1100) setChatOpen(false);
            }}
            onOpenHosts={() => {
              setSidebarOpen(true);
              if (window.innerWidth <= 1100) setChatOpen(false);
            }}
            onClose={() => setChatOpen(false)}
          />
        </aside>
      </div>

      <footer className="statusbar">
        <span className="status-item" title={storage.text} data-tone={storage.ok ? undefined : 'danger'}>
          <span className="dot" data-status={storage.ok ? 'ok' : 'failed'} />
          {storage.ok ? 'Vault ready' : storage.text}
        </span>
        {focusedTerminal && (
          <span className="status-item">
            <TerminalIcon size={12} />
            {focusedTerminal.host.label} · {statuses[focusedTerminal.key] ?? 'connecting'}
          </span>
        )}
        <span className="grow" />
        <button
          type="button"
          className="status-item status-btn"
          onClick={() => setTheme(THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length] ?? 'dark')}
        >
          Theme: {theme === 'system' ? 'System' : theme === 'dark' ? 'Dark' : 'Light'}
        </button>
        <span className="status-item">{info ? `Selai ${info.version}` : ''}</span>
      </footer>
    </div>
  );
}

function Welcome({ onLocal, onHosts, onChat, onProviders }: {
  onLocal: () => void;
  onHosts: () => void;
  onChat: () => void;
  onProviders: () => void;
}) {
  return (
    <div className="welcome">
      <div className="welcome-inner">
        <span className="eyebrow">YOUR WORKSPACE</span>
        <h1>Ready when you are.</h1>
        <p className="welcome-intro">Open a terminal, connect to a server, or work through a problem with the assistant.</p>
        <h2>Get started</h2>
        <ul className="welcome-actions">
          <li>
            <button type="button" onClick={onLocal}>
              <span className="welcome-action-icon"><TerminalIcon size={19} /></span>
              <span className="welcome-action-copy">
                <strong>Open a terminal on this computer</strong>
                <small>Start a local shell, right here.</small>
              </span>
              <span className="welcome-arrow" aria-hidden="true">↗</span>
            </button>
          </li>
          <li>
            <button type="button" onClick={onHosts}>
              <span className="welcome-action-icon"><ServerIcon size={19} /></span>
              <span className="welcome-action-copy">
                <strong>Connect to an SSH host</strong>
                <small>Choose a saved host or add a new one.</small>
              </span>
              <kbd>Ctrl+Shift+E</kbd>
            </button>
          </li>
          <li>
            <button type="button" onClick={onChat}>
              <span className="welcome-action-icon"><SelaiLogo size={24} /></span>
              <span className="welcome-action-copy">
                <strong>Ask the AI</strong>
                <small>Investigate an issue or think through a change.</small>
              </span>
              <kbd>Ctrl+Shift+L</kbd>
            </button>
          </li>
        </ul>
        <div className="welcome-foot">
          <span>Set up the assistant to work your way.</span>
          <button type="button" onClick={onProviders}>Configure AI providers <span aria-hidden="true">→</span></button>
        </div>
      </div>
    </div>
  );
}
