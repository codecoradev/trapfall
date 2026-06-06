-- Migration 001: Initial schema
-- Solo mode MVP (no TEAMS tables)

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    dsn_key TEXT NOT NULL,
    dsn TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS issues (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id),
    fingerprint TEXT NOT NULL,
    title TEXT NOT NULL,
    culprit TEXT,
    status TEXT NOT NULL DEFAULT 'unresolved',
    level TEXT NOT NULL DEFAULT 'error',
    count INTEGER NOT NULL DEFAULT 1,
    user_count INTEGER NOT NULL DEFAULT 0,
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    UNIQUE(project_id, fingerprint)
);

CREATE INDEX IF NOT EXISTS idx_issues_project ON issues(project_id);
CREATE INDEX IF NOT EXISTS idx_issues_status ON issues(status);
CREATE INDEX IF NOT EXISTS idx_issues_last_seen ON issues(last_seen);

CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY NOT NULL,
    issue_id TEXT NOT NULL REFERENCES issues(id),
    project_id TEXT NOT NULL REFERENCES projects(id),
    data TEXT NOT NULL,
    received_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_issue ON events(issue_id);
CREATE INDEX IF NOT EXISTS idx_events_received ON events(received_at);

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'admin',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL REFERENCES users(id),
    token TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);

CREATE TABLE IF NOT EXISTS auth_attempts (
    id TEXT PRIMARY KEY NOT NULL,
    email TEXT NOT NULL,
    ip TEXT NOT NULL,
    success INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_auth_attempts_email ON auth_attempts(email, created_at);
CREATE INDEX IF NOT EXISTS idx_auth_attempts_ip ON auth_attempts(ip, created_at);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id),
    key_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- TEAMS tables DEFERRED to post-MVP
-- Migration 004 will add: teams, team_members, team_projects
-- See Outline Doc 11 for full schema
