pub mod types;
pub mod safepath;
pub mod parse;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use parking_lot::RwLock;
use thiserror::Error;

pub use types::*;

const PRIMARY_ATTACHMENTS_DIR: &str = "attachements";
const INTERNAL_VAULT_DIR: &str = ".zenvoy";
const VAULT_SETTINGS_FILE: &str = "vault.json";
const NOTE_COMMENTS_DIR: &str = "comments";
const WELCOME_NOTE: &str = "# Welcome to Zenvoy\n\nThis is your first note. Start writing!\n";

static RESERVED_ROOT_NAMES: &[&str] = &[
    "inbox", "quick", "archive", "trash", "attachements", "_assets", ".zenvoy",
];

static VALID_FOLDER_ICON_IDS: &[&str] = &[
    "folder", "bolt", "tray", "archive", "trash", "book", "bookmark", "calendar",
    "briefcase", "tag", "document", "sparkle", "code", "user", "star", "heart",
    "link", "lightbulb", "flask", "graduation", "music", "image", "palette",
    "terminal", "wrench", "globe", "map", "chart", "home",
];

#[derive(Error, Debug)]
pub enum VaultError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("path escapes vault root")]
    PathEscape,
    #[error("invalid folder: {0}")]
    InvalidFolder(String),
    #[error("note not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("asset too large")]
    AssetTooLarge,
}

pub type VaultResult<T> = Result<T, VaultError>;

pub struct Vault {
    root: PathBuf,
    file_mode: u32,
    dir_mode: u32,
    max_asset_bytes: i64,
    meta_cache: RwLock<HashMap<String, NoteMetaCacheEntry>>,
}

struct NoteMetaCacheEntry {
    mtime_ms: f64,
    _size: i64,
    meta: NoteMeta,
}

impl Vault {
    pub fn new(root: impl AsRef<Path>, opts: VaultOptions) -> VaultResult<Self> {
        let root_path = root.as_ref();
        fs::create_dir_all(root_path)?;
        let abs = fs::canonicalize(root_path)?;
        let file_mode = if opts.file_mode == 0 { 0o600 } else { opts.file_mode };
        let dir_mode = if opts.dir_mode == 0 { 0o700 } else { opts.dir_mode };
        let max_asset_bytes = if opts.max_asset_bytes <= 0 { 50 << 20 } else { opts.max_asset_bytes };

        let vault = Self {
            root: abs,
            file_mode,
            dir_mode,
            max_asset_bytes,
            meta_cache: RwLock::new(HashMap::new()),
        };
        vault.ensure_layout()?;
        Ok(vault)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn info(&self) -> VaultInfo {
        VaultInfo {
            root: self.root.to_string_lossy().to_string(),
            name: self.root.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        }
    }

    fn settings_path(&self) -> PathBuf {
        self.root.join(INTERNAL_VAULT_DIR).join(VAULT_SETTINGS_FILE)
    }

    fn comments_root(&self) -> PathBuf {
        self.root.join(INTERNAL_VAULT_DIR).join(NOTE_COMMENTS_DIR)
    }

    fn infer_primary_notes_location(&self) -> String {
        let entries = match fs::read_dir(&self.root) {
            Ok(e) => e,
            Err(_) => return "inbox".to_string(),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { continue; }
            if RESERVED_ROOT_NAMES.contains(&name.as_str()) { continue; }
            if entry.path().is_dir() || name.to_lowercase().ends_with(".md") {
                return "root".to_string();
            }
        }
        "inbox".to_string()
    }

    fn vault_looks_empty(&self) -> bool {
        let entries = match fs::read_dir(&self.root) {
            Ok(e) => e,
            Err(_) => return true,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == INTERNAL_VAULT_DIR { continue; }
            return false;
        }
        true
    }

    pub fn get_settings(&self) -> VaultResult<VaultSettings> {
        let fallback = self.infer_primary_notes_location();
        let path = self.settings_path();
        match fs::read_to_string(&path) {
            Ok(raw) => {
                let settings: VaultSettings = serde_json::from_str(&raw)?;
                Ok(normalize_vault_settings(settings, &fallback))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(normalize_vault_settings(VaultSettings::default(), &fallback))
            }
            Err(e) => Err(VaultError::Io(e)),
        }
    }

    pub fn set_settings(&self, next: VaultSettings) -> VaultResult<VaultSettings> {
        let fallback = self.infer_primary_notes_location();
        let normalized = normalize_vault_settings(next, &fallback);
        let path = self.settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&normalized)?;
        fs::write(&path, json)?;
        if normalized.primary_notes_location == "inbox" {
            fs::create_dir_all(self.root.join("inbox"))?;
        }
        self.invalidate_caches();
        Ok(normalized)
    }

    fn primary_notes_root(&self) -> VaultResult<PathBuf> {
        let settings = self.get_settings()?;
        if settings.primary_notes_location == "root" {
            Ok(self.root.clone())
        } else {
            Ok(self.root.join("inbox"))
        }
    }

    pub fn folder_root(&self, folder: &NoteFolder) -> VaultResult<PathBuf> {
        match folder {
            NoteFolder::Inbox => self.primary_notes_root(),
            other => Ok(self.root.join(other.as_str())),
        }
    }

    fn ensure_layout(&self) -> VaultResult<()> {
        let was_empty = self.vault_looks_empty();

        // Ensure internal directory exists first
        fs::create_dir_all(self.root.join(INTERNAL_VAULT_DIR))?;

        let settings = self.get_settings()?;

        // Create system folders
        for folder in &[NoteFolder::Inbox, NoteFolder::Quick, NoteFolder::Archive, NoteFolder::Trash] {
            if *folder == NoteFolder::Inbox && settings.primary_notes_location == "root" {
                continue;
            }
            fs::create_dir_all(self.root.join(folder.as_str()))?;
        }

        // Create attachments dir
        fs::create_dir_all(self.root.join(PRIMARY_ATTACHMENTS_DIR))?;

        // Seed welcome note if vault is brand new
        if was_empty {
            let notes_root = self.primary_notes_root()?;
            fs::create_dir_all(&notes_root)?;
            let welcome = notes_root.join("Welcome.md");
            if !welcome.exists() {
                fs::write(&welcome, WELCOME_NOTE)?;
            }
        }

        Ok(())
    }

    fn invalidate_caches(&self) {
        let mut cache = self.meta_cache.write();
        cache.clear();
    }
}

fn normalize_vault_settings(mut settings: VaultSettings, fallback_primary: &str) -> VaultSettings {
    if settings.primary_notes_location.is_empty() {
        settings.primary_notes_location = fallback_primary.to_string();
    }
    if settings.primary_notes_location != "root" && settings.primary_notes_location != "inbox" {
        settings.primary_notes_location = "inbox".to_string();
    }
    settings.daily_notes.directory = normalize_daily_notes_dir(&settings.daily_notes.directory);
    settings.weekly_notes.directory = normalize_weekly_notes_dir(&settings.weekly_notes.directory);

    // Validate folder icons
    let mut valid_icons = HashMap::new();
    for (key, value) in &settings.folder_icons {
        if key.is_empty() { continue; }
        if VALID_FOLDER_ICON_IDS.contains(&value.as_str()) {
            valid_icons.insert(key.clone(), value.clone());
        }
    }
    settings.folder_icons = valid_icons;
    settings
}

fn normalize_daily_notes_dir(value: &str) -> String {
    let trimmed = value.trim_matches('/').trim();
    if trimmed.is_empty() { "Daily Notes".to_string() } else { trimmed.to_string() }
}

fn normalize_weekly_notes_dir(value: &str) -> String {
    let trimmed = value.trim_matches('/').trim();
    if trimmed.is_empty() { "Weekly Notes".to_string() } else { trimmed.to_string() }
}

pub fn folder_icon_key(folder: &NoteFolder, subpath: &str) -> String {
    format!("{}:{}", folder.as_str(), subpath)
}

pub fn rewrite_folder_icons_for_rename(
    icons: &HashMap<String, String>,
    folder: &NoteFolder,
    old_subpath: &str,
    new_subpath: &str,
) -> HashMap<String, String> {
    let exact_key = folder_icon_key(folder, old_subpath);
    let prefix = format!("{}/", exact_key);
    let mut next = HashMap::new();
    for (key, value) in icons {
        if key == &exact_key {
            next.insert(folder_icon_key(folder, new_subpath), value.clone());
        } else if key.starts_with(&prefix) {
            let suffix = &key[exact_key.len()..];
            next.insert(format!("{}{}", folder_icon_key(folder, new_subpath), suffix), value.clone());
        } else {
            next.insert(key.clone(), value.clone());
        }
    }
    next
}

pub fn remove_folder_icons(
    icons: &HashMap<String, String>,
    folder: &NoteFolder,
    subpath: &str,
) -> HashMap<String, String> {
    let exact_key = folder_icon_key(folder, subpath);
    let prefix = format!("{}/", exact_key);
    icons.iter()
        .filter(|(key, _)| *key != &exact_key && !key.starts_with(&prefix))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_vault() -> (TempDir, Vault) {
        let dir = TempDir::new().unwrap();
        let vault = Vault::new(dir.path(), VaultOptions::default()).unwrap();
        (dir, vault)
    }

    #[test]
    fn test_vault_creation_creates_layout() {
        let (_dir, vault) = test_vault();
        assert!(vault.root().join("inbox").is_dir());
        assert!(vault.root().join("quick").is_dir());
        assert!(vault.root().join("archive").is_dir());
        assert!(vault.root().join("trash").is_dir());
        assert!(vault.root().join(PRIMARY_ATTACHMENTS_DIR).is_dir());
        assert!(vault.root().join(INTERNAL_VAULT_DIR).is_dir());
    }

    #[test]
    fn test_vault_seeds_welcome_note() {
        let (_dir, vault) = test_vault();
        let welcome = vault.root().join("inbox").join("Welcome.md");
        assert!(welcome.exists());
        let content = fs::read_to_string(&welcome).unwrap();
        assert!(content.contains("Welcome"));
    }

    #[test]
    fn test_vault_info() {
        let (_dir, vault) = test_vault();
        let info = vault.info();
        assert!(!info.root.is_empty());
        assert!(!info.name.is_empty());
    }

    #[test]
    fn test_get_default_settings() {
        let (_dir, vault) = test_vault();
        let settings = vault.get_settings().unwrap();
        assert_eq!(settings.primary_notes_location, "inbox");
        assert_eq!(settings.daily_notes.directory, "Daily Notes");
        assert_eq!(settings.weekly_notes.directory, "Weekly Notes");
    }

    #[test]
    fn test_set_settings_persists() {
        let (_dir, vault) = test_vault();
        let mut settings = vault.get_settings().unwrap();
        settings.daily_notes.enabled = true;
        settings.daily_notes.directory = "Journal".to_string();
        let saved = vault.set_settings(settings).unwrap();
        assert!(saved.daily_notes.enabled);
        assert_eq!(saved.daily_notes.directory, "Journal");
        // Re-read
        let reloaded = vault.get_settings().unwrap();
        assert!(reloaded.daily_notes.enabled);
        assert_eq!(reloaded.daily_notes.directory, "Journal");
    }

    #[test]
    fn test_settings_normalizes_invalid_folder_icons() {
        let (_dir, vault) = test_vault();
        let mut settings = vault.get_settings().unwrap();
        settings.folder_icons.insert("inbox:projects".to_string(), "invalid_icon".to_string());
        settings.folder_icons.insert("inbox:docs".to_string(), "book".to_string());
        let saved = vault.set_settings(settings).unwrap();
        assert!(!saved.folder_icons.contains_key("inbox:projects"));
        assert_eq!(saved.folder_icons.get("inbox:docs").unwrap(), "book");
    }

    #[test]
    fn test_folder_icon_rename() {
        let mut icons = HashMap::new();
        icons.insert("inbox:projects".to_string(), "code".to_string());
        icons.insert("inbox:projects/sub".to_string(), "terminal".to_string());
        icons.insert("inbox:other".to_string(), "book".to_string());

        let result = rewrite_folder_icons_for_rename(&icons, &NoteFolder::Inbox, "projects", "work");
        assert_eq!(result.get("inbox:work").unwrap(), "code");
        assert_eq!(result.get("inbox:work/sub").unwrap(), "terminal");
        assert_eq!(result.get("inbox:other").unwrap(), "book");
        assert!(!result.contains_key("inbox:projects"));
    }

    #[test]
    fn test_remove_folder_icons() {
        let mut icons = HashMap::new();
        icons.insert("inbox:projects".to_string(), "code".to_string());
        icons.insert("inbox:projects/sub".to_string(), "terminal".to_string());
        icons.insert("inbox:other".to_string(), "book".to_string());

        let result = remove_folder_icons(&icons, &NoteFolder::Inbox, "projects");
        assert!(!result.contains_key("inbox:projects"));
        assert!(!result.contains_key("inbox:projects/sub"));
        assert_eq!(result.get("inbox:other").unwrap(), "book");
    }
}
