use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use super::AppState;

pub fn api_routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/version", get(version))
        .route("/platform", get(platform))
        .route("/capabilities", get(capabilities))
        .with_state(state)
}

async fn healthz() -> Json<Value> {
    Json(json!({"ok": true}))
}

async fn version() -> Json<Value> {
    Json(json!({"version": "1.0.0"}))
}

async fn platform() -> Json<Value> {
    let p = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "win32"
    } else {
        "linux"
    };
    Json(json!({"platform": p}))
}

async fn capabilities(State(state): State<Arc<AppState>>) -> Json<Value> {
    let config = state.config.read();
    Json(json!({
        "version": "1.0.0",
        "auth_required": !config.auth_token.is_empty(),
        "behind_tls": config.behind_tls,
        "max_asset_bytes": config.max_asset_bytes,
        "max_note_bytes": config.max_note_bytes,
    }))
}
