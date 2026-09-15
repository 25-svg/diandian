PRAGMA foreign_keys = ON;

ALTER TABLE admins ADD COLUMN must_change_password INTEGER NOT NULL DEFAULT 1 CHECK (must_change_password IN (0, 1));
ALTER TABLE admins ADD COLUMN failed_attempts INTEGER NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0);
ALTER TABLE admins ADD COLUMN locked_until INTEGER;
ALTER TABLE admins ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;

CREATE TABLE admin_sessions (
  id TEXT PRIMARY KEY,
  admin_id TEXT NOT NULL REFERENCES admins(id),
  token_hash TEXT NOT NULL UNIQUE,
  expires_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  revoked_at INTEGER
);

CREATE INDEX idx_admin_sessions_token ON admin_sessions(token_hash, revoked_at, expires_at);
CREATE INDEX idx_admin_sessions_admin ON admin_sessions(admin_id, revoked_at, expires_at);
