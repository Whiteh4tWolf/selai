import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import * as ipc from '../ipc';
import { flushVaultViews } from '../vault';
import { useChatSessions, type ChatTurn } from './useChatSessions';

const turn = (prompt: string): ChatTurn => ({ id:'turn',hostId:null,hostLabel:null,showPrompt:true,taskPrompt:prompt,
  mentionText:null,prompt,items:[],taskId:null,running:false,notice:null,error:null });

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(ipc, 'chatSessionList').mockResolvedValue([]);
});
afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });

it('coalesces slow saves and flushes the latest revision before the vault locks', async () => {
  let release = () => {};
  const first = new Promise<void>((resolve) => { release = resolve; });
  const save = vi.spyOn(ipc, 'chatSessionSave').mockResolvedValue().mockReturnValueOnce(first);
  const { result, unmount } = renderHook(() => useChatSessions());
  await act(async () => { await Promise.resolve(); });
  act(() => result.current.setTurns([turn('first')]));
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(save).toHaveBeenCalledTimes(1);
  act(() => result.current.setTurns([turn('second')]));
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  act(() => result.current.setTurns([turn('latest')]));
  await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
  expect(save).toHaveBeenCalledTimes(1);
  await act(async () => {
    const flushing = flushVaultViews();
    release();
    await flushing;
  });
  expect(save).toHaveBeenCalledTimes(2);
  expect(JSON.parse(save.mock.calls[1]![0].transcript).turns[0].prompt).toBe('latest');
  unmount();
});
