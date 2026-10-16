// Turns the task's event stream into the transcript the operator reads. Pure:
// the Rust core decides what happens; this only arranges what is shown.
import type { TaskEvent } from '../ipc';

export type ApprovalState = 'pending' | 'sending' | 'approved' | 'denied';

export type TranscriptItem =
  | { kind: 'text'; text: string }
  | { kind: 'status'; text: string }
  | { kind: 'tool'; executionId: string; command: string; hostLabel: string; status: string; exitCode: number | null }
  | {
      kind: 'approval';
      approvalId: string;
      executionId: string;
      command: string;
      expiresAtMs: number;
      state: ApprovalState;
      error: string | null;
    }
  | { kind: 'failed'; code: string; message: string };

export function isTerminal(event: TaskEvent): boolean {
  return (
    (event.type === 'state' && ['completed', 'failed', 'cancelled'].includes(event.state)) ||
    event.type === 'completed' ||
    event.type === 'failed'
  );
}

export function applyEvent(items: TranscriptItem[], event: TaskEvent): TranscriptItem[] {
  switch (event.type) {
    case 'textDelta': {
      const last = items.at(-1);
      if (last?.kind === 'text') {
        return [...items.slice(0, -1), { kind: 'text', text: last.text + event.text }];
      }
      return [...items, { kind: 'text', text: event.text }];
    }
    case 'state':
      // "running" is noise between steps; the operator sees the outcome instead.
      if (event.state === 'running') return items;
      return [...items, { kind: 'status', text: stateText(event.state, event.errorCode) }];
    case 'toolProposed':
      return [
        ...items,
        {
          kind: 'tool',
          executionId: event.executionId,
          command: event.command,
          hostLabel: event.hostLabel,
          status: 'proposed',
          exitCode: null,
        },
      ];
    case 'approvalRequired':
      return [
        ...items,
        {
          kind: 'approval',
          approvalId: event.approvalId,
          executionId: event.executionId,
          command: event.command,
          expiresAtMs: event.expiresAtMs,
          state: 'pending',
          error: null,
        },
      ];
    case 'toolFinished':
      return items.map((item) =>
        item.kind === 'tool' && item.executionId === event.executionId
          ? { ...item, status: event.state, exitCode: event.exitCode }
          : item,
      );
    case 'completed':
      return items;
    case 'failed':
      return [...items, { kind: 'failed', code: event.code, message: event.message }];
  }
}

export function setApprovalState(
  items: TranscriptItem[],
  approvalId: string,
  state: ApprovalState,
  error: string | null = null,
): TranscriptItem[] {
  return items.map((item) =>
    item.kind === 'approval' && item.approvalId === approvalId ? { ...item, state, error } : item,
  );
}

function stateText(state: string, errorCode: string | null): string {
  const label = state.replace(/_/g, ' ');
  return errorCode ? `${label} (${errorCode})` : label;
}
