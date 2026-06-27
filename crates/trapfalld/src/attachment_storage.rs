//! Disk storage for attachment binary data.
//!
//! Attachment metadata lives in the database; the raw binary content is
//! stored on the filesystem under a content-addressable directory layout:
//!
//! ```text
//! data/attachments/
//!   {project_id}/
//!     {attachment_id[:2]}/
//!       {attachment_id}
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result};

const ATTACHMENTS_DIR: &str = "data/attachments";

/// Filesystem-backed storage for attachment binary data.
pub struct AttachmentStorage {
    base_dir: PathBuf,
}

impl AttachmentStorage {
    /// Create a new attachment storage rooted at the given directory.
    ///
    /// If `base_dir` is `None`, defaults to `data/attachments`.
    pub fn new(base_dir: Option<PathBuf>) -> Self {
        let base = base_dir.unwrap_or_else(|| PathBuf::from(ATTACHMENTS_DIR));
        Self { base_dir: base }
    }

    /// Save attachment binary data to disk.
    ///
    /// Creates the nested directory structure `{base}/{project_id}/{id[:2]}/`
    /// and writes the file `{base}/{project_id}/{id[:2]}/{id}`.
    ///
    /// Returns the full path to the written file.
    pub fn save(&self, project_id: &str, attachment_id: &str, data: &[u8]) -> Result<PathBuf> {
        let dir = self.base_dir.join(project_id).join(&attachment_id[..2]);
        std::fs::create_dir_all(&dir).context("failed to create attachment directory")?;
        let file_path = dir.join(attachment_id);
        std::fs::write(&file_path, data).context("failed to write attachment file")?;
        Ok(file_path)
    }

    /// Read attachment binary data from a disk path.
    pub fn read(&self, disk_path: &str) -> Result<Vec<u8>> {
        std::fs::read(disk_path).context("failed to read attachment file")
    }

    /// Delete attachment binary data from disk.
    ///
    /// No-op if the file does not exist.
    pub fn delete(&self, disk_path: &str) -> Result<()> {
        let path = std::path::Path::new(disk_path);
        if path.exists() {
            std::fs::remove_file(path).context("failed to delete attachment file")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_and_read_attachment() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = AttachmentStorage::new(Some(tmp.path().to_path_buf()));
        let data = b"hello world attachment data";

        let path = storage.save("proj-123", "abc12345", data).unwrap();
        let read = storage.read(path.to_str().unwrap()).unwrap();
        assert_eq!(read, data);

        // Verify path structure
        assert!(path.to_str().unwrap().contains("proj-123"));
        assert!(path.to_str().unwrap().contains("ab"));
        assert!(path.to_str().unwrap().ends_with("abc12345"));
    }

    #[test]
    fn test_delete_attachment() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = AttachmentStorage::new(Some(tmp.path().to_path_buf()));
        let data = b"to be deleted";

        let path = storage.save("proj-456", "def67890", data).unwrap();
        assert!(path.exists());

        storage.delete(path.to_str().unwrap()).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_delete_nonexistent_is_ok() {
        let storage = AttachmentStorage::new(None);
        storage.delete("/nonexistent/path/file").unwrap();
    }
}
