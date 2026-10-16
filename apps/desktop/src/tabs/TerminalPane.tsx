import { useEffect, useRef, useState } from 'react';
import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import {
  decodeBase64,
  hostKeyProblem,
  isCommandError,
  terminalConnect,
  terminalDisconnect,
  terminalInput,
  terminalResize,
  type CommandError,
  type HostKeyProblem,
  type HostView,
  type TerminalEvent,
} from '../ipc';
import { HostKeyDialog } from '../ui/HostKeyDialog';
import { AlertIcon, EditIcon, RefreshIcon } from '../ui/icons';
import type { SessionStatus } from './TerminalTab';

interface Props {
  host: HostView;
  hidden: boolean;
  onStatus?: (status: SessionStatus) => void;
  onClose?: () => void;
  onEditHost?: () => void;
}

const THEME = {
  background: '#0a0c12',
  foreground: '#d8dbe8',
  cursor: '#7391ff',
  cursorAccent: '#0a0c12',
  selectionBackground: 'rgba(115, 145, 255, 0.35)',
  black: '#1c2031',
  red: '#ef6b6b',
  green: '#3fcf86',
  yellow: '#f0b84a',
  blue: '#6b8cff',
  magenta: '#b58cff',
  cyan: '#3cc8dc',
  white: '#d8dbe8',
  brightBlack: '#5d6380',
  brightRed: '#ff8a8a',
  brightGreen: '#6be3a4',
  brightYellow: '#ffd27a',
  brightBlue: '#93acff',
  brightMagenta: '#cfb0ff',
  brightCyan: '#7adcea',
  brightWhite: '#ffffff',
};

const FAILURE_TITLES: Record<string, string> = {
  SSH_AUTH_FAILED: 'Authentication failed',
  SSH_DNS: 'Host name not found',
  SSH_CONNECT_TIMEOUT: 'Connection timed out',
  SSH_CONNECT_FAILED: 'Could not reach the server',
  SSH_HOSTKEY_UNKNOWN: 'Server key not trusted',
  SSH_HOSTKEY_CHANGED: 'Server key not trusted',
};

/**
 * One interactive shell. The session stays open while this pane is mounted, so
 * the parent must keep panes mounted when their tab is hidden. A connect that
 * stops at an unknown or changed server key asks for approval here and then
 * connects again; nothing is sent to the server before the key is trusted.
 */
export function TerminalPane({ host, hidden, onStatus, onClose, onEditHost }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const sessionRef = useRef<string | null>(null);
  const [status, setStatus] = useState<SessionStatus>('connecting');
  const [failure, setFailure] = useState<CommandError | null>(null);
  const [keyProblem, setKeyProblem] = useState<HostKeyProblem | null>(null);
  const [exitStatus, setExitStatus] = useState<number | null>(null);
  const [attempt, setAttempt] = useState(0);
  const statusCallback = useRef(onStatus);
  statusCallback.current = onStatus;

  useEffect(() => {
    statusCallback.current?.(status);
  }, [status]);

  // The terminal itself lives as long as the pane.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    const term = new Terminal({
      cursorBlink: true,
      scrollback: 10000,
      fontFamily: "'JetBrains Mono', 'Cascadia Code', 'SF Mono', Menlo, Consolas, 'DejaVu Sans Mono', monospace",
      fontSize: 13.5,
      lineHeight: 1.25,
      letterSpacing: 0,
      theme: THEME,
      allowProposedApi: false,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;

    const inputSub = term.onData((data) => {
      const id = sessionRef.current;
      if (id) void terminalInput(id, data).catch(() => undefined);
    });
    const resizeSub = term.onResize(({ cols, rows }) => {
      const id = sessionRef.current;
      if (id) void terminalResize(id, cols, rows).catch(() => undefined);
    });
    const observer =
      typeof ResizeObserver === 'undefined'
        ? undefined
        : new ResizeObserver(() => {
            if (container.offsetWidth > 0) fit.fit();
          });
    observer?.observe(container);

    return () => {
      observer?.disconnect();
      inputSub.dispose();
      resizeSub.dispose();
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
  }, []);

  // One connection per attempt. Retry and "Trust & connect" start a new attempt.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    let disposed = false;
    let sessionId: string | null = null;
    setStatus('connecting');
    setFailure(null);
    setKeyProblem(null);
    setExitStatus(null);
    if (attempt > 0) term.write('\r\n\x1b[2m── reconnecting ──\x1b[0m\r\n');

    const onEvent = (event: TerminalEvent) => {
      if (disposed) return;
      switch (event.type) {
        case 'data':
          term.write(decodeBase64(event.data));
          break;
        case 'exit':
          break;
        case 'closed':
          sessionRef.current = null;
          setExitStatus(event.exitStatus);
          setStatus('closed');
          break;
        case 'error':
          term.writeln(`\r\n\x1b[31m[${event.code}] ${event.message}\x1b[0m`);
          break;
      }
    };

    terminalConnect(host.id, term.cols, term.rows, onEvent)
      .then((id) => {
        if (disposed) {
          void terminalDisconnect(id);
          return;
        }
        sessionId = id;
        sessionRef.current = id;
        setStatus('connected');
        fitRef.current?.fit();
        term.focus();
      })
      .catch((err: unknown) => {
        if (disposed) return;
        const problem = hostKeyProblem(err);
        if (problem) {
          setKeyProblem(problem);
          setStatus('hostkey');
          return;
        }
        setFailure(isCommandError(err) ? err : { code: 'ERROR', message: String(err) });
        setStatus('failed');
      });

    return () => {
      disposed = true;
      if (sessionId) void terminalDisconnect(sessionId).catch(() => undefined);
      sessionRef.current = null;
    };
  }, [host.id, attempt]);

  // A pane that was hidden has no size; fit it again when it is shown.
  useEffect(() => {
    if (!hidden) {
      fitRef.current?.fit();
      if (status === 'connected') termRef.current?.focus();
    }
  }, [hidden]);

  const retry = () => setAttempt((n) => n + 1);

  function disconnect() {
    const id = sessionRef.current;
    if (!id) return;
    void terminalDisconnect(id)
      .catch(() => undefined)
      .finally(() => {
        sessionRef.current = null;
        setStatus('closed');
      });
  }

  const address =
    host.kind === 'local'
      ? 'Local shell'
      : `${host.username}@${host.hostname}${host.port !== 22 ? `:${host.port}` : ''}`;
  const statusText: Record<SessionStatus, string> = {
    connecting: 'Connecting…',
    connected: 'Connected',
    hostkey: 'Waiting for key approval',
    failed: 'Not connected',
    closed: exitStatus === null ? 'Session ended' : `Session ended (exit ${exitStatus})`,
  };

  return (
    <div className="terminal-pane" hidden={hidden}>
      <div className="term-bar">
        <span className="dot" data-status={status} />
        <strong>{host.label}</strong>
        <span>{address}</span>
        <span className="grow" />
        <span aria-live="polite">{statusText[status]}</span>
        {status === 'connected' && (
          <button type="button" className="btn btn-ghost btn-sm" onClick={disconnect}>
            Disconnect
          </button>
        )}
        {status === 'closed' && (
          <button type="button" className="btn btn-sm" onClick={retry}>
            <RefreshIcon size={13} /> Reconnect
          </button>
        )}
      </div>
      <div className="terminal" ref={containerRef} aria-label={`Terminal ${host.label}`} />

      {status === 'connecting' && (
        <div className="term-overlay">
          <div className="term-card" role="status">
            <span className="spinner spinner-lg" />
            <h2>Connecting to {host.label}</h2>
            <span>{address}</span>
          </div>
        </div>
      )}

      {status === 'failed' && failure && (
        <div className="term-overlay">
          <div className="term-card" role="alert">
            <AlertIcon size={28} style={{ color: 'var(--danger)' }} />
            <h2>{FAILURE_TITLES[failure.code] ?? 'Connection failed'}</h2>
            <p>{failure.message}</p>
            <span className="code">{failure.code}</span>
            <div className="actions">
              {onClose && (
                <button type="button" className="btn btn-ghost" onClick={onClose}>
                  Close tab
                </button>
              )}
              {onEditHost && (
                <button type="button" className="btn" onClick={onEditHost}>
                  <EditIcon size={14} /> Edit host
                </button>
              )}
              <button type="button" className="btn btn-primary" onClick={retry}>
                <RefreshIcon size={14} /> Retry
              </button>
            </div>
          </div>
        </div>
      )}

      {status === 'hostkey' && keyProblem && (
        <HostKeyDialog
          contained
          hostId={host.id}
          hostLabel={host.label}
          address={`${host.hostname}:${host.port}`}
          problem={keyProblem}
          onTrusted={retry}
          onCancel={() => {
            setKeyProblem(null);
            setFailure({
              code: keyProblem.kind === 'changed' ? 'SSH_HOSTKEY_CHANGED' : 'SSH_HOSTKEY_UNKNOWN',
              message: 'The server key was not approved, so nothing was sent to the server.',
            });
            setStatus('failed');
          }}
        />
      )}
    </div>
  );
}
