import { useCallback, useEffect, useState } from 'react';
import {
  describeError,
  hostArchive,
  hostList,
  type HostView,
} from '../ipc';
import { HostForm } from './HostForm';
import { HostKeyPanel } from './HostKeyPanel';

interface Props {
  onConnect: (host: HostView) => void;
}

export function ConnectionsTab({ onConnect }: Props) {
  const [hosts, setHosts] = useState<HostView[]>([]);
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState<HostView | 'new' | null>(null);
  const [verifying, setVerifying] = useState<HostView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

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

  return (
    <section aria-label="Connections" className="connections">
      <div className="toolbar">
        <h2>Connections</h2>
        <input
          type="search"
          placeholder="Filter by label"
          aria-label="Filter hosts"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <button type="button" onClick={() => setEditing('new')}>
          Add host
        </button>
      </div>

      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}

      {editing && (
        <HostForm
          host={editing === 'new' ? null : editing}
          onCancel={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await refresh(query);
          }}
        />
      )}

      {verifying && <HostKeyPanel host={verifying} onDone={() => setVerifying(null)} />}

      {loaded && hosts.length === 0 && !error && (
        <p>No saved hosts yet. Add one to open a terminal.</p>
      )}

      <ul className="host-list">
        {hosts.map((host) => (
          <li key={host.id} className="host-row">
            <div className="host-main">
              <strong>{host.label}</strong>
              <span className="muted">
                {host.username}@{host.hostname}:{host.port} · {host.authMethod === 'password' ? 'password' : 'key'}
              </span>
              {!host.credentialConfigured && host.authMethod === 'password' && (
                <span className="badge">no password saved</span>
              )}
            </div>
            <div className="host-actions">
              <button type="button" onClick={() => onConnect(host)}>
                Connect
              </button>
              <button type="button" onClick={() => setVerifying(host)}>
                Host key
              </button>
              <button type="button" onClick={() => setEditing(host)}>
                Edit
              </button>
              <button type="button" onClick={() => void archive(host)}>
                Archive
              </button>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
