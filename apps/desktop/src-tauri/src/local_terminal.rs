//! An interactive shell on this computer, in a PTY (ConPTY on Windows). It uses
//! the same input channel and events as an SSH terminal, so the webview treats
//! both alike.

use std::{
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use selai_domain::ErrorCode;
use tokio::sync::mpsc;

use crate::{
    dto::{CommandError, TerminalEvent},
    local_exec::home_dir,
    service::{EventSink, TerminalInput},
};

fn pty_size(cols: u32, rows: u32) -> PtySize {
    PtySize {
        rows: u16::try_from(rows).unwrap_or(u16::MAX),
        cols: u16::try_from(cols).unwrap_or(u16::MAX),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn failure(what: &str, e: &dyn std::fmt::Display) -> CommandError {
    CommandError::new(ErrorCode::Internal, format!("{what}: {e}"))
}

/// The user's login shell, or the platform default.
fn default_shell() -> CommandBuilder {
    #[cfg(windows)]
    {
        CommandBuilder::new("powershell.exe")
    }
    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "/bin/sh".to_owned());
        let mut cmd = CommandBuilder::new(shell);
        cmd.arg("-l");
        cmd
    }
}

/// Starts the shell and two threads: one pumps PTY output to `sink`, one
/// applies input, resizes and close requests. `on_end` runs once the shell has
/// exited, before the final `Closed` event.
pub(crate) fn spawn_local_shell(
    cols: u32,
    rows: u32,
    mut input: mpsc::Receiver<TerminalInput>,
    sink: EventSink,
    on_end: Box<dyn FnOnce() + Send>,
) -> Result<Arc<dyn Fn() + Send + Sync>, CommandError> {
    let pair = native_pty_system()
        .openpty(pty_size(cols, rows))
        .map_err(|e| failure("could not open a terminal", &e))?;
    let mut cmd = default_shell();
    cmd.env("TERM", "xterm-256color");
    if let Some(home) = home_dir() {
        cmd.cwd(home);
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| failure("could not start the shell", &e))?;
    drop(pair.slave);
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| failure("could not read the terminal", &e))?;
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| failure("could not write to the terminal", &e))?;
    // Shared so the waiter can close it when the shell exits: ConPTY only ends
    // the output stream once its master is closed.
    let master = Arc::new(Mutex::new(Some(pair.master)));
    let killer = Arc::new(Mutex::new(child.clone_killer()));
    let stop: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        if let Ok(mut killer) = killer.lock() {
            let _ = killer.kill();
        }
    });
    let close = stop.clone();

    let resize_master = Arc::clone(&master);
    std::thread::spawn(move || {
        while let Some(command) = input.blocking_recv() {
            match command {
                TerminalInput::Data(bytes) => {
                    if writer
                        .write_all(&bytes)
                        .and_then(|()| writer.flush())
                        .is_err()
                    {
                        break;
                    }
                }
                TerminalInput::Resize { cols, rows } => {
                    if let Ok(guard) = resize_master.lock() {
                        if let Some(m) = guard.as_ref() {
                            let _ = m.resize(pty_size(cols, rows));
                        }
                    }
                }
            }
        }
        close();
    });

    let (exited_tx, exited_rx) = std::sync::mpsc::channel::<Option<u32>>();
    std::thread::spawn(move || {
        let status = child.wait().ok().map(|s| s.exit_code());
        // Let the last output drain, then close the master.
        std::thread::sleep(Duration::from_millis(150));
        if let Ok(mut guard) = master.lock() {
            guard.take();
        }
        let _ = exited_tx.send(status);
    });

    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => sink(TerminalEvent::Data {
                    data: B64.encode(&buf[..n]),
                }),
            }
        }
        let exit_status = exited_rx.recv().ok().flatten();
        if let Some(status) = exit_status {
            sink(TerminalEvent::Exit { status });
        }
        on_end();
        sink(TerminalEvent::Closed { exit_status });
    });
    Ok(stop)
}
