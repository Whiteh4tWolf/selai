import type { HostView } from '../ipc';
import { TerminalPane } from './TerminalPane';

export type SessionStatus = 'connecting' | 'connected' | 'hostkey' | 'failed' | 'closed';

export interface TerminalSessionTab {
  key: string;
  host: HostView;
}

interface Props {
  tabs: TerminalSessionTab[];
  activeKey: string | null;
  visible: boolean;
  onClose: (key: string) => void;
  onStatus: (key: string, status: SessionStatus) => void;
  onEditHost: (hostId: string) => void;
}

/** The open terminals. Panes stay mounted while hidden so their sessions survive tab changes. */
export function TerminalStack({ tabs, activeKey, visible, onClose, onStatus, onEditHost }: Props) {
  return (
    <div className="term-stack" hidden={!visible}>
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
  );
}
