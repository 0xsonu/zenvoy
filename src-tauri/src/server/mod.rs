pub mod auth;
pub mod middleware;
pub mod routes;

use std::sync::Arc;
use std::time::Duration;

use axum::response::IntoResponse;
use axum::routing::get;
use axum::http::{StatusCode, Uri};
use axum::Router;
use parking_lot::RwLock;
use rust_embed::RustEmbed;
use tower_http::catch_panic::CatchPanicLayer;

use crate::config::Config;
use crate::vault::Vault;
use crate::watcher::VaultWatcher;

use auth::{AttemptLimiter, SessionStore};

pub struct AppState {
    pub vault: RwLock<Option<Arc<Vault>>>,
    pub watcher: RwLock<Option<Arc<VaultWatcher>>>,
    pub config: RwLock<Config>,
    pub sessions: SessionStore,
    pub limiter: AttemptLimiter,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            vault: RwLock::new(None),
            watcher: RwLock::new(None),
            config: RwLock::new(config),
            sessions: SessionStore::new(),
            limiter: AttemptLimiter::new(Duration::from_secs(600), 10),
        }
    }
}

pub fn create_router(state: Arc<AppState>) -> Router {
    let config = state.config.read().clone();

    let public_routes = routes::public_routes(state.clone());
    let session_routes = auth::session_routes(state.clone());
    let protected_routes = routes::protected_routes(state.clone());

    let cors = middleware::build_cors(&config);

    let mut app = Router::new()
        .nest("/api", public_routes)
        .nest("/api", session_routes)
        .nest(
            "/api",
            protected_routes.layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth::require_auth,
            )),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::security_headers,
        ))
        .layer(cors)
        .layer(CatchPanicLayer::new())
        .fallback(get({
            let state = state.clone();
            move |uri| serve_static(state.clone(), uri)
        }));

    if !config.base_path.is_empty() {
        app = Router::new().nest(&config.base_path, app);
    }

    app
}

#[derive(RustEmbed)]
#[folder = "../dist/"]
struct StaticAssets;

async fn serve_static(
    state: Arc<AppState>,
    uri: Uri,
) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    if let Some(content) = StaticAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return ([("content-type", mime.as_ref())], content.data.to_vec()).into_response();
    }
    match StaticAssets::get("index.html") {
        Some(content) => {
            let html = String::from_utf8_lossy(&content.data);
            let base_path = state.config.read().base_path.clone();
            let html = if !base_path.is_empty() {
                html.replace("</head>", &format!("<meta name=\"zn-base-path\" content=\"{}\">\n</head>", base_path))
            } else {
                html.to_string()
            };
            ([("content-type", "text/html; charset=utf-8")], html).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
