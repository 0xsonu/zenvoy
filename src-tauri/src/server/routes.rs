use std::sync::Arc;
use std::path::{Path, PathBuf};

use axum::{extract::{Query, State}, http::StatusCode, routing::{get, post}, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::vault::{Vault, VaultOptions, VaultSettings};
use crate::watcher::VaultWatcher;
use super::AppState;

pub fn public_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/version", get(version))
        .route("/platform", get(platform))
        .route("/capabilities", get(capabilities))
        .with_state(state)
}

pub fn protected_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/vault", get(vault_info))
        .route("/vault/settings", get(vault_settings).post(set_vault_settings))
        .route("/vault/select", post(select_vault))
        .route("/fs/browse", get(browse_directories))
        .route("/notes", get(list_notes))
        .route("/notes/read", get(read_note))
        .route("/notes/write", post(write_note))
        .route("/notes/create", post(create_note))
        .route("/notes/rename", post(rename_note))
        .route("/notes/delete", post(delete_note))
        .route("/notes/duplicate", post(duplicate_note))
        .route("/notes/move", post(move_note))
        .route("/notes/trash", post(trash_note))
        .route("/notes/restore", post(restore_note))
        .route("/notes/empty-trash", post(empty_trash))
        .route("/notes/archive", post(archive_note))
        .route("/notes/unarchive", post(unarchive_note))
        .route("/folders", get(list_folders))
        .route("/folders/create", post(create_folder))
        .route("/folders/rename", post(rename_folder))
        .route("/folders/delete", post(delete_folder))
        .route("/folders/duplicate", post(duplicate_folder))
        .route("/assets", get(list_assets))
        .route("/assets/exists", get(assets_exists))
        .route("/comments/read", get(read_comments))
        .route("/comments/write", post(write_comments))
        .route("/search/capabilities", get(search_capabilities))
        .route("/search/text", get(search_text))
        .route("/tasks", get(all_tasks))
        .route("/tasks/for", get(tasks_for))
        .route("/demo/generate", post(demo_generate))
        .route("/demo/remove", post(demo_remove))
        .with_state(state)
}

fn get_vault(state: &AppState) -> Result<Arc<Vault>, StatusCode> {
    state.vault.read().clone().ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

// --- Meta ---

async fn healthz() -> Json<Value> { Json(json!({"ok": true})) }

async fn version() -> Json<Value> { Json(json!({"version": "1.0.0"})) }

async fn platform() -> Json<Value> {
    let p = if cfg!(target_os = "macos") { "darwin" } else if cfg!(target_os = "windows") { "win32" } else { "linux" };
    Json(json!({"platform": p}))
}

async fn capabilities(State(state): State<Arc<AppState>>) -> Json<Value> {
    let config = state.config.read();
    Json(json!({
        "version": "1.0.0",
        "platform": if cfg!(target_os = "macos") { "darwin" } else if cfg!(target_os = "windows") { "win32" } else { "linux" },
        "authRequired": !config.auth_token.is_empty(),
        "supportsSessionLogin": true,
        "browseRootsEnforced": !config.allow_unscoped_browse,
        "supportsVaultSelection": true,
        "supportsDirectoryBrowsing": true,
        "supportsWatch": true,
    }))
}

// --- Vault ---

async fn vault_info(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    Ok(Json(serde_json::to_value(vault.info()).unwrap()))
}

async fn vault_settings(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.get_settings().map(|s| Json(serde_json::to_value(s).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn set_vault_settings(State(state): State<Arc<AppState>>, Json(body): Json<VaultSettings>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.set_settings(body).map(|s| Json(serde_json::to_value(s).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct SelectVaultReq { path: String }

async fn select_vault(State(state): State<Arc<AppState>>, Json(body): Json<SelectVaultReq>) -> Result<Json<Value>, StatusCode> {
    if body.path.trim().is_empty() { return Err(StatusCode::BAD_REQUEST); }
    let config = state.config.read().clone();
    let new_vault = Vault::new(&body.path, VaultOptions { file_mode: config.vault_file_mode, dir_mode: config.vault_dir_mode, max_asset_bytes: config.max_asset_bytes }).map_err(|_| StatusCode::BAD_REQUEST)?;
    let new_watcher = VaultWatcher::start(new_vault.root()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let info = new_vault.info();
    *state.vault.write() = Some(Arc::new(new_vault));
    *state.watcher.write() = Some(Arc::new(new_watcher));
    state.config.write().vault_path = body.path;
    Ok(Json(serde_json::to_value(info).unwrap()))
}

// --- Directory Browsing ---

#[derive(Deserialize)]
struct BrowseQuery { path: Option<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowseResult { current_path: String, parent_path: Option<String>, entries: Vec<BrowseEntry>, shortcuts: Vec<BrowseShortcut> }

#[derive(Serialize)]
struct BrowseEntry { name: String, path: String }

#[derive(Serialize)]
struct BrowseShortcut { label: String, path: String }

async fn browse_directories(State(state): State<Arc<AppState>>, Query(q): Query<BrowseQuery>) -> Result<Json<BrowseResult>, StatusCode> {
    let target = q.path.unwrap_or_else(|| dirs::home_dir().map(|h| h.to_string_lossy().to_string()).unwrap_or_else(|| "/".to_string()));
    let target_path = Path::new(&target);
    if !target_path.is_dir() { return Err(StatusCode::BAD_REQUEST); }

    let entries: Vec<BrowseEntry> = std::fs::read_dir(&target).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| BrowseEntry { name: e.file_name().to_string_lossy().to_string(), path: e.path().to_string_lossy().to_string() })
        .collect();

    let mut sorted = entries;
    sorted.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let parent_path = target_path.parent().filter(|p| *p != target_path).map(|p| p.to_string_lossy().to_string());

    let mut shortcuts = vec![];
    if let Some(home) = dirs::home_dir() { shortcuts.push(BrowseShortcut { label: "Home".to_string(), path: home.to_string_lossy().to_string() }); }
    let config = state.config.read();
    if !config.vault_path.is_empty() { shortcuts.push(BrowseShortcut { label: "Vault".to_string(), path: config.vault_path.clone() }); }

    Ok(Json(BrowseResult { current_path: target, parent_path, entries: sorted, shortcuts }))
}

// --- Notes ---

async fn list_notes(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.list_notes().map(|n| Json(serde_json::to_value(n).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct PathQuery { path: String }

async fn read_note(State(state): State<Arc<AppState>>, Query(q): Query<PathQuery>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.read_note(&q.path).map(|n| Json(serde_json::to_value(n).unwrap())).map_err(|_| StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct WriteNoteReq { path: String, body: String }

async fn write_note(State(state): State<Arc<AppState>>, Json(req): Json<WriteNoteReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.write_note(&req.path, &req.body).map(|m| Json(serde_json::to_value(m).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct CreateNoteReq { folder: String, title: Option<String>, subpath: Option<String> }

async fn create_note(State(state): State<Arc<AppState>>, Json(req): Json<CreateNoteReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    let folder = crate::vault::NoteFolder::from_str(&req.folder).ok_or(StatusCode::BAD_REQUEST)?;
    vault.create_note(&folder, req.title.as_deref(), req.subpath.as_deref()).map(|m| Json(serde_json::to_value(m).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct RenameReq { path: String, title: String }

async fn rename_note(State(state): State<Arc<AppState>>, Json(req): Json<RenameReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.rename_note(&req.path, &req.title).map(|m| Json(serde_json::to_value(m).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct PathReq { path: String }

async fn delete_note(State(state): State<Arc<AppState>>, Json(req): Json<PathReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.delete_note(&req.path).map(|_| Json(json!({"ok": true}))).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn duplicate_note(State(state): State<Arc<AppState>>, Json(req): Json<PathReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.duplicate_note(&req.path).map(|m| Json(serde_json::to_value(m).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct MoveNoteReq { path: String, folder: String, subpath: Option<String> }

async fn move_note(State(state): State<Arc<AppState>>, Json(req): Json<MoveNoteReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    let folder = crate::vault::NoteFolder::from_str(&req.folder).ok_or(StatusCode::BAD_REQUEST)?;
    vault.move_note(&req.path, &folder, req.subpath.as_deref()).map(|m| Json(serde_json::to_value(m).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn trash_note(State(state): State<Arc<AppState>>, Json(req): Json<PathReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.move_to_trash(&req.path).map(|m| Json(serde_json::to_value(m).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn restore_note(State(state): State<Arc<AppState>>, Json(req): Json<PathReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.restore_from_trash(&req.path).map(|m| Json(serde_json::to_value(m).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn empty_trash(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.empty_trash().map(|_| Json(json!({"ok": true}))).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn archive_note(State(state): State<Arc<AppState>>, Json(req): Json<PathReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.archive_note(&req.path).map(|m| Json(serde_json::to_value(m).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn unarchive_note(State(state): State<Arc<AppState>>, Json(req): Json<PathReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.unarchive_note(&req.path).map(|m| Json(serde_json::to_value(m).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// --- Folders ---

async fn list_folders(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.list_folders().map(|f| Json(serde_json::to_value(f).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct CreateFolderReq { folder: String, subpath: String }

async fn create_folder(State(state): State<Arc<AppState>>, Json(req): Json<CreateFolderReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    let folder = crate::vault::NoteFolder::from_str(&req.folder).ok_or(StatusCode::BAD_REQUEST)?;
    vault.create_folder(&folder, &req.subpath).map(|_| Json(json!({"ok": true}))).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct RenameFolderReq { folder: String, old: String, new: String }

async fn rename_folder(State(state): State<Arc<AppState>>, Json(req): Json<RenameFolderReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    let folder = crate::vault::NoteFolder::from_str(&req.folder).ok_or(StatusCode::BAD_REQUEST)?;
    vault.rename_folder(&folder, &req.old, &req.new).map(|s| Json(json!({"subpath": s}))).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct DeleteFolderReq { folder: String, subpath: String }

async fn delete_folder(State(state): State<Arc<AppState>>, Json(req): Json<DeleteFolderReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    let folder = crate::vault::NoteFolder::from_str(&req.folder).ok_or(StatusCode::BAD_REQUEST)?;
    vault.delete_folder(&folder, &req.subpath).map(|_| Json(json!({"ok": true}))).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct DuplicateFolderReq { folder: String, subpath: String }

async fn duplicate_folder(State(state): State<Arc<AppState>>, Json(req): Json<DuplicateFolderReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    let folder = crate::vault::NoteFolder::from_str(&req.folder).ok_or(StatusCode::BAD_REQUEST)?;
    vault.duplicate_folder(&folder, &req.subpath).map(|s| Json(json!({"subpath": s}))).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// --- Assets ---

async fn list_assets(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.list_assets().map(|a| Json(serde_json::to_value(a).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn assets_exists(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    Ok(Json(json!({"exists": vault.has_assets_dir()})))
}

// --- Comments ---

async fn read_comments(State(state): State<Arc<AppState>>, Query(q): Query<PathQuery>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.read_note_comments(&q.path).map(|c| Json(serde_json::to_value(c).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Deserialize)]
struct WriteCommentsReq { path: String, comments: Vec<crate::vault::NoteComment> }

async fn write_comments(State(state): State<Arc<AppState>>, Json(req): Json<WriteCommentsReq>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.write_note_comments(&req.path, req.comments).map(|c| Json(serde_json::to_value(c).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// --- Search ---

async fn search_capabilities(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    Ok(Json(serde_json::to_value(vault.get_text_search_capabilities()).unwrap()))
}

#[derive(Deserialize)]
struct SearchQuery { query: String, backend: Option<String> }

async fn search_text(State(state): State<Arc<AppState>>, Query(q): Query<SearchQuery>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.search_vault_text(&q.query, q.backend.as_deref()).map(|r| Json(serde_json::to_value(r).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// --- Tasks ---

async fn all_tasks(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.scan_tasks().map(|t| Json(serde_json::to_value(t).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn tasks_for(State(state): State<Arc<AppState>>, Query(q): Query<PathQuery>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.scan_tasks_for_path(&q.path).map(|t| Json(serde_json::to_value(t).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

// --- Demo Tour ---

async fn demo_generate(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.generate_demo_tour().map(|r| Json(serde_json::to_value(r).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn demo_remove(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let vault = get_vault(&state)?;
    vault.remove_demo_tour().map(|r| Json(serde_json::to_value(r).unwrap())).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
