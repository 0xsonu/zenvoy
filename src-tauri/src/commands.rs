use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{Emitter, State, WebviewWindow, Window};

use crate::vault::types::*;
use crate::vault::{Vault, VaultOptions};
use crate::watcher::VaultWatcher;

pub struct TauriAppState {
    pub vault: RwLock<Option<Arc<Vault>>>,
    pub watcher: RwLock<Option<Arc<VaultWatcher>>>,
    pub zoom_factor: RwLock<f64>,
}

impl Default for TauriAppState {
    fn default() -> Self {
        Self::new()
    }
}

impl TauriAppState {
    pub fn new() -> Self {
        Self {
            vault: RwLock::new(None),
            watcher: RwLock::new(None),
            zoom_factor: RwLock::new(1.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalVaultEntry {
    pub root: String,
    pub name: String,
    pub last_opened: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryBrowseResult {
    pub path: String,
    pub entries: Vec<DirectoryBrowseEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryBrowseEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PastedImageInput {
    pub note_path: String,
    pub data_base64: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultTextSearchToolPaths {
    pub rg: Option<String>,
    pub fzf: Option<String>,
}

fn vault(state: &TauriAppState) -> Result<Arc<Vault>, String> {
    state
        .vault
        .read()
        .clone()
        .ok_or_else(|| "No vault open".to_string())
}

fn local_vaults_path() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".zenvoy")
        .join("local-vaults.json")
}

fn read_local_vaults() -> Vec<LocalVaultEntry> {
    let path = local_vaults_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_local_vaults(entries: &[LocalVaultEntry]) {
    let path = local_vaults_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        &path,
        serde_json::to_string_pretty(entries).unwrap_or_default(),
    );
}

fn register_vault_entry(root: &str) {
    let mut entries = read_local_vaults();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    if let Some(e) = entries.iter_mut().find(|e| e.root == root) {
        e.last_opened = now;
    } else {
        let name = std::path::Path::new(root)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        entries.push(LocalVaultEntry {
            root: root.to_string(),
            name,
            last_opened: now,
        });
    }
    save_local_vaults(&entries);
}

fn open_vault_at(
    state: &TauriAppState,
    root: &str,
    app: &tauri::AppHandle,
) -> Result<VaultInfo, String> {
    let v = Vault::new(root, VaultOptions::default()).map_err(|e| e.to_string())?;
    let info = v.info();
    let arc = Arc::new(v);
    *state.vault.write() = Some(arc.clone());

    let watcher = VaultWatcher::start(root).map_err(|e| e.to_string())?;
    let mut rx = watcher.subscribe();
    let app_handle = app.clone();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let _ = app_handle.emit("vault-change", &event);
        }
    });
    *state.watcher.write() = Some(Arc::new(watcher));

    register_vault_entry(root);
    Ok(info)
}

// ── Platform ─────────────────────────────────────────────────────

#[tauri::command]
pub fn platform() -> String {
    std::env::consts::OS.to_string()
}

#[tauri::command]
pub fn list_system_fonts() -> Vec<String> {
    Vec::new()
}

#[tauri::command]
pub fn get_app_icon_data_url() -> Option<String> {
    None
}

// ── Vault management ─────────────────────────────────────────────

#[tauri::command]
pub fn get_current_vault(state: State<'_, TauriAppState>) -> Option<VaultInfo> {
    state.vault.read().as_ref().map(|v| v.info())
}

#[tauri::command]
pub fn list_local_vaults() -> Vec<LocalVaultEntry> {
    read_local_vaults()
}

#[tauri::command]
pub fn open_local_vault(
    root: String,
    state: State<'_, TauriAppState>,
    app: tauri::AppHandle,
) -> Result<Option<VaultInfo>, String> {
    let info = open_vault_at(&state, &root, &app)?;
    Ok(Some(info))
}

#[tauri::command]
pub fn close_vault(state: State<'_, TauriAppState>) -> Option<VaultInfo> {
    *state.watcher.write() = None;
    *state.vault.write() = None;
    None
}

#[tauri::command]
pub async fn pick_vault(
    state: State<'_, TauriAppState>,
    app: tauri::AppHandle,
) -> Result<Option<VaultInfo>, String> {
    use tauri_plugin_dialog::DialogExt;
    let path = app.dialog().file().blocking_pick_folder();
    match path {
        Some(p) => {
            let root = p.to_string();
            let info = open_vault_at(&state, &root, &app)?;
            Ok(Some(info))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn select_vault_path(
    path: String,
    state: State<'_, TauriAppState>,
    app: tauri::AppHandle,
) -> Result<VaultInfo, String> {
    open_vault_at(&state, &path, &app)
}

#[tauri::command]
pub fn browse_server_directories(path: Option<String>) -> Result<DirectoryBrowseResult, String> {
    let dir = path.unwrap_or_else(|| {
        dirs::home_dir()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string())
    });
    let entries = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| DirectoryBrowseEntry {
            name: e.file_name().to_string_lossy().to_string(),
            path: e.path().to_string_lossy().to_string(),
            is_dir: true,
        })
        .collect();
    Ok(DirectoryBrowseResult { path: dir, entries })
}

#[tauri::command]
pub fn get_vault_settings(state: State<'_, TauriAppState>) -> Result<VaultSettings, String> {
    vault(&state)?.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_vault_settings(
    next: VaultSettings,
    state: State<'_, TauriAppState>,
) -> Result<VaultSettings, String> {
    vault(&state)?.set_settings(next).map_err(|e| e.to_string())
}

// ── Notes ────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_notes(state: State<'_, TauriAppState>) -> Result<Vec<NoteMeta>, String> {
    vault(&state)?.list_notes().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_folders(state: State<'_, TauriAppState>) -> Result<Vec<FolderEntry>, String> {
    vault(&state)?.list_folders().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_assets(state: State<'_, TauriAppState>) -> Result<Vec<AssetMeta>, String> {
    vault(&state)?.list_assets().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn has_assets_dir(state: State<'_, TauriAppState>) -> Result<bool, String> {
    Ok(vault(&state)?.has_assets_dir())
}

#[tauri::command]
pub fn generate_demo_tour(state: State<'_, TauriAppState>) -> Result<VaultDemoTourResult, String> {
    vault(&state)?
        .generate_demo_tour()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_demo_tour(state: State<'_, TauriAppState>) -> Result<VaultDemoTourResult, String> {
    vault(&state)?.remove_demo_tour().map_err(|e| e.to_string())
}

// ── Templates ────────────────────────────────────────────────────

#[tauri::command]
pub fn list_templates(state: State<'_, TauriAppState>) -> Result<Vec<CustomTemplateFile>, String> {
    vault(&state)?.list_templates().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_template(
    source_path: String,
    state: State<'_, TauriAppState>,
) -> Result<String, String> {
    vault(&state)?
        .read_template(&source_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_template(
    input: WriteTemplateInput,
    state: State<'_, TauriAppState>,
) -> Result<CustomTemplateFile, String> {
    vault(&state)?
        .write_template(&input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_template(source_path: String, state: State<'_, TauriAppState>) -> Result<(), String> {
    vault(&state)?
        .delete_template(&source_path)
        .map_err(|e| e.to_string())
}

// ── Search ───────────────────────────────────────────────────────

#[tauri::command]
pub fn get_vault_text_search_capabilities(
    _paths: Option<VaultTextSearchToolPaths>,
    state: State<'_, TauriAppState>,
) -> Result<TextSearchCapabilities, String> {
    Ok(vault(&state)?.get_text_search_capabilities())
}

#[tauri::command]
pub fn search_vault_text(
    query: String,
    backend: Option<String>,
    _paths: Option<VaultTextSearchToolPaths>,
    state: State<'_, TauriAppState>,
) -> Result<Vec<TextSearchMatch>, String> {
    vault(&state)?
        .search_vault_text(&query, backend.as_deref())
        .map_err(|e| e.to_string())
}

// ── Note CRUD ────────────────────────────────────────────────────

#[tauri::command]
pub fn read_note(rel_path: String, state: State<'_, TauriAppState>) -> Result<NoteContent, String> {
    vault(&state)?
        .read_note(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_note_comments(
    rel_path: String,
    state: State<'_, TauriAppState>,
) -> Result<Vec<NoteComment>, String> {
    vault(&state)?
        .read_note_comments(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_note_comments(
    rel_path: String,
    comments: Vec<NoteComment>,
    state: State<'_, TauriAppState>,
) -> Result<Vec<NoteComment>, String> {
    vault(&state)?
        .write_note_comments(&rel_path, comments)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scan_tasks(state: State<'_, TauriAppState>) -> Result<Vec<VaultTask>, String> {
    vault(&state)?.scan_tasks().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn scan_tasks_for_path(
    rel_path: String,
    state: State<'_, TauriAppState>,
) -> Result<Vec<VaultTask>, String> {
    vault(&state)?
        .scan_tasks_for_path(&rel_path)
        .map_err(|e| e.to_string())
}

// ── Databases ────────────────────────────────────────────────────

#[tauri::command]
pub fn open_database(
    rel_path: String,
    state: State<'_, TauriAppState>,
) -> Result<DatabaseDoc, String> {
    vault(&state)?
        .open_database(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_database_rows(
    rel_path: String,
    rows: Vec<DbRow>,
    state: State<'_, TauriAppState>,
) -> Result<DatabaseDoc, String> {
    vault(&state)?
        .write_database_rows(&rel_path, rows)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_database_schema(
    rel_path: String,
    sidecar: DatabaseSidecar,
    rows: Vec<DbRow>,
    state: State<'_, TauriAppState>,
) -> Result<DatabaseDoc, String> {
    vault(&state)?
        .write_database_schema(&rel_path, sidecar, rows)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_database(
    folder: NoteFolder,
    subpath: String,
    title: Option<String>,
    state: State<'_, TauriAppState>,
) -> Result<DatabaseDoc, String> {
    vault(&state)?
        .create_database(&folder, &subpath, title.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_record_page(
    csv_path: String,
    title: String,
    body: String,
    state: State<'_, TauriAppState>,
) -> Result<String, String> {
    vault(&state)?
        .create_record_page(&csv_path, &title, &body)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_databases(state: State<'_, TauriAppState>) -> Result<Vec<DatabaseSummary>, String> {
    vault(&state)?.list_databases().map_err(|e| e.to_string())
}

// ── Note mutations ───────────────────────────────────────────────

#[tauri::command]
pub fn write_note(
    rel_path: String,
    body: String,
    state: State<'_, TauriAppState>,
) -> Result<NoteMeta, String> {
    vault(&state)?
        .write_note(&rel_path, &body)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn append_to_note(
    rel_path: String,
    body: String,
    position: Option<String>,
    state: State<'_, TauriAppState>,
) -> Result<NoteMeta, String> {
    let pos = position.unwrap_or_else(|| "append".to_string());
    vault(&state)?
        .append_to_note(&rel_path, &body, &pos)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_note(
    folder: NoteFolder,
    title: Option<String>,
    subpath: Option<String>,
    state: State<'_, TauriAppState>,
) -> Result<NoteMeta, String> {
    vault(&state)?
        .create_note(&folder, title.as_deref(), subpath.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_note(
    rel_path: String,
    next_title: String,
    state: State<'_, TauriAppState>,
) -> Result<NoteMeta, String> {
    vault(&state)?
        .rename_note(&rel_path, &next_title)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_note(rel_path: String, state: State<'_, TauriAppState>) -> Result<(), String> {
    vault(&state)?
        .delete_note(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn move_to_trash(
    rel_path: String,
    state: State<'_, TauriAppState>,
) -> Result<NoteMeta, String> {
    vault(&state)?
        .move_to_trash(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_from_trash(
    rel_path: String,
    state: State<'_, TauriAppState>,
) -> Result<NoteMeta, String> {
    vault(&state)?
        .restore_from_trash(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn empty_trash(state: State<'_, TauriAppState>) -> Result<(), String> {
    vault(&state)?.empty_trash().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn archive_note(rel_path: String, state: State<'_, TauriAppState>) -> Result<NoteMeta, String> {
    vault(&state)?
        .archive_note(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn unarchive_note(
    rel_path: String,
    state: State<'_, TauriAppState>,
) -> Result<NoteMeta, String> {
    vault(&state)?
        .unarchive_note(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duplicate_note(
    rel_path: String,
    state: State<'_, TauriAppState>,
) -> Result<NoteMeta, String> {
    vault(&state)?
        .duplicate_note(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn move_note(
    rel_path: String,
    target_folder: NoteFolder,
    target_subpath: Option<String>,
    state: State<'_, TauriAppState>,
) -> Result<NoteMeta, String> {
    vault(&state)?
        .move_note(&rel_path, &target_folder, target_subpath.as_deref())
        .map_err(|e| e.to_string())
}

// ── Assets ───────────────────────────────────────────────────────

#[tauri::command]
pub fn import_files_to_note(
    note_path: String,
    source_paths: Vec<String>,
    state: State<'_, TauriAppState>,
) -> Result<Vec<ImportedAsset>, String> {
    vault(&state)?
        .import_files_to_note(&note_path, &source_paths)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_pasted_image(
    input: PastedImageInput,
    state: State<'_, TauriAppState>,
) -> Result<ImportedAsset, String> {
    use base64::Engine;
    let v = vault(&state)?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(&input.data_base64)
        .map_err(|e| e.to_string())?;
    let attachments_dir = v.root().join("attachements");
    std::fs::create_dir_all(&attachments_dir).map_err(|e| e.to_string())?;
    let stem = std::path::Path::new(&input.filename)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = std::path::Path::new(&input.filename)
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let ext = if ext.is_empty() {
        "png".to_string()
    } else {
        ext
    };
    // Find unique path
    let dest = {
        let candidate = attachments_dir.join(format!("{}.{}", stem, ext));
        if !candidate.exists() {
            candidate
        } else {
            let mut i = 2;
            loop {
                let p = attachments_dir.join(format!("{} {}.{}", stem, i, ext));
                if !p.exists() {
                    break p;
                }
                i += 1;
            }
        }
    };
    std::fs::write(&dest, &data).map_err(|e| e.to_string())?;
    let dest_name = dest.file_name().unwrap().to_string_lossy().to_string();
    let rel = dest.strip_prefix(v.root()).unwrap();
    let rel_str = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    let markdown = format!("![{}](../attachements/{})", input.filename, dest_name);
    Ok(ImportedAsset {
        name: dest_name,
        path: rel_str,
        markdown,
        kind: "image".to_string(),
    })
}

#[tauri::command]
pub fn rename_asset(
    rel_path: String,
    next_name: String,
    state: State<'_, TauriAppState>,
) -> Result<AssetMeta, String> {
    vault(&state)?
        .rename_asset(&rel_path, &next_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn move_asset(
    rel_path: String,
    target_dir: String,
    state: State<'_, TauriAppState>,
) -> Result<AssetMeta, String> {
    vault(&state)?
        .move_asset(&rel_path, &target_dir)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duplicate_asset(
    rel_path: String,
    state: State<'_, TauriAppState>,
) -> Result<AssetMeta, String> {
    vault(&state)?
        .duplicate_asset(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_asset(
    rel_path: String,
    state: State<'_, TauriAppState>,
) -> Result<DeletedAsset, String> {
    vault(&state)?
        .delete_asset(&rel_path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restore_deleted_asset(
    asset: DeletedAsset,
    state: State<'_, TauriAppState>,
) -> Result<AssetMeta, String> {
    vault(&state)?
        .restore_deleted_asset(&asset)
        .map_err(|e| e.to_string())
}

// ── Folders ──────────────────────────────────────────────────────

#[tauri::command]
pub fn create_folder(
    folder: NoteFolder,
    subpath: String,
    state: State<'_, TauriAppState>,
) -> Result<(), String> {
    vault(&state)?
        .create_folder(&folder, &subpath)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_folder(
    folder: NoteFolder,
    old_subpath: String,
    new_subpath: String,
    state: State<'_, TauriAppState>,
) -> Result<String, String> {
    vault(&state)?
        .rename_folder(&folder, &old_subpath, &new_subpath)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_folder(
    folder: NoteFolder,
    subpath: String,
    state: State<'_, TauriAppState>,
) -> Result<(), String> {
    vault(&state)?
        .delete_folder(&folder, &subpath)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn duplicate_folder(
    folder: NoteFolder,
    subpath: String,
    state: State<'_, TauriAppState>,
) -> Result<String, String> {
    vault(&state)?
        .duplicate_folder(&folder, &subpath)
        .map_err(|e| e.to_string())
}

// ── Reveal in file manager ───────────────────────────────────────

#[tauri::command]
pub fn reveal_note(rel_path: String, state: State<'_, TauriAppState>) -> Result<(), String> {
    let v = vault(&state)?;
    let abs = v.root().join(&rel_path);
    opener_reveal(&abs)
}

#[tauri::command]
pub fn reveal_folder(
    folder: NoteFolder,
    subpath: Option<String>,
    state: State<'_, TauriAppState>,
) -> Result<(), String> {
    let v = vault(&state)?;
    let base = v.root().join(folder.as_str());
    let target = match subpath {
        Some(sp) if !sp.is_empty() => base.join(sp),
        _ => base,
    };
    opener_reveal(&target)
}

#[tauri::command]
pub fn reveal_assets_dir(state: State<'_, TauriAppState>) -> Result<(), String> {
    let v = vault(&state)?;
    let dir = v.root().join("attachements");
    opener_reveal(&dir)
}

fn opener_reveal(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(path))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Window management ────────────────────────────────────────────

#[tauri::command]
pub fn window_minimize(window: Window) {
    let _ = window.minimize();
}

#[tauri::command]
pub fn window_toggle_maximize(window: Window) {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
    } else {
        let _ = window.maximize();
    }
}

#[tauri::command]
pub fn window_close(window: Window) {
    let _ = window.close();
}

// ── Zoom ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn zoom_in_app(state: State<'_, TauriAppState>, window: WebviewWindow) -> f64 {
    let mut z = state.zoom_factor.write();
    *z = (*z + 0.1).min(3.0);
    let _ = window.set_zoom(*z);
    *z
}

#[tauri::command]
pub fn zoom_out_app(state: State<'_, TauriAppState>, window: WebviewWindow) -> f64 {
    let mut z = state.zoom_factor.write();
    *z = (*z - 0.1).max(0.5);
    let _ = window.set_zoom(*z);
    *z
}

#[tauri::command]
pub fn reset_app_zoom(state: State<'_, TauriAppState>, window: WebviewWindow) -> f64 {
    let mut z = state.zoom_factor.write();
    *z = 1.0;
    let _ = window.set_zoom(1.0);
    1.0
}

// ── Clipboard ────────────────────────────────────────────────────

#[tauri::command]
pub fn clipboard_write_text(text: String, app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(&text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clipboard_read_text(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().read_text().map_err(|e| e.to_string())
}

// ── External File Handling ───────────────────────────────────────

#[tauri::command]
pub async fn read_external_file(
    _state: State<'_, TauriAppState>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"path": "", "body": "", "title": ""}))
}

#[tauri::command]
pub async fn write_external_file(_body: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn move_external_file_to_vault() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"moved": false, "path": null}))
}

#[tauri::command]
pub async fn open_markdown_file(abs_path: String, app: tauri::AppHandle) -> Result<bool, String> {
    let label = format!("external-{}", abs_path.len());
    let url = format!("index.html?externalFile={}", urlencoding_encode(&abs_path));
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App(url.into()))
        .title(&abs_path)
        .inner_size(900.0, 700.0)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(true)
}

fn urlencoding_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{:02X}", b),
        })
        .collect()
}

// ── Remote Workspace ─────────────────────────────────────────────

#[tauri::command]
pub async fn get_remote_workspace_info() -> Result<serde_json::Value, String> {
    Ok(serde_json::Value::Null)
}

#[tauri::command]
pub async fn connect_remote_workspace(
    _base_url: String,
    _auth_token: Option<String>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"vault": null, "capabilities": {}}))
}

#[tauri::command]
pub async fn disconnect_remote_workspace() -> Result<serde_json::Value, String> {
    Ok(serde_json::Value::Null)
}

#[tauri::command]
pub async fn list_remote_workspace_profiles() -> Result<Vec<serde_json::Value>, String> {
    Ok(vec![])
}

#[tauri::command]
pub async fn save_remote_workspace_profile(
    input: serde_json::Value,
) -> Result<serde_json::Value, String> {
    Ok(input)
}

#[tauri::command]
pub async fn delete_remote_workspace_profile(_id: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub async fn connect_remote_workspace_profile(_id: String) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"vault": null, "capabilities": {}}))
}

#[tauri::command]
pub async fn get_server_capabilities() -> Result<serde_json::Value, String> {
    Ok(serde_json::Value::Null)
}

#[tauri::command]
pub async fn get_server_session() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"authenticated": false, "requiresAuth": false}))
}

#[tauri::command]
pub async fn login_server_session(_token: String) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"authenticated": false, "requiresAuth": true}))
}

#[tauri::command]
pub async fn logout_server_session() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"authenticated": false, "requiresAuth": true}))
}

// ── TikZ Rendering ───────────────────────────────────────────────

#[tauri::command]
pub async fn render_tikz(source: String) -> Result<serde_json::Value, String> {
    let tmp = std::env::temp_dir().join(format!("tikz-{}.tex", uuid::Uuid::new_v4()));
    let doc = format!(
        r"\documentclass[tikz,border=2pt]{{standalone}}\begin{{document}}{}\end{{document}}",
        source
    );
    if std::fs::write(&tmp, &doc).is_err() {
        return Ok(serde_json::json!({"svg": null, "error": "Failed to write temp file"}));
    }
    let output = std::process::Command::new("pdflatex")
        .args(["-interaction=nonstopmode", "-output-directory"])
        .arg(tmp.parent().unwrap())
        .arg(&tmp)
        .output();
    let _ = std::fs::remove_file(&tmp);
    match output {
        Ok(out) if out.status.success() => {
            let pdf = tmp.with_extension("pdf");
            let svg_out = std::process::Command::new("pdf2svg")
                .arg(&pdf)
                .arg("-")
                .output();
            let _ = std::fs::remove_file(&pdf);
            let _ = std::fs::remove_file(tmp.with_extension("aux"));
            let _ = std::fs::remove_file(tmp.with_extension("log"));
            match svg_out {
                Ok(svg) if svg.status.success() => Ok(
                    serde_json::json!({"svg": String::from_utf8_lossy(&svg.stdout).to_string(), "error": null}),
                ),
                _ => Ok(serde_json::json!({"svg": null, "error": "pdf2svg not found or failed"})),
            }
        }
        _ => Ok(
            serde_json::json!({"svg": null, "error": "pdflatex not found or compilation failed"}),
        ),
    }
}
