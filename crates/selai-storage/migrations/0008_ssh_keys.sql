CREATE TABLE ssh_keys (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL COLLATE NOCASE UNIQUE,
  algorithm TEXT NOT NULL,
  public_key TEXT NOT NULL,
  fingerprint_sha256 TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL
);
