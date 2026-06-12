-- Add archived_at column to projects for soft-delete/archive.
ALTER TABLE projects ADD COLUMN archived_at TEXT DEFAULT NULL;
