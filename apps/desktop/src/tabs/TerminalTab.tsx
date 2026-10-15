import { useEffect, useRef } from 'react';
import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';

export function TerminalTab() {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const term = new Terminal({ convertEol: true, cursorBlink: true });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(container);
    fit.fit();
    // No PTY backend exists yet (S-013). Make that visible instead of
    // pretending a shell is attached.
    term.writeln('Terminal backend not connected: SSH PTY arrives in S-013.');
    term.writeln('No shell is running.');

    const observer =
      typeof ResizeObserver === 'undefined' ? undefined : new ResizeObserver(() => fit.fit());
    observer?.observe(container);

    return () => {
      observer?.disconnect();
      term.dispose();
    };
  }, []);

  return <div className="terminal" ref={containerRef} aria-label="Terminal output" />;
}
