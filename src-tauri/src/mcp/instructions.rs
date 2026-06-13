use std::fs;
use std::path::PathBuf;

const INSTRUCTIONS_FILE: &str = "mcp-instructions.md";
const ZENVOY_DIR: &str = ".zenvoy";

fn instructions_path(vault_root: &str) -> PathBuf {
    PathBuf::from(vault_root)
        .join(ZENVOY_DIR)
        .join(INSTRUCTIONS_FILE)
}

pub fn read_instructions(vault_root: &str) -> String {
    fs::read_to_string(instructions_path(vault_root)).unwrap_or_default()
}

pub fn write_instructions(vault_root: &str, content: &str) -> Result<(), String> {
    let path = instructions_path(vault_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, content).map_err(|e| e.to_string())
}

pub fn claude_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("claude_desktop_config.json"))
}

pub fn cursor_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".cursor").join("mcp.json"))
}

pub fn detect_clients() -> Vec<String> {
    let mut clients = Vec::new();
    if claude_config_path().map(|p| p.exists()).unwrap_or(false) {
        clients.push("claude".into());
    }
    if cursor_config_path().map(|p| p.exists()).unwrap_or(false) {
        clients.push("cursor".into());
    }
    clients
}

pub fn install_client(client: &str, cli_path: &str) -> Result<(), String> {
    let path = match client {
        "claude" => claude_config_path().ok_or("cannot find claude config dir")?,
        "cursor" => cursor_config_path().ok_or("cannot find cursor config dir")?,
        _ => return Err(format!("unsupported client: {}", client)),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut config: serde_json::Value = if path.exists() {
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    let servers = config
        .as_object_mut()
        .unwrap()
        .entry("mcpServers")
        .or_insert(serde_json::json!({}));
    servers.as_object_mut().unwrap().insert(
        "zenvoy".into(),
        serde_json::json!({
            "command": cli_path, "args": ["mcp"]
        }),
    );
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).map_err(|e| e.to_string())
}

pub fn uninstall_client(client: &str) -> Result<(), String> {
    let path = match client {
        "claude" => claude_config_path().ok_or("cannot find claude config dir")?,
        "cursor" => cursor_config_path().ok_or("cannot find cursor config dir")?,
        _ => return Err(format!("unsupported client: {}", client)),
    };
    if !path.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut config: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::json!({}));
    if let Some(servers) = config.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        servers.remove("zenvoy");
    }
    fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).map_err(|e| e.to_string())
}
