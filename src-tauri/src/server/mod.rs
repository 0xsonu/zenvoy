pub mod auth;
pub mod middleware;
pub mod routes;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use parking_lot::RwLock;
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
        .layer(CatchPanicLayer::new());

    if !config.base_path.is_empty() {
        app = Router::new().nest(&config.base_path, app);
    }

    app
}
