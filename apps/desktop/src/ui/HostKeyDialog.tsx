import { useState } from 'react';
import { describeError, hostTrust, type HostKeyProblem } from '../ipc';
import { Dialog } from './Overlay';
import { ShieldAlertIcon, ShieldIcon } from './icons';

interface Props {
  hostId: string;
  hostLabel: string;
  address: string;
  problem: HostKeyProblem;
  /** Called after Rust pinned the key; the caller then connects again. */
  onTrusted: () => void;
  onCancel: () => void;
  contained?: boolean;
}

/**
 * Asks the operator to approve the server key a refused connect observed. The
 * fingerprint comes from the Rust error, and Rust pins only the key it observed
 * itself, so the webview never supplies key material.
 */
export function HostKeyDialog({ hostId, hostLabel, address, problem, onTrusted, onCancel, contained }: Props) {
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const changed = problem.kind === 'changed';
  const { details } = problem;

  async function trust() {
    setBusy(true);
    setError(null);
    try {
      await hostTrust(hostId, details.fingerprintSha256, changed ? 'replace' : 'first');
      onTrusted();
    } catch (err) {
      setError(describeError(err));
      setBusy(false);
    }
  }

  return (
    <Dialog
      contained={contained}
      title={changed ? 'Server key has changed' : 'Verify the server'}
      tone={changed ? 'danger' : 'default'}
      icon={changed ? <ShieldAlertIcon size={20} /> : <ShieldIcon size={20} />}
      onClose={onCancel}
      footer={
        <>
          <button type="button" className="btn btn-ghost" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className={changed ? 'btn btn-danger' : 'btn btn-primary'}
            disabled={busy || (changed && !confirmed)}
            onClick={() => void trust()}
          >
            {busy ? 'Saving…' : changed ? 'Replace key & connect' : 'Trust & connect'}
          </button>
        </>
      }
    >
      {changed ? (
        <p>
          <strong>{hostLabel}</strong> ({address}) presented a different key than the one you trusted.
          The server may have been reinstalled, or someone may be intercepting the connection. No
          credential was sent.
        </p>
      ) : (
        <p>
          This is the first connection to <strong>{hostLabel}</strong> ({address}). Compare the
          fingerprint with the one your server administrator gave you before you trust it. No
          credential was sent yet.
        </p>
      )}
      <dl className="fingerprint">
        <dt>Algorithm</dt>
        <dd>{details.algorithm}</dd>
        {changed && details.pinnedFingerprintSha256 && (
          <>
            <dt>Trusted</dt>
            <dd className="old">{details.pinnedFingerprintSha256}</dd>
          </>
        )}
        <dt>{changed ? 'Presented' : 'Fingerprint'}</dt>
        <dd>{details.fingerprintSha256}</dd>
      </dl>
      {changed && (
        <label className="check">
          <input type="checkbox" checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />
          <span>I verified the new fingerprint through another channel.</span>
        </label>
      )}
      {error && (
        <p role="alert" className="banner" data-tone="danger">
          {error}
        </p>
      )}
    </Dialog>
  );
}
