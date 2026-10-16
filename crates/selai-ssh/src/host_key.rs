//! Host-key observation and pin checking.

use std::sync::{Arc, Mutex};

use russh::client::{self, Handler};
use russh::keys::{HashAlg, PublicKeyOrCertificate};

use crate::{session::Endpoint, SshError};

/// A key as the server presented it. Only public material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedHostKey {
    pub algorithm: String,
    pub public_key_base64: String,
    pub fingerprint_sha256: String,
}

/// A key we trust for a host. Built from the local pin store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedHostKey {
    pub algorithm: String,
    pub public_key_base64: String,
    pub fingerprint_sha256: String,
}

impl PinnedHostKey {
    /// Parses one `known_hosts`-style public key line: `algorithm base64 [comment]`.
    pub fn from_openssh_line(line: &str) -> Result<Self, SshError> {
        let key = russh::keys::PublicKey::from_openssh(line.trim())
            .map_err(|e| SshError::Protocol(format!("invalid public key: {e}")))?;
        Ok(Self {
            algorithm: key.algorithm().as_str().to_owned(),
            public_key_base64: base64_of(&key)?,
            fingerprint_sha256: key.fingerprint(HashAlg::Sha256).to_string(),
        })
    }
}

impl ObservedHostKey {
    fn from_public_key(key: &russh::keys::PublicKey) -> Result<Self, SshError> {
        Ok(Self {
            algorithm: key.algorithm().as_str().to_owned(),
            public_key_base64: base64_of(key)?,
            fingerprint_sha256: key.fingerprint(HashAlg::Sha256).to_string(),
        })
    }
}

fn base64_of(key: &russh::keys::PublicKey) -> Result<String, SshError> {
    // OpenSSH text form is "<algorithm> <base64> [comment]". Taking field 2 is
    // exact because algorithm names and base64 contain no spaces.
    let line = key
        .to_openssh()
        .map_err(|e| SshError::Protocol(format!("cannot encode public key: {e}")))?;
    line.split_whitespace()
        .nth(1)
        .map(str::to_owned)
        .ok_or_else(|| SshError::Protocol("malformed public key encoding".to_owned()))
}

/// What the handshake saw and decided. Shared between the handler and the caller
/// because russh reports a rejected key only as a generic error.
#[derive(Debug, Default)]
pub(crate) struct Outcome {
    pub observed: Option<ObservedHostKey>,
    pub verdict: Verdict,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    #[default]
    Undecided,
    Trusted,
    Unknown,
    Changed {
        pinned_fingerprint: String,
    },
    Certificate,
}

/// Verifies the server key during key exchange. Accepts only an exact match of
/// a pinned key; every other case is rejected.
#[derive(Clone)]
pub(crate) struct PinVerifier {
    pins: Vec<PinnedHostKey>,
    outcome: Arc<Mutex<Outcome>>,
}

impl PinVerifier {
    pub(crate) fn new(pins: Vec<PinnedHostKey>) -> (Self, Arc<Mutex<Outcome>>) {
        let outcome = Arc::new(Mutex::new(Outcome::default()));
        (
            Self {
                pins,
                outcome: Arc::clone(&outcome),
            },
            outcome,
        )
    }

    fn decide(&self, observed: &ObservedHostKey) -> Verdict {
        if let Some(pin) = self.pins.iter().find(|p| p.algorithm == observed.algorithm) {
            return if pin.public_key_base64 == observed.public_key_base64 {
                Verdict::Trusted
            } else {
                Verdict::Changed {
                    pinned_fingerprint: pin.fingerprint_sha256.clone(),
                }
            };
        }
        // The host has pins, but none for the algorithm now offered: treat as a
        // change, not as first contact.
        match self.pins.first() {
            Some(other) => Verdict::Changed {
                pinned_fingerprint: other.fingerprint_sha256.clone(),
            },
            None => Verdict::Unknown,
        }
    }
}

impl Handler for PinVerifier {
    type Error = SshError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let (verdict, observed) = match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => {
                let observed = ObservedHostKey::from_public_key(key)?;
                (self.decide(&observed), Some(observed))
            }
            PublicKeyOrCertificate::Certificate(_) => (Verdict::Certificate, None),
        };
        let trusted = verdict == Verdict::Trusted;
        if let Ok(mut out) = self.outcome.lock() {
            out.observed = observed;
            out.verdict = verdict;
        }
        Ok(trusted)
    }
}

/// Connects far enough to read the server's host key, then disconnects.
///
/// No authentication method is ever invoked. This is how the first-trust prompt
/// obtains the fingerprint to show.
pub async fn probe_host_key(
    endpoint: &Endpoint,
    timeout: std::time::Duration,
) -> Result<ObservedHostKey, SshError> {
    let (verifier, outcome) = PinVerifier::new(Vec::new());
    let config = Arc::new(client::Config::default());
    let attempt = client::connect(config, endpoint.socket_addr_pair(), verifier);
    let result = tokio::time::timeout(timeout, attempt).await;

    let guard = outcome
        .lock()
        .map_err(|_| SshError::Protocol("probe state poisoned".to_owned()))?;
    match result {
        Err(_) => Err(SshError::ConnectTimeout),
        Ok(Ok(_handle)) => Err(SshError::Protocol(
            "server accepted without a host key check".to_owned(),
        )),
        Ok(Err(err)) => match guard.observed.clone() {
            Some(observed) => Ok(observed),
            None => Err(map_connect_error(err)),
        },
    }
}

pub(crate) fn map_connect_error(err: SshError) -> SshError {
    match err {
        SshError::ConnectFailed(ref msg) if msg.contains("lookup") => SshError::Dns,
        other => other,
    }
}
