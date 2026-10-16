import type { HostView } from '../ipc';
import { CloseIcon, PlusIcon, TerminalIcon } from '../ui/icons';
import { TerminalPane } from './TerminalPane';

export type SessionStatus = 'connecting' | 'connected' | 'hostkey' | 'failed' | 'closed';

export interface TerminalSessionTab {
  key: string;
  host: HostView;
}

interface Props {
  tabs: TerminalSessionTab[];
  statuses: Record<string, SessionStatus>;
  activeKey: string | null;
  visible: boolean;
  onSelect: (key: string) => void;
  onClose: (key: string) => void;
  onStatus: (key: string, status: SessionStatus) => void;
  onNew: () => void;
  onEditHost: (hostId: string) => void;
}

export function TerminalTab({ tabs, statuses, activeKey, visible, onSelect, onClose, onStatus, onNew, onEditHost }: Props) {
  return (
    <section aria-label="Terminal" className="term-view" hidden={!visible}>
      {tabs.length === 0 ? (
        <div className="empty term-empty">
          <span className="empty-icon">
            <TerminalIcon size={26} />
          </span>
          <h2>No open sessions</h2>
          <p>Pick a host to open an SSH terminal. Sessions stay open while you switch views.</p>
          <button type="button" className="btn btn-primary" onClick={onNew}>
            Choose a host
          </button>
        </div>
      ) : (
        <div role="tablist" aria-label="Open terminals" className="term-tabs">
          {tabs.map((tab) => (
            <span key={tab.key} className="term-tab" data-active={tab.key === activeKey}>
              <button
                type="button"
                role="tab"
                aria-selected={tab.key === activeKey}
                onClick={() => onSelect(tab.key)}
                title={`${tab.host.username}@${tab.host.hostname}`}
              >
                <span className="dot" data-status={statuses[tab.key] ?? 'connecting'} />
                <span>{tab.host.label}</span>
              </button>
              <button
                type="button"
                className="icon-btn"
                aria-label={`Close ${tab.host.label}`}
                onClick={() => onClose(tab.key)}
              >
                <CloseIcon size={13} />
              </button>
            </span>
          ))}
          <button type="button" className="icon-btn add" aria-label="New session" title="New session" onClick={onNew}>
            <PlusIcon />
          </button>
        </div>
      )}
      {tabs.length > 0 && (
        <div className="term-stack">
          {/* Panes stay mounted while hidden so their sessions survive tab changes. */}
          {tabs.map((tab) => (
            <TerminalPane
              key={tab.key}
              host={tab.host}
              hidden={!visible || tab.key !== activeKey}
              onStatus={(status) => onStatus(tab.key, status)}
              onClose={() => onClose(tab.key)}
              onEditHost={() => onEditHost(tab.host.id)}
            />
          ))}
        </div>
      )}
    </section>
  );
}
