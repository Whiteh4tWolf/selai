CREATE TABLE vault_credentials (
  account TEXT PRIMARY KEY,
  secret BLOB NOT NULL,
  legacy_cleanup INTEGER NOT NULL DEFAULT 0 CHECK(legacy_cleanup IN (0,1))
);
CREATE TABLE vault_files (name TEXT PRIMARY KEY, data BLOB NOT NULL);
