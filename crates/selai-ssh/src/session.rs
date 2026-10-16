//! Authenticated SSH sessions.

use std::{path::PathBuf, sync::Arc, time::Duration};

use russh::{
    client::{self},
    keys::PrivateKeyWithHashAlg,
    Disconnect,
};
use selai_secrets::SecretValue;

use crate::{
    host_key::{map_connect_error, Outcome, PinVerifier, Verdict},
    PinnedHostKey, Shell, SshError,
};

/// Host and port. The host is a DNS name or an IP literal without brackets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    pub(crate) fn socket_addr_pair(&self) -> (String, u16) {
        (self.host.clone(), self.port)
    }
}

/// Credential for one connection. Secrets are only read here, never stored.
pub enum Auth {
    Password(SecretValue),
    PrivateKey {
        path: PathBuf,
        passphrase: Option<SecretValue>,
    },
}

pub struct ConnectOptions {
    pub endpoint: Endpoint,
    pub username: String,
    pub auth: Auth,
    /// Keys trusted for this host. Empty means no key is trusted, so every
    /// connection is refused before authentication.
    pub pinned: Vec<PinnedHostKey>,
    pub connect_timeout: Duration,
    pub keepalive: Option<Duration>,
}

/// An authenticated transport. Terminal and exec channels are opened on it.
pub struct Session {
    handle: client::Handle<PinVerifier>,
    endpoint: Endpoint,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl Session {
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Opens a PTY channel with a login shell.
    pub async fn open_shell(&self, cols: u32, rows: u32, term: &str) -> Result<Shell, SshError> {
        let channel = self.handle.channel_open_session().await?;
        channel
            .request_pty(true, term, cols, rows, 0, 0, &[])
            .await?;
        channel.request_shell(true).await?;
        Ok(Shell::new(channel))
    }

    pub async fn disconnect(self) -> Result<(), SshError> {
        self.handle
            .disconnect(Disconnect::ByApplication, "", "en")
            .await?;
        Ok(())
    }
}

/// Connects, verifies the host key against the pins, and authenticates.
///
/// The whole sequence (TCP, key exchange, authentication) is bounded by
/// `connect_timeout`.
pub async fn connect(opts: ConnectOptions) -> Result<Session, SshError> {
    let (verifier, outcome) = PinVerifier::new(opts.pinned.clone());
    let config = Arc::new(client::Config {
        keepalive_interval: opts.keepalive,
        ..client::Config::default()
    });
    let addr = opts.endpoint.socket_addr_pair();
    let endpoint = opts.endpoint.clone();

    let attempt = async {
        let mut handle = client::connect(config, addr, verifier).await?;
        authenticate(&mut handle, &opts.username, &opts.auth).await?;
        Ok::<_, SshError>(handle)
    };

    match tokio::time::timeout(opts.connect_timeout, attempt).await {
        Err(_) => Err(SshError::ConnectTimeout),
        Ok(Ok(handle)) => Ok(Session { handle, endpoint }),
        Ok(Err(err)) => Err(explain(err, &outcome)),
    }
}

async fn authenticate(
    handle: &mut client::Handle<PinVerifier>,
    username: &str,
    auth: &Auth,
) -> Result<(), SshError> {
    let result = match auth {
        Auth::Password(secret) => {
            let password = utf8(secret.expose_for_transport()).ok_or(SshError::AuthFailed)?;
            handle.authenticate_password(username, password).await?
        }
        Auth::PrivateKey { path, passphrase } => {
            let passphrase = match passphrase {
                Some(p) => Some(
                    utf8(p.expose_for_transport())
                        .ok_or_else(|| SshError::KeyLoad("passphrase is not UTF-8".to_owned()))?,
                ),
                None => None,
            };
            let key = russh::keys::load_secret_key(path, passphrase)
                .map_err(|e| SshError::KeyLoad(e.to_string()))?;
            // RSA private-key operations are affected by RUSTSEC-2023-0071 and have
            // no fix yet. Refuse them before any authentication is attempted.
            if matches!(key.algorithm(), russh::keys::Algorithm::Rsa { .. }) {
                return Err(SshError::KeyLoad(
                    "RSA private keys are not supported in v1; use an Ed25519 or ECDSA key"
                        .to_owned(),
                ));
            }
            let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
            handle
                .authenticate_publickey(
                    username,
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
                )
                .await?
        }
    };
    if result.success() {
        Ok(())
    } else {
        Err(SshError::AuthFailed)
    }
}

/// Turns a failed connect into the most specific error. Key verdicts come from
/// the handshake outcome, because russh reports a rejected key only generically.
fn explain(err: SshError, outcome: &std::sync::Mutex<Outcome>) -> SshError {
    let Ok(out) = outcome.lock() else {
        return map_connect_error(err);
    };
    match (&out.verdict, out.observed.clone()) {
        (Verdict::Unknown, Some(observed)) => SshError::HostKeyUnknown(observed),
        (Verdict::Changed { pinned_fingerprint }, Some(observed)) => SshError::HostKeyChanged {
            observed,
            pinned_fingerprint: pinned_fingerprint.clone(),
        },
        (Verdict::Certificate, _) => SshError::UnsupportedHostKey,
        _ => map_connect_error(err),
    }
}

fn utf8(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
}
