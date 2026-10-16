import { lazy, Suspense, useEffect, useState } from 'react';
import { AiTab } from './tabs/AiTab';
import { HistoryTab } from './tabs/HistoryTab';
import { ConnectionsTab } from './tabs/ConnectionsTab';
import { appInfo, describeError, storageStatus, type AppInfo, type HostView } from './ipc';
import type { TerminalSessionTab } from './tabs/TerminalTab';

// xterm.js is large; load it only when a terminal is first needed.
const TerminalTab = lazy(() => import('./tabs/TerminalTab').then((m) => ({ default: m.TerminalTab })));

type TabId = 'connections' | 'terminal' | 'ai' | 'history';

const TABS: { id: TabId; label: string }[] = [
  { id: 'connections', label: 'Connections' },
  { id: 'terminal', label: 'Terminal' },
  { id: 'ai', label: 'AI' },
  { id: 'history', label: 'AI history' },
];

let nextTabNumber = 0;

export function App() {
  const [tab, setTab] = useState<TabId>('connections');
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [storage, setStorage] = useState('checking local database…');
  const [terminals, setTerminals] = useState<TerminalSessionTab[]>([]);
  const [activeTerminal, setActiveTerminal] = useState<string | null>(null);

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

  function openTerminal(host: HostView) {
    nextTabNumber += 1;
    const key = `term-${nextTabNumber}`;
    setTerminals((prev) => [...prev, { key, hostId: host.id, label: host.label }]);
    setActiveTerminal(key);
    setTab('terminal');
  }

  function closeTerminal(key: string) {
    const next = terminals.filter((t) => t.key !== key);
    setTerminals(next);
    if (activeTerminal === key) {
      setActiveTerminal(next.at(-1)?.key ?? null);
    }
  }

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

      <main>
        <div role="tabpanel" id="panel-connections" aria-labelledby="tab-connections" hidden={tab !== 'connections'}>
          <ConnectionsTab onConnect={openTerminal} />
        </div>
        <div role="tabpanel" id="panel-terminal" aria-labelledby="tab-terminal" hidden={tab !== 'terminal'}>
          {(tab === 'terminal' || terminals.length > 0) && (
            <Suspense fallback={<p>Loading terminal…</p>}>
              <TerminalTab
                tabs={terminals}
                activeKey={activeTerminal}
                visible={tab === 'terminal'}
                onSelect={setActiveTerminal}
                onClose={closeTerminal}
              />
            </Suspense>
          )}
        </div>
        <div role="tabpanel" id="panel-ai" aria-labelledby="tab-ai" hidden={tab !== 'ai'}>
          <AiTab />
        </div>
        <div role="tabpanel" id="panel-history" aria-labelledby="tab-history" hidden={tab !== 'history'}>
          <HistoryTab visible={tab === 'history'} />
        </div>
      </main>

      <footer className="app-footer">
        <span>{info ? `v${info.version}` : ''}</span>
        <span>{storage}</span>
      </footer>
    </div>
  );
}
