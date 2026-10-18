CREATE TABLE network_profiles (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('proxy', 'forward')),
  config_json TEXT NOT NULL,
  updated_at_ms INTEGER NOT NULL
);
CREATE INDEX network_profiles_kind ON network_profiles(kind);
CREATE TABLE host_proxies (
  host_id TEXT PRIMARY KEY REFERENCES hosts(id) ON DELETE CASCADE,
  proxy_id TEXT NOT NULL REFERENCES network_profiles(id) ON DELETE RESTRICT
);
