import { lazy, Suspense, useEffect, useState } from 'react';
import { AiTab } from './tabs/AiTab';
import { ConnectionsTab } from './tabs/ConnectionsTab';
import { appInfo, describeError, storageStatus, type AppInfo } from './ipc';

// xterm.js is large; load it only when the Terminal tab is opened.
const TerminalTab = lazy(() =>
  import('./tabs/TerminalTab').then((m) => ({ default: m.TerminalTab })),
);

type TabId = 'connections' | 'terminal' | 'ai';

const TABS: { id: TabId; label: string }[] = [
  { id: 'connections', label: 'Connections' },
  { id: 'terminal', label: 'Terminal' },
  { id: 'ai', label: 'AI' },
];

export function App() {
  const [tab, setTab] = useState<TabId>('connections');
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [storage, setStorage] = useState('checking local database…');

  useEffect(() => {
    let active = true;
    appInfo()
      .then((value) => active && setInfo(value))
      .catch(() => active && setInfo(null));
    storageStatus()
      .then((status) => {
        if (active) {
          setStorage(
            `database schema v${status.schemaVersion}, ${status.journalMode} mode, ` +
              `${status.interruptedTasksAtStartup} task(s) marked interrupted at startup`,
          );
        }
      })
      .catch((err: unknown) => active && setStorage(`database unavailable: ${describeError(err)}`));
    return () => {
      active = false;
    };
  }, []);

  return (
    <div className="app">
      <header className="app-header">
        <h1>{info?.name ?? 'Selai'}</h1>
        <nav role="tablist" aria-label="Main">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              id={`tab-${t.id}`}
              aria-selected={tab === t.id}
              aria-controls={`panel-${t.id}`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </header>

      <main role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`}>
        {tab === 'connections' && <ConnectionsTab />}
        {tab === 'terminal' && (
          <Suspense fallback={<p>Loading terminal…</p>}>
            <TerminalTab />
          </Suspense>
        )}
        {tab === 'ai' && <AiTab />}
      </main>

      <footer className="app-footer">
        <span>{info ? `v${info.version}` : ''}</span>
        <span>{storage}</span>
      </footer>
    </div>
  );
}
