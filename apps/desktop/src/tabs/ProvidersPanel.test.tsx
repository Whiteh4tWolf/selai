import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { ProvidersPanel } from './ProvidersPanel';

afterEach(() => vi.restoreAllMocks());

const provider: ipc.ProviderView = {
  id: 'p1',
  name: 'local-vllm',
  protocol: 'openai_chat',
  baseUrl: 'http://127.0.0.1:8000/v1',
  modelId: 'qwen-tools',
  timeoutSeconds: 120,
  maxOutputTokens: 4096,
  keyConfigured: false,
  isLoopback: true,
  allowInsecureHttp: false,
  updatedAtMs: 7,
  toolCalling: { status: 'not_verified', lastCheckedAtMs: null },
};

function mockTemplates() {
  vi.spyOn(ipc, 'providerTemplates').mockResolvedValue([
    {
      id: 'vllm',
      displayName: 'vLLM',
      protocol: 'openai_chat',
      defaultBaseUrl: 'http://localhost:8000/v1',
      note: 'Needs the tool parser.',
    },
  ]);
}

describe('ProvidersPanel', () => {
  it('shows verification status as not verified until the Rust probe says otherwise', async () => {
    vi.spyOn(ipc, 'providerList').mockResolvedValue({ items: [provider], nextCursor: null });
    mockTemplates();
    render(<ProvidersPanel />);
    expect(await screen.findByText('local-vllm')).toBeInTheDocument();
    expect(screen.getByText('Not verified')).toBeInTheDocument();
  });

  it('one Test button checks the connection, then tool calling, and shows the Rust results', async () => {
    vi.spyOn(ipc, 'providerList')
      .mockResolvedValueOnce({ items: [provider], nextCursor: null })
      .mockResolvedValue({
        items: [{ ...provider, toolCalling: { status: 'verified', lastCheckedAtMs: 9 } }],
        nextCursor: null,
      });
    mockTemplates();
    const test = vi.spyOn(ipc, 'providerTest').mockImplementation(async (_id, kind) => ({
      kind,
      ok: true,
      verified: kind === 'tool_calling',
      code: null,
      message: kind === 'tool_calling' ? 'tool call and tool result round trip completed' : 'reachable',
      latencyMs: 12,
      replyPreview: null,
    }));
    render(<ProvidersPanel />);
    fireEvent.click(await screen.findByRole('button', { name: 'Test' }));
    await waitFor(() => expect(test).toHaveBeenCalledTimes(2));
    expect(test.mock.calls.map((c) => c[1])).toEqual(['connectivity', 'tool_calling']);
    expect(await screen.findByText(/Tool calling: passed/)).toBeInTheDocument();
    expect(screen.getByText(/Connection: passed/)).toBeInTheDocument();
    expect(await screen.findByText('Verified')).toBeInTheDocument();
  });

  it('skips the tool-calling check when the connection fails', async () => {
    vi.spyOn(ipc, 'providerList').mockResolvedValue({ items: [provider], nextCursor: null });
    mockTemplates();
    const test = vi.spyOn(ipc, 'providerTest').mockResolvedValue({
      kind: 'connectivity',
      ok: false,
      verified: false,
      code: 'LLM_UNREACHABLE',
      message: 'connection refused',
      latencyMs: 3,
      replyPreview: null,
    });
    render(<ProvidersPanel />);
    fireEvent.click(await screen.findByRole('button', { name: 'Test' }));
    expect(await screen.findByText(/Connection: failed \(LLM_UNREACHABLE\)/)).toBeInTheDocument();
    expect(screen.getByText(/skipped because the connection failed/)).toBeInTheDocument();
    expect(test).toHaveBeenCalledTimes(1);
  });

  it('offers plain HTTP to a remote gateway only as an explicit opt-in', async () => {
    vi.spyOn(ipc, 'providerList').mockResolvedValue({ items: [], nextCursor: null });
    mockTemplates();
    const save = vi.spyOn(ipc, 'providerSave').mockResolvedValue(provider);
    render(<ProvidersPanel />);
    fireEvent.click((await screen.findAllByRole('button', { name: 'Add provider' }))[0]!);
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'litellm' } });
    fireEvent.change(screen.getByLabelText('Base URL'), { target: { value: 'https://llm.internal/v1' } });
    expect(screen.queryByLabelText('Allow plain HTTP')).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText('Base URL'), { target: { value: 'http://10.1.2.3:4000/v1' } });
    fireEvent.click(screen.getByLabelText('Allow plain HTTP'));
    fireEvent.change(screen.getByLabelText('Model ID'), { target: { value: 'gpt-4o' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save provider' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]?.[0]).toMatchObject({ baseUrl: 'http://10.1.2.3:4000/v1', allowInsecureHttp: true });
  });

  it('applies a template without filling a model ID and sends the key only once', async () => {
    vi.spyOn(ipc, 'providerList').mockResolvedValue({ items: [], nextCursor: null });
    mockTemplates();
    const save = vi.spyOn(ipc, 'providerSave').mockResolvedValue(provider);
    render(<ProvidersPanel />);
    fireEvent.click((await screen.findAllByRole('button', { name: 'Add provider' }))[0]!);
    fireEvent.change(screen.getByLabelText('Start from a template'), { target: { value: 'vllm' } });
    expect(screen.getByLabelText('Base URL')).toHaveValue('http://localhost:8000/v1');
    expect(screen.getByLabelText('Model ID')).toHaveValue('');

    fireEvent.change(screen.getByLabelText(/Model ID/), { target: { value: 'qwen-tools' } });
    fireEvent.change(screen.getByLabelText(/API key/), { target: { value: 'k-1' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save provider' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]?.[0]).toMatchObject({ modelId: 'qwen-tools', apiKey: 'k-1' });
    await waitFor(() => expect(screen.queryByLabelText(/API key/)).not.toBeInTheDocument());
  });
});
