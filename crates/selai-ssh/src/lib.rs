//! SSH transport for Selai (ADR-0002).
//!
//! Two operations matter for security:
//! - [`probe_host_key`] reads the server's host key and never authenticates.
//! - [`connect`] authenticates only after the server key equals a pinned key.
//!
//! Anything else (unknown key, changed key, certificate) is rejected during key
//! exchange, before a credential is sent.

mod error;
mod exec;
mod host_key;
mod session;
mod shell;

pub use error::SshError;
pub use exec::{ExecEnd, ExecOutput};
pub use host_key::{probe_host_key, ObservedHostKey, PinnedHostKey};
pub use session::{connect, Auth, ConnectOptions, Endpoint, Session};
pub use shell::{Shell, ShellEvent};
