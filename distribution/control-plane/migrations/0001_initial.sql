PRAGMA foreign_keys = ON;

CREATE TABLE admins (
  id TEXT PRIMARY KEY,
  email TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('owner', 'operator')),
  created_at INTEGER NOT NULL
);

CREATE TABLE devices (
  id TEXT PRIMARY KEY,
  fingerprint_hash TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL CHECK (status IN ('active', 'revoked')) DEFAULT 'active',
  activated_at INTEGER NOT NULL,
  revoked_at INTEGER,
  last_seen_at INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE activation_codes (
  id TEXT PRIMARY KEY,
  code_hash TEXT NOT NULL UNIQUE,
  max_activations INTEGER NOT NULL CHECK (max_activations > 0),
  activation_count INTEGER NOT NULL DEFAULT 0 CHECK (activation_count >= 0),
  expires_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE releases (
  id TEXT PRIMARY KEY,
  version TEXT NOT NULL UNIQUE,
  status TEXT NOT NULL CHECK (status IN ('draft', 'testing', 'production', 'halted')) DEFAULT 'draft',
  notes TEXT NOT NULL DEFAULT '',
  pub_date TEXT NOT NULL,
  url TEXT NOT NULL,
  signature TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE download_tickets (
  id TEXT PRIMARY KEY,
  token_hash TEXT NOT NULL UNIQUE,
  device_id TEXT NOT NULL REFERENCES devices(id),
  release_id TEXT NOT NULL REFERENCES releases(id),
  expires_at INTEGER NOT NULL,
  consumed_at INTEGER,
  created_at INTEGER NOT NULL
);

CREATE TABLE update_events (
  id TEXT PRIMARY KEY,
  device_id TEXT NOT NULL REFERENCES devices(id),
  release_id TEXT REFERENCES releases(id),
  current_version TEXT NOT NULL,
  event_type TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE audit_logs (
  id TEXT PRIMARY KEY,
  admin_id TEXT REFERENCES admins(id),
  action TEXT NOT NULL,
  target_type TEXT NOT NULL,
  target_id TEXT,
  details_json TEXT NOT NULL DEFAULT '{}',
  created_at INTEGER NOT NULL
);

CREATE INDEX idx_admins_role ON admins(role);
CREATE INDEX idx_devices_status_last_seen ON devices(status, last_seen_at);
CREATE INDEX idx_activation_codes_expires_at ON activation_codes(expires_at);
CREATE INDEX idx_releases_status_version ON releases(status, version);
CREATE INDEX idx_download_tickets_token_hash ON download_tickets(token_hash);
CREATE INDEX idx_download_tickets_expires_at ON download_tickets(expires_at);
CREATE INDEX idx_update_events_device_created_at ON update_events(device_id, created_at);
CREATE INDEX idx_update_events_release_created_at ON update_events(release_id, created_at);
CREATE INDEX idx_audit_logs_admin_created_at ON audit_logs(admin_id, created_at);
