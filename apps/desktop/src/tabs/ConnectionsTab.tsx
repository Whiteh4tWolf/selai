import { useCallback, useEffect, useState } from 'react';
import { describeError, hostArchive, hostList, localHost, type HostView } from '../ipc';
import { HostAvatar } from '../ui/HostAvatar';
import { ArchiveIcon, EditIcon, KeyIcon, LockIcon, PlusIcon, SearchIcon, ServerIcon, TerminalIcon } from '../ui/icons';
import { HostForm } from './HostForm';

interface Props {
  onConnect: (host: HostView) => void;
  /** Another view asked to edit this host, for example after a failed login. */
  editRequest?: { hostId: string; n: number } | null;
}

export function ConnectionsTab({ onConnect, editRequest }: Props) {
  const [hosts, setHosts] = useState<HostView[]>([]);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState<HostView | 'new' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [local, setLocal] = useState<HostView | null>(null);

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
      return page.items;
    } catch (err) {
      setError(describeError(err));
      return [];
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
      await refresh(query);
    } catch (err) {
      setError(describeError(err));
    }
  }

  const empty = loaded && hosts.length === 0 && !error;

  return (
    <section aria-label="Hosts" className="page">
      <header className="page-head">
        <div className="grow">
          <h1>Hosts</h1>
          <p>Saved SSH servers. Click a host to open a terminal.</p>
        </div>
        <label className="search">
          <SearchIcon />
          <input
            type="search"
            placeholder="Search hosts"
            aria-label="Filter hosts"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <button type="button" className="btn btn-primary" onClick={() => setEditing('new')}>
          <PlusIcon /> New host
        </button>
      </header>

      {error && (
        <p role="alert" className="banner" data-tone="danger">
          {error}
        </p>
      )}

      {empty && query === '' && (
        <div className="empty">
          <span className="empty-icon">
            <ServerIcon size={26} />
          </span>
          <h2>No hosts yet</h2>
          <p>Add your first SSH server. You will confirm its key the first time you connect.</p>
          <button type="button" className="btn btn-primary" onClick={() => setEditing('new')}>
            <PlusIcon /> New host
          </button>
        </div>
      )}
      {empty && query !== '' && <p className="muted">No host matches “{query}”.</p>}

      {local && (query === '' || local.label.toLowerCase().includes(query.toLowerCase()) || 'local'.includes(query.toLowerCase())) && (
        <>
          <h2 className="section-title">This computer</h2>
          <div className="host-grid" style={{ marginBottom: '1.4rem' }}>
            <div className="host-card">
              <button
                type="button"
                className="host-open"
                aria-label={`Open a terminal on ${local.label}`}
                onClick={() => onConnect(local)}
              />
              <span className="avatar local-avatar">
                <TerminalIcon size={18} />
              </span>
              <div className="host-text">
                <strong>{local.label}</strong>
                <span>Local shell · no SSH</span>
                <div className="host-tags">
                  <span className="badge" data-tone="accent">
                    Local
                  </span>
                </div>
              </div>
            </div>
          </div>
        </>
      )}

      {hosts.length > 0 && <h2 className="section-title">SSH hosts · {hosts.length}</h2>}
      <ul className="host-grid" style={{ listStyle: 'none', margin: 0, padding: 0 }}>
        {hosts.map((host) => (
          <li key={host.id} className="host-card">
            <button
              type="button"
              className="host-open"
              aria-label={`Connect to ${host.label}`}
              onClick={() => onConnect(host)}
            />
            <HostAvatar label={host.label} />
            <div className="host-text">
              <strong>{host.label}</strong>
              <span>
                {host.username}@{host.hostname}
                {host.port !== 22 ? `:${host.port}` : ''}
              </span>
              <div className="host-tags">
                <span className="badge">
                  {host.authMethod === 'password' ? <LockIcon size={11} /> : <KeyIcon size={11} />}
                  {host.authMethod === 'password' ? 'Password' : 'Key'}
                </span>
                {!host.credentialConfigured && host.authMethod === 'password' && (
                  <span className="badge" data-tone="warning">
                    No password saved
                  </span>
                )}
              </div>
            </div>
            <div className="host-tools">
              <button type="button" className="icon-btn" aria-label={`Open terminal on ${host.label}`} title="Connect" onClick={() => onConnect(host)}>
                <TerminalIcon />
              </button>
              <button type="button" className="icon-btn" aria-label={`Edit ${host.label}`} title="Edit" onClick={() => setEditing(host)}>
                <EditIcon />
              </button>
              <button type="button" className="icon-btn" aria-label={`Archive ${host.label}`} title="Archive" onClick={() => void archive(host)}>
                <ArchiveIcon />
              </button>
            </div>
          </li>
        ))}
      </ul>

      {editing && (
        <HostForm
          host={editing === 'new' ? null : editing}
          onCancel={() => setEditing(null)}
          onSaved={async (saved, connect) => {
            setEditing(null);
            await refresh(query);
            if (connect) onConnect(saved);
          }}
        />
      )}
    </section>
  );
}
