PRAGMA foreign_keys = ON;

CREATE TABLE app_users (
  id TEXT PRIMARY KEY,
  username TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  password_hash TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('operations_manager', 'anchor')),
  anchor_id TEXT,
  managed_team_ids_json TEXT NOT NULL DEFAULT '[]',
  must_change_password INTEGER NOT NULL DEFAULT 1 CHECK (must_change_password IN (0, 1)),
  failed_attempts INTEGER NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
  locked_until INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  disabled_at INTEGER,
  CHECK ((role = 'anchor' AND anchor_id IS NOT NULL) OR (role = 'operations_manager' AND anchor_id IS NULL))
);

CREATE TABLE app_user_sessions (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES app_users(id),
  token_hash TEXT NOT NULL UNIQUE,
  expires_at INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,
  revoked_at INTEGER
);

CREATE INDEX idx_app_users_role_active ON app_users(role, disabled_at);
CREATE INDEX idx_app_users_anchor_active ON app_users(anchor_id, disabled_at);
CREATE INDEX idx_app_sessions_token ON app_user_sessions(token_hash, revoked_at, expires_at);
CREATE INDEX idx_app_sessions_user ON app_user_sessions(user_id, revoked_at, expires_at);
