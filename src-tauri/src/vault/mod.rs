pub mod parse;
pub mod safepath;
pub mod types;

use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub use types::*;

const PRIMARY_ATTACHMENTS_DIR: &str = "attachements";
const INTERNAL_VAULT_DIR: &str = ".zenvoy";
const VAULT_SETTINGS_FILE: &str = "vault.json";
const NOTE_COMMENTS_DIR: &str = "comments";
const WELCOME_NOTE: &str = "# Welcome to Zenvoy\n\nThis is your first note. Start writing!\n";

static RESERVED_ROOT_NAMES: &[&str] = &[
    "inbox",
    "quick",
    "archive",
    "trash",
    "attachements",
    "_assets",
    ".zenvoy",
];

static VALID_FOLDER_ICON_IDS: &[&str] = &[
    "folder",
    "bolt",
    "tray",
    "archive",
    "trash",
    "book",
    "bookmark",
    "calendar",
    "briefcase",
    "tag",
    "document",
    "sparkle",
    "code",
    "user",
    "star",
    "heart",
    "link",
    "lightbulb",
    "flask",
    "graduation",
    "music",
    "image",
    "palette",
    "terminal",
    "wrench",
    "globe",
    "map",
    "chart",
    "home",
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
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
    #[error("asset too large")]
    AssetTooLarge,
}

pub type VaultResult<T> = Result<T, VaultError>;

pub struct Vault {
    root: PathBuf,
    _file_mode: u32,
    _dir_mode: u32,
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
        let file_mode = if opts.file_mode == 0 {
            0o600
        } else {
            opts.file_mode
        };
        let dir_mode = if opts.dir_mode == 0 {
            0o700
        } else {
            opts.dir_mode
        };
        let max_asset_bytes = if opts.max_asset_bytes <= 0 {
            50 << 20
        } else {
            opts.max_asset_bytes
        };

        let vault = Self {
            root: abs,
            _file_mode: file_mode,
            _dir_mode: dir_mode,
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
            name: self
                .root
                .file_name()
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
            if name.starts_with('.') {
                continue;
            }
            if RESERVED_ROOT_NAMES.contains(&name.as_str()) {
                continue;
            }
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
            if name.starts_with('.') || name == INTERNAL_VAULT_DIR {
                continue;
            }
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
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(normalize_vault_settings(
                VaultSettings::default(),
                &fallback,
            )),
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
        for folder in &[
            NoteFolder::Inbox,
            NoteFolder::Quick,
            NoteFolder::Archive,
            NoteFolder::Trash,
        ] {
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

    pub fn list_notes(&self) -> VaultResult<Vec<NoteMeta>> {
        let settings = self.get_settings()?;
        let folders = [
            NoteFolder::Inbox,
            NoteFolder::Quick,
            NoteFolder::Archive,
            NoteFolder::Trash,
        ];
        let mut all = Vec::new();

        for folder in &folders {
            let base = self.folder_root(folder)?;
            if !base.is_dir() {
                continue;
            }
            let is_root_inbox =
                *folder == NoteFolder::Inbox && settings.primary_notes_location == "root";
            let mut dir_indices: HashMap<PathBuf, i32> = HashMap::new();

            self.walk_dir(&base, folder, is_root_inbox, &mut dir_indices, &mut all)?;
        }
        Ok(all)
    }

    fn walk_dir(
        &self,
        dir: &Path,
        folder: &NoteFolder,
        skip_reserved: bool,
        dir_indices: &mut HashMap<PathBuf, i32>,
        out: &mut Vec<NoteMeta>,
    ) -> VaultResult<()> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|a| a.file_name());

        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                if skip_reserved && RESERVED_ROOT_NAMES.contains(&name.as_str()) {
                    continue;
                }
                self.walk_dir(&path, folder, false, dir_indices, out)?;
            } else if name.to_lowercase().ends_with(".md") {
                let idx = dir_indices.entry(dir.to_path_buf()).or_insert(0);
                let sibling_order = *idx;
                *idx += 1;
                let mut meta = self.read_meta(folder, &path)?;
                meta.sibling_order = sibling_order;
                out.push(meta);
            }
        }
        Ok(())
    }

    pub fn read_note(&self, rel: &str) -> VaultResult<NoteContent> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let body = fs::read_to_string(&abs)?;
        let folder = self.folder_of(&abs);
        let meta = self.read_meta(&folder, &abs)?;
        Ok(NoteContent { meta, body })
    }

    pub fn write_note(&self, rel: &str, body: &str) -> VaultResult<NoteMeta> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&abs, body)?;
        self.invalidate_caches();
        let folder = self.folder_of(&abs);
        let mut meta = self.read_meta(&folder, &abs)?;
        // Use filename stem as title for write_note
        meta.title = abs
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        Ok(meta)
    }

    pub fn create_note(
        &self,
        folder: &NoteFolder,
        title: Option<&str>,
        subpath: Option<&str>,
    ) -> VaultResult<NoteMeta> {
        let stem = match title {
            Some(t) => {
                let s = sanitize_file_stem(t);
                if s.is_empty() {
                    default_title()
                } else {
                    s
                }
            }
            None => default_title(),
        };
        let base = self.folder_root(folder)?;
        let dir = match subpath {
            Some(sp) if !sp.is_empty() => base.join(sp),
            _ => base,
        };
        fs::create_dir_all(&dir)?;
        let path = unique_path(&dir, &stem, "md");
        fs::write(&path, "")?;
        self.invalidate_caches();
        self.read_meta(folder, &path)
    }

    pub fn rename_note(&self, rel: &str, next_title: &str) -> VaultResult<NoteMeta> {
        let notes_before = self.list_notes()?;
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let stem = sanitize_file_stem(next_title);
        let stem = if stem.is_empty() {
            default_title()
        } else {
            stem
        };
        let dir = abs.parent().unwrap();
        let dest = unique_path(dir, &stem, "md");
        fs::rename(&abs, &dest)?;
        self.invalidate_caches();
        let folder = self.folder_of(&dest);
        let mut meta = self.read_meta(&folder, &dest)?;
        // Use the intended title (from filename stem) rather than body heading
        meta.title = dest
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        self.rewrite_inbound_wikilinks(&notes_before, rel, next_title);
        Ok(meta)
    }

    pub fn rewrite_inbound_wikilinks(
        &self,
        notes_before: &[NoteMeta],
        old_path: &str,
        new_title: &str,
    ) {
        let notes: Vec<_> = notes_before
            .iter()
            .filter(|n| n.folder != NoteFolder::Trash)
            .collect();
        for note in &notes {
            if note.path == old_path {
                continue;
            }
            let abs = match safepath::safe_join(&self.root, &note.path) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let body = match fs::read_to_string(&abs) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let (new_body, count) =
                rewrite_wikilinks_for_rename(&body, notes_before, old_path, new_title);
            if count > 0 {
                let _ = fs::write(&abs, new_body);
            }
        }
    }

    pub fn create_excalidraw(
        &self,
        folder: &NoteFolder,
        subpath: &str,
        title: Option<&str>,
    ) -> VaultResult<NoteMeta> {
        let stem = match title {
            Some(t) => {
                let s = sanitize_file_stem(t);
                if s.is_empty() {
                    "Untitled drawing".to_string()
                } else {
                    s
                }
            }
            None => "Untitled drawing".to_string(),
        };
        let base = self.folder_root(folder)?;
        let dir = if subpath.is_empty() {
            base
        } else {
            base.join(subpath)
        };
        fs::create_dir_all(&dir)?;
        let path = unique_path(&dir, &stem, "excalidraw");
        let doc = serde_json::json!({
            "type": "excalidraw",
            "version": 2,
            "source": "zenvoy",
            "elements": [],
            "appState": {},
            "files": {}
        });
        fs::write(&path, serde_json::to_string_pretty(&doc).unwrap())?;
        self.invalidate_caches();
        self.read_meta(folder, &path)
    }

    pub fn rename_database(&self, csv_path: &str, new_title: &str) -> VaultResult<String> {
        let safe_name = new_title
            .trim()
            .chars()
            .map(|c| if "\\/:*?\"<>|".contains(c) { '-' } else { c })
            .collect::<String>();
        let safe_name = if safe_name.is_empty() {
            "Untitled Database".to_string()
        } else {
            safe_name
        };
        let csv_abs = safepath::safe_join(&self.root, csv_path)?;
        let form_dir = csv_abs
            .parent()
            .ok_or_else(|| VaultError::NotFound(csv_path.to_string()))?;
        let parent = form_dir
            .parent()
            .ok_or_else(|| VaultError::NotFound(csv_path.to_string()))?;
        let new_dir_name = format!("{}.form", safe_name);
        let new_dir = unique_path_dir(parent, &new_dir_name);
        fs::rename(form_dir, &new_dir)?;
        let csv_name = csv_abs.file_name().unwrap().to_string_lossy().to_string();
        let new_csv = new_dir.join(&csv_name);
        let rel = new_csv
            .strip_prefix(&self.root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        self.invalidate_caches();
        Ok(rel)
    }

    pub fn root_content_hidden_by_inbox_mode(&self) -> VaultResult<bool> {
        let settings = self.get_settings()?;
        if settings.primary_notes_location != "inbox" {
            return Ok(false);
        }
        // Check if vault looks like it was originally using root layout
        let inbox_dir = self.root.join("inbox");
        let has_root_md = fs::read_dir(&self.root)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.path().extension().map(|ext| ext == "md").unwrap_or(false))
            })
            .unwrap_or(false);
        Ok(has_root_md && inbox_dir.exists())
    }

    pub fn delete_note(&self, rel: &str) -> VaultResult<()> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        fs::remove_file(&abs)?;
        self.invalidate_caches();
        Ok(())
    }

    pub fn duplicate_note(&self, rel: &str) -> VaultResult<NoteMeta> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let stem = abs.file_stem().unwrap_or_default().to_string_lossy();
        let copy_stem = format!("{} copy", stem);
        let dir = abs.parent().unwrap();
        let dest = unique_path(dir, &copy_stem, "md");
        copy_file(&abs, &dest)?;
        self.invalidate_caches();
        let folder = self.folder_of(&dest);
        self.read_meta(&folder, &dest)
    }

    pub fn append_to_note(&self, rel: &str, body: &str, position: &str) -> VaultResult<NoteMeta> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let existing = fs::read_to_string(&abs)?;
        let combined = if position == "prepend" {
            format!("{}\n{}", body, existing)
        } else {
            format!("{}\n{}", existing, body)
        };
        fs::write(&abs, combined)?;
        self.invalidate_caches();
        let folder = self.folder_of(&abs);
        self.read_meta(&folder, &abs)
    }

    pub fn move_to_trash(&self, rel: &str) -> VaultResult<NoteMeta> {
        self.move_between_folders(rel, &NoteFolder::Trash)
    }

    pub fn restore_from_trash(&self, rel: &str) -> VaultResult<NoteMeta> {
        self.move_between_folders(rel, &NoteFolder::Inbox)
    }

    pub fn empty_trash(&self) -> VaultResult<()> {
        let trash = self.root.join("trash");
        if trash.is_dir() {
            for entry in fs::read_dir(&trash)? {
                let path = entry?.path();
                if path.is_dir() {
                    fs::remove_dir_all(&path)?;
                } else {
                    fs::remove_file(&path)?;
                }
            }
        }
        self.invalidate_caches();
        Ok(())
    }

    pub fn archive_note(&self, rel: &str) -> VaultResult<NoteMeta> {
        self.move_between_folders(rel, &NoteFolder::Archive)
    }

    pub fn unarchive_note(&self, rel: &str) -> VaultResult<NoteMeta> {
        self.move_between_folders(rel, &NoteFolder::Inbox)
    }

    pub fn move_note(
        &self,
        rel: &str,
        target: &NoteFolder,
        target_subpath: Option<&str>,
    ) -> VaultResult<NoteMeta> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let stem = abs
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let target_root = self.folder_root(target)?;
        let dest_dir = match target_subpath {
            Some(sp) if !sp.is_empty() => target_root.join(sp),
            _ => target_root,
        };
        fs::create_dir_all(&dest_dir)?;
        let dest = unique_path(&dest_dir, &stem, "md");
        fs::rename(&abs, &dest)?;
        self.invalidate_caches();
        self.read_meta(target, &dest)
    }

    fn move_between_folders(&self, rel: &str, target: &NoteFolder) -> VaultResult<NoteMeta> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let stem = abs
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let subpath = self.folder_subpath_of(&abs);
        let target_root = self.folder_root(target)?;
        let dest_dir = if subpath.is_empty() {
            target_root
        } else {
            target_root.join(&subpath)
        };
        fs::create_dir_all(&dest_dir)?;
        let dest = unique_path(&dest_dir, &stem, "md");
        fs::rename(&abs, &dest)?;
        self.invalidate_caches();
        self.read_meta(target, &dest)
    }

    fn folder_subpath_of(&self, abs: &Path) -> String {
        let rel = abs.strip_prefix(&self.root).unwrap_or(abs);
        let components: Vec<_> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
        // components: [folder, ...subpath..., filename]
        if components.len() <= 2 {
            return String::new();
        }
        // Skip first (folder name) and last (filename)
        components[1..components.len() - 1].join("/")
    }

    fn folder_of(&self, abs: &Path) -> NoteFolder {
        let rel = abs.strip_prefix(&self.root).unwrap_or(abs);
        let rel_str = rel.to_string_lossy();
        safepath::folder_for_relative_path(&rel_str).unwrap_or(NoteFolder::Inbox)
    }

    fn read_meta(&self, folder: &NoteFolder, abs_path: &Path) -> VaultResult<NoteMeta> {
        let key = abs_path.to_string_lossy().to_string();
        let fs_meta = fs::metadata(abs_path)?;
        let mtime_ms = fs_meta
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
            * 1000.0;

        {
            let cache = self.meta_cache.read();
            if let Some(entry) = cache.get(&key) {
                if (entry.mtime_ms - mtime_ms).abs() < 1.0 {
                    return Ok(entry.meta.clone());
                }
            }
        }

        let body = fs::read_to_string(abs_path)?;
        let stem = abs_path.file_stem().unwrap_or_default().to_string_lossy();
        let title = parse::extract_title(&body, &stem);
        let tags = parse::extract_tags(&body);
        let wikilinks = parse::extract_wikilinks(&body);
        let has_attachments = parse::body_has_local_asset(&body);
        let asset_embeds = parse::extract_asset_embeds(&body);
        let excerpt = parse::build_excerpt(&body);
        let size = fs_meta.len() as i64;

        let updated_at = (mtime_ms / 1000.0) as i64;
        let created_at = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let ct = fs_meta.ctime();
                if ct > 0 {
                    ct
                } else {
                    updated_at
                }
            }
            #[cfg(not(unix))]
            {
                updated_at
            }
        };

        let rel = abs_path.strip_prefix(&self.root).unwrap_or(abs_path);
        let path = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");

        let meta = NoteMeta {
            path,
            title,
            folder: folder.clone(),
            sibling_order: 0,
            created_at,
            updated_at,
            size,
            tags,
            wikilinks,
            has_attachments,
            excerpt,
            asset_embeds,
        };

        {
            let mut cache = self.meta_cache.write();
            cache.insert(
                key,
                NoteMetaCacheEntry {
                    mtime_ms,
                    _size: size,
                    meta: meta.clone(),
                },
            );
        }
        Ok(meta)
    }

    pub fn list_folders(&self) -> VaultResult<Vec<FolderEntry>> {
        let settings = self.get_settings()?;
        let folders = [
            NoteFolder::Inbox,
            NoteFolder::Quick,
            NoteFolder::Archive,
            NoteFolder::Trash,
        ];
        let mut result: Vec<FolderEntry> = Vec::new();

        for folder in &folders {
            let base = self.folder_root(folder)?;
            if !base.is_dir() {
                continue;
            }
            let is_root_inbox =
                *folder == NoteFolder::Inbox && settings.primary_notes_location == "root";
            self.walk_folders(&base, &base, folder, is_root_inbox, &mut result)?;
        }

        // Sort by folder then subpath
        result.sort_by(|a, b| {
            a.folder
                .as_str()
                .cmp(b.folder.as_str())
                .then(a.subpath.cmp(&b.subpath))
        });

        // Assign sibling_order per parent directory within each folder
        let mut parent_counts: HashMap<(String, String), i32> = HashMap::new();
        for entry in &mut result {
            let parent = if let Some(pos) = entry.subpath.rfind('/') {
                entry.subpath[..pos].to_string()
            } else {
                String::new()
            };
            let key = (entry.folder.as_str().to_string(), parent);
            let count = parent_counts.entry(key).or_insert(0);
            entry.sibling_order = *count;
            *count += 1;
        }

        Ok(result)
    }

    fn walk_folders(
        &self,
        base: &Path,
        dir: &Path,
        folder: &NoteFolder,
        skip_reserved: bool,
        out: &mut Vec<FolderEntry>,
    ) -> VaultResult<()> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|a| a.file_name());

        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if skip_reserved && RESERVED_ROOT_NAMES.contains(&name.as_str()) {
                continue;
            }
            let subpath = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push(FolderEntry {
                folder: folder.clone(),
                subpath,
                sibling_order: 0,
            });
            self.walk_folders(base, &path, folder, false, out)?;
        }
        Ok(())
    }

    pub fn create_folder(&self, folder: &NoteFolder, subpath: &str) -> VaultResult<()> {
        let base = self.folder_root(folder)?;
        let target = safepath::safe_join(&base, subpath)?;
        fs::create_dir_all(&target)?;
        Ok(())
    }

    pub fn rename_folder(
        &self,
        folder: &NoteFolder,
        old_subpath: &str,
        new_subpath: &str,
    ) -> VaultResult<String> {
        let base = self.folder_root(folder)?;
        let old_path = safepath::safe_join(&base, old_subpath)?;
        let new_path = safepath::safe_join(&base, new_subpath)?;
        if let Some(parent) = new_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&old_path, &new_path)?;
        // Update folder icons
        let mut settings = self.get_settings()?;
        settings.folder_icons = rewrite_folder_icons_for_rename(
            &settings.folder_icons,
            folder,
            old_subpath,
            new_subpath,
        );
        self.set_settings(settings)?;
        Ok(new_subpath.to_string())
    }

    pub fn delete_folder(&self, folder: &NoteFolder, subpath: &str) -> VaultResult<()> {
        let base = self.folder_root(folder)?;
        let target = safepath::safe_join(&base, subpath)?;
        fs::remove_dir_all(&target)?;
        // Update folder icons
        let mut settings = self.get_settings()?;
        settings.folder_icons = remove_folder_icons(&settings.folder_icons, folder, subpath);
        self.set_settings(settings)?;
        Ok(())
    }

    pub fn duplicate_folder(&self, folder: &NoteFolder, subpath: &str) -> VaultResult<String> {
        let base = self.folder_root(folder)?;
        let src = safepath::safe_join(&base, subpath)?;
        let original_name = src.file_name().unwrap().to_string_lossy().to_string();
        let copy_name = format!("{} copy", original_name);
        let parent = src.parent().unwrap();
        let dest = unique_dir(parent, &copy_name);
        copy_dir(&src, &dest)?;
        let new_subpath = dest
            .strip_prefix(&base)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        Ok(new_subpath)
    }

    // --- Asset management ---

    pub fn get_text_search_capabilities(&self) -> TextSearchCapabilities {
        TextSearchCapabilities {
            ripgrep: which_exists("rg"),
            fzf: which_exists("fzf"),
        }
    }

    pub fn search_vault_text(
        &self,
        query: &str,
        _backend: Option<&str>,
    ) -> VaultResult<Vec<TextSearchMatch>> {
        const MAX_RESULTS: usize = 200;
        if which_exists("rg") {
            if let Ok(results) = self.search_with_ripgrep(query, MAX_RESULTS) {
                return Ok(results);
            }
        }
        self.search_fallback(query, MAX_RESULTS)
    }

    fn search_with_ripgrep(&self, query: &str, max: usize) -> VaultResult<Vec<TextSearchMatch>> {
        let output = std::process::Command::new("rg")
            .args([
                "--no-heading",
                "-n",
                "--color",
                "never",
                "-g",
                "*.md",
                "--",
                query,
            ])
            .arg(&self.root)
            .output()?;
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if results.len() >= max {
                break;
            }
            // Format: <filepath>:<line_number>:<line_text>
            let Some((file_path, rest)) = line.split_once(':') else {
                continue;
            };
            let Some((line_num_str, line_text)) = rest.split_once(':') else {
                continue;
            };
            let Ok(line_number) = line_num_str.parse::<i32>() else {
                continue;
            };
            let rel = match Path::new(file_path).strip_prefix(&self.root) {
                Ok(r) => r
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("/"),
                Err(_) => continue,
            };
            if rel.starts_with("trash/") {
                continue;
            }
            let title = Path::new(file_path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let folder = safepath::folder_for_relative_path(&rel).unwrap_or(NoteFolder::Inbox);
            let offset = line_text
                .to_lowercase()
                .find(&query_lower)
                .map(|i| i as i32)
                .unwrap_or(0);
            results.push(TextSearchMatch {
                path: rel,
                title,
                folder,
                line_number,
                offset,
                line_text: line_text.to_string(),
            });
        }
        Ok(results)
    }

    fn search_fallback(&self, query: &str, max: usize) -> VaultResult<Vec<TextSearchMatch>> {
        let notes = self.list_notes()?;
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();
        for note in &notes {
            if note.folder == NoteFolder::Trash {
                continue;
            }
            if results.len() >= max {
                break;
            }
            let content = self.read_note(&note.path)?;
            for (i, line) in content.body.lines().enumerate() {
                if results.len() >= max {
                    break;
                }
                if let Some(offset) = line.to_lowercase().find(&query_lower) {
                    results.push(TextSearchMatch {
                        path: note.path.clone(),
                        title: Path::new(&note.path)
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        folder: note.folder.clone(),
                        line_number: (i + 1) as i32,
                        offset: offset as i32,
                        line_text: line.to_string(),
                    });
                }
            }
        }
        Ok(results)
    }

    pub fn scan_tasks(&self) -> VaultResult<Vec<VaultTask>> {
        let notes = self.list_notes()?;
        let mut tasks = Vec::new();
        for note in &notes {
            if note.folder == NoteFolder::Trash {
                continue;
            }
            let content = self.read_note(&note.path)?;
            tasks.extend(parse::parse_tasks(
                &note.path,
                &note.title,
                &note.folder,
                &content.body,
            ));
        }
        Ok(tasks)
    }

    pub fn scan_tasks_for_path(&self, rel: &str) -> VaultResult<Vec<VaultTask>> {
        let content = self.read_note(rel)?;
        Ok(parse::parse_tasks(
            &content.meta.path,
            &content.meta.title,
            &content.meta.folder,
            &content.body,
        ))
    }

    // --- Comments ---

    fn comments_path_for(&self, rel: &str) -> PathBuf {
        self.comments_root().join(format!("{}.comments.json", rel))
    }

    pub fn read_note_comments(&self, rel: &str) -> VaultResult<Vec<NoteComment>> {
        let path = self.comments_path_for(rel);
        match fs::read_to_string(&path) {
            Ok(raw) => {
                let wrapper: serde_json::Value = serde_json::from_str(&raw)?;
                let comments: Vec<NoteComment> = serde_json::from_value(
                    wrapper
                        .get("comments")
                        .cloned()
                        .unwrap_or(serde_json::Value::Array(vec![])),
                )?;
                Ok(normalize_comments(comments, rel))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
            Err(e) => Err(VaultError::Io(e)),
        }
    }

    pub fn write_note_comments(
        &self,
        rel: &str,
        comments: Vec<NoteComment>,
    ) -> VaultResult<Vec<NoteComment>> {
        let path = self.comments_path_for(rel);
        let normalized = normalize_comments(comments, rel);
        if normalized.is_empty() {
            let _ = fs::remove_file(&path);
            return Ok(vec![]);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let wrapper = serde_json::json!({ "version": 1, "comments": normalized });
        fs::write(&path, serde_json::to_string_pretty(&wrapper)?)?;
        Ok(normalized)
    }

    pub fn has_assets_dir(&self) -> bool {
        self.root.join("attachements").is_dir() || self.root.join("_assets").is_dir()
    }

    pub fn list_assets(&self) -> VaultResult<Vec<AssetMeta>> {
        let mut assets = Vec::new();
        for dir_name in &["attachements", "_assets"] {
            let dir = self.root.join(dir_name);
            if dir.is_dir() {
                self.walk_assets(&dir, &mut assets)?;
            }
        }
        assets.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
        Ok(assets)
    }

    fn walk_assets(&self, dir: &Path, out: &mut Vec<AssetMeta>) -> VaultResult<()> {
        let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|a| a.file_name());
        let mut idx = 0i32;
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                self.walk_assets(&path, out)?;
            } else {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().ends_with(".md") {
                    continue;
                }
                let ext = path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let rel = path.strip_prefix(&self.root).unwrap();
                let rel_str = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("/");
                let m = fs::metadata(&path)?;
                let mtime_ms = m
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64;
                out.push(AssetMeta {
                    path: rel_str,
                    name,
                    kind: kind_for_ext(&ext).to_string(),
                    sibling_order: idx,
                    size: m.len() as i64,
                    updated_at: mtime_ms,
                });
                idx += 1;
            }
        }
        Ok(())
    }

    pub fn import_files_to_note(
        &self,
        _note_path: &str,
        source_paths: &[String],
    ) -> VaultResult<Vec<ImportedAsset>> {
        let note_subfolder = note_path_to_asset_folder(_note_path);
        let dest_dir = self.root.join("attachements").join(&note_subfolder);
        fs::create_dir_all(&dest_dir)?;
        let mut results = Vec::new();
        for src in source_paths {
            let src_path = Path::new(src);
            let m = fs::metadata(src_path)?;
            if m.len() as i64 > self.max_asset_bytes {
                return Err(VaultError::AssetTooLarge);
            }
            let stem = src_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let ext = src_path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let dest = if ext.is_empty() {
                unique_path(&dest_dir, &stem, "bin")
            } else {
                let candidate = dest_dir.join(format!("{}.{}", stem, ext));
                if candidate.exists()
                    && fs::metadata(&candidate).map(|m| m.len()).unwrap_or(0) == m.len()
                {
                    // Same name and size — reuse existing asset
                    candidate
                } else if candidate.exists() {
                    unique_path(&dest_dir, &stem, &ext)
                } else {
                    candidate
                }
            };
            if !dest.exists() {
                fs::copy(src_path, &dest)?;
            }
            let dest_name = dest.file_name().unwrap().to_string_lossy().to_string();
            let kind = kind_for_ext(&ext);
            let rel_str = if note_subfolder.is_empty() {
                format!("attachements/{}", dest_name)
            } else {
                format!("attachements/{}/{}", note_subfolder, dest_name)
            };
            let markdown = if kind == "image" {
                format!("![[{}]]", rel_str)
            } else {
                format!("[[{}]]", rel_str)
            };
            results.push(ImportedAsset {
                name: dest_name,
                path: rel_str,
                markdown,
                kind: kind.to_string(),
            });
        }
        Ok(results)
    }

    pub fn rename_asset(&self, rel: &str, next_name: &str) -> VaultResult<AssetMeta> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let dest = abs.parent().unwrap().join(next_name);
        fs::rename(&abs, &dest)?;
        self.asset_meta(&dest)
    }

    pub fn move_asset(&self, rel: &str, target_dir: &str) -> VaultResult<AssetMeta> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let dest_dir = safepath::safe_join(&self.root, target_dir)?;
        fs::create_dir_all(&dest_dir)?;
        let dest = dest_dir.join(abs.file_name().unwrap());
        fs::rename(&abs, &dest)?;
        self.asset_meta(&dest)
    }

    pub fn duplicate_asset(&self, rel: &str) -> VaultResult<AssetMeta> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let stem = abs
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let ext = abs
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let copy_stem = format!("{} copy", stem);
        let dest = unique_path(abs.parent().unwrap(), &copy_stem, &ext);
        fs::copy(&abs, &dest)?;
        self.asset_meta(&dest)
    }

    pub fn delete_asset(&self, rel: &str) -> VaultResult<DeletedAsset> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let data = fs::read(&abs)?;
        let name = abs.file_name().unwrap().to_string_lossy().to_string();
        let ext = abs
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let rel_str = abs
            .strip_prefix(&self.root)
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        fs::remove_file(&abs)?;
        Ok(DeletedAsset {
            path: rel_str,
            name,
            kind: kind_for_ext(&ext).to_string(),
            data,
        })
    }

    pub fn restore_deleted_asset(&self, asset: &DeletedAsset) -> VaultResult<AssetMeta> {
        let abs = safepath::safe_join(&self.root, &asset.path)?;
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&abs, &asset.data)?;
        self.asset_meta(&abs)
    }

    fn asset_meta(&self, abs: &Path) -> VaultResult<AssetMeta> {
        let m = fs::metadata(abs)?;
        let name = abs.file_name().unwrap().to_string_lossy().to_string();
        let ext = abs
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let rel = abs.strip_prefix(&self.root).unwrap();
        let rel_str = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        let mtime_ms = m
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        Ok(AssetMeta {
            path: rel_str,
            name,
            kind: kind_for_ext(&ext).to_string(),
            sibling_order: 0,
            size: m.len() as i64,
            updated_at: mtime_ms,
        })
    }

    // --- Database (CSV) operations ---

    pub fn open_database(&self, rel: &str) -> VaultResult<DatabaseDoc> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let sidecar_path = PathBuf::from(format!("{}.base.json", abs.display()));
        let sidecar = if sidecar_path.is_file() {
            serde_json::from_str(&fs::read_to_string(&sidecar_path)?)?
        } else {
            self.infer_sidecar_from_csv(&abs)?
        };
        let rows = self.read_csv_rows(&abs)?;
        let title = abs
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        Ok(DatabaseDoc {
            sidecar,
            path: rel.to_string(),
            title,
            rows,
        })
    }

    pub fn write_database_rows(&self, rel: &str, rows: Vec<DbRow>) -> VaultResult<DatabaseDoc> {
        let abs = safepath::safe_join(&self.root, rel)?;
        let sidecar_path = PathBuf::from(format!("{}.base.json", abs.display()));
        let sidecar: DatabaseSidecar = if sidecar_path.is_file() {
            serde_json::from_str(&fs::read_to_string(&sidecar_path)?)?
        } else {
            self.infer_sidecar_from_csv(&abs)?
        };
        self.write_csv_rows(&abs, &sidecar, &rows)?;
        self.open_database(rel)
    }

    pub fn write_database_schema(
        &self,
        rel: &str,
        sidecar: DatabaseSidecar,
        rows: Vec<DbRow>,
    ) -> VaultResult<DatabaseDoc> {
        let abs = safepath::safe_join(&self.root, rel)?;
        let sidecar_path = PathBuf::from(format!("{}.base.json", abs.display()));
        fs::write(&sidecar_path, serde_json::to_string_pretty(&sidecar)?)?;
        self.write_csv_rows(&abs, &sidecar, &rows)?;
        self.open_database(rel)
    }

    pub fn create_database(
        &self,
        folder: &NoteFolder,
        subpath: &str,
        title: Option<&str>,
    ) -> VaultResult<DatabaseDoc> {
        let stem = title.map(sanitize_file_stem).unwrap_or_else(default_title);
        let stem = if stem.is_empty() {
            default_title()
        } else {
            stem
        };
        let base = self.folder_root(folder)?;
        let dir = if subpath.is_empty() {
            base
        } else {
            base.join(subpath)
        };
        fs::create_dir_all(&dir)?;
        let csv_path = unique_path(&dir, &stem, "csv");
        let sidecar_path = PathBuf::from(format!("{}.base.json", csv_path.display()));

        let headers = vec!["id".to_string(), "Title".to_string(), "Status".to_string()];
        let sidecar = self.default_sidecar(&headers);
        fs::write(&sidecar_path, serde_json::to_string_pretty(&sidecar)?)?;

        // Write empty CSV with just headers
        let mut wtr = csv::Writer::from_path(&csv_path)?;
        wtr.write_record(&headers)?;
        wtr.flush()?;

        let rel = csv_path
            .strip_prefix(&self.root)
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        let title_str = csv_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        Ok(DatabaseDoc {
            sidecar,
            path: rel,
            title: title_str,
            rows: vec![],
        })
    }

    pub fn create_record_page(
        &self,
        csv_path: &str,
        title: &str,
        body: &str,
    ) -> VaultResult<String> {
        let abs = safepath::safe_join(&self.root, csv_path)?;
        let db_name = abs
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let dir = abs.parent().unwrap().join(&db_name);
        fs::create_dir_all(&dir)?;
        let stem = sanitize_file_stem(title);
        let stem = if stem.is_empty() {
            default_title()
        } else {
            stem
        };
        let dest = unique_path(&dir, &stem, "md");
        fs::write(&dest, body)?;
        let rel = dest
            .strip_prefix(&self.root)
            .unwrap()
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        Ok(rel)
    }

    pub fn list_databases(&self) -> VaultResult<Vec<DatabaseSummary>> {
        let folders = [NoteFolder::Inbox, NoteFolder::Quick, NoteFolder::Archive];
        let mut result = Vec::new();
        for folder in &folders {
            let base = self.folder_root(folder)?;
            if !base.is_dir() {
                continue;
            }
            self.walk_csv(&base, folder, &mut result)?;
        }
        Ok(result)
    }

    fn walk_csv(
        &self,
        dir: &Path,
        folder: &NoteFolder,
        out: &mut Vec<DatabaseSummary>,
    ) -> VaultResult<()> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.walk_csv(&path, folder, out)?;
            } else if path.extension().map(|e| e == "csv").unwrap_or(false) {
                let row_count = self.count_csv_rows(&path);
                let rel = path
                    .strip_prefix(&self.root)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join("/");
                let title = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                out.push(DatabaseSummary {
                    path: rel,
                    title,
                    folder: folder.clone(),
                    row_count,
                });
            }
        }
        Ok(())
    }

    fn count_csv_rows(&self, abs: &Path) -> usize {
        let mut rdr = match csv::Reader::from_path(abs) {
            Ok(r) => r,
            Err(_) => return 0,
        };
        rdr.records().count()
    }

    fn read_csv_rows(&self, abs: &Path) -> VaultResult<Vec<DbRow>> {
        let mut rdr = csv::Reader::from_path(abs)?;
        let headers: Vec<String> = rdr.headers()?.iter().map(|h| h.to_string()).collect();
        let mut rows = Vec::new();
        for record in rdr.records() {
            let record = record?;
            let mut cells = HashMap::new();
            for (i, val) in record.iter().enumerate() {
                if let Some(header) = headers.get(i) {
                    cells.insert(header.clone(), val.to_string());
                }
            }
            let id = cells.get("id").cloned().unwrap_or_default();
            rows.push(DbRow { id, cells });
        }
        Ok(rows)
    }

    fn write_csv_rows(
        &self,
        abs: &Path,
        sidecar: &DatabaseSidecar,
        rows: &[DbRow],
    ) -> VaultResult<()> {
        let headers = self.field_names_from_sidecar(sidecar);
        let mut wtr = csv::Writer::from_path(abs)?;
        wtr.write_record(&headers)?;
        for row in rows {
            let record: Vec<String> = headers
                .iter()
                .map(|h| row.cells.get(h).cloned().unwrap_or_default())
                .collect();
            wtr.write_record(&record)?;
        }
        wtr.flush()?;
        Ok(())
    }

    fn field_names_from_sidecar(&self, sidecar: &DatabaseSidecar) -> Vec<String> {
        let mut names: Vec<String> = sidecar
            .fields
            .iter()
            .filter_map(|f| {
                f.get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .collect();
        if names.is_empty() {
            names = vec!["id".to_string(), "Title".to_string(), "Status".to_string()];
        }
        names
    }

    fn infer_sidecar_from_csv(&self, abs: &Path) -> VaultResult<DatabaseSidecar> {
        let mut rdr = csv::Reader::from_path(abs)?;
        let headers: Vec<String> = rdr.headers()?.iter().map(|h| h.to_string()).collect();
        Ok(self.default_sidecar(&headers))
    }

    // --- Templates ---

    fn templates_dir(&self) -> PathBuf {
        self.root.join(INTERNAL_VAULT_DIR).join("templates")
    }

    pub fn list_templates(&self) -> VaultResult<Vec<CustomTemplateFile>> {
        let dir = self.templates_dir();
        if !dir.is_dir() {
            return Ok(vec![]);
        }
        let mut results = Vec::new();
        for entry in fs::read_dir(&dir)?.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "md").unwrap_or(false) {
                let rel = format!(
                    ".zenvoy/templates/{}",
                    path.file_name().unwrap().to_string_lossy()
                );
                let raw = fs::read_to_string(&path)?;
                results.push(CustomTemplateFile {
                    source_path: rel,
                    raw,
                });
            }
        }
        results.sort_by(|a, b| a.source_path.cmp(&b.source_path));
        Ok(results)
    }

    pub fn read_template(&self, source_path: &str) -> VaultResult<String> {
        if !source_path.starts_with(".zenvoy/templates/")
            || !source_path.ends_with(".md")
            || source_path.contains("..")
        {
            return Err(VaultError::PathEscape);
        }
        let abs = self.root.join(source_path);
        if !abs.is_file() {
            return Err(VaultError::NotFound(source_path.to_string()));
        }
        Ok(fs::read_to_string(&abs)?)
    }

    pub fn write_template(&self, input: &WriteTemplateInput) -> VaultResult<CustomTemplateFile> {
        let dir = self.templates_dir();
        fs::create_dir_all(&dir)?;
        let slug = safe_slug(&input.slug);
        let path = unique_path(&dir, &slug, "md");
        fs::write(&path, &input.raw)?;
        let source_path = format!(
            ".zenvoy/templates/{}",
            path.file_name().unwrap().to_string_lossy()
        );
        if let Some(prev) = &input.previous_source_path {
            if prev != &source_path {
                let prev_abs = self.root.join(prev);
                if prev_abs.is_file() {
                    fs::remove_file(&prev_abs)?;
                }
            }
        }
        Ok(CustomTemplateFile {
            source_path,
            raw: input.raw.clone(),
        })
    }

    pub fn delete_template(&self, source_path: &str) -> VaultResult<()> {
        if !source_path.starts_with(".zenvoy/templates/")
            || !source_path.ends_with(".md")
            || source_path.contains("..")
        {
            return Err(VaultError::PathEscape);
        }
        let abs = self.root.join(source_path);
        if !abs.is_file() {
            return Err(VaultError::NotFound(source_path.to_string()));
        }
        fs::remove_file(&abs)?;
        Ok(())
    }

    // --- Demo Tour ---

    const DEMO_ENTRIES: &'static [(&'static str, &'static str)] = &[
        ("inbox/Getting Started.md", "# Getting Started with Zenvoy\n\nWelcome! Here's a quick overview of what you can do:\n\n## Writing Notes\n\nUse **bold**, *italic*, and ~~strikethrough~~ formatting.\n\n## Tasks\n\n- [ ] Try creating a new note\n- [ ] Explore the sidebar folders\n- [x] Open the demo tour ✓\n\n## Tags & Links\n\nOrganize with #tags and connect ideas with [[Quick Thought]].\n\n## Code\n\n```rust\nfn main() {\n    println!(\"Hello, Zenvoy!\");\n}\n```\n\n## Math\n\nInline $E = mc^2$ or block:\n\n$$\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}$$\n"),
        ("inbox/Daily Notes/2024-01-15.md", "# 2024-01-15\n\n## Morning\n\n- [ ] Review pull requests\n- [x] Stand-up meeting\n\n## Notes\n\nDiscussed the new feature with the team. See [[Web App]] for details.\n\n#daily #journal\n"),
        ("inbox/Projects/Web App.md", "# Web App\n\n## Overview\n\nBuilding a note-taking app with offline-first sync.\n\n## Tasks\n\n- [ ] Design the API schema !high\n- [ ] Set up CI/CD pipeline due:2024-02-01\n- [ ] Write integration tests\n- [x] Choose tech stack\n\n## Stack\n\n- Frontend: React + TypeScript\n- Backend: Rust + Tauri\n- Storage: SQLite\n\n#project #dev\n"),
        ("quick/Quick Thought.md", "# Quick Thought\n\nJust a fleeting idea captured on the go.\n\n> The best time to capture a thought is the moment it appears.\n\nRelated: [[Getting Started]]\n\n#idea\n"),
    ];

    pub fn generate_demo_tour(&self) -> VaultResult<VaultDemoTourResult> {
        let mut paths = Vec::new();
        for (rel, body) in Self::DEMO_ENTRIES {
            let abs = safepath::safe_join(&self.root, rel)?;
            if abs.exists() {
                continue;
            }
            if let Some(parent) = abs.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&abs, body)?;
            paths.push(rel.to_string());
        }
        self.invalidate_caches();
        Ok(VaultDemoTourResult {
            success: true,
            paths,
        })
    }

    pub fn remove_demo_tour(&self) -> VaultResult<VaultDemoTourResult> {
        let mut paths = Vec::new();
        for (rel, _) in Self::DEMO_ENTRIES {
            let abs = safepath::safe_join(&self.root, rel)?;
            if abs.is_file() {
                fs::remove_file(&abs)?;
                paths.push(rel.to_string());
            }
        }
        self.invalidate_caches();
        Ok(VaultDemoTourResult {
            success: true,
            paths,
        })
    }

    fn default_sidecar(&self, headers: &[String]) -> DatabaseSidecar {
        let fields: Vec<serde_json::Value> = headers
            .iter()
            .map(|name| serde_json::json!({ "name": name, "type": "text", "id": name }))
            .collect();
        let view_id = "default-view".to_string();
        DatabaseSidecar {
            version: 1,
            id_field_id: "id".to_string(),
            fields,
            views: vec![serde_json::json!({ "id": view_id, "type": "table" })],
            active_view_id: view_id,
            pages: None,
        }
    }
}

fn which_exists(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("--version")
        .output()
        .is_ok()
}

fn safe_slug(slug: &str) -> String {
    let s: String = slug
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "template".to_string()
    } else {
        s
    }
}

fn sanitize_file_stem(title: &str) -> String {
    title
        .chars()
        .filter(|c| !"/\\:*?\"<>|".contains(*c))
        .collect::<String>()
        .trim()
        .to_string()
}

fn default_title() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let dt = chrono_lite(secs);
    format!("Untitled-{}", dt)
}

fn chrono_lite(epoch: u64) -> String {
    let days = epoch / 86400;
    let time_of_day = epoch % 86400;
    let h = time_of_day / 3600;
    let m = (time_of_day % 3600) / 60;
    let s = time_of_day % 60;
    // Simple date calculation
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        y += 1;
    }
    let months_days: [i64; 12] = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut mon = 1;
    for md in months_days {
        if remaining < md {
            break;
        }
        remaining -= md;
        mon += 1;
    }
    let day = remaining + 1;
    format!("{:04}-{:02}-{:02}-{:02}{:02}{:02}", y, mon, day, h, m, s)
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let candidate = dir.join(format!("{}.{}", stem, ext));
    if !candidate.exists() {
        return candidate;
    }
    let mut i = 2;
    loop {
        let p = dir.join(format!("{} {}.{}", stem, i, ext));
        if !p.exists() {
            return p;
        }
        i += 1;
    }
}

fn unique_path_dir(parent: &Path, name: &str) -> PathBuf {
    let candidate = parent.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let mut i = 2;
    loop {
        let p = parent.join(format!("{} {}", name, i));
        if !p.exists() {
            return p;
        }
        i += 1;
    }
}

fn copy_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::copy(src, dst)?;
    Ok(())
}

fn unique_dir(parent: &Path, base: &str) -> PathBuf {
    let candidate = parent.join(base);
    if !candidate.exists() {
        return candidate;
    }
    let mut i = 2;
    loop {
        let p = parent.join(format!("{} {}", base, i));
        if !p.exists() {
            return p;
        }
        i += 1;
    }
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &dest_path)?;
        } else {
            fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
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
        if key.is_empty() {
            continue;
        }
        if VALID_FOLDER_ICON_IDS.contains(&value.as_str()) {
            valid_icons.insert(key.clone(), value.clone());
        }
    }
    settings.folder_icons = valid_icons;
    settings
}

fn normalize_daily_notes_dir(value: &str) -> String {
    let trimmed = value.trim_matches('/').trim();
    if trimmed.is_empty() {
        "Daily Notes".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_weekly_notes_dir(value: &str) -> String {
    let trimmed = value.trim_matches('/').trim();
    if trimmed.is_empty() {
        "Weekly Notes".to_string()
    } else {
        trimmed.to_string()
    }
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
            next.insert(
                format!("{}{}", folder_icon_key(folder, new_subpath), suffix),
                value.clone(),
            );
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
    icons
        .iter()
        .filter(|(key, _)| *key != &exact_key && !key.starts_with(&prefix))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn wiki_is_path_like(target: &str) -> bool {
    target.starts_with('/') || target.contains('/') || target.ends_with(".md")
}

fn wiki_split_content(content: &str) -> (&str, &str, &str) {
    // Split into (target, anchor, alias)
    let (before_pipe, alias) = match content.find('|') {
        Some(i) => (&content[..i], &content[i + 1..]),
        None => (content, ""),
    };
    let (target, anchor) = match before_pipe.find('#') {
        Some(i) => (&before_pipe[..i], &before_pipe[i..]),
        None => match before_pipe.find('^') {
            Some(i) => (&before_pipe[..i], &before_pipe[i..]),
            None => (before_pipe, ""),
        },
    };
    (target, anchor, alias)
}

fn wiki_swap_basename(target: &str, new_title: &str) -> String {
    if let Some(pos) = target.rfind('/') {
        format!("{}/{}", &target[..pos], new_title)
    } else {
        new_title.to_string()
    }
}

fn wiki_code_mask(body: &str) -> Vec<bool> {
    let bytes = body.as_bytes();
    let len = bytes.len();
    let mut mask = vec![false; len];
    let mut i = 0;
    while i < len {
        // Fenced code block
        if i + 3 <= len && &body[i..i + 3] == "```" {
            let start = i;
            // Skip to end of opening fence line
            i += 3;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            if i < len {
                i += 1;
            }
            // Find closing ```
            let mut found_close = false;
            while i < len {
                if bytes[i] == b'`' && i + 3 <= len && &body[i..i + 3] == "```" {
                    // Mark through end of closing fence line
                    i += 3;
                    while i < len && bytes[i] != b'\n' {
                        i += 1;
                    }
                    if i < len {
                        i += 1;
                    }
                    found_close = true;
                    break;
                }
                i += 1;
            }
            if !found_close {
                i = len;
            }
            mask[start..i].fill(true);
        }
        // Inline code
        else if bytes[i] == b'`' {
            let start = i;
            i += 1;
            while i < len && bytes[i] != b'`' {
                i += 1;
            }
            if i < len {
                i += 1;
            }
            mask[start..i].fill(true);
        } else {
            i += 1;
        }
    }
    mask
}

fn wiki_resolve_target<'a>(notes: &'a [NoteMeta], target: &str) -> Option<&'a NoteMeta> {
    let active: Vec<_> = notes
        .iter()
        .filter(|n| n.folder != NoteFolder::Trash)
        .collect();
    if wiki_is_path_like(target) {
        let clean = target.trim_start_matches('/');
        // Try exact path match
        if let Some(n) = active.iter().find(|n| n.path == clean) {
            return Some(n);
        }
        // Try suffix match
        active.iter().find(|n| n.path.ends_with(clean)).copied()
    } else {
        let lower = target.to_lowercase();
        active
            .iter()
            .find(|n| n.title.to_lowercase() == lower)
            .copied()
    }
}

fn rewrite_wikilinks_for_rename(
    body: &str,
    notes: &[NoteMeta],
    old_path: &str,
    new_title: &str,
) -> (String, usize) {
    let mask = wiki_code_mask(body);
    let mut result = String::with_capacity(body.len());
    let mut count = 0usize;
    let mut i = 0;
    let bytes = body.as_bytes();
    let len = bytes.len();

    while i < len {
        if i + 1 < len && bytes[i] == b'[' && bytes[i + 1] == b'[' && !mask[i] {
            // Find closing ]]
            let start = i;
            i += 2;
            let content_start = i;
            while i + 1 < len && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                i += 1;
            }
            if i + 1 >= len {
                // No closing ]], just append rest
                result.push_str(&body[start..]);

                break;
            }
            let content = &body[content_start..i];
            i += 2; // skip ]]

            let (target, anchor, alias) = wiki_split_content(content);
            if let Some(resolved) = wiki_resolve_target(notes, target) {
                if resolved.path == old_path {
                    count += 1;
                    let new_target = if wiki_is_path_like(target) {
                        wiki_swap_basename(target, new_title)
                    } else {
                        new_title.to_string()
                    };
                    result.push_str("[[");
                    result.push_str(&new_target);
                    result.push_str(anchor);
                    if !alias.is_empty() {
                        result.push('|');
                        result.push_str(alias);
                    }
                    result.push_str("]]");
                    continue;
                }
            }
            result.push_str(&body[start..i]);
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    (result, count)
}

fn new_comment_id() -> String {
    uuid::Uuid::new_v4().to_string().replace('-', "")
}

fn normalize_comments(comments: Vec<NoteComment>, note_path: &str) -> Vec<NoteComment> {
    use std::collections::HashSet;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let mut seen = HashSet::new();
    let mut out: Vec<NoteComment> = comments
        .into_iter()
        .filter(|c| !c.body.trim().is_empty())
        .map(|mut c| {
            if c.id.is_empty() {
                c.id = new_comment_id();
            }
            if c.created_at == 0 {
                c.created_at = now;
            }
            if c.updated_at == 0 {
                c.updated_at = c.created_at;
            }
            if c.anchor_start < 0 {
                c.anchor_start = 0;
            }
            if c.anchor_end < c.anchor_start {
                c.anchor_end = c.anchor_start;
            }
            c.note_path = note_path.to_string();
            c
        })
        .filter(|c| seen.insert(c.id.clone()))
        .collect();
    out.sort_by_key(|c| c.created_at);
    out
}

fn kind_for_ext(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "avif" | "apng" => "image",
        "pdf" => "pdf",
        "mp3" | "wav" | "ogg" | "flac" | "m4a" | "aac" => "audio",
        "mp4" | "mov" | "webm" | "ogv" | "m4v" => "video",
        _ => "file",
    }
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
        settings
            .folder_icons
            .insert("inbox:projects".to_string(), "invalid_icon".to_string());
        settings
            .folder_icons
            .insert("inbox:docs".to_string(), "book".to_string());
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

        let result =
            rewrite_folder_icons_for_rename(&icons, &NoteFolder::Inbox, "projects", "work");
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

    #[test]
    fn test_list_notes_finds_notes() {
        let (_dir, vault) = test_vault();
        let notes = vault.list_notes().unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].title, "Welcome to Zenvoy");
        assert_eq!(notes[0].folder, NoteFolder::Inbox);
    }

    #[test]
    fn test_list_notes_extracts_metadata() {
        let (_dir, vault) = test_vault();
        std::fs::write(
            vault.root().join("inbox").join("Tagged.md"),
            "# Tagged Note\n\nHello #rust #programming\n\nSee [[Other Note]]\n",
        )
        .unwrap();
        let notes = vault.list_notes().unwrap();
        let tagged = notes.iter().find(|n| n.title == "Tagged Note").unwrap();
        assert!(tagged.tags.contains(&"rust".to_string()));
        assert!(tagged.tags.contains(&"programming".to_string()));
        assert!(tagged.wikilinks.contains(&"Other Note".to_string()));
    }

    #[test]
    fn test_read_note() {
        let (_dir, vault) = test_vault();
        let content = vault.read_note("inbox/Welcome.md").unwrap();
        assert!(content.body.contains("Welcome"));
        assert_eq!(content.meta.title, "Welcome to Zenvoy");
    }

    #[test]
    fn test_write_note() {
        let (_dir, vault) = test_vault();
        let meta = vault
            .write_note("inbox/Welcome.md", "# Updated\n\nNew body")
            .unwrap();
        assert_eq!(meta.title, "Welcome");
        let content = vault.read_note("inbox/Welcome.md").unwrap();
        assert!(content.body.contains("New body"));
    }

    #[test]
    fn test_create_note() {
        let (_dir, vault) = test_vault();
        let meta = vault
            .create_note(&NoteFolder::Inbox, Some("My Note"), None)
            .unwrap();
        assert_eq!(meta.title, "My Note");
        assert!(meta.path.starts_with("inbox/"));
        assert!(meta.path.ends_with(".md"));
    }

    #[test]
    fn test_create_note_deduplicates() {
        let (_dir, vault) = test_vault();
        vault
            .create_note(&NoteFolder::Inbox, Some("Test"), None)
            .unwrap();
        let meta2 = vault
            .create_note(&NoteFolder::Inbox, Some("Test"), None)
            .unwrap();
        assert!(meta2.path.contains("Test 2"));
    }

    #[test]
    fn test_rename_note() {
        let (_dir, vault) = test_vault();
        let meta = vault
            .rename_note("inbox/Welcome.md", "Hello World")
            .unwrap();
        assert_eq!(meta.title, "Hello World");
        assert!(vault.read_note("inbox/Hello World.md").is_ok());
        assert!(vault.read_note("inbox/Welcome.md").is_err());
    }

    #[test]
    fn test_delete_note() {
        let (_dir, vault) = test_vault();
        vault.delete_note("inbox/Welcome.md").unwrap();
        assert!(vault.read_note("inbox/Welcome.md").is_err());
    }

    #[test]
    fn test_duplicate_note() {
        let (_dir, vault) = test_vault();
        let dup = vault.duplicate_note("inbox/Welcome.md").unwrap();
        assert!(dup.path.contains("copy"));
        assert!(vault.read_note(&dup.path).is_ok());
    }

    #[test]
    fn test_move_to_trash() {
        let (_dir, vault) = test_vault();
        let meta = vault.move_to_trash("inbox/Welcome.md").unwrap();
        assert_eq!(meta.folder, NoteFolder::Trash);
        assert!(meta.path.starts_with("trash/"));
        assert!(vault.read_note("inbox/Welcome.md").is_err());
    }

    #[test]
    fn test_restore_from_trash() {
        let (_dir, vault) = test_vault();
        let trashed = vault.move_to_trash("inbox/Welcome.md").unwrap();
        let restored = vault.restore_from_trash(&trashed.path).unwrap();
        assert_eq!(restored.folder, NoteFolder::Inbox);
        assert!(restored.path.starts_with("inbox/"));
    }

    #[test]
    fn test_empty_trash() {
        let (_dir, vault) = test_vault();
        vault.move_to_trash("inbox/Welcome.md").unwrap();
        vault.empty_trash().unwrap();
        let notes = vault.list_notes().unwrap();
        assert!(notes.iter().all(|n| n.folder != NoteFolder::Trash));
    }

    #[test]
    fn test_archive_note() {
        let (_dir, vault) = test_vault();
        let meta = vault.archive_note("inbox/Welcome.md").unwrap();
        assert_eq!(meta.folder, NoteFolder::Archive);
        assert!(meta.path.starts_with("archive/"));
    }

    #[test]
    fn test_unarchive_note() {
        let (_dir, vault) = test_vault();
        let archived = vault.archive_note("inbox/Welcome.md").unwrap();
        let restored = vault.unarchive_note(&archived.path).unwrap();
        assert_eq!(restored.folder, NoteFolder::Inbox);
    }

    #[test]
    fn test_move_preserves_subpath() {
        let (_dir, vault) = test_vault();
        std::fs::create_dir_all(vault.root().join("inbox").join("projects")).unwrap();
        std::fs::write(
            vault.root().join("inbox").join("projects").join("deep.md"),
            "# Deep\n",
        )
        .unwrap();
        let trashed = vault.move_to_trash("inbox/projects/deep.md").unwrap();
        assert!(trashed.path.contains("projects"));
        let restored = vault.restore_from_trash(&trashed.path).unwrap();
        assert!(restored.path.contains("projects"));
    }

    #[test]
    fn test_list_folders() {
        let (_dir, vault) = test_vault();
        std::fs::create_dir_all(vault.root().join("inbox").join("projects")).unwrap();
        std::fs::create_dir_all(vault.root().join("inbox").join("projects").join("sub")).unwrap();
        let folders = vault.list_folders().unwrap();
        assert!(folders
            .iter()
            .any(|f| f.subpath == "projects" && f.folder == NoteFolder::Inbox));
        assert!(folders
            .iter()
            .any(|f| f.subpath == "projects/sub" && f.folder == NoteFolder::Inbox));
    }

    #[test]
    fn test_create_folder() {
        let (_dir, vault) = test_vault();
        vault
            .create_folder(&NoteFolder::Inbox, "new-folder")
            .unwrap();
        let folders = vault.list_folders().unwrap();
        assert!(folders.iter().any(|f| f.subpath == "new-folder"));
    }

    #[test]
    fn test_rename_folder() {
        let (_dir, vault) = test_vault();
        vault.create_folder(&NoteFolder::Inbox, "old-name").unwrap();
        let new_sub = vault
            .rename_folder(&NoteFolder::Inbox, "old-name", "new-name")
            .unwrap();
        assert_eq!(new_sub, "new-name");
        let folders = vault.list_folders().unwrap();
        assert!(!folders.iter().any(|f| f.subpath == "old-name"));
        assert!(folders.iter().any(|f| f.subpath == "new-name"));
    }

    #[test]
    fn test_delete_folder() {
        let (_dir, vault) = test_vault();
        vault.create_folder(&NoteFolder::Inbox, "doomed").unwrap();
        vault.delete_folder(&NoteFolder::Inbox, "doomed").unwrap();
        let folders = vault.list_folders().unwrap();
        assert!(!folders.iter().any(|f| f.subpath == "doomed"));
    }

    #[test]
    fn test_duplicate_folder() {
        let (_dir, vault) = test_vault();
        vault.create_folder(&NoteFolder::Inbox, "original").unwrap();
        std::fs::write(
            vault.root().join("inbox").join("original").join("note.md"),
            "# Hi\n",
        )
        .unwrap();
        let new_sub = vault
            .duplicate_folder(&NoteFolder::Inbox, "original")
            .unwrap();
        assert!(new_sub.contains("copy"));
        let folders = vault.list_folders().unwrap();
        assert!(folders.iter().any(|f| f.subpath == new_sub));
    }

    #[test]
    fn test_has_assets_dir() {
        let (_dir, vault) = test_vault();
        assert!(vault.has_assets_dir());
    }

    #[test]
    fn test_list_assets_empty() {
        let (_dir, vault) = test_vault();
        let assets = vault.list_assets().unwrap();
        assert!(assets.is_empty());
    }

    #[test]
    fn test_import_and_list_assets() {
        let (dir, vault) = test_vault();
        // Create a source file to import
        let src = dir.path().join("photo.png");
        std::fs::write(&src, b"fake png data").unwrap();
        let imported = vault
            .import_files_to_note("inbox/Welcome.md", &[src.to_string_lossy().to_string()])
            .unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].kind, "image");
        assert!(imported[0]
            .markdown
            .contains("![[attachements/welcome/photo.png]]"));
        let assets = vault.list_assets().unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].kind, "image");
    }

    #[test]
    fn test_rename_asset() {
        let (_dir, vault) = test_vault();
        std::fs::write(vault.root().join("attachements").join("test.png"), b"data").unwrap();
        let meta = vault
            .rename_asset("attachements/test.png", "renamed.png")
            .unwrap();
        assert_eq!(meta.name, "renamed.png");
    }

    #[test]
    fn test_delete_and_restore_asset() {
        let (_dir, vault) = test_vault();
        std::fs::write(
            vault.root().join("attachements").join("doomed.pdf"),
            b"pdf data",
        )
        .unwrap();
        let deleted = vault.delete_asset("attachements/doomed.pdf").unwrap();
        assert_eq!(deleted.name, "doomed.pdf");
        assert!(!vault
            .root()
            .join("attachements")
            .join("doomed.pdf")
            .exists());
        let restored = vault.restore_deleted_asset(&deleted).unwrap();
        assert_eq!(restored.name, "doomed.pdf");
        assert!(vault
            .root()
            .join("attachements")
            .join("doomed.pdf")
            .exists());
    }

    #[test]
    fn test_scan_tasks() {
        let (_dir, vault) = test_vault();
        std::fs::write(
            vault.root().join("inbox").join("Tasks.md"),
            "# My Tasks\n\n- [ ] Buy groceries due:2024-01-15 !high\n- [x] Clean house @waiting\n- [ ] Read book #reading\n"
        ).unwrap();
        let tasks = vault.scan_tasks().unwrap();
        assert!(tasks.len() >= 3);
        let buy = tasks
            .iter()
            .find(|t| t.content.contains("Buy groceries"))
            .unwrap();
        assert_eq!(buy.due, "2024-01-15");
        assert_eq!(buy.priority, "high");
        assert!(!buy.checked);
        let clean = tasks
            .iter()
            .find(|t| t.content.contains("Clean house"))
            .unwrap();
        assert!(clean.checked);
        assert!(clean.waiting);
        let read = tasks
            .iter()
            .find(|t| t.content.contains("Read book"))
            .unwrap();
        assert!(read.tags.contains(&"reading".to_string()));
    }

    #[test]
    fn test_scan_tasks_for_path() {
        let (_dir, vault) = test_vault();
        std::fs::write(
            vault.root().join("inbox").join("Specific.md"),
            "# Specific\n\n- [ ] Task A\n- [ ] Task B\n",
        )
        .unwrap();
        let tasks = vault.scan_tasks_for_path("inbox/Specific.md").unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].source_path, "inbox/Specific.md");
    }

    #[test]
    fn test_scan_tasks_skips_trash() {
        let (_dir, vault) = test_vault();
        std::fs::write(
            vault.root().join("trash").join("Old.md"),
            "# Old\n\n- [ ] Deleted task\n",
        )
        .unwrap();
        let tasks = vault.scan_tasks().unwrap();
        assert!(!tasks.iter().any(|t| t.content.contains("Deleted task")));
    }

    #[test]
    fn test_search_capabilities() {
        let (_dir, vault) = test_vault();
        let caps = vault.get_text_search_capabilities();
        let _ = caps;
    }

    #[test]
    fn test_search_vault_text() {
        let (_dir, vault) = test_vault();
        std::fs::write(
            vault.root().join("inbox").join("Searchable.md"),
            "# Search Target\n\nThis contains the keyword findme in a sentence.\n",
        )
        .unwrap();
        let results = vault.search_vault_text("findme", None).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].path, "inbox/Searchable.md");
        assert!(results[0].line_text.contains("findme"));
        assert_eq!(results[0].line_number, 3);
    }

    #[test]
    fn test_search_skips_trash() {
        let (_dir, vault) = test_vault();
        std::fs::write(
            vault.root().join("trash").join("Old.md"),
            "# Old\n\nfindme in trash\n",
        )
        .unwrap();
        let results = vault.search_vault_text("findme", None).unwrap();
        assert!(results.iter().all(|r| !r.path.starts_with("trash/")));
    }

    #[test]
    fn test_rename_updates_wikilinks() {
        let (_dir, vault) = test_vault();
        std::fs::write(
            vault.root().join("inbox").join("Linker.md"),
            "# Linker\n\nSee [[Welcome to Zenvoy]] for info.\n",
        )
        .unwrap();
        vault
            .rename_note("inbox/Welcome.md", "Getting Started")
            .unwrap();
        let linker = vault.read_note("inbox/Linker.md").unwrap();
        assert!(linker.body.contains("[[Getting Started]]"));
        assert!(!linker.body.contains("[[Welcome to Zenvoy]]"));
    }

    #[test]
    fn test_rename_preserves_alias() {
        let (_dir, vault) = test_vault();
        std::fs::write(
            vault.root().join("inbox").join("WithAlias.md"),
            "# With Alias\n\nSee [[Welcome to Zenvoy|my welcome]] for info.\n",
        )
        .unwrap();
        vault
            .rename_note("inbox/Welcome.md", "Hello World")
            .unwrap();
        let content = vault.read_note("inbox/WithAlias.md").unwrap();
        assert!(content.body.contains("[[Hello World|my welcome]]"));
    }

    #[test]
    fn test_rename_skips_code_blocks() {
        let (_dir, vault) = test_vault();
        std::fs::write(
            vault.root().join("inbox").join("Code.md"),
            "# Code\n\n```\n[[Welcome to Zenvoy]]\n```\n\nAlso [[Welcome to Zenvoy]] outside.\n",
        )
        .unwrap();
        vault.rename_note("inbox/Welcome.md", "New Name").unwrap();
        let content = vault.read_note("inbox/Code.md").unwrap();
        assert!(content.body.contains("```\n[[Welcome to Zenvoy]]\n```"));
        assert!(content.body.contains("[[New Name]] outside"));
    }

    #[test]
    fn test_read_comments_empty() {
        let (_dir, vault) = test_vault();
        let comments = vault.read_note_comments("inbox/Welcome.md").unwrap();
        assert!(comments.is_empty());
    }

    #[test]
    fn test_write_and_read_comments() {
        let (_dir, vault) = test_vault();
        let input = vec![NoteComment {
            id: String::new(),
            note_path: "inbox/Welcome.md".to_string(),
            anchor_start: 0,
            anchor_end: 10,
            anchor_text: "Welcome to".to_string(),
            body: "Great intro!".to_string(),
            created_at: 0,
            updated_at: 0,
            resolved_at: None,
        }];
        let saved = vault
            .write_note_comments("inbox/Welcome.md", input)
            .unwrap();
        assert_eq!(saved.len(), 1);
        assert!(!saved[0].id.is_empty());
        assert!(saved[0].created_at > 0);
        let loaded = vault.read_note_comments("inbox/Welcome.md").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].body, "Great intro!");
    }

    #[test]
    fn test_write_empty_comments_deletes_file() {
        let (_dir, vault) = test_vault();
        let input = vec![NoteComment {
            id: "abc".to_string(),
            note_path: "inbox/Welcome.md".to_string(),
            anchor_start: 0,
            anchor_end: 5,
            anchor_text: "test".to_string(),
            body: "comment".to_string(),
            created_at: 1000,
            updated_at: 1000,
            resolved_at: None,
        }];
        vault
            .write_note_comments("inbox/Welcome.md", input)
            .unwrap();
        vault
            .write_note_comments("inbox/Welcome.md", vec![])
            .unwrap();
        let loaded = vault.read_note_comments("inbox/Welcome.md").unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn test_create_and_open_database() {
        let (_dir, vault) = test_vault();
        let doc = vault
            .create_database(&NoteFolder::Inbox, "", Some("Projects"))
            .unwrap();
        assert_eq!(doc.title, "Projects");
        assert!(doc.rows.is_empty());
        let opened = vault.open_database(&doc.path).unwrap();
        assert_eq!(opened.title, "Projects");
    }

    #[test]
    fn test_list_databases() {
        let (_dir, vault) = test_vault();
        vault
            .create_database(&NoteFolder::Inbox, "", Some("DB1"))
            .unwrap();
        vault
            .create_database(&NoteFolder::Inbox, "", Some("DB2"))
            .unwrap();
        let dbs = vault.list_databases().unwrap();
        assert_eq!(dbs.len(), 2);
    }

    #[test]
    fn test_write_database_rows() {
        let (_dir, vault) = test_vault();
        let doc = vault
            .create_database(&NoteFolder::Inbox, "", Some("Tasks"))
            .unwrap();
        let mut cells = std::collections::HashMap::new();
        cells.insert("id".to_string(), "row-1".to_string());
        cells.insert("Title".to_string(), "First Task".to_string());
        cells.insert("Status".to_string(), "Todo".to_string());
        let rows = vec![DbRow {
            id: "row-1".to_string(),
            cells,
        }];
        let updated = vault.write_database_rows(&doc.path, rows).unwrap();
        assert_eq!(updated.rows.len(), 1);
    }

    #[test]
    fn test_list_templates_empty() {
        let (_dir, vault) = test_vault();
        let templates = vault.list_templates().unwrap();
        assert!(templates.is_empty());
    }

    #[test]
    fn test_write_and_read_template() {
        let (_dir, vault) = test_vault();
        let input = WriteTemplateInput {
            slug: "Meeting Notes".to_string(),
            raw: "---\nname: Meeting Notes\n---\n# {{title}}\n\nDate: {{date}}\n".to_string(),
            previous_source_path: None,
        };
        let created = vault.write_template(&input).unwrap();
        assert!(created.source_path.contains("meeting-notes"));
        let content = vault.read_template(&created.source_path).unwrap();
        assert!(content.contains("Meeting Notes"));
        let templates = vault.list_templates().unwrap();
        assert_eq!(templates.len(), 1);
    }

    #[test]
    fn test_generate_demo_tour() {
        let (_dir, vault) = test_vault();
        let result = vault.generate_demo_tour().unwrap();
        assert!(result.success);
        assert!(!result.paths.is_empty());
        // Verify notes were created
        let notes = vault.list_notes().unwrap();
        assert!(notes.len() > 1); // More than just Welcome.md
    }

    #[test]
    fn test_remove_demo_tour() {
        let (_dir, vault) = test_vault();
        vault.generate_demo_tour().unwrap();
        let result = vault.remove_demo_tour().unwrap();
        assert!(result.success);
        assert!(!result.paths.is_empty());
    }

    #[test]
    fn test_delete_template() {
        let (_dir, vault) = test_vault();
        let input = WriteTemplateInput {
            slug: "doomed".to_string(),
            raw: "# Doomed\n".to_string(),
            previous_source_path: None,
        };
        let created = vault.write_template(&input).unwrap();
        vault.delete_template(&created.source_path).unwrap();
        let templates = vault.list_templates().unwrap();
        assert!(templates.is_empty());
    }
}

/// Convert a note path like "inbox/Hello World.md" to a folder name "hello-world"
pub fn note_path_to_asset_folder(note_path: &str) -> String {
    let name = std::path::Path::new(note_path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let result: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    result.trim_matches('-').to_string()
}
