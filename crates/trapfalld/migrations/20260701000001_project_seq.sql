-- Numeric per-project sequence used as the DSN project id.
-- Sentry JS SDKs require an all-digit project id in the DSN path (#328).
-- The UUID `id` stays the primary key; `seq` is the public, DSN-facing id.
ALTER TABLE projects ADD COLUMN seq INTEGER;

-- Backfill existing rows in creation order (1, 2, 3, ...).
UPDATE projects SET seq = (SELECT COUNT(*) FROM projects p2 WHERE p2.rowid <= projects.rowid);

CREATE UNIQUE INDEX IF NOT EXISTS idx_projects_seq ON projects(seq);
