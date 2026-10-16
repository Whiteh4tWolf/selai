import { useEffect, useState, type FormEvent } from 'react';
import {
  describeError,
  providerDelete,
  providerList,
  providerSave,
  providerTemplates,
  providerTest,
  type ProviderInput,
  type ProviderProtocolName,
  type ProviderTestView,
  type ProviderView,
  type TemplateView,
} from '../ipc';

const PROTOCOLS: { value: ProviderProtocolName; label: string }[] = [
  { value: 'openai_chat', label: 'OpenAI-compatible Chat Completions' },
  { value: 'openai_responses', label: 'OpenAI Responses' },
  { value: 'anthropic_messages', label: 'Anthropic Messages' },
];

const STATUS_TEXT: Record<ProviderView['toolCalling']['status'], string> = {
  verified: 'tool calling verified for this model',
  failed: 'tool calling test failed',
  not_verified: 'not verified',
};

interface FormState {
  id?: string;
  expectedUpdatedAtMs?: number;
  name: string;
  protocol: ProviderProtocolName;
  baseUrl: string;
  modelId: string;
  timeoutSeconds: string;
  maxOutputTokens: string;
  keyConfigured: boolean;
  note: string;
}

const EMPTY: FormState = {
  name: '',
  protocol: 'openai_chat',
  baseUrl: '',
  modelId: '',
  timeoutSeconds: '120',
  maxOutputTokens: '4096',
  keyConfigured: false,
  note: '',
};

function fromProvider(p: ProviderView): FormState {
  return {
    id: p.id,
    expectedUpdatedAtMs: p.updatedAtMs,
    name: p.name,
    protocol: p.protocol,
    baseUrl: p.baseUrl,
    modelId: p.modelId,
    timeoutSeconds: String(p.timeoutSeconds),
    maxOutputTokens: String(p.maxOutputTokens),
    keyConfigured: p.keyConfigured,
    note: '',
  };
}

export function ProvidersPanel() {
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [templates, setTemplates] = useState<TemplateView[]>([]);
  const [form, setForm] = useState<FormState | null>(null);
  const [apiKey, setApiKey] = useState('');
  const [clearKey, setClearKey] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [results, setResults] = useState<Record<string, ProviderTestView>>({});
  const [busy, setBusy] = useState(false);

  async function refresh() {
    try {
      setProviders((await providerList()).items);
    } catch (err) {
      setError(describeError(err));
    }
  }

  useEffect(() => {
    void refresh();
    providerTemplates()
      .then(setTemplates)
      .catch((err: unknown) => setError(describeError(err)));
  }, []);

  function applyTemplate(id: string) {
    const t = templates.find((x) => x.id === id);
    if (!t) return;
    setForm((prev) => ({
      ...(prev ?? EMPTY),
      protocol: t.protocol,
      baseUrl: t.defaultBaseUrl,
      note: t.note,
      name: prev?.name || t.displayName,
    }));
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!form) return;
    const input: ProviderInput = {
      id: form.id,
      expectedUpdatedAtMs: form.expectedUpdatedAtMs,
      name: form.name.trim(),
      protocol: form.protocol,
      baseUrl: form.baseUrl.trim(),
      modelId: form.modelId.trim(),
      timeoutSeconds: Number(form.timeoutSeconds),
      maxOutputTokens: Number(form.maxOutputTokens),
      apiKey: apiKey || undefined,
      clearApiKey: clearKey || undefined,
    };
    setBusy(true);
    try {
      await providerSave(input);
      setApiKey('');
      setClearKey(false);
      setForm(null);
      setError(null);
      await refresh();
    } catch (err) {
      setError(describeError(err));
    } finally {
      setBusy(false);
    }
  }

  async function test(provider: ProviderView, kind: 'connectivity' | 'tool_calling') {
    setBusy(true);
    try {
      const result = await providerTest(provider.id, kind);
      setResults((prev) => ({ ...prev, [provider.id]: result }));
      await refresh();
    } catch (err) {
      setError(describeError(err));
    } finally {
      setBusy(false);
    }
  }

  async function remove(provider: ProviderView) {
    if (!window.confirm(`Delete provider ${provider.name} and its saved key?`)) return;
    try {
      await providerDelete(provider.id);
      await refresh();
    } catch (err) {
      setError(describeError(err));
    }
  }

  const keyPlaceholder = form?.keyConfigured ? '•••• saved (leave blank to keep)' : '';

  return (
    <section aria-label="AI providers" className="providers">
      <div className="toolbar">
        <h3>Providers</h3>
        <button type="button" onClick={() => setForm({ ...EMPTY })}>
          Add provider
        </button>
      </div>
      {error && (
        <p role="alert" className="error">
          {error}
        </p>
      )}

      {form && (
        <form className="host-form" aria-label={form.id ? 'Edit provider' : 'New provider'} onSubmit={submit}>
          <label>
            Start from a template
            <select defaultValue="" onChange={(e) => applyTemplate(e.target.value)}>
              <option value="" disabled>
                Choose…
              </option>
              {templates.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.displayName}
                </option>
              ))}
            </select>
          </label>
          {form.note && <p className="muted">{form.note}</p>}
          <label>
            Name
            <input value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} />
          </label>
          <label>
            Protocol
            <select
              value={form.protocol}
              onChange={(e) => setForm({ ...form, protocol: e.target.value as ProviderProtocolName })}
            >
              {PROTOCOLS.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Base URL (API root, without the endpoint path)
            <input value={form.baseUrl} onChange={(e) => setForm({ ...form, baseUrl: e.target.value })} />
          </label>
          <label>
            Model ID (copy it from your provider)
            <input value={form.modelId} onChange={(e) => setForm({ ...form, modelId: e.target.value })} />
          </label>
          <label>
            API key (write-only; blank for a local server)
            <input
              type="password"
              autoComplete="off"
              value={apiKey}
              placeholder={keyPlaceholder}
              onChange={(e) => setApiKey(e.target.value)}
            />
          </label>
          {form.keyConfigured && (
            <label className="confirm">
              <input type="checkbox" checked={clearKey} onChange={(e) => setClearKey(e.target.checked)} />
              Remove the saved key
            </label>
          )}
          <label>
            Timeout (seconds)
            <input value={form.timeoutSeconds} onChange={(e) => setForm({ ...form, timeoutSeconds: e.target.value })} />
          </label>
          <label>
            Max output tokens per turn
            <input value={form.maxOutputTokens} onChange={(e) => setForm({ ...form, maxOutputTokens: e.target.value })} />
          </label>
          <div className="form-actions">
            <button type="submit" disabled={busy}>
              Save provider
            </button>
            <button type="button" onClick={() => setForm(null)}>
              Cancel
            </button>
          </div>
        </form>
      )}

      <ul className="host-list">
        {providers.map((p) => {
          const result = results[p.id];
          return (
            <li key={p.id} className="host-row">
              <div className="host-main">
                <strong>{p.name}</strong>
                <span className="muted">
                  {p.modelId} · {p.baseUrl}
                </span>
                <span className="badge" data-status={p.toolCalling.status}>
                  {STATUS_TEXT[p.toolCalling.status]}
                </span>
                {!p.keyConfigured && !p.isLoopback && <span className="badge">no key saved</span>}
                {result && (
                  <span role="status" className={result.ok ? '' : 'error'}>
                    {result.kind === 'tool_calling' ? 'Tool calling' : 'Connectivity'}:{' '}
                    {result.ok ? 'passed' : 'failed'}
                    {result.code ? ` (${result.code})` : ''} — {result.message}
                    {result.replyPreview ? ` “${result.replyPreview}”` : ''}
                  </span>
                )}
              </div>
              <div className="host-actions">
                <button type="button" onClick={() => void test(p, 'connectivity')} disabled={busy}>
                  Test connectivity
                </button>
                <button type="button" onClick={() => void test(p, 'tool_calling')} disabled={busy}>
                  Test tool calling
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setForm(fromProvider(p));
                    setApiKey('');
                    setClearKey(false);
                  }}
                >
                  Edit
                </button>
                <button type="button" onClick={() => void remove(p)}>
                  Delete
                </button>
              </div>
            </li>
          );
        })}
      </ul>
      {providers.length === 0 && <p>No providers yet.</p>}
    </section>
  );
}
