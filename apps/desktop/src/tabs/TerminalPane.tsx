import { useEffect, useRef, useState } from 'react';
import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import {
  decodeBase64,
  describeError,
  terminalConnect,
  terminalDisconnect,
  terminalInput,
  terminalResize,
  type TerminalEvent,
} from '../ipc';

type Status = 'connecting' | 'connected' | 'closed' | 'failed';

interface Props {
  hostId: string;
  label: string;
  hidden: boolean;
}

/**
 * One interactive shell. The session stays open while this pane is mounted, so
 * the parent must keep panes mounted when their tab is hidden.
 */
export function TerminalPane({ hostId, label, hidden }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<Status>('connecting');

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    let disposed = false;
    let sessionId: string | null = null;

    const term = new Terminal({ cursorBlink: true, scrollback: 5000 });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);
    fit.fit();

    const onEvent = (event: TerminalEvent) => {
      if (disposed) return;
      switch (event.type) {
        case 'data':
          term.write(decodeBase64(event.data));
          break;
        case 'exit':
          break;
        case 'closed':
          term.writeln(
            `\r\n[session closed${event.exitStatus === null ? '; the server did not report an exit status' : `; exit status ${event.exitStatus}`}]`,
          );
          setStatus('closed');
          break;
        case 'error':
          term.writeln(`\r\n[${event.code}] ${event.message}`);
          break;
      }
    };

    terminalConnect(hostId, term.cols, term.rows, onEvent)
      .then((id) => {
        if (disposed) {
          void terminalDisconnect(id);
          return;
        }
        sessionId = id;
        setStatus('connected');
      })
      .catch((err: unknown) => {
        if (disposed) return;
        term.writeln(`[failed] ${describeError(err)}`);
        setStatus('failed');
      });

    const inputSub = term.onData((data) => {
      if (sessionId) {
        void terminalInput(sessionId, data).catch(() => undefined);
      }
    });
    const resizeSub = term.onResize(({ cols, rows }) => {
      if (sessionId) {
        void terminalResize(sessionId, cols, rows).catch(() => undefined);
      }
    });
    const observer =
      typeof ResizeObserver === 'undefined'
        ? undefined
        : new ResizeObserver(() => {
            if (container.offsetWidth > 0) fit.fit();
          });
    observer?.observe(container);

    return () => {
      disposed = true;
      observer?.disconnect();
      inputSub.dispose();
      resizeSub.dispose();
      if (sessionId) {
        void terminalDisconnect(sessionId).catch(() => undefined);
      }
      term.dispose();
    };
  }, [hostId]);

  return (
    <div className="terminal-pane" hidden={hidden}>
      <p className="terminal-status" aria-live="polite">
        {label}: {status}
      </p>
      <div className="terminal" ref={containerRef} aria-label={`Terminal ${label}`} />
    </div>
  );
}
