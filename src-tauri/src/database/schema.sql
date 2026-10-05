-- Download history persistence: stores completed/errored task records
-- independently from the aria2 session file (which only keeps active tasks).
CREATE TABLE IF NOT EXISTS download_history (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  gid           TEXT    NOT NULL UNIQUE,
  name          TEXT    NOT NULL,
  uri           TEXT,
  dir           TEXT,
  total_length  INTEGER DEFAULT 0,
  status        TEXT    NOT NULL,
  task_type     TEXT    DEFAULT 'uri',
  added_at      DATETIME,
  created_at    DATETIME DEFAULT CURRENT_TIMESTAMP,
  completed_at  DATETIME,
  meta          TEXT
);

CREATE INDEX IF NOT EXISTS idx_dh_status    ON download_history(status);
CREATE INDEX IF NOT EXISTS idx_dh_completed ON download_history(completed_at);

-- Default database browsing reads in index order without a temporary sort.
CREATE INDEX IF NOT EXISTS idx_dh_order
  ON download_history(COALESCE(added_at, completed_at) DESC, id DESC);
CREATE TABLE IF NOT EXISTS task_birth (gid TEXT PRIMARY KEY, added_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE IF NOT EXISTS bt_selection (
    gid TEXT PRIMARY KEY,
    policy TEXT NOT NULL,
    deferred INTEGER NOT NULL DEFAULT 0 CHECK(deferred IN (0, 1))
);
-- HTTP Basic Auth credentials scoped by normalized URL origin.
CREATE TABLE IF NOT EXISTS http_auth_credentials (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  origin        TEXT     NOT NULL,
  username      TEXT     NOT NULL,
  password      TEXT     NOT NULL,
  created_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  last_used_at  DATETIME,
  UNIQUE(origin, username)
);

CREATE INDEX IF NOT EXISTS idx_http_auth_credentials_origin
  ON http_auth_credentials(origin);

-- Pending confirmation retains its request until submission; receipts retain only identities.
CREATE TABLE IF NOT EXISTS download_submissions (
  id TEXT PRIMARY KEY,
  fingerprint TEXT NOT NULL,
  gid TEXT NOT NULL UNIQUE,
  state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','confirming','submitted','cancelled')),
  request TEXT
);

CREATE TABLE IF NOT EXISTS file_renames (
    gid TEXT PRIMARY KEY REFERENCES download_history(gid) ON DELETE CASCADE,
    old_path TEXT NOT NULL,
    new_path TEXT NOT NULL,
    name TEXT NOT NULL
);
