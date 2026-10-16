import { useState, type FormEvent } from 'react';
import { describeError, hostSave, type AuthMethod, type HostInput, type HostView } from '../ipc';
import { Field } from '../ui/Field';
import { Drawer } from '../ui/Overlay';
import { ChevronIcon } from '../ui/icons';

interface Props {
  host: HostView | null;
  onCancel: () => void;
  /** `connect` is true when the operator chose "Save & connect". */
  onSaved: (host: HostView, connect: boolean) => void;
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

  async function save(connect: boolean) {
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
      const saved = await hostSave(input);
      setPassword('');
      setPassphrase('');
      onSaved(saved, connect);
    } catch (err) {
      setError(describeError(err));
    } finally {
      setBusy(false);
    }
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    void save(false);
  }

  const savedPlaceholder = host?.credentialConfigured ? '•••• saved (leave blank to keep)' : '';
  const formId = 'host-form';

  return (
    <Drawer
      title={host ? `Edit ${host.label}` : 'New host'}
      subtitle={host ? `${host.username}@${host.hostname}` : 'The server key is confirmed on first connect.'}
      onClose={onCancel}
      footer={
        <>
          <button type="button" className="btn btn-ghost" onClick={onCancel}>
            Cancel
          </button>
          <span className="spacer" />
          <button type="submit" form={formId} className="btn" disabled={busy}>
            {busy ? 'Saving…' : 'Save'}
          </button>
          <button type="button" className="btn btn-primary" disabled={busy} onClick={() => void save(true)}>
            Save & connect
          </button>
        </>
      }
    >
      <form id={formId} aria-label={host ? 'Edit host' : 'New host'} onSubmit={submit}>
        {error && (
          <p role="alert" className="banner" data-tone="danger" style={{ marginBottom: '1rem' }}>
            {error}
          </p>
        )}
        <div className="form-section">
          <h3>Server</h3>
          <Field label="Label" hint="Spaces are fine. Mention it as @Label in the AI Agent.">
            <input
              value={form.label}
              placeholder="Production Web"
              onChange={(e) => set('label')(e.target.value)}
              autoFocus
            />
          </Field>
          <div className="field-row">
            <label className="field">
              Host name or IP
              <input value={form.hostname} placeholder="10.0.0.5" onChange={(e) => set('hostname')(e.target.value)} />
            </label>
            <label className="field">
              Port
              <input inputMode="numeric" value={form.port} onChange={(e) => set('port')(e.target.value)} />
            </label>
          </div>
        </div>

        <div className="form-section">
          <h3>Credentials</h3>
          <label className="field">
            Username
            <input value={form.username} placeholder="root" onChange={(e) => set('username')(e.target.value)} />
          </label>
          <div className="field">
            <span id="auth-method">Authentication</span>
            <div className="segmented" role="group" aria-labelledby="auth-method">
              <button
                type="button"
                aria-pressed={form.authMethod === 'password'}
                onClick={() => set('authMethod')('password')}
              >
                Password
              </button>
              <button
                type="button"
                aria-pressed={form.authMethod === 'private_key'}
                onClick={() => set('authMethod')('private_key')}
              >
                Private key
              </button>
            </div>
          </div>
          {form.authMethod === 'password' ? (
            <Field label="Password" hint="Stored in the OS keychain, never in the database.">
              <input
                type="password"
                autoComplete="off"
                value={password}
                placeholder={savedPlaceholder}
                onChange={(e) => setPassword(e.target.value)}
              />
            </Field>
          ) : (
            <>
              <label className="field">
                Private key file path
                <input
                  value={form.privateKeyPath}
                  placeholder="~/.ssh/id_ed25519"
                  onChange={(e) => set('privateKeyPath')(e.target.value)}
                />
              </label>
              <label className="field">
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
        </div>

        <div className="form-section">
          <details className="advanced">
            <summary>
              <ChevronIcon size={14} /> Advanced
            </summary>
            <div className="form-section">
              <div className="field-grid">
                <label className="field">
                  Connect timeout (seconds)
                  <input
                    value={form.connectTimeoutSeconds}
                    onChange={(e) => set('connectTimeoutSeconds')(e.target.value)}
                  />
                </label>
                <label className="field">
                  Keepalive (seconds, 0 disables)
                  <input value={form.keepAliveSeconds} onChange={(e) => set('keepAliveSeconds')(e.target.value)} />
                </label>
              </div>
              <label className="field">
                Terminal type
                <input value={form.terminalType} onChange={(e) => set('terminalType')(e.target.value)} />
              </label>
              <label className="field">
                Notes
                <textarea rows={3} value={form.notes} onChange={(e) => set('notes')(e.target.value)} />
              </label>
            </div>
          </details>
        </div>
      </form>
    </Drawer>
  );
}
