pub mod types;
pub mod safepath;
pub mod parse;

use std::fs;
use std::path::{Path, PathBuf};
use parking_lot::RwLock;
use thiserror::Error;

pub use types::*;

const PRIMARY_ATTACHMENTS_DIR: &str = "attachements";
const INTERNAL_VAULT_DIR: &str = ".zenvoy";
const VAULT_SETTINGS_FILE: &str = "vault.json";

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
    meta_cache: RwLock<std::collections::HashMap<String, NoteMetaCacheEntry>>,
}

struct NoteMetaCacheEntry {
    mtime_ms: f64,
    size: i64,
    meta: NoteMeta,
}

impl Vault {
    pub fn new(root: impl AsRef<Path>, opts: VaultOptions) -> VaultResult<Self> {
        let root = root.as_ref().canonicalize().unwrap_or_else(|_| root.as_ref().to_path_buf());
        let file_mode = if opts.file_mode == 0 { 0o600 } else { opts.file_mode };
        let dir_mode = if opts.dir_mode == 0 { 0o700 } else { opts.dir_mode };
        let max_asset_bytes = if opts.max_asset_bytes <= 0 { 50 << 20 } else { opts.max_asset_bytes };

        fs::create_dir_all(&root)?;

        let vault = Self {
            root,
            file_mode,
            dir_mode,
            max_asset_bytes,
            meta_cache: RwLock::new(std::collections::HashMap::new()),
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

    fn ensure_layout(&self) -> VaultResult<()> {
        let dirs = [
            self.root.join("inbox"),
            self.root.join("quick"),
            self.root.join("archive"),
            self.root.join("trash"),
            self.root.join(PRIMARY_ATTACHMENTS_DIR),
            self.root.join(INTERNAL_VAULT_DIR),
        ];
        for dir in &dirs {
            fs::create_dir_all(dir)?;
        }
        let settings_path = self.root.join(INTERNAL_VAULT_DIR).join(VAULT_SETTINGS_FILE);
        if !settings_path.exists() {
            let default = VaultSettings::default();
            let json = serde_json::to_string_pretty(&default)?;
            fs::write(&settings_path, json)?;
        }
        Ok(())
    }

    pub fn get_settings(&self) -> VaultResult<VaultSettings> {
        let path = self.root.join(INTERNAL_VAULT_DIR).join(VAULT_SETTINGS_FILE);
        let content = fs::read_to_string(&path)?;
        let settings: VaultSettings = serde_json::from_str(&content)?;
        Ok(normalize_vault_settings(settings))
    }

    pub fn set_settings(&self, next: VaultSettings) -> VaultResult<VaultSettings> {
        let normalized = normalize_vault_settings(next);
        let path = self.root.join(INTERNAL_VAULT_DIR).join(VAULT_SETTINGS_FILE);
        let json = serde_json::to_string_pretty(&normalized)?;
        fs::write(&path, json)?;
        Ok(normalized)
    }
}

fn normalize_vault_settings(mut settings: VaultSettings) -> VaultSettings {
    if settings.primary_notes_location.is_empty() {
        settings.primary_notes_location = "inbox".to_string();
    }
    if settings.primary_notes_location != "root" {
        settings.primary_notes_location = "inbox".to_string();
    }
    if settings.daily_notes.directory.is_empty() {
        settings.daily_notes.directory = "Daily Notes".to_string();
    }
    if settings.weekly_notes.directory.is_empty() {
        settings.weekly_notes.directory = "Weekly Notes".to_string();
    }
    settings
}
