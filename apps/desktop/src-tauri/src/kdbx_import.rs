//! Local-only parsing for password-protected KeePass-compatible exports.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use keepass::{db::fields, Database, DatabaseKey};
use serde::Serialize;
use zeroize::Zeroize;

const MAX_KDBX_BYTES: usize = 10 * 1024 * 1024;
const MAX_KDBX_ENTRIES: usize = 10_000;
const MAX_IMPORTED_PROFILES: usize = 500;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KdbxProfile {
    title: String,
    url: String,
    username: String,
    password: String,
    notes: String,
}

/// Decrypts the selected vault in-process and returns only SSH/SFTP entries.
/// The passphrase and decrypted database are never logged or written to disk.
pub fn preview(file_base64: &str, mut passphrase: String) -> Result<Vec<KdbxProfile>, String> {
    if passphrase.is_empty() {
        return Err("Enter the KDBX passphrase.".into());
    }
    if passphrase.len() > 4096 {
        passphrase.zeroize();
        return Err("The KDBX passphrase is too long.".into());
    }
    if file_base64.len() > (MAX_KDBX_BYTES * 4 / 3) + 8 {
        passphrase.zeroize();
        return Err("KDBX files must be 10 MB or smaller.".into());
    }

    let bytes = STANDARD.decode(file_base64).map_err(|_| {
        passphrase.zeroize();
        "The selected KDBX file could not be read.".to_string()
    })?;
    if bytes.len() > MAX_KDBX_BYTES {
        passphrase.zeroize();
        return Err("KDBX files must be 10 MB or smaller.".into());
    }

    let key = DatabaseKey::new().with_password(&passphrase);
    passphrase.zeroize();
    let database = Database::parse(&bytes, key).map_err(|_| {
        "Could not unlock this KDBX file. Check the passphrase and file format.".to_string()
    })?;

    if database.num_entries() > MAX_KDBX_ENTRIES {
        return Err("This KDBX vault has too many entries to import safely.".into());
    }

    let mut profiles = Vec::new();
    for entry in database.iter_all_entries() {
        if is_in_recycle_bin(&entry) {
            continue;
        }
        let Some(url) = entry.get(fields::URL).filter(|url| is_ssh_uri(url)) else {
            continue;
        };
        if profiles.len() == MAX_IMPORTED_PROFILES {
            return Err("KDBX imports are limited to 500 SSH profiles.".into());
        }
        profiles.push(KdbxProfile {
            title: entry.get(fields::TITLE).unwrap_or_default().to_owned(),
            url: url.to_owned(),
            username: entry.get(fields::USERNAME).unwrap_or_default().to_owned(),
            password: entry.get(fields::PASSWORD).unwrap_or_default().to_owned(),
            notes: entry.get(fields::NOTES).unwrap_or_default().to_owned(),
        });
    }

    if profiles.is_empty() {
        return Err("No entries with an SSH or SFTP URL were found in this KDBX file.".into());
    }
    Ok(profiles)
}

fn is_ssh_uri(value: &str) -> bool {
    let value = value.trim_start();
    value
        .get(..6)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("ssh://"))
        || value
            .get(..7)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("sftp://"))
}

fn is_in_recycle_bin(entry: &keepass::db::EntryRef<'_>) -> bool {
    group_or_ancestor_is_recycle_bin(&entry.parent())
}

/// Walks up the group chain. Recursion keeps each parent alive for its own call;
/// a loop that reassigns a borrowed parent does not compile.
fn group_or_ancestor_is_recycle_bin(group: &keepass::db::GroupRef<'_>) -> bool {
    group.name.eq_ignore_ascii_case("Recycle Bin")
        || group
            .parent()
            .is_some_and(|parent| group_or_ancestor_is_recycle_bin(&parent))
}

#[cfg(test)]
mod tests {
    use super::is_ssh_uri;

    #[test]
    fn recognizes_only_ssh_and_sftp_schemes() {
        assert!(is_ssh_uri("ssh://ops@example.com"));
        assert!(is_ssh_uri("SFTP://ops@example.com"));
        assert!(!is_ssh_uri("https://example.com"));
        assert!(!is_ssh_uri("example.com"));
    }
}
