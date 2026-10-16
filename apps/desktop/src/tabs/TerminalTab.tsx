import { TerminalPane } from './TerminalPane';

export interface TerminalSessionTab {
  key: string;
  hostId: string;
  label: string;
}

interface Props {
  tabs: TerminalSessionTab[];
  activeKey: string | null;
  visible: boolean;
  onSelect: (key: string) => void;
  onClose: (key: string) => void;
}

export function TerminalTab({ tabs, activeKey, visible, onSelect, onClose }: Props) {
  return (
    <section aria-label="Terminal" hidden={!visible}>
      {tabs.length === 0 ? (
        <p>No open terminals. Choose Connect on a host in Connections.</p>
      ) : (
        <div role="tablist" aria-label="Open terminals" className="session-tabs">
          {tabs.map((tab) => (
            <span key={tab.key} className="session-tab" data-active={tab.key === activeKey}>
              <button
                type="button"
                role="tab"
                aria-selected={tab.key === activeKey}
                onClick={() => onSelect(tab.key)}
              >
                {tab.label}
              </button>
              <button
                type="button"
                aria-label={`Close ${tab.label}`}
                onClick={() => onClose(tab.key)}
              >
                ×
              </button>
            </span>
          ))}
        </div>
      )}
      {/* Panes stay mounted while hidden so their sessions survive tab changes. */}
      {tabs.map((tab) => (
        <TerminalPane
          key={tab.key}
          hostId={tab.hostId}
          label={tab.label}
          hidden={!visible || tab.key !== activeKey}
        />
      ))}
    </section>
  );
}
