pub mod routes;
pub mod auth;
pub mod middleware;

use std::sync::Arc;
use axum::Router;
use parking_lot::RwLock;

use crate::config::Config;
use crate::vault::Vault;
use crate::watcher::VaultWatcher;

pub struct AppState {
    pub vault: RwLock<Option<Arc<Vault>>>,
    pub watcher: RwLock<Option<Arc<VaultWatcher>>>,
    pub config: RwLock<Config>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            vault: RwLock::new(None),
            watcher: RwLock::new(None),
            config: RwLock::new(config),
        }
    }
}

pub fn create_router(_state: Arc<AppState>) -> Router {
    Router::new()
}
