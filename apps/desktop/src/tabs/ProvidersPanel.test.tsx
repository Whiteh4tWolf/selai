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
    expect(screen.getByText('not verified')).toBeInTheDocument();
  });

  it('runs the tool-calling test and shows the Rust result, not a guess', async () => {
    vi.spyOn(ipc, 'providerList')
      .mockResolvedValueOnce({ items: [provider], nextCursor: null })
      .mockResolvedValueOnce({
        items: [{ ...provider, toolCalling: { status: 'verified', lastCheckedAtMs: 9 } }],
        nextCursor: null,
      });
    mockTemplates();
    const test = vi.spyOn(ipc, 'providerTest').mockResolvedValue({
      kind: 'tool_calling',
      ok: true,
      verified: true,
      code: null,
      message: 'tool call and tool result round trip completed',
      latencyMs: 12,
      replyPreview: null,
    });
    render(<ProvidersPanel />);
    fireEvent.click(await screen.findByRole('button', { name: 'Test tool calling' }));
    await waitFor(() => expect(test).toHaveBeenCalledWith('p1', 'tool_calling'));
    expect(await screen.findByText(/Tool calling: passed/)).toBeInTheDocument();
    expect(screen.getByText('tool calling verified for this model')).toBeInTheDocument();
  });

  it('applies a template without filling a model ID and sends the key only once', async () => {
    vi.spyOn(ipc, 'providerList').mockResolvedValue({ items: [], nextCursor: null });
    mockTemplates();
    const save = vi.spyOn(ipc, 'providerSave').mockResolvedValue(provider);
    render(<ProvidersPanel />);
    fireEvent.click(await screen.findByRole('button', { name: 'Add provider' }));
    fireEvent.change(screen.getByLabelText('Start from a template'), { target: { value: 'vllm' } });
    expect(screen.getByLabelText(/Base URL/)).toHaveValue('http://localhost:8000/v1');
    expect(screen.getByLabelText(/Model ID/)).toHaveValue('');

    fireEvent.change(screen.getByLabelText(/Model ID/), { target: { value: 'qwen-tools' } });
    fireEvent.change(screen.getByLabelText(/API key/), { target: { value: 'k-1' } });
    fireEvent.click(screen.getByRole('button', { name: 'Save provider' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]?.[0]).toMatchObject({ modelId: 'qwen-tools', apiKey: 'k-1' });
    await waitFor(() => expect(screen.queryByLabelText(/API key/)).not.toBeInTheDocument());
  });
});
