//! Passphrase-protected local vault. No passphrase, recovery key, or unwrapped
//! data key is persisted. SQLCipher protects every database page and its WAL.
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use selai_storage::Database;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

pub const MANIFEST: &str = "vault.json";
const MAGIC: &[u8; 16] = b"SELAI-VAULT-01\0\0";
const MAX_HEADER: usize = 16 * 1024;
const MAX_BACKUP: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Could not unlock the vault. Check the passphrase or recovery key.")]
    Unlock,
    #[error("Unsupported or damaged vault file.")]
    Format,
    #[error("Use a passphrase with at least 12 characters (maximum 1024 bytes).")]
    Passphrase,
    #[error("Vault I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Vault storage failed: {0}")]
    Storage(#[from] selai_storage::StorageError),
    #[error("Secure random generator unavailable.")]
    Random,
    #[error("Vault backup exceeds the supported 2 GiB limit.")]
    BackupTooLarge,
}

type Result<T> = std::result::Result<T, VaultError>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wrapped {
    nonce: String,
    ciphertext: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    version: u32,
    database: String,
    salt: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    passphrase: Wrapped,
    recovery: Wrapped,
}

pub struct Vault {
    dir: PathBuf,
    header: Header,
    key: Zeroizing<[u8; 32]>,
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| VaultError::Random)?;
    Ok(bytes)
}

fn wrap(key: &[u8; 32], secret: &[u8], purpose: &[u8]) -> Result<Wrapped> {
    let nonce = random::<12>()?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::Format)?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: secret,
                aad: purpose,
            },
        )
        .map_err(|_| VaultError::Format)?;
    Ok(Wrapped {
        nonce: B64.encode(nonce),
        ciphertext: B64.encode(ciphertext),
    })
}

fn unwrap(key: &[u8; 32], wrapped: &Wrapped, purpose: &[u8]) -> Result<Zeroizing<[u8; 32]>> {
    let nonce = B64.decode(&wrapped.nonce).map_err(|_| VaultError::Format)?;
    if nonce.len() != 12 {
        return Err(VaultError::Format);
    }
    let ciphertext = B64
        .decode(&wrapped.ciphertext)
        .map_err(|_| VaultError::Format)?;
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| VaultError::Format)?;
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: purpose,
                },
            )
            .map_err(|_| VaultError::Unlock)?,
    );
    let key: [u8; 32] = plain
        .as_slice()
        .try_into()
        .map_err(|_| VaultError::Format)?;
    Ok(Zeroizing::new(key))
}

fn derive(passphrase: &str, header: &Header) -> Result<Zeroizing<[u8; 32]>> {
    if !(19_456..=262_144).contains(&header.memory_kib)
        || !(2..=10).contains(&header.iterations)
        || !(1..=4).contains(&header.parallelism)
    {
        return Err(VaultError::Format);
    }
    let salt = B64.decode(&header.salt).map_err(|_| VaultError::Format)?;
    if salt.len() != 16 || passphrase.len() > 1024 {
        return Err(VaultError::Format);
    }
    let params = Params::new(
        header.memory_kib,
        header.iterations,
        header.parallelism,
        Some(32),
    )
    .map_err(|_| VaultError::Format)?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(passphrase.as_bytes(), &salt, key.as_mut())
        .map_err(|_| VaultError::Unlock)?;
    Ok(key)
}

fn valid_passphrase(passphrase: &str) -> Result<()> {
    if passphrase.chars().count() < 12 || passphrase.len() > 1024 {
        Err(VaultError::Passphrase)
    } else {
        Ok(())
    }
}

fn parse_header(bytes: &[u8]) -> Result<Header> {
    if bytes.len() > MAX_HEADER {
        return Err(VaultError::Format);
    }
    let header: Header = serde_json::from_slice(bytes).map_err(|_| VaultError::Format)?;
    if header.version != 1
        || !header.database.starts_with("vault-")
        || !header.database.ends_with(".sqlite3")
        || header.database.contains(['/', '\\'])
        || header.database.len() > 100
    {
        return Err(VaultError::Format);
    }
    Ok(header)
}

pub fn private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let user = std::process::Command::new("whoami")
            .creation_flags(0x08000000)
            .output()?;
        if !user.status.success() {
            return Err(VaultError::Format);
        }
        let user = String::from_utf8_lossy(&user.stdout).trim().to_owned();
        let output = std::process::Command::new("icacls")
            .arg(path)
            .args([
                "/inheritance:r",
                "/grant:r",
                &format!("{user}:(OI)(CI)F"),
                "*S-1-5-18:(OI)(CI)F",
                "*S-1-5-32-544:(OI)(CI)F",
            ])
            .creation_flags(0x08000000)
            .output()?;
        if !output.status.success() {
            return Err(VaultError::Format);
        }
    }
    Ok(())
}

fn create_private(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

/// Create with private permissions and replace the selected directory entry.
/// Renaming avoids following an existing symlink/hardlink when exporting.
pub fn write_export(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_file_name(format!(".selai-export-{}.tmp", B64.encode(random::<12>()?)));
    let result = (|| {
        let mut file = create_private(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        Ok(())
    })();
    let _ = fs::remove_file(temp);
    result
}

impl Vault {
    pub fn create(dir: &Path, passphrase: &str) -> Result<(Self, Zeroizing<String>)> {
        valid_passphrase(passphrase)?;
        private_dir(dir)?;
        let key = Zeroizing::new(random::<32>()?);
        let recovery = Zeroizing::new(random::<32>()?);
        let mut header = Header {
            version: 1,
            database: format!("vault-{}.sqlite3", B64.encode(random::<16>()?)),
            salt: B64.encode(random::<16>()?),
            memory_kib: 65_536,
            iterations: 3,
            parallelism: 1,
            passphrase: Wrapped {
                nonce: String::new(),
                ciphertext: String::new(),
            },
            recovery: wrap(&recovery, key.as_ref(), b"selai-recovery-v1")?,
        };
        header.passphrase = wrap(
            &*derive(passphrase, &header)?,
            key.as_ref(),
            b"selai-passphrase-v1",
        )?;
        Ok((
            Self {
                dir: dir.to_path_buf(),
                header,
                key,
            },
            Zeroizing::new(B64.encode(recovery.as_ref())),
        ))
    }

    pub fn unlock(dir: &Path, passphrase: &str) -> Result<Self> {
        let path = dir.join(MANIFEST);
        if fs::metadata(&path)?.len() > MAX_HEADER as u64 {
            return Err(VaultError::Format);
        }
        let header = parse_header(&fs::read(path)?)?;
        let key = unwrap(
            &*derive(passphrase, &header)?,
            &header.passphrase,
            b"selai-passphrase-v1",
        )?;
        Ok(Self {
            dir: dir.into(),
            header,
            key,
        })
    }

    pub fn recover(
        dir: &Path,
        recovery: &str,
        new_passphrase: &str,
    ) -> Result<(Self, Zeroizing<String>)> {
        if recovery.len() > 128 || fs::metadata(dir.join(MANIFEST))?.len() > MAX_HEADER as u64 {
            return Err(VaultError::Format);
        }
        let header = parse_header(&fs::read(dir.join(MANIFEST))?)?;
        let recovery = Zeroizing::new(
            B64.decode(recovery.trim())
                .map_err(|_| VaultError::Unlock)?,
        );
        let recovery: Zeroizing<[u8; 32]> = Zeroizing::new(
            recovery
                .as_slice()
                .try_into()
                .map_err(|_| VaultError::Unlock)?,
        );
        let key = unwrap(&recovery, &header.recovery, b"selai-recovery-v1")?;
        let mut vault = Self {
            dir: dir.into(),
            header,
            key,
        };
        vault.rewrap(new_passphrase)?;
        let new_recovery = Zeroizing::new(random::<32>()?);
        vault.header.recovery = wrap(&new_recovery, vault.key.as_ref(), b"selai-recovery-v1")?;
        Ok((vault, Zeroizing::new(B64.encode(new_recovery.as_ref()))))
    }

    pub fn database_path(&self) -> PathBuf {
        self.dir.join(&self.header.database)
    }
    pub fn key(&self) -> &[u8; 32] {
        &self.key
    }

    pub fn verify(&self, passphrase: &str) -> Result<()> {
        unwrap(
            &*derive(passphrase, &self.header)?,
            &self.header.passphrase,
            b"selai-passphrase-v1",
        )
        .map(|_| ())
    }

    fn rewrap(&mut self, passphrase: &str) -> Result<()> {
        valid_passphrase(passphrase)?;
        self.header.salt = B64.encode(random::<16>()?);
        self.header.passphrase = wrap(
            &*derive(passphrase, &self.header)?,
            self.key.as_ref(),
            b"selai-passphrase-v1",
        )?;
        Ok(())
    }

    pub fn change_passphrase(&mut self, current: &str, new: &str) -> Result<()> {
        self.verify(current)?;
        let old = self.header.clone();
        if let Err(error) = self.rewrap(new).and_then(|_| self.commit()) {
            self.header = old;
            return Err(error);
        }
        Ok(())
    }

    pub fn rotate_recovery(&mut self, passphrase: &str) -> Result<Zeroizing<String>> {
        self.verify(passphrase)?;
        let recovery = Zeroizing::new(random::<32>()?);
        let old = self.header.clone();
        self.header.recovery = wrap(&recovery, self.key.as_ref(), b"selai-recovery-v1")?;
        if let Err(error) = self.commit() {
            self.header = old;
            return Err(error);
        }
        Ok(Zeroizing::new(B64.encode(recovery.as_ref())))
    }

    /// Portable encrypted history document. The header only contains wrapped keys.
    pub fn seal_document(&self, bytes: &[u8]) -> Result<Vec<u8>> {
        let sealed = wrap(&self.key, bytes, b"selai-document-v1")?;
        serde_json::to_vec(&serde_json::json!({"format":"selai-encrypted-history-v1", "header":self.header, "sealed":sealed})).map_err(|_| VaultError::Format)
    }

    pub fn open_document(bytes: &[u8], passphrase: &str) -> Result<Zeroizing<Vec<u8>>> {
        #[derive(Deserialize)]
        struct Document {
            format: String,
            header: Header,
            sealed: Wrapped,
        }
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(VaultError::Format);
        }
        let document: Document = serde_json::from_slice(bytes).map_err(|_| VaultError::Format)?;
        if document.format != "selai-encrypted-history-v1" {
            return Err(VaultError::Format);
        }
        let key = unwrap(
            &*derive(passphrase, &document.header)?,
            &document.header.passphrase,
            b"selai-passphrase-v1",
        )?;
        let nonce = B64
            .decode(document.sealed.nonce)
            .map_err(|_| VaultError::Format)?;
        if nonce.len() != 12 {
            return Err(VaultError::Format);
        }
        let ciphertext = B64
            .decode(document.sealed.ciphertext)
            .map_err(|_| VaultError::Format)?;
        let cipher = Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| VaultError::Format)?;
        Ok(Zeroizing::new(
            cipher
                .decrypt(
                    Nonce::from_slice(&nonce),
                    Payload {
                        msg: &ciphertext,
                        aad: b"selai-document-v1",
                    },
                )
                .map_err(|_| VaultError::Unlock)?,
        ))
    }

    /// The manifest is committed last. A crash before rename leaves the previous
    /// generation usable; staging files contain encrypted pages only.
    pub fn commit(&self) -> Result<()> {
        let temp = self
            .dir
            .join(format!("header-{}.tmp", B64.encode(random::<12>()?)));
        let mut file = create_private(&temp)?;
        file.write_all(&serde_json::to_vec(&self.header).map_err(|_| VaultError::Format)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, self.dir.join(MANIFEST))?;
        #[cfg(unix)]
        File::open(&self.dir)?.sync_all()?;
        Ok(())
    }

    pub fn backup(&self, db: &Database, path: &Path) -> Result<()> {
        let snapshot = self
            .dir
            .join(format!("snapshot-{}.sqlite3", B64.encode(random::<12>()?)));
        if let Err(error) = db.export_encrypted(&snapshot, &self.key) {
            let _ = fs::remove_file(&snapshot);
            return Err(error.into());
        }
        let destination =
            path.with_file_name(format!(".selai-backup-{}.tmp", B64.encode(random::<12>()?)));
        let result = (|| {
            let header = serde_json::to_vec(&self.header).map_err(|_| VaultError::Format)?;
            if fs::metadata(&snapshot)?.len() > MAX_BACKUP - 20 - header.len() as u64 {
                return Err(VaultError::BackupTooLarge);
            }
            let mut out = create_private(&destination)?;
            out.write_all(MAGIC)?;
            out.write_all(&(header.len() as u32).to_le_bytes())?;
            out.write_all(&header)?;
            std::io::copy(&mut File::open(&snapshot)?, &mut out)?;
            out.sync_all()?;
            drop(out);
            fs::rename(&destination, path)?;
            Ok(())
        })();
        let _ = fs::remove_file(snapshot);
        let _ = fs::remove_file(destination);
        result
    }

    pub fn restore(dir: &Path, backup: &Path, passphrase: &str) -> Result<Self> {
        Self::restore_inner(dir, backup, passphrase, None).map(|(vault, _)| vault)
    }

    pub fn restore_with_recovery(
        dir: &Path,
        backup: &Path,
        recovery: &str,
        new_passphrase: &str,
    ) -> Result<(Self, Zeroizing<String>)> {
        let (vault, code) = Self::restore_inner(dir, backup, recovery, Some(new_passphrase))?;
        Ok((vault, code.ok_or(VaultError::Format)?))
    }

    fn restore_inner(
        dir: &Path,
        backup: &Path,
        credential: &str,
        new_passphrase: Option<&str>,
    ) -> Result<(Self, Option<Zeroizing<String>>)> {
        private_dir(dir)?;
        let mut input = File::open(backup)?;
        if input.metadata()?.len() > MAX_BACKUP {
            return Err(VaultError::BackupTooLarge);
        }
        let mut magic = [0u8; 16];
        input.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(VaultError::Format);
        }
        let mut length = [0u8; 4];
        input.read_exact(&mut length)?;
        let length = u32::from_le_bytes(length) as usize;
        if length > MAX_HEADER {
            return Err(VaultError::Format);
        }
        let mut bytes = vec![0; length];
        input.read_exact(&mut bytes)?;
        let mut header = parse_header(&bytes)?;
        let key = if new_passphrase.is_some() {
            if credential.len() > 128 {
                return Err(VaultError::Unlock);
            }
            let bytes = Zeroizing::new(
                B64.decode(credential.trim())
                    .map_err(|_| VaultError::Unlock)?,
            );
            let recovery: Zeroizing<[u8; 32]> = Zeroizing::new(
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| VaultError::Unlock)?,
            );
            unwrap(&recovery, &header.recovery, b"selai-recovery-v1")?
        } else {
            unwrap(
                &*derive(credential, &header)?,
                &header.passphrase,
                b"selai-passphrase-v1",
            )?
        };
        header.database = format!("vault-{}.sqlite3", B64.encode(random::<16>()?));
        let mut vault = Self {
            dir: dir.into(),
            header,
            key,
        };
        let mut out = create_private(&vault.database_path())?;
        std::io::copy(&mut input, &mut out)?;
        out.sync_all()?;
        drop(out);
        let db = Database::open_encrypted(&vault.database_path(), vault.key())?;
        db.integrity_check()?;
        db.checkpoint()?;
        drop(db);
        let code = if let Some(phrase) = new_passphrase {
            vault.rewrap(phrase)?;
            let recovery = Zeroizing::new(random::<32>()?);
            vault.header.recovery = wrap(&recovery, vault.key.as_ref(), b"selai-recovery-v1")?;
            Some(Zeroizing::new(B64.encode(recovery.as_ref())))
        } else {
            None
        };
        // Caller commits after validating replacement authorization.
        Ok((vault, code))
    }
}
