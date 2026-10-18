import { useEffect, useState, type FormEvent } from 'react';
import {
  describeError,
  groupList,
  hostForgetSudoPassword,
  hostRevealPassword,
  hostSave,
  sshKeyList,
  type AuthMethod,
  type HostInput,
  type HostView,
  type GroupView,
  type SshKeyView,
} from '../ipc';
import { Field } from '../ui/Field';
import { Drawer } from '../ui/Overlay';
import { PasswordInput } from '../ui/PasswordInput';
import { ChevronIcon, KeyIcon } from '../ui/icons';

interface Props {
  host: HostView | null;
  onCancel: () => void;
  /** `connect` is true when the operator chose "Save & connect". */
  onSaved: (host: HostView, connect: boolean) => void;
  onManageKeys?: () => void;
  suspended?: boolean;
}

interface FormState {
  label: string;
  groupId: string;
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
    groupId: host?.groupId ?? '',
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
    return 'Choose an SSH key for key authentication.';
  }
  return null;
}

export function HostForm({ host, onCancel, onSaved, onManageKeys, suspended = false }: Props) {
  const [form, setForm] = useState<FormState>(() => initial(host));
  // Secrets live only in this component's local state until submit. They are
  // cleared right after a save. Saved values are revealed only on demand.
  const [password, setPassword] = useState('');
  const [passphrase, setPassphrase] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [sudoNote, setSudoNote] = useState<string | null>(null);
  const [keys, setKeys] = useState<SshKeyView[]>([]);
  const [keyError, setKeyError] = useState<string | null>(null);
  const [keysLoading, setKeysLoading] = useState(false);
  const [groups, setGroups] = useState<GroupView[]>([]);


  useEffect(() => {
    if (suspended || form.authMethod !== 'private_key') return;
    let active = true;
    setKeysLoading(true);
    setKeyError(null);
    sshKeyList().then((items) => { if (active) setKeys(items); })
      .catch((err) => { if (active) setKeyError(describeError(err)); })
      .finally(() => { if (active) setKeysLoading(false); });
    return () => { active = false; };
  }, [suspended, form.authMethod]);
  useEffect(() => {
    let active = true;
    groupList().then((items) => { if (active) setGroups(items); })
      .catch((err) => { if (active) setError(describeError(err)); });
    return () => { active = false; };
  }, []);

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
      groupId: form.groupId || null,
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

  const hasSavedPassword = host?.authMethod === 'password' && host.credentialConfigured;
  const savedPlaceholder = hasSavedPassword ? '•••• saved (leave blank to keep)' : '';
  const formId = 'host-form';

  // Keep the draft in component state while the operator visits SSH Keys.
  if (suspended) return null;

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
          <Field label="Group" hint="Create groups with + beside SSH hosts in the sidebar. You can also drag profiles into groups.">
            <select value={form.groupId} onChange={(e) => set('groupId')(e.target.value)}>
              <option value="">Ungrouped</option>
              {groups.map((group) => <option key={group.id} value={group.id}>{group.name}</option>)}
            </select>
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
            <Field label="Password" hint={hasSavedPassword ? 'Saved in your encrypted vault. Click the eye to reveal it; leave blank to keep it.' : 'Stored inside your encrypted vault.'}>
              <PasswordInput
                value={password}
                placeholder={savedPlaceholder}
                disabled={busy}
                onChange={setPassword}
                loadSaved={hasSavedPassword && host ? () => hostRevealPassword(host.id) : undefined}
              />
            </Field>
          ) : (
            <>
              <Field label="SSH key" hint="Select a key from your encrypted vault.">
                <select value={form.privateKeyPath} disabled={keysLoading}
                  onChange={(e) => set('privateKeyPath')(e.target.value)}>
                  <option value="">{keysLoading ? 'Loading keys…' : 'Choose an SSH key…'}</option>
                  {form.privateKeyPath && !keys.some((key) => form.privateKeyPath === `vault://key:${key.id}`) && (
                    <option value={form.privateKeyPath}>
                      {form.privateKeyPath.startsWith('vault://key:') ? 'Previously selected key (unavailable)' :
                        form.privateKeyPath.startsWith('vault://') ? 'Existing key saved for this host' : 'Existing key file'}
                    </option>
                  )}
                  {keys.map((key) => <option key={key.id} value={`vault://key:${key.id}`}>{key.name} · {key.algorithm}</option>)}
                </select>
              </Field>
              {keyError && <p role="alert" className="banner" data-tone="danger">{keyError}</p>}
              {!keysLoading && !keyError && keys.length === 0 && <p className="hint">No managed SSH keys yet. Generate or import one in SSH Keys, then return to this profile.</p>}
              {form.privateKeyPath && !form.privateKeyPath.startsWith('vault://') && <p className="hint">This profile uses an existing key file. Import it in SSH Keys to reuse it across profiles.</p>}
              {onManageKeys && <button className="btn btn-sm" type="button" onClick={onManageKeys}>
                <KeyIcon size={14} /> {keys.length === 0 ? 'Add SSH key' : 'Manage SSH keys'}
              </button>}

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
