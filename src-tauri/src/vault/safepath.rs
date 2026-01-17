use std::path::{Component, Path, PathBuf};
use super::VaultError;

/// Validate and resolve a relative path within the vault root.
/// Prevents directory traversal attacks.
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf, VaultError> {
    if rel.is_empty() {
        return Err(VaultError::PathEscape);
    }
    let normalized = rel.replace('\\', "/");
    let path = Path::new(&normalized);
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => return Err(VaultError::PathEscape),
        }
    }
    let joined = root.join(&normalized);
    // Canonicalize the root for comparison
    let canon_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    // If the file doesn't exist yet we can't canonicalize it, so check prefix
    let canon_joined = joined.canonicalize().unwrap_or_else(|_| joined.clone());
    if !canon_joined.starts_with(&canon_root) && !joined.starts_with(root) {
        return Err(VaultError::PathEscape);
    }
    Ok(joined)
}

/// Determine which NoteFolder a relative path belongs to.
pub fn folder_for_relative_path(rel: &str) -> Option<super::types::NoteFolder> {
    let normalized = rel.replace('\\', "/");
    let top = normalized.split('/').next().unwrap_or("");
    super::types::NoteFolder::from_str(top)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_safe_join_normal() {
        let root = Path::new("/tmp/vault");
        assert!(safe_join(root, "inbox/test.md").is_ok());
        assert!(safe_join(root, "quick/sub/note.md").is_ok());
    }

    #[test]
    fn test_safe_join_rejects_traversal() {
        let root = Path::new("/tmp/vault");
        assert!(safe_join(root, "../etc/passwd").is_err());
        assert!(safe_join(root, "inbox/../../secret").is_err());
        assert!(safe_join(root, "..").is_err());
    }

    #[test]
    fn test_safe_join_rejects_empty() {
        let root = Path::new("/tmp/vault");
        assert!(safe_join(root, "").is_err());
    }

    #[test]
    fn test_folder_for_relative_path() {
        use super::super::types::NoteFolder;
        assert_eq!(folder_for_relative_path("inbox/test.md"), Some(NoteFolder::Inbox));
        assert_eq!(folder_for_relative_path("trash/old.md"), Some(NoteFolder::Trash));
        assert_eq!(folder_for_relative_path("archive/done.md"), Some(NoteFolder::Archive));
        assert_eq!(folder_for_relative_path("quick/fast.md"), Some(NoteFolder::Quick));
        assert_eq!(folder_for_relative_path("random/thing.md"), None);
    }
}
