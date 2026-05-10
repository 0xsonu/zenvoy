pub mod tools;
pub mod instructions;

use std::io::{BufRead, Write};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::vault::{Vault, VaultOptions};
use crate::config::Config;

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    params: Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Value>,
}

pub fn run_mcp_server() {
    let config = Config::load();
    let vault = Vault::new(&config.vault_path, VaultOptions::default())
        .expect("Failed to open vault for MCP");

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() { continue; }
        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let response = handle_request(&vault, &request);
        let json = serde_json::to_string(&response).unwrap();
        writeln!(stdout, "{}", json).ok();
        stdout.flush().ok();
    }
}

fn handle_request(vault: &Vault, req: &JsonRpcRequest) -> JsonRpcResponse {
    let id = req.id.clone().unwrap_or(Value::Null);
    match req.method.as_str() {
        "initialize" => success(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "zen-mcp", "version": "1.0.0"}
        })),
        "tools/list" => success(id, json!({"tools": tools::list_tools()})),
        "tools/call" => {
            let params = req.params.clone().unwrap_or_default();
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_default();
            match tools::call_tool(vault, name, args) {
                Ok(result) => success(id, json!({"content": [{"type": "text", "text": result}]})),
                Err(e) => error(id, -1, &e),
            }
        }
        "notifications/initialized" => JsonRpcResponse { jsonrpc: "2.0".into(), id, result: None, error: None },
        _ => error(id, -32601, "method not found"),
    }
}

fn success(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse { jsonrpc: "2.0".into(), id, result: Some(result), error: None }
}

fn error(id: Value, code: i32, msg: &str) -> JsonRpcResponse {
    JsonRpcResponse { jsonrpc: "2.0".into(), id, result: None, error: Some(json!({"code": code, "message": msg})) }
}
