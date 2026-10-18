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
    PrivateKeyData {
        pem: SecretValue,
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
    pub(crate) handle: client::Handle<PinVerifier>,
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
        Auth::PrivateKey { passphrase, .. } | Auth::PrivateKeyData { passphrase, .. } => {
            let passphrase = match passphrase {
                Some(p) => Some(
                    utf8(p.expose_for_transport())
                        .ok_or_else(|| SshError::KeyLoad("passphrase is not UTF-8".to_owned()))?,
                ),
                None => None,
            };
            let key = match auth {
                Auth::PrivateKey { path, .. } => russh::keys::load_secret_key(path, passphrase),
                Auth::PrivateKeyData { pem, .. } => russh::keys::decode_secret_key(
                    utf8(pem.expose_for_transport())
                        .ok_or_else(|| SshError::KeyLoad("Private key is not UTF-8".into()))?,
                    passphrase,
                ),
                Auth::Password(_) => unreachable!(),
            }
            .map_err(|e| SshError::KeyLoad(e.to_string()))?;
            if matches!(key.algorithm(), russh::keys::Algorithm::Rsa { .. }) {
                let mut signer = crate::private_key::RsaSigner::new(&key)?;
                let hashes = match handle.best_supported_rsa_hash().await? {
                    // Some servers advertise SHA-512 globally even when the
                    // account's policy only permits SHA-256. Retry SHA-256 on
                    // an authentication refusal, never SHA-1.
                    Some(Some(russh::keys::HashAlg::Sha512)) => vec![
                        russh::keys::HashAlg::Sha512, russh::keys::HashAlg::Sha256,
                    ],
                    Some(Some(hash)) => vec![hash],
                    Some(None) => return Err(SshError::KeyLoad(
                        "Server only supports legacy ssh-rsa/SHA-1. Enable rsa-sha2-256 or rsa-sha2-512 on the server, or use an Ed25519/ECDSA key.".into()
                    )),
                    // Older servers may omit server-sig-algs but accept SHA-2.
                    None => vec![russh::keys::HashAlg::Sha512, russh::keys::HashAlg::Sha256],
                };
                for hash in hashes {
                    let result = handle
                        .authenticate_publickey_with(
                            username,
                            key.public_key().clone(),
                            Some(hash),
                            &mut signer,
                        )
                        .await?;
                    if result.success() {
                        return Ok(());
                    }
                }
                return Err(SshError::AuthFailed);
            }
            crate::validate_private_key(&key)?;
            handle
                .authenticate_publickey(username, PrivateKeyWithHashAlg::new(Arc::new(key), None))
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
