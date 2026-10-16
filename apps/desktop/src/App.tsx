import { lazy, Suspense, useCallback, useEffect, useState, type ReactNode } from 'react';
import { AgentPanel } from './tabs/AgentPanel';
import { ConnectionsTab } from './tabs/ConnectionsTab';
import { HistoryTab } from './tabs/HistoryTab';
import { ProvidersPanel } from './tabs/ProvidersPanel';
import { appInfo, describeError, localHost, storageStatus, type AppInfo, type HostView } from './ipc';
import type { SessionStatus, TerminalSessionTab } from './tabs/TerminalTab';
import { ClockIcon, PlugIcon, ServerIcon, SparkIcon, TerminalIcon } from './ui/icons';

// xterm.js is large; load it only when a terminal is first needed.
const TerminalTab = lazy(() => import('./tabs/TerminalTab').then((m) => ({ default: m.TerminalTab })));

type ViewId = 'hosts' | 'terminal' | 'agent' | 'providers' | 'history';

const NAV: { section: string; items: { id: ViewId; label: string; icon: ReactNode }[] }[] = [
  {
    section: 'Workspace',
    items: [
      { id: 'hosts', label: 'Hosts', icon: <ServerIcon /> },
      { id: 'terminal', label: 'Terminal', icon: <TerminalIcon /> },
    ],
  },
  {
    section: 'AI',
    items: [
      { id: 'agent', label: 'AI Agent', icon: <SparkIcon /> },
      { id: 'providers', label: 'Providers', icon: <PlugIcon /> },
      { id: 'history', label: 'History', icon: <ClockIcon /> },
    ],
  },
];

let nextTabNumber = 0;

type Theme = 'dark' | 'light' | 'system';
const THEMES: Theme[] = ['dark', 'light', 'system'];
const THEME_KEY = 'selai.theme';

/** The theme is a per-device display preference, so browser storage is enough. */
function storedTheme(): Theme {
  try {
    const value = window.localStorage.getItem(THEME_KEY);
    return THEMES.includes(value as Theme) ? (value as Theme) : 'dark';
  } catch {
    return 'dark';
  }
}

export function App() {
  const [view, setView] = useState<ViewId>('hosts');
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [storage, setStorage] = useState<{ ok: boolean; text: string }>({ ok: true, text: 'Checking database…' });
  const [terminals, setTerminals] = useState<TerminalSessionTab[]>([]);
  const [activeTerminal, setActiveTerminal] = useState<string | null>(null);
  const [statuses, setStatuses] = useState<Record<string, SessionStatus>>({});
  const [editRequest, setEditRequest] = useState<{ hostId: string; n: number } | null>(null);
  const [theme, setTheme] = useState<Theme>(storedTheme);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === 'system') delete root.dataset.theme;
    else root.dataset.theme = theme;
    try {
      window.localStorage.setItem(THEME_KEY, theme);
    } catch {
      // Not saved; the choice still applies until the app closes.
    }
  }, [theme]);

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

  function openTerminal(host: HostView) {
    nextTabNumber += 1;
    const key = `term-${nextTabNumber}`;
    // A tab for this host that is not connected (failed login, refused key, ended
    // session) is replaced in place instead of piling up a second tab.
    const stale = terminals.find(
      (t) => t.host.id === host.id && ['failed', 'closed', 'hostkey'].includes(statuses[t.key] ?? ''),
    );
    setTerminals((prev) =>
      stale ? prev.map((t) => (t.key === stale.key ? { key, host } : t)) : [...prev, { key, host }],
    );
    setActiveTerminal(key);
    setView('terminal');
  }

  function closeTerminal(key: string) {
    const next = terminals.filter((t) => t.key !== key);
    setTerminals(next);
    setStatuses((prev) => Object.fromEntries(Object.entries(prev).filter(([k]) => k !== key)));
    if (activeTerminal === key) {
      setActiveTerminal(next.at(-1)?.key ?? null);
    }
  }

  const onStatus = useCallback((key: string, status: SessionStatus) => {
    setStatuses((prev) => (prev[key] === status ? prev : { ...prev, [key]: status }));
  }, []);

  function editHost(hostId: string) {
    setEditRequest((prev) => ({ hostId, n: (prev?.n ?? 0) + 1 }));
    setView('hosts');
  }

  const counts: Partial<Record<ViewId, number>> = { terminal: terminals.length };

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">S</span>
          <span className="brand-text">
            <span className="brand-name">{info?.name ?? 'Selai'}</span>
            <br />
            <span className="brand-sub">SSH workspace with AI</span>
          </span>
        </div>
        <nav className="nav" role="tablist" aria-label="Main" aria-orientation="vertical">
          {NAV.map((group) => (
            <div key={group.section} role="presentation">
              <div className="nav-section" role="presentation">
                {group.section}
              </div>
              {group.items.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  role="tab"
                  id={`tab-${item.id}`}
                  className="nav-item"
                  aria-selected={view === item.id}
                  aria-controls={`panel-${item.id}`}
                  onClick={() => setView(item.id)}
                >
                  {item.icon}
                  <span className="label">{item.label}</span>
                  {counts[item.id] ? <span className="nav-count">{counts[item.id]}</span> : null}
                </button>
              ))}
            </div>
          ))}
        </nav>
        <div className="sidebar-foot">
          <span className="health" title={storage.text}>
            <span className="dot" data-status={storage.ok ? 'ok' : 'failed'} />
            <span>{storage.ok ? 'Local vault ready' : 'Database problem'}</span>
          </span>
          <span>{storage.text}</span>
          <span>{info ? `Version ${info.version}` : ''}</span>
          <button
            type="button"
            className="btn btn-ghost btn-sm theme-toggle"
            onClick={() => setTheme(THEMES[(THEMES.indexOf(theme) + 1) % THEMES.length] ?? 'dark')}
          >
            Theme: {theme === 'system' ? 'System' : theme === 'dark' ? 'Dark' : 'Light'}
          </button>
        </div>
      </aside>

      <main className="main">
        <div role="tabpanel" id="panel-hosts" aria-labelledby="tab-hosts" className="view" hidden={view !== 'hosts'}>
          <ConnectionsTab onConnect={openTerminal} editRequest={editRequest} />
        </div>
        <div role="tabpanel" id="panel-terminal" aria-labelledby="tab-terminal" className="view" hidden={view !== 'terminal'}>
          {(view === 'terminal' || terminals.length > 0) && (
            <Suspense fallback={<div className="empty"><span className="spinner spinner-lg" /></div>}>
              <TerminalTab
                tabs={terminals}
                statuses={statuses}
                activeKey={activeTerminal}
                visible={view === 'terminal'}
                onSelect={setActiveTerminal}
                onClose={closeTerminal}
                onStatus={onStatus}
                onNew={() => setView('hosts')}
                onEditHost={editHost}
                onNewLocal={() => {
                  localHost()
                    .then(openTerminal)
                    .catch(() => setView('hosts'));
                }}
              />
            </Suspense>
          )}
        </div>
        <div role="tabpanel" id="panel-agent" aria-labelledby="tab-agent" className="view" hidden={view !== 'agent'}>
          <AgentPanel onOpenProviders={() => setView('providers')} onOpenHosts={() => setView('hosts')} />
        </div>
        <div role="tabpanel" id="panel-providers" aria-labelledby="tab-providers" className="view" hidden={view !== 'providers'}>
          <ProvidersPanel />
        </div>
        <div role="tabpanel" id="panel-history" aria-labelledby="tab-history" className="view" hidden={view !== 'history'}>
          <HistoryTab visible={view === 'history'} />
        </div>
      </main>
    </div>
  );
}
