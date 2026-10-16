import { describe, expect, it } from 'vitest';
import type { TaskEvent } from '../ipc';
import { applyEvent, isTerminal, setApprovalState, type TranscriptItem } from './agentTranscript';

function run(events: TaskEvent[]): TranscriptItem[] {
  return events.reduce<TranscriptItem[]>(applyEvent, []);
}

describe('agent transcript', () => {
  it('joins streamed text into one reply', () => {
    const items = run([
      { type: 'textDelta', text: 'The disk is ' },
      { type: 'textDelta', text: 'fine.' },
    ]);
    expect(items).toEqual([{ kind: 'text', text: 'The disk is fine.' }]);
  });

  it('hides running between steps but shows the outcome', () => {
    const items = run([
      { type: 'state', state: 'running', errorCode: null },
      { type: 'state', state: 'completed', errorCode: null },
    ]);
    expect(items).toEqual([{ kind: 'status', text: 'completed' }]);
  });

  it('keeps the exact command and updates the tool card when it finishes', () => {
    const items = run([
      { type: 'toolProposed', executionId: 'e1', tool: 'ssh.run_diagnostic', command: 'df -hP', hostLabel: 'production-web' },
      { type: 'toolFinished', executionId: 'e1', state: 'completed', exitCode: 0, errorCode: null },
    ]);
    expect(items).toEqual([
      {
        kind: 'tool',
        executionId: 'e1',
        command: 'df -hP',
        hostLabel: 'production-web',
        status: 'completed',
        exitCode: 0,
      },
    ]);
  });

  it('adds an approval card that waits for a decision', () => {
    const items = run([
      {
        type: 'approvalRequired',
        approvalId: 'a1',
        executionId: 'e1',
        command: 'rm -rf /tmp/x',
        expiresAtMs: 1_000,
      },
    ]);
    expect(items[0]).toMatchObject({ kind: 'approval', state: 'pending', error: null });
  });

  it('records a decision per approval and can carry an error', () => {
    const start = run([
      { type: 'approvalRequired', approvalId: 'a1', executionId: 'e1', command: 'id', expiresAtMs: 1 },
    ]);
    const sent = setApprovalState(start, 'a1', 'approved');
    expect(sent[0]).toMatchObject({ state: 'approved' });
    const refused = setApprovalState(start, 'a1', 'pending', 'APPROVAL_EXPIRED: too late');
    expect(refused[0]).toMatchObject({ state: 'pending', error: 'APPROVAL_EXPIRED: too late' });
  });

  it('recognises terminal events only', () => {
    expect(isTerminal({ type: 'state', state: 'cancelled', errorCode: null })).toBe(true);
    expect(isTerminal({ type: 'state', state: 'awaiting_approval', errorCode: null })).toBe(false);
    expect(isTerminal({ type: 'completed', text: '' })).toBe(true);
    expect(isTerminal({ type: 'textDelta', text: 'x' })).toBe(false);
  });
});
