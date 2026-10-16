import { describe, expect, it } from 'vitest';
import { restoreTranscript, type ChatTurn } from './useChatSessions';

describe('restoring chat history', () => {
  it('makes unfinished runs and approval cards inactive without replaying them', () => {
    const turn: ChatTurn = { id: 'turn-1', hostId: 'h1', hostLabel: 'web', showPrompt: true,
      taskPrompt: 'restart nginx', mentionText: ['@web'], prompt: '@web restart nginx',
      taskId: 'task-1', running: true, notice: null, error: null, items: [
        { kind: 'tool', executionId: 'e1', command: 'systemctl restart nginx', hostLabel: 'web', status: 'proposed', exitCode: null },
        { kind: 'approval', approvalId: 'a1', executionId: 'e1', command: 'systemctl restart nginx',
          expiresAtMs: Date.now() + 60_000, state: 'pending', error: null, sudo: null },
      ] };
    const restored = restoreTranscript(JSON.stringify({ version: 1, turns: [turn] }))[0]!;
    expect(restored.running).toBe(false);
    expect(restored.notice).toContain('Check History');
    expect(restored.items[0]).toMatchObject({ status: 'detached' });
    expect(restored.items[1]).toMatchObject({ expiresAtMs: 0 });
    expect(turn.items[0]).toMatchObject({ status: 'proposed' });
  });

  it('refuses unsupported or malformed snapshots', () => {
    expect(() => restoreTranscript('{"version":2,"turns":[]}')).toThrow();
    expect(() => restoreTranscript('{"version":1,"turns":[{}]}')).toThrow();
  });
});
