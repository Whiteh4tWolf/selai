import { useEffect, useState, type FormEvent } from 'react';
import {
  describeError,
  hostForgetSudoPassword,
  hostSave,
  sshKeyDelete,
  sshKeyGenerate,
  sshKeyImport,
  sshKeyList,
  type AuthMethod,
  type HostInput,
  type HostView,
  type SshKeyView,
} from '../ipc';
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
  const [sudoNote, setSudoNote] = useState<string | null>(null);
  const [keys, setKeys] = useState<SshKeyView[]>([]);
  const [manageKeys, setManageKeys] = useState(false);
  const [keyName, setKeyName] = useState('');
  const [keyPass, setKeyPass] = useState('');
  const [keyError, setKeyError] = useState<string | null>(null);
  const [keyBusy, setKeyBusy] = useState(false);

  async function refreshKeys() { setKeys(await sshKeyList()); }
  useEffect(() => { void refreshKeys().catch((err) => setKeyError(describeError(err))); }, []);

  async function generateKey(algorithm: string) {
    if (!keyName.trim()) { setKeyError('Enter a name for the key.'); return; }
    setKeyBusy(true); setKeyError(null);
    try { const key = await sshKeyGenerate(keyName, algorithm); await refreshKeys(); set('privateKeyPath')(`vault://key:${key.id}`); set('authMethod')('private_key'); setKeyName(''); }
    catch (err) { setKeyError(describeError(err)); } finally { setKeyBusy(false); }
  }

  async function importKey(file?: File) {
    if (!file) return;
    if (!keyName.trim()) { setKeyError('Enter a name for the key before importing.'); return; }
    setKeyBusy(true); setKeyError(null);
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      let binary = ''; for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
      const key = await sshKeyImport(keyName, btoa(binary), keyPass || undefined);
      await refreshKeys(); set('privateKeyPath')(`vault://key:${key.id}`); set('authMethod')('private_key'); setKeyName(''); setKeyPass('');
    } catch (err) { setKeyError(describeError(err)); } finally { setKeyBusy(false); }
  }

  async function removeKey(key: SshKeyView) {
    if (!window.confirm(`Delete key “${key.name}”?`)) return;
    try { await sshKeyDelete(key.id); if (form.privateKeyPath === `vault://key:${key.id}`) set('privateKeyPath')(''); await refreshKeys(); }
    catch (err) { setKeyError(describeError(err)); }
  }

  async function forgetSudo() {
    if (!host) return;
    try {
      await hostForgetSudoPassword(host.id);
      setSudoNote('No sudo password is saved for this host now.');
    } catch (err) {
      setSudoNote(describeError(err));
    }
  }

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
            <Field label="Password" hint="Stored inside your encrypted vault.">
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
              <label className="field">SSH key
                <select aria-label="SSH key" value={form.privateKeyPath.startsWith('vault://key:') ? form.privateKeyPath : form.privateKeyPath.startsWith('vault://') ? form.privateKeyPath : ''} onChange={(e) => set('privateKeyPath')(e.target.value)}>
                  <option value="">Choose a managed key…</option>
                  {form.privateKeyPath.startsWith('vault://') && !form.privateKeyPath.startsWith('vault://key:') && <option value={form.privateKeyPath}>Existing key saved for this host</option>}
                  {form.privateKeyPath && !form.privateKeyPath.startsWith('vault://') && <option value={form.privateKeyPath}>Existing file path: {form.privateKeyPath}</option>}
                  {keys.map((key) => <option key={key.id} value={`vault://key:${key.id}`}>{key.name} · {key.algorithm}</option>)}
                </select>
                <span className="hint">Managed keys are stored in the encrypted vault and can be reused across hosts.</span>
              </label>
              {form.privateKeyPath && !form.privateKeyPath.startsWith('vault://') && <p className="hint">This profile uses a legacy key file path. Import it into key management to reuse it from other profiles.</p>}
              <button className="btn btn-sm" type="button" onClick={() => setManageKeys((open) => !open)}>{manageKeys ? 'Hide key manager' : 'Manage SSH keys'}</button>
              {manageKeys && <div className="form-section" aria-label="SSH key manager">
                <h3>SSH keys</h3>
                {keyError && <p role="alert" className="banner" data-tone="danger">{keyError}</p>}
                <label className="field">Key name<input value={keyName} onChange={(e) => setKeyName(e.target.value)} placeholder="Work laptop" /></label>
                <div className="field-row">
                  <button type="button" className="btn btn-sm" disabled={keyBusy} onClick={() => void generateKey('ed25519')}>Generate Ed25519</button>
                  <button type="button" className="btn btn-sm" disabled={keyBusy} onClick={() => void generateKey('ecdsa-p256')}>ECDSA P-256</button>
                </div>
                <div className="field-row">
                  <button type="button" className="btn btn-sm" disabled={keyBusy} onClick={() => void generateKey('ecdsa-p384')}>ECDSA P-384</button>
                  <button type="button" className="btn btn-sm" disabled={keyBusy} onClick={() => void generateKey('ecdsa-p521')}>ECDSA P-521</button>
                </div>
                <label className="field">Private key passphrase (if encrypted)<input type="password" autoComplete="off" value={keyPass} onChange={(e) => setKeyPass(e.target.value)} /></label>
                <label className="field">Import private key<input type="file" accept=".pem,.key,id_*" disabled={keyBusy} onChange={(e) => { void importKey(e.target.files?.[0]); e.currentTarget.value = ''; }} /></label>
                <span className="hint">Supports Ed25519 and ECDSA P-256/P-384/P-521. Imported private keys are encrypted in your vault. Generated private keys never leave the backend.</span>
                {keys.map((key) => <div key={key.id} className="key-manager-row"><div><strong>{key.name}</strong><div className="hint">{key.algorithm} · {key.fingerprintSha256}</div><code>{key.publicKey}</code></div><div className="field-row"><button type="button" className="btn btn-sm" onClick={() => { void navigator.clipboard.writeText(key.publicKey); }}>Copy public key</button><button type="button" className="btn btn-sm" onClick={() => set('privateKeyPath')(`vault://key:${key.id}`)}>Use</button><button type="button" className="btn btn-sm btn-ghost" onClick={() => void removeKey(key)}>Delete</button></div></div>)}
              </div>}
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
              {host && (
                <div className="field">
                  <span>Sudo password</span>
                  <span className="hint">
                    Saved only when you tick "Remember for this host" while approving a root command.
                  </span>
                  <div>
                    <button type="button" className="btn btn-sm" onClick={() => void forgetSudo()}>
                      Forget saved sudo password
                    </button>
                  </div>
                  {sudoNote && (
                    <span role="status" className="hint">
                      {sudoNote}
                    </span>
                  )}
                </div>
              )}
            </div>
          </details>
        </div>
      </form>
    </Drawer>
  );
}
