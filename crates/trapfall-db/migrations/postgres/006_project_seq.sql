-- Numeric per-project sequence used as the DSN project id.
-- Sentry JS SDKs require an all-digit project id in the DSN path (#328).
-- The UUID `id` stays the primary key; `seq` is the public, DSN-facing id.
CREATE SEQUENCE IF NOT EXISTS projects_seq_seq;

ALTER TABLE projects ADD COLUMN IF NOT EXISTS seq BIGINT;

-- Backfill existing rows in creation order. Idempotent: only touches NULL rows.
DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN SELECT id FROM projects WHERE seq IS NULL ORDER BY created_at, id LOOP
        UPDATE projects SET seq = nextval('projects_seq_seq') WHERE id = r.id;
    END LOOP;
END
$$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_projects_seq ON projects(seq);
