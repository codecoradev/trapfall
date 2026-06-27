CREATE TABLE IF NOT EXISTS release_health (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    release TEXT NOT NULL,
    environment TEXT,
    started_at TEXT NOT NULL,
    distinct_id TEXT,
    exited INTEGER NOT NULL DEFAULT 0,
    errored INTEGER NOT NULL DEFAULT 0,
    abnormal INTEGER NOT NULL DEFAULT 0,
    crashed INTEGER NOT NULL DEFAULT 0,
    received_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_release_health_project ON release_health(project_id);
CREATE INDEX IF NOT EXISTS idx_release_health_release_env ON release_health(project_id, release, environment);
