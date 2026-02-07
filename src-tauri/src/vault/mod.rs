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

    pub fn list_notes(&self) -> VaultResult<Vec<NoteMeta>> {
        let settings = self.get_settings()?;
        let folders = [NoteFolder::Inbox, NoteFolder::Quick, NoteFolder::Archive, NoteFolder::Trash];
        let mut all = Vec::new();

        for folder in &folders {
            let base = self.folder_root(folder)?;
            if !base.is_dir() {
                continue;
            }
            let is_root_inbox = *folder == NoteFolder::Inbox && settings.primary_notes_location == "root";
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
        entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

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
        meta.title = abs.file_stem().unwrap_or_default().to_string_lossy().to_string();
        Ok(meta)
    }

    pub fn create_note(&self, folder: &NoteFolder, title: Option<&str>, subpath: Option<&str>) -> VaultResult<NoteMeta> {
        let stem = match title {
            Some(t) => {
                let s = sanitize_file_stem(t);
                if s.is_empty() { default_title() } else { s }
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
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let stem = sanitize_file_stem(next_title);
        let stem = if stem.is_empty() { default_title() } else { stem };
        let dir = abs.parent().unwrap();
        let dest = unique_path(dir, &stem, "md");
        fs::rename(&abs, &dest)?;
        self.invalidate_caches();
        let folder = self.folder_of(&dest);
        let mut meta = self.read_meta(&folder, &dest)?;
        // Use the intended title (from filename stem) rather than body heading
        meta.title = dest.file_stem().unwrap_or_default().to_string_lossy().to_string();
        Ok(meta)
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

    pub fn move_note(&self, rel: &str, target: &NoteFolder, target_subpath: Option<&str>) -> VaultResult<NoteMeta> {
        let abs = safepath::safe_join(&self.root, rel)?;
        if !abs.is_file() {
            return Err(VaultError::NotFound(rel.to_string()));
        }
        let stem = abs.file_stem().unwrap_or_default().to_string_lossy().to_string();
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
        let stem = abs.file_stem().unwrap_or_default().to_string_lossy().to_string();
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
        let components: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect();
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
        let mtime_ms = fs_meta.modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64() * 1000.0;

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
        let excerpt = parse::build_excerpt(&body);
        let size = fs_meta.len() as i64;

        let updated_at = (mtime_ms / 1000.0) as i64;
        let created_at = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                let ct = fs_meta.ctime();
                if ct > 0 { ct } else { updated_at }
            }
            #[cfg(not(unix))]
            { updated_at }
        };

        let rel = abs_path.strip_prefix(&self.root).unwrap_or(abs_path);
        let path = rel.components()
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
        };

        {
            let mut cache = self.meta_cache.write();
            cache.insert(key, NoteMetaCacheEntry { mtime_ms, _size: size, meta: meta.clone() });
        }
        Ok(meta)
    }

    pub fn list_folders(&self) -> VaultResult<Vec<FolderEntry>> {
        let settings = self.get_settings()?;
        let folders = [NoteFolder::Inbox, NoteFolder::Quick, NoteFolder::Archive, NoteFolder::Trash];
        let mut result: Vec<FolderEntry> = Vec::new();

        for folder in &folders {
            let base = self.folder_root(folder)?;
            if !base.is_dir() { continue; }
            let is_root_inbox = *folder == NoteFolder::Inbox && settings.primary_notes_location == "root";
            self.walk_folders(&base, &base, folder, is_root_inbox, &mut result)?;
        }

        // Sort by folder then subpath
        result.sort_by(|a, b| {
            a.folder.as_str().cmp(b.folder.as_str()).then(a.subpath.cmp(&b.subpath))
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
        entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for entry in entries {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') { continue; }
            let path = entry.path();
            if !path.is_dir() { continue; }
            if skip_reserved && RESERVED_ROOT_NAMES.contains(&name.as_str()) { continue; }
            let subpath = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
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

    pub fn rename_folder(&self, folder: &NoteFolder, old_subpath: &str, new_subpath: &str) -> VaultResult<String> {
        let base = self.folder_root(folder)?;
        let old_path = safepath::safe_join(&base, old_subpath)?;
        let new_path = safepath::safe_join(&base, new_subpath)?;
        if let Some(parent) = new_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&old_path, &new_path)?;
        // Update folder icons
        let mut settings = self.get_settings()?;
        settings.folder_icons = rewrite_folder_icons_for_rename(&settings.folder_icons, folder, old_subpath, new_subpath);
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
        let new_subpath = dest.strip_prefix(&base).unwrap().to_string_lossy().replace('\\', "/");
        Ok(new_subpath)
    }
}

fn sanitize_file_stem(title: &str) -> String {
    title.chars().filter(|c| !"/\\:*?\"<>|".contains(*c)).collect::<String>().trim().to_string()
}

fn default_title() -> String {
    use std::time::SystemTime;
    let secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs();
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
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let months_days: [i64; 12] = if is_leap(y) {
        [31,29,31,30,31,30,31,31,30,31,30,31]
    } else {
        [31,28,31,30,31,30,31,31,30,31,30,31]
    };
    let mut mon = 1;
    for md in months_days {
        if remaining < md { break; }
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
    if !candidate.exists() { return candidate; }
    let mut i = 2;
    loop {
        let p = dir.join(format!("{} {}.{}", stem, i, ext));
        if !p.exists() { return p; }
        i += 1;
    }
}

fn copy_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::copy(src, dst)?;
    Ok(())
}

fn unique_dir(parent: &Path, base: &str) -> PathBuf {
    let candidate = parent.join(base);
    if !candidate.exists() { return candidate; }
    let mut i = 2;
    loop {
        let p = parent.join(format!("{} {}", base, i));
        if !p.exists() { return p; }
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
        std::fs::write(vault.root().join("inbox").join("Tagged.md"), "# Tagged Note\n\nHello #rust #programming\n\nSee [[Other Note]]\n").unwrap();
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
        let meta = vault.write_note("inbox/Welcome.md", "# Updated\n\nNew body").unwrap();
        assert_eq!(meta.title, "Welcome");
        let content = vault.read_note("inbox/Welcome.md").unwrap();
        assert!(content.body.contains("New body"));
    }

    #[test]
    fn test_create_note() {
        let (_dir, vault) = test_vault();
        let meta = vault.create_note(&NoteFolder::Inbox, Some("My Note"), None).unwrap();
        assert_eq!(meta.title, "My Note");
        assert!(meta.path.starts_with("inbox/"));
        assert!(meta.path.ends_with(".md"));
    }

    #[test]
    fn test_create_note_deduplicates() {
        let (_dir, vault) = test_vault();
        vault.create_note(&NoteFolder::Inbox, Some("Test"), None).unwrap();
        let meta2 = vault.create_note(&NoteFolder::Inbox, Some("Test"), None).unwrap();
        assert!(meta2.path.contains("Test 2"));
    }

    #[test]
    fn test_rename_note() {
        let (_dir, vault) = test_vault();
        let meta = vault.rename_note("inbox/Welcome.md", "Hello World").unwrap();
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
        std::fs::write(vault.root().join("inbox").join("projects").join("deep.md"), "# Deep\n").unwrap();
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
        assert!(folders.iter().any(|f| f.subpath == "projects" && f.folder == NoteFolder::Inbox));
        assert!(folders.iter().any(|f| f.subpath == "projects/sub" && f.folder == NoteFolder::Inbox));
    }

    #[test]
    fn test_create_folder() {
        let (_dir, vault) = test_vault();
        vault.create_folder(&NoteFolder::Inbox, "new-folder").unwrap();
        let folders = vault.list_folders().unwrap();
        assert!(folders.iter().any(|f| f.subpath == "new-folder"));
    }

    #[test]
    fn test_rename_folder() {
        let (_dir, vault) = test_vault();
        vault.create_folder(&NoteFolder::Inbox, "old-name").unwrap();
        let new_sub = vault.rename_folder(&NoteFolder::Inbox, "old-name", "new-name").unwrap();
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
        std::fs::write(vault.root().join("inbox").join("original").join("note.md"), "# Hi\n").unwrap();
        let new_sub = vault.duplicate_folder(&NoteFolder::Inbox, "original").unwrap();
        assert!(new_sub.contains("copy"));
        let folders = vault.list_folders().unwrap();
        assert!(folders.iter().any(|f| f.subpath == new_sub));
    }
}
