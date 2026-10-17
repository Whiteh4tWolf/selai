# Import SSH profiles

Open **Hosts → Import SSH profiles**. Choose a plaintext CSV/JSON file, a password-protected KDBX export, or an OpenSSH config file. Check the mapping and preview, select the profiles, then import. Selai creates new hosts only; it does not overwrite an existing label or a matching `hostname:port` and username. An import can finish partially, and the result reports each failed row.

## OpenSSH config

Upload a file named `config`, `ssh_config`, `ssh-config`, or ending in
`.sshconfig` through **Profile export or SSH config file**. Alternatively, click
**Import ~/.ssh/config** to read the current user's default config on this
computer. Both paths show the same preview and require selecting rows before
saving. On Windows, the default path is `%USERPROFILE%\.ssh\config`.

```sshconfig
Host production-web
    HostName web.example.com
    User deploy
    Port 2222
    IdentityFile ~/.ssh/id_ed25519
    ConnectTimeout 15
    ServerAliveInterval 30
```

Each concrete `Host` alias becomes a candidate profile. The importer reads
`HostName`, `User`, `Port`, `IdentityFile`, `ConnectTimeout`, and
`ServerAliveInterval`. Missing `HostName` uses the alias; missing `User` makes
the row invalid. If several identity files are configured, only the first
non-`none` entry is used. Relative identity paths are resolved under the current
user's home directory, not beside the uploaded config. Key files must be
readable on this computer when the profiles are saved.

Wildcard `Host` patterns can supply options to concrete aliases but do not
create profiles themselves. `Include` files are not read and `Match` blocks
are ignored. Profiles requiring `ProxyJump` or `ProxyCommand` are skipped;
unsupported expansion tokens are reported in the preview. Review the imported
values before saving; this is an importer for common options, not a complete
OpenSSH configuration engine.

## Shared CSV format

Download the template in the import drawer, or use these columns:

```csv
label,hostname,port,username,auth_method,password,private_key_path,key_passphrase,notes
Production Web,web.example.com,22,deploy,password,,,,
Database,db.example.com,2222,admin,private_key,,/home/admin/.ssh/id_ed25519,,
```

`label`, `hostname`, and `username` identify a host. `port` defaults to `22`. `auth_method` is `password` or `private_key`; it can be omitted when `private_key_path` is present. For key authentication, the private key must exist as a readable local file at the indicated path. Selai copies its contents into the encrypted vault when saving the host; inline private keys in profile rows are not accepted. You may use an `ssh://user@host:port` or `sftp://user@host:port` value in a `uri` column in place of `hostname` and `username`.

Labels are cleaned to Selai's 48-character limit and allowed characters. Check the resulting names in the preview before importing.

The importer recognizes common alternate headers, including `name`, `address`, `uri`, `login_uri`, `login_username`, and `login_password`. The mapping can be changed in the preview. Plaintext JSON arrays and objects with an `items` or `hosts` array also work; Bitwarden login objects and host objects shaped like the [Termius API Bridge host payload](https://termius.com/api-docs/) are read into the same profile fields.

## Password manager exports

- **Bitwarden:** A plaintext CSV or JSON export can be selected directly when an SSH login has an `ssh://` or `sftp://` URI. Website logins and other vault item types are skipped. [Bitwarden documents its CSV and JSON fields](https://bitwarden.com/en-gb/help/condition-bitwarden-import/).
- **Passbolt:** Select its password-protected KDBX export and enter the export passphrase in Selai. The vault is decrypted locally; only entries whose URL starts with `ssh://` or `sftp://` appear in the preview. A permitted plaintext CSV export is also supported. [Passbolt documents KDBX export and passphrase use](https://www.passbolt.com/docs/user/basic-features/browser/export/).
- **Termius and other SSH managers:** Convert their exported host list to the shared CSV columns above, or use a compatible plaintext JSON host list. This importer does not connect to another app or account and does not decrypt a proprietary vault.

Passwords and key passphrases are never displayed in the preview. The host-save path writes them into the passphrase-protected encrypted vault. Notes are excluded by default to avoid retaining unnecessary information. A supplied private key path is imported into the vault; inline private keys in export rows are not supported. Selai does not retain the source export file, but its original copy remains wherever you exported it and needs separate protection.

To generate a key or reuse one across profiles, open a host's **Private key →
Manage SSH keys**. This manager imports private key files separately from host
profile exports and lets profiles select a named key. Keys imported through an
export's file-path column remain per-host credentials.

CSV, JSON, and OpenSSH config files are limited to 2 MB; KDBX files are limited to 10 MB. Imports are limited to 500 profiles. An import does not connect to a host or trust its SSH key. The first connection still uses Selai's host-key confirmation flow.
