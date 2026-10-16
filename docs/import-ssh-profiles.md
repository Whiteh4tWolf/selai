# Import SSH profiles

Open **Hosts → Import SSH profiles**. Choose a plaintext CSV or JSON file, check the column mapping and preview, select the profiles, then import. Selai creates new hosts only; it does not overwrite an existing label or a matching `hostname:port` and username. An import can finish partially, and the result reports each failed row.

## Shared CSV format

Download the template in the import drawer, or use these columns:

```csv
label,hostname,port,username,auth_method,password,private_key_path,key_passphrase,notes
Production Web,web.example.com,22,deploy,password,,,,
Database,db.example.com,2222,admin,private_key,,/home/admin/.ssh/id_ed25519,,
```

`label`, `hostname`, and `username` identify a host. `port` defaults to `22`. `auth_method` is `password` or `private_key`; it can be omitted when `private_key_path` is present. For key authentication, the private key must already exist as a local file at the indicated path. Selai does not import private key contents. You may use an `ssh://user@host:port` or `sftp://user@host:port` value in a `uri` column in place of `hostname` and `username`.

Labels are cleaned to Selai's 48-character limit and allowed characters. Check the resulting names in the preview before importing.

The importer recognizes common alternate headers, including `name`, `address`, `uri`, `login_uri`, `login_username`, and `login_password`. The mapping can be changed in the preview. Plaintext JSON arrays and objects with an `items` or `hosts` array also work; Bitwarden login objects and host objects shaped like the [Termius API Bridge host payload](https://termius.com/api-docs/) are read into the same profile fields.

## Password manager exports

- **Bitwarden:** A plaintext CSV or JSON export can be selected directly when an SSH login has an `ssh://` or `sftp://` URI. Website logins and other vault item types are skipped. [Bitwarden documents its CSV and JSON fields](https://bitwarden.com/en-gb/help/condition-bitwarden-import/).
- **Passbolt:** Select its password-protected KDBX export and enter the export passphrase in Selai. The vault is decrypted locally; only entries whose URL starts with `ssh://` or `sftp://` appear in the preview. A permitted plaintext CSV export is also supported. [Passbolt documents KDBX export and passphrase use](https://www.passbolt.com/docs/user/basic-features/browser/export/).
- **Termius and other SSH managers:** Convert their exported host list to the shared CSV columns above, or use a compatible plaintext JSON host list. This importer does not connect to another app or account and does not decrypt a proprietary vault.

Passwords and key passphrases are never displayed in the preview. The existing host-save path writes them to the operating system's credential store. Notes are excluded by default because Selai stores host notes in its local database; enable the notes checkbox only when the source notes contain no secrets. Selai does not copy or retain the source export file, but the original plaintext file remains wherever you exported it.

CSV and JSON files are limited to 2 MB; KDBX files are limited to 10 MB. Imports are limited to 500 profiles. An import does not connect to a host or trust its SSH key. The first connection still uses Selai's host-key confirmation flow.
