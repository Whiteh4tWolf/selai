import { useState, type FormEvent } from 'react';
import { describeError, hostSave, type AuthMethod, type HostInput, type HostView } from '../ipc';

interface Props {
  host: HostView | null;
  onCancel: () => void;
  onSaved: () => void;
}

interface FormState {
  label: string;
  hostname: string;
  port: string;
  username: string;
  authMethod: AuthMethod;
  privateKeyPath: string;
  connectTimeoutSeconds: string;
  keepAliveSeconds: string;
  terminalType: string;
  notes: string;
}

function initial(host: HostView | null): FormState {
  return {
    label: host?.label ?? '',
    hostname: host?.hostname ?? '',
    port: String(host?.port ?? 22),
    username: host?.username ?? '',
    authMethod: host?.authMethod ?? 'password',
    privateKeyPath: host?.privateKeyPath ?? '',
    connectTimeoutSeconds: String(host?.connectTimeoutSeconds ?? 15),
    keepAliveSeconds: String(host?.keepAliveSeconds ?? 30),
    terminalType: host?.terminalType ?? 'xterm-256color',
    notes: host?.notes ?? '',
  };
}

/** Client-side checks for quick feedback. The Rust core validates again. */
function validate(form: FormState): string | null {
  if (!form.label.trim()) return 'Label is required.';
  if (!form.hostname.trim()) return 'Host name or IP is required.';
  if (!form.username.trim()) return 'Username is required.';
  const port = Number(form.port);
  if (!Number.isInteger(port) || port < 1 || port > 65535) return 'Port must be 1 to 65535.';
  if (form.authMethod === 'private_key' && !form.privateKeyPath.trim()) {
    return 'Private key path is required for key authentication.';
  }
  return null;
}

export function HostForm({ host, onCancel, onSaved }: Props) {
  const [form, setForm] = useState<FormState>(() => initial(host));
  // Secrets live only in this component's local state until submit. They are
  // cleared right after a save and never prefilled from the host record.
  const [password, setPassword] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const set = (key: keyof FormState) => (value: string) =>
    setForm((prev) => ({ ...prev, [key]: value }));

  async function submit(event: FormEvent) {
    event.preventDefault();
    const problem = validate(form);
    if (problem) {
      setError(problem);
      return;
    }
    const input: HostInput = {
      id: host?.id,
      expectedUpdatedAtMs: host?.updatedAtMs,
      label: form.label.trim(),
      groupId: host?.groupId ?? null,
      hostname: form.hostname.trim(),
      port: Number(form.port),
      username: form.username.trim(),
      authMethod: form.authMethod,
      privateKeyPath: form.authMethod === 'private_key' ? form.privateKeyPath.trim() : null,
      connectTimeoutSeconds: Number(form.connectTimeoutSeconds),
      keepAliveSeconds: Number(form.keepAliveSeconds),
      terminalType: form.terminalType.trim(),
      notes: form.notes.trim() ? form.notes : null,
      password: form.authMethod === 'password' && password ? password : undefined,
      keyPassphrase: form.authMethod === 'private_key' && passphrase ? passphrase : undefined,
    };
    setBusy(true);
    try {
      await hostSave(input);
      setPassword('');
      setPassphrase('');
      onSaved();
    } catch (err) {
      setError(describeError(err));
    } finally {
      setBusy(false);
    }
  }

  const savedPlaceholder = host?.credentialConfigured ? '•••• saved (leave blank to keep)' : '';

  return (
    <form className="host-form" aria-label={host ? 'Edit host' : 'New host'} onSubmit={submit}>
      <h3>{host ? `Edit ${host.label}` : 'New host'}</h3>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}
      <label>
        Label
        <input value={form.label} onChange={(e) => set('label')(e.target.value)} />
      </label>
      <label>
        Host name or IP
        <input value={form.hostname} onChange={(e) => set('hostname')(e.target.value)} />
      </label>
      <label>
        Port
        <input inputMode="numeric" value={form.port} onChange={(e) => set('port')(e.target.value)} />
      </label>
      <label>
        Username
        <input value={form.username} onChange={(e) => set('username')(e.target.value)} />
      </label>
      <label>
        Authentication
        <select
          value={form.authMethod}
          onChange={(e) => set('authMethod')(e.target.value)}
        >
          <option value="password">Password</option>
          <option value="private_key">Private key file</option>
        </select>
      </label>
      {form.authMethod === 'password' ? (
        <label>
          Password
          <input
            type="password"
            autoComplete="off"
            value={password}
            placeholder={savedPlaceholder}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
      ) : (
        <>
          <label>
            Private key file path
            <input value={form.privateKeyPath} onChange={(e) => set('privateKeyPath')(e.target.value)} />
          </label>
          <label>
            Key passphrase (optional)
            <input
              type="password"
              autoComplete="off"
              value={passphrase}
              placeholder={savedPlaceholder}
              onChange={(e) => setPassphrase(e.target.value)}
            />
          </label>
        </>
      )}
      <label>
        Connect timeout (seconds)
        <input value={form.connectTimeoutSeconds} onChange={(e) => set('connectTimeoutSeconds')(e.target.value)} />
      </label>
      <label>
        Keepalive (seconds, 0 disables)
        <input value={form.keepAliveSeconds} onChange={(e) => set('keepAliveSeconds')(e.target.value)} />
      </label>
      <label>
        Terminal type
        <input value={form.terminalType} onChange={(e) => set('terminalType')(e.target.value)} />
      </label>
      <label>
        Notes
        <textarea value={form.notes} onChange={(e) => set('notes')(e.target.value)} />
      </label>
      <div className="form-actions">
        <button type="submit" disabled={busy}>
          {busy ? 'Saving…' : 'Save'}
        </button>
        <button type="button" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
