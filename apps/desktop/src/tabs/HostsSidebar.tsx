import { useCallback, useEffect, useState } from 'react';
import { describeError, HOSTS_CHANGED, hostArchive, hostList, localHost, type HostView } from '../ipc';
import { HostAvatar } from '../ui/HostAvatar';
import { ArchiveIcon, ChevronIcon, EditIcon, PlusIcon, RefreshIcon, SearchIcon, AssistantIcon, TerminalIcon } from '../ui/icons';
import { HostForm } from './HostForm';
import type { SessionStatus } from './TerminalTab';

interface Props {
  onConnect: (host: HostView) => void;
  /** Makes this host the chat's server. */
  onAskAi?: (host: HostView) => void;
  /** Status of the open terminal for each host, if any. */
  sessions?: Record<string, SessionStatus>;
  /** Another view asked to edit this host, for example after a failed login. */
  editRequest?: { hostId: string; n: number } | null;
}

/** The host explorer in the left sidebar: this computer and the saved SSH hosts. */
export function HostsSidebar({ onConnect, onAskAi, sessions = {}, editRequest }: Props) {
  const [hosts, setHosts] = useState<HostView[]>([]);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState<HostView | 'new' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [local, setLocal] = useState<HostView | null>(null);
  const [open, setOpen] = useState({ local: true, ssh: true });

  useEffect(() => {
    let active = true;
    localHost()
      .then((h) => active && setLocal(h))
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);

  const refresh = useCallback(async (filter: string) => {
    try {
      const page = await hostList(filter || undefined);
      setHosts(page.items);
      setError(null);
    } catch (err) {
      setError(describeError(err));
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refresh(query);
  }, [query, refresh]);

  useEffect(() => {
    if (!editRequest) return;
    let active = true;
    // Look the host up without the current filter, so it is found even when hidden.
    hostList()
      .then((page) => {
        const host = page.items.find((h) => h.id === editRequest.hostId);
        if (active && host) setEditing(host);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [editRequest]);

  async function archive(host: HostView) {
    if (!window.confirm(`Archive ${host.label}? Its history is kept; saved passwords are removed.`)) {
      return;
    }
    try {
      await hostArchive(host.id);
      window.dispatchEvent(new Event(HOSTS_CHANGED));
      await refresh(query);
    } catch (err) {
      setError(describeError(err));
    }
  }

  const q = query.trim().toLowerCase();
  const showLocal = local !== null && (q === '' || local.label.toLowerCase().includes(q) || 'local'.includes(q));

  return (
    <section aria-label="Hosts" className="side-panel">
      <header className="panel-head">
        <h2>Hosts</h2>
        <span className="grow" />
        <button type="button" className="icon-btn" aria-label="New host" title="New host" onClick={() => setEditing('new')}>
          <PlusIcon size={15} />
        </button>
        <button type="button" className="icon-btn" aria-label="Refresh hosts" title="Refresh" onClick={() => void refresh(query)}>
          <RefreshIcon size={14} />
        </button>
      </header>
      <label className="side-search">
        <SearchIcon size={13} />
        <input
          type="search"
          placeholder="Filter hosts"
          aria-label="Filter hosts"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>

      {error && (
        <p role="alert" className="side-error">
          {error}
        </p>
      )}

      <div className="tree">
        {showLocal && local && (
          <>
            <button
              type="button"
              className="tree-group"
              aria-expanded={open.local}
              onClick={() => setOpen((o) => ({ ...o, local: !o.local }))}
            >
              <ChevronIcon size={12} />
              This computer
            </button>
            {open.local && (
              <ul className="tree-list">
                <HostRow
                  host={local}
                  sub="local shell"
                  status={sessions[local.id]}
                  onConnect={onConnect}
                  onAskAi={onAskAi}
                />
              </ul>
            )}
          </>
        )}

        <button
          type="button"
          className="tree-group"
          aria-expanded={open.ssh}
          onClick={() => setOpen((o) => ({ ...o, ssh: !o.ssh }))}
        >
          <ChevronIcon size={12} />
          SSH hosts
          {hosts.length > 0 && <span className="tree-count">{hosts.length}</span>}
        </button>
        {open.ssh && (
          <ul className="tree-list">
            {hosts.map((host) => (
              <HostRow
                key={host.id}
                host={host}
                sub={`${host.username}@${host.hostname}${host.port !== 22 ? `:${host.port}` : ''}`}
                warn={!host.credentialConfigured && host.authMethod === 'password' ? 'No password saved' : undefined}
                status={sessions[host.id]}
                onConnect={onConnect}
                onAskAi={onAskAi}
                onEdit={() => setEditing(host)}
                onArchive={() => void archive(host)}
              />
            ))}
          </ul>
        )}
        {loaded && hosts.length === 0 && !error && open.ssh && (
          <div className="tree-empty">
            {q ? (
              <p>No host matches “{query}”.</p>
            ) : (
              <>
                <p>No SSH hosts yet.</p>
                <button type="button" className="btn btn-sm" onClick={() => setEditing('new')}>
                  <PlusIcon size={13} /> New host
                </button>
              </>
            )}
          </div>
        )}
      </div>

      {editing && (
        <HostForm
          host={editing === 'new' ? null : editing}
          onCancel={() => setEditing(null)}
          onSaved={async (saved, connect) => {
            setEditing(null);
            window.dispatchEvent(new Event(HOSTS_CHANGED));
            await refresh(query);
            if (connect) onConnect(saved);
          }}
        />
      )}
    </section>
  );
}

function HostRow({
  host,
  sub,
  warn,
  status,
  onConnect,
  onAskAi,
  onEdit,
  onArchive,
}: {
  host: HostView;
  sub: string;
  warn?: string;
  status?: SessionStatus;
  onConnect: (host: HostView) => void;
  onAskAi?: (host: HostView) => void;
  onEdit?: () => void;
  onArchive?: () => void;
}) {
  return (
    <li className="tree-row" title={warn ? `${sub} · ${warn}` : sub}>
      <button
        type="button"
        className="tree-open"
        aria-label={`Connect to ${host.label}`}
        onClick={() => onConnect(host)}
      />
      {host.kind === 'local' ? (
        <span className="avatar local-avatar">
          <TerminalIcon size={12} />
        </span>
      ) : (
        <HostAvatar label={host.label} size={12} />
      )}
      <span className="tree-text">
        <span className="tree-label">{host.label}</span>
        <span className="tree-sub">{sub}</span>
      </span>
      {warn && <span className="dot" data-status="hostkey" aria-label={warn} />}
      {status && <span className="dot" data-status={status === 'connected' ? 'ok' : status} aria-label={`Terminal ${status}`} />}
      <span className="tree-tools">
        {onAskAi && (
          <button type="button" className="icon-btn" aria-label={`Use ${host.label} in chat`} title="Use in chat" onClick={() => onAskAi(host)}>
            <AssistantIcon size={13} />
          </button>
        )}
        {onEdit && (
          <button type="button" className="icon-btn" aria-label={`Edit ${host.label}`} title="Edit" onClick={onEdit}>
            <EditIcon size={13} />
          </button>
        )}
        {onArchive && (
          <button type="button" className="icon-btn" aria-label={`Archive ${host.label}`} title="Archive" onClick={onArchive}>
            <ArchiveIcon size={13} />
          </button>
        )}
      </span>
    </li>
  );
}
