import { useState } from 'react';
import { describeError, hostProbe, hostTrust, type HostView, type ProbeView } from '../ipc';

interface Props {
  host: HostView;
  onDone: () => void;
}

/**
 * Shows the server key for a host and lets the user trust it. Trusting uses the
 * key from the last probe in Rust, so the webview never supplies key material.
 */
export function HostKeyPanel({ host, onDone }: Props) {
  const [probe, setProbe] = useState<ProbeView | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);

  async function check() {
    setBusy(true);
    setError(null);
    setMessage(null);
    setConfirmed(false);
    try {
      setProbe(await hostProbe(host.id));
    } catch (err) {
      setError(describeError(err));
    } finally {
      setBusy(false);
    }
  }

  async function trust(mode: 'first' | 'replace') {
    if (!probe) return;
    setBusy(true);
    setError(null);
    try {
      const result = await hostTrust(host.id, probe.observed.fingerprintSha256, mode);
      setMessage(
        result.outcome === 'pinned'
          ? 'Key trusted and saved.'
          : result.outcome === 'rotated'
            ? 'Key replaced. The previous key is kept in history.'
            : 'The key was already trusted.',
      );
      setProbe(null);
    } catch (err) {
      setError(describeError(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="host-key" aria-label={`Host key for ${host.label}`}>
      <h3>Host key: {host.label}</h3>
      <p className="muted">
        {host.hostname}:{host.port}
      </p>
      {!probe && (
        <button type="button" onClick={() => void check()} disabled={busy}>
          {busy ? 'Connecting…' : 'Read server key'}
        </button>
      )}
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      {message && <p role="status">{message}</p>}

      {probe && probe.status === 'trusted' && (
        <p role="status">The server key matches the trusted key.</p>
      )}

      {probe && probe.status === 'unknown' && (
        <>
          <p>
            First contact. Compare this fingerprint with the server before trusting it:
          </p>
          <p>
            <code>{probe.observed.algorithm}</code> <code>{probe.observed.fingerprintSha256}</code>
          </p>
          <button type="button" onClick={() => void trust('first')} disabled={busy}>
            Trust and save
          </button>
        </>
      )}

      {probe && probe.status === 'changed' && (
        <>
          <p className="warning" role="alert">
            The server presented a different key. This can mean the server was reinstalled, or
            that someone is intercepting the connection. Confirm the new fingerprint through
            another channel before replacing the trusted key.
          </p>
          <p>
            Trusted: <code>{probe.pinnedFingerprintSha256}</code>
          </p>
          <p>
            Presented: <code>{probe.observed.algorithm}</code>{' '}
            <code>{probe.observed.fingerprintSha256}</code>
          </p>
          <label className="confirm">
            <input type="checkbox" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />
            I verified the new fingerprint out of band.
          </label>
          <button type="button" onClick={() => void trust('replace')} disabled={!confirmed || busy}>
            Replace trusted key
          </button>
        </>
      )}

      <div className="form-actions">
        <button type="button" onClick={onDone}>
          Close
        </button>
      </div>
    </section>
  );
}
