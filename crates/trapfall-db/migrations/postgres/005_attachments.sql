CREATE TABLE IF NOT EXISTS attachments (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    content_type TEXT,
    attachment_type TEXT,
    size_bytes INTEGER NOT NULL,
    disk_path TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_attachments_event ON attachments(event_id);
CREATE INDEX IF NOT EXISTS idx_attachments_project ON attachments(project_id);
