use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "default_vault_path")]
    pub vault_path: String,
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub base_path: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(skip)]
    pub browse_roots: Vec<String>,
    #[serde(skip)]
    pub allowed_origins: Vec<String>,
    #[serde(skip)]
    pub allow_unscoped_browse: bool,
    #[serde(skip)]
    pub allow_insecure_no_auth: bool,
    #[serde(skip)]
    pub behind_tls: bool,
    #[serde(skip)]
    pub max_asset_bytes: i64,
    #[serde(skip)]
    pub max_note_bytes: i64,
    #[serde(skip)]
    pub vault_file_mode: u32,
    #[serde(skip)]
    pub vault_dir_mode: u32,
}

fn default_vault_path() -> String {
    if let Some(home) = dirs::home_dir() {
        home.join("ZenvoyVault").to_string_lossy().to_string()
    } else {
        "./vault".to_string()
    }
}

fn default_bind() -> String {
    "127.0.0.1:7878".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vault_path: default_vault_path(),
            bind: default_bind(),
            base_path: String::new(),
            auth_token: String::new(),
            browse_roots: vec![],
            allowed_origins: vec![],
            allow_unscoped_browse: false,
            allow_insecure_no_auth: false,
            behind_tls: false,
            max_asset_bytes: 50 << 20,
            max_note_bytes: 10 << 20,
            vault_file_mode: 0o600,
            vault_dir_mode: 0o700,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut cfg = Self::default();

        // Load from config file
        if let Ok(raw) = fs::read_to_string(config_file_path()) {
            if let Ok(stored) = serde_json::from_str::<Config>(&raw) {
                if !stored.vault_path.is_empty() { cfg.vault_path = stored.vault_path; }
                if !stored.bind.is_empty() { cfg.bind = stored.bind; }
                if !stored.base_path.is_empty() { cfg.base_path = stored.base_path; }
                if !stored.auth_token.is_empty() { cfg.auth_token = stored.auth_token; }
            }
        }

        // Env overrides
        if let Ok(v) = std::env::var("ZENVOY_VAULT_PATH") { cfg.vault_path = v; }
        if let Ok(v) = std::env::var("ZENVOY_BIND") { cfg.bind = v; }
        if let Ok(v) = std::env::var("ZENVOY_BASE_PATH") { cfg.base_path = v; }
        if let Ok(v) = std::env::var("ZENVOY_AUTH_TOKEN") { cfg.auth_token = v; }
        if let Ok(v) = std::env::var("ZENVOY_BROWSE_ROOTS") {
            cfg.browse_roots = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        }
        if let Ok(v) = std::env::var("ZENVOY_ALLOWED_ORIGINS") {
            cfg.allowed_origins = v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        }
        cfg.allow_unscoped_browse = env_enabled("ZENVOY_ALLOW_UNSCOPED_BROWSE");
        cfg.allow_insecure_no_auth = env_enabled("ZENVOY_ALLOW_INSECURE_NOAUTH");
        cfg.behind_tls = env_enabled("ZENVOY_BEHIND_TLS");

        if let Ok(v) = std::env::var("ZENVOY_MAX_ASSET_BYTES") {
            if let Ok(n) = v.parse::<i64>() { if n > 0 { cfg.max_asset_bytes = n; } }
        }
        if let Ok(v) = std::env::var("ZENVOY_MAX_NOTE_BYTES") {
            if let Ok(n) = v.parse::<i64>() { if n > 0 { cfg.max_note_bytes = n; } }
        }

        cfg.base_path = normalize_base_path(&cfg.base_path);
        cfg
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = config_file_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        fs::write(path, json)
    }

    pub fn bind_is_loopback(&self) -> bool {
        let host = self.bind.split(':').next().unwrap_or("");
        if host.eq_ignore_ascii_case("localhost") { return true; }
        if let Ok(ip) = host.parse::<IpAddr>() { return ip.is_loopback(); }
        false
    }
}

fn config_file_path() -> PathBuf {
    if let Ok(v) = std::env::var("ZENVOY_CONFIG_PATH") {
        return PathBuf::from(v);
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".zenvoy").join("server.json");
    }
    PathBuf::from(".zenvoy-server.json")
}

pub fn normalize_base_path(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "/" { return String::new(); }
    let mut result = if !trimmed.starts_with('/') {
        format!("/{}", trimmed)
    } else {
        trimmed.to_string()
    };
    while result.contains("//") {
        result = result.replace("//", "/");
    }
    result = result.trim_end_matches('/').to_string();
    if result.is_empty() { return String::new(); }
    result
}

fn env_enabled(name: &str) -> bool {
    matches!(
        std::env::var(name).unwrap_or_default().trim().to_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}
