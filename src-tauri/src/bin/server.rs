use std::sync::Arc;

use zenvoy_lib::config::Config;
use zenvoy_lib::server::{create_router, AppState};
use zenvoy_lib::vault::{types::VaultOptions, Vault};
use zenvoy_lib::watcher::VaultWatcher;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = Config::load();

    if config.auth_token.trim().is_empty()
        && !config.allow_insecure_no_auth
        && !config.bind_is_loopback()
    {
        eprintln!("refusing to start without ZENVOY_AUTH_TOKEN on a non-loopback bind");
        std::process::exit(1);
    }

    tracing::info!("vault: {}", config.vault_path);
    tracing::info!("bind: {}", config.bind);

    let vault = Vault::new(
        &config.vault_path,
        VaultOptions {
            file_mode: config.vault_file_mode,
            dir_mode: config.vault_dir_mode,
            max_asset_bytes: config.max_asset_bytes,
        },
    )
    .expect("failed to initialize vault");

    let watcher = VaultWatcher::start(vault.root()).expect("failed to start watcher");

    let state = Arc::new(AppState::new(config.clone()));
    {
        let mut v = state.vault.write();
        *v = Some(Arc::new(vault));
        let mut w = state.watcher.write();
        *w = Some(Arc::new(watcher));
    }

    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(&config.bind)
        .await
        .expect("failed to bind");
    tracing::info!("listening on http://{}", config.bind);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("server error");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl+c");
    tracing::info!("shutting down...");
}
