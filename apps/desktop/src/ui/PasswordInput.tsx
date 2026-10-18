import { useCallback, useEffect, useRef, useState } from 'react';
import { describeError } from '../ipc';
import { registerVaultFlusher } from '../vault';
import { EyeIcon, EyeOffIcon } from './icons';

interface Props {
  id?: string;
  'aria-describedby'?: string;
  value: string;
  placeholder?: string;
  disabled?: boolean;
  onChange: (value: string) => void;
  loadSaved?: () => Promise<string | null>;
}

/** Saved values are fetched on demand, never passed back as an edit. */
export function PasswordInput({ id, 'aria-describedby': hint, value, placeholder, disabled, onChange, loadSaved }: Props) {
  const [visible, setVisible] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);
  const hide = useCallback(() => {
    request.current += 1;
    setVisible(false);
    setSaved(null);
    setLoading(false);
  }, []);

  // The workspace stays mounted briefly while vault locking drains tasks.
  // Clear previews as soon as the existing vault flush starts.
  useEffect(() => registerVaultFlusher(async () => { hide(); }), [hide]);

  useEffect(() => {
    const onHidden = () => { if (document.hidden) hide(); };
    window.addEventListener('blur', hide);
    document.addEventListener('visibilitychange', onHidden);
    return () => {
      request.current += 1;
      window.removeEventListener('blur', hide);
      document.removeEventListener('visibilitychange', onHidden);
    };
  }, [hide]);
  useEffect(() => {
    if (!visible) return;
    const timer = window.setTimeout(hide, 30_000);
    return () => window.clearTimeout(timer);
  }, [visible, hide]);
  useEffect(() => { if (disabled) hide(); }, [disabled, hide]);

  async function toggle() {
    setError(null);
    if (visible) { hide(); return; }
    if (value || !loadSaved) { setVisible(true); return; }
    const current = ++request.current;
    setLoading(true);
    try {
      const secret = await loadSaved();
      if (request.current !== current) return;
      if (secret === null) { setError('No password is saved for this profile.'); return; }
      setSaved(secret);
      setVisible(true);
    } catch (err) {
      if (request.current === current) setError(describeError(err));
    } finally {
      if (request.current === current) setLoading(false);
    }
  }

  return <>
    <div className="password-input">
      <input id={id} aria-describedby={hint} type={visible ? 'text' : 'password'}
        autoComplete="off" spellCheck={false} value={saved ?? value} placeholder={placeholder}
        disabled={disabled} onChange={(e) => {
          request.current += 1;
          setSaved(null); setLoading(false); setError(null);
          onChange(e.target.value);
        }} />
      <button type="button" className="icon-btn" aria-label={visible ? 'Hide password' : 'Show password'}
        title={visible ? 'Hide password' : 'Show password'} aria-pressed={visible}
        disabled={disabled || loading} onClick={() => void toggle()}>
        {loading ? <span className="spinner" /> : visible ? <EyeOffIcon size={16} /> : <EyeIcon size={16} />}
      </button>
    </div>
    {error && <span role="alert" className="hint">{error}</span>}
  </>;
}
