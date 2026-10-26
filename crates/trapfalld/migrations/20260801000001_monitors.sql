-- Sentry Crons: monitors (one per project+slug, auto-registered on first
-- check-in) and their check-in history.
CREATE TABLE IF NOT EXISTS monitors (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    slug TEXT NOT NULL,
    schedule TEXT,
    checkin_margin INTEGER,
    last_seen TEXT NOT NULL,
    last_status TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_monitors_project_slug ON monitors(project_id, slug);

CREATE TABLE IF NOT EXISTS monitor_checkins (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES monitors(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    check_in_id TEXT NOT NULL,
    status TEXT NOT NULL,
    duration REAL,
    received_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_monitor_checkins_monitor ON monitor_checkins(monitor_id, received_at);
