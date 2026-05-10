use serde_json::{json, Value};
use crate::vault::{Vault, NoteFolder};

pub fn list_tools() -> Value {
    json!([
        tool("list_notes", "List all notes in the vault", json!({"type":"object","properties":{}})),
        tool("read_note", "Read a note's content", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
        tool("write_note", "Overwrite a note's body", json!({"type":"object","properties":{"path":{"type":"string"},"body":{"type":"string"}},"required":["path","body"]})),
        tool("create_note", "Create a new note", json!({"type":"object","properties":{"folder":{"type":"string"},"title":{"type":"string"}},"required":[]})),
        tool("append_to_note", "Append text to a note", json!({"type":"object","properties":{"path":{"type":"string"},"body":{"type":"string"}},"required":["path","body"]})),
        tool("prepend_to_note", "Prepend text to a note", json!({"type":"object","properties":{"path":{"type":"string"},"body":{"type":"string"}},"required":["path","body"]})),
        tool("rename_note", "Rename a note", json!({"type":"object","properties":{"path":{"type":"string"},"title":{"type":"string"}},"required":["path","title"]})),
        tool("move_note", "Move a note to a folder", json!({"type":"object","properties":{"path":{"type":"string"},"folder":{"type":"string"}},"required":["path","folder"]})),
        tool("archive_note", "Archive a note", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
        tool("unarchive_note", "Unarchive a note", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
        tool("move_to_trash", "Move a note to trash", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
        tool("restore_from_trash", "Restore a note from trash", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
        tool("empty_trash", "Empty the trash folder", json!({"type":"object","properties":{}})),
        tool("delete_note", "Permanently delete a note", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
        tool("duplicate_note", "Duplicate a note", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
        tool("search_text", "Full-text search across notes", json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]})),
        tool("backlinks", "Find notes linking to a target", json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]})),
        tool("scan_all_tasks", "List all tasks across notes", json!({"type":"object","properties":{}})),
        tool("toggle_task", "Toggle a task's checked state", json!({"type":"object","properties":{"path":{"type":"string"},"line":{"type":"integer"}},"required":["path","line"]})),
        tool("list_folders", "List all folders", json!({"type":"object","properties":{}})),
        tool("create_folder", "Create a subfolder", json!({"type":"object","properties":{"folder":{"type":"string"},"subpath":{"type":"string"}},"required":["folder","subpath"]})),
        tool("rename_folder", "Rename a folder", json!({"type":"object","properties":{"folder":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"}},"required":["folder","old","new"]})),
        tool("delete_folder", "Delete a folder", json!({"type":"object","properties":{"folder":{"type":"string"},"subpath":{"type":"string"}},"required":["folder","subpath"]})),
        tool("list_assets", "List all assets/attachments", json!({"type":"object","properties":{}})),
        tool("insert_at_line", "Insert text at a specific line", json!({"type":"object","properties":{"path":{"type":"string"},"line":{"type":"integer"},"text":{"type":"string"}},"required":["path","line","text"]})),
        tool("replace_in_note", "Find and replace text in a note", json!({"type":"object","properties":{"path":{"type":"string"},"find":{"type":"string"},"replace":{"type":"string"}},"required":["path","find","replace"]})),
    ])
}

fn tool(name: &str, desc: &str, schema: Value) -> Value {
    json!({"name": name, "description": desc, "inputSchema": schema})
}

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
}

pub fn call_tool(vault: &Vault, name: &str, args: Value) -> Result<String, String> {
    match name {
        "list_notes" => {
            let notes = vault.list_notes().map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&notes).unwrap())
        }
        "read_note" => {
            let content = vault.read_note(&s(&args, "path")).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&content).unwrap())
        }
        "write_note" => {
            let meta = vault.write_note(&s(&args, "path"), &s(&args, "body")).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "create_note" => {
            let folder_str = s(&args, "folder");
            let folder = if folder_str.is_empty() { NoteFolder::Inbox } else { NoteFolder::from_str(&folder_str).ok_or("invalid folder")? };
            let title = args.get("title").and_then(|v| v.as_str());
            let meta = vault.create_note(&folder, title, None).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "append_to_note" => {
            let meta = vault.append_to_note(&s(&args, "path"), &s(&args, "body"), "end").map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "prepend_to_note" => {
            let meta = vault.append_to_note(&s(&args, "path"), &s(&args, "body"), "prepend").map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "rename_note" => {
            let meta = vault.rename_note(&s(&args, "path"), &s(&args, "title")).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "move_note" => {
            let folder = NoteFolder::from_str(&s(&args, "folder")).ok_or("invalid folder")?;
            let meta = vault.move_note(&s(&args, "path"), &folder, None).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "archive_note" => {
            let meta = vault.archive_note(&s(&args, "path")).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "unarchive_note" => {
            let meta = vault.unarchive_note(&s(&args, "path")).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "move_to_trash" => {
            let meta = vault.move_to_trash(&s(&args, "path")).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "restore_from_trash" => {
            let meta = vault.restore_from_trash(&s(&args, "path")).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "empty_trash" => {
            vault.empty_trash().map_err(|e| e.to_string())?;
            Ok(json!({"status":"ok"}).to_string())
        }
        "delete_note" => {
            vault.delete_note(&s(&args, "path")).map_err(|e| e.to_string())?;
            Ok(json!({"status":"deleted"}).to_string())
        }
        "duplicate_note" => {
            let meta = vault.duplicate_note(&s(&args, "path")).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "search_text" => {
            let results = vault.search_vault_text(&s(&args, "query"), None).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&results).unwrap())
        }
        "backlinks" => {
            let path = s(&args, "path");
            let notes = vault.list_notes().map_err(|e| e.to_string())?;
            let target_title = notes.iter().find(|n| n.path == path).map(|n| n.title.clone()).unwrap_or_default();
            let links: Vec<_> = notes.iter().filter(|n| n.wikilinks.iter().any(|w| w.eq_ignore_ascii_case(&target_title))).collect();
            Ok(serde_json::to_string(&links).unwrap())
        }
        "scan_all_tasks" => {
            let tasks = vault.scan_tasks().map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&tasks).unwrap())
        }
        "toggle_task" => {
            let path = s(&args, "path");
            let line = args.get("line").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
            let content = vault.read_note(&path).map_err(|e| e.to_string())?;
            let mut lines: Vec<&str> = content.body.lines().collect();
            if line == 0 || line > lines.len() { return Err("invalid line number".into()); }
            let idx = line - 1;
            let l = lines[idx];
            let toggled = if l.contains("- [ ]") {
                l.replacen("- [ ]", "- [x]", 1)
            } else if l.contains("- [x]") {
                l.replacen("- [x]", "- [ ]", 1)
            } else {
                return Err("line is not a task".into());
            };
            lines[idx] = &toggled;
            let new_body = lines.join("\n");
            vault.write_note(&path, &new_body).map_err(|e| e.to_string())?;
            Ok(json!({"status":"toggled","line":line}).to_string())
        }
        "list_folders" => {
            let folders = vault.list_folders().map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&folders).unwrap())
        }
        "create_folder" => {
            let folder = NoteFolder::from_str(&s(&args, "folder")).ok_or("invalid folder")?;
            vault.create_folder(&folder, &s(&args, "subpath")).map_err(|e| e.to_string())?;
            Ok(json!({"status":"ok"}).to_string())
        }
        "rename_folder" => {
            let folder = NoteFolder::from_str(&s(&args, "folder")).ok_or("invalid folder")?;
            let result = vault.rename_folder(&folder, &s(&args, "old"), &s(&args, "new")).map_err(|e| e.to_string())?;
            Ok(json!({"newPath": result}).to_string())
        }
        "delete_folder" => {
            let folder = NoteFolder::from_str(&s(&args, "folder")).ok_or("invalid folder")?;
            vault.delete_folder(&folder, &s(&args, "subpath")).map_err(|e| e.to_string())?;
            Ok(json!({"status":"deleted"}).to_string())
        }
        "list_assets" => {
            let assets = vault.list_assets().map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&assets).unwrap())
        }
        "insert_at_line" => {
            let path = s(&args, "path");
            let line = args.get("line").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
            let text = s(&args, "text");
            let content = vault.read_note(&path).map_err(|e| e.to_string())?;
            let mut lines: Vec<String> = content.body.lines().map(|l| l.to_string()).collect();
            let idx = if line == 0 { 0 } else { (line - 1).min(lines.len()) };
            lines.insert(idx, text);
            let new_body = lines.join("\n");
            let meta = vault.write_note(&path, &new_body).map_err(|e| e.to_string())?;
            Ok(serde_json::to_string(&meta).unwrap())
        }
        "replace_in_note" => {
            let path = s(&args, "path");
            let find = s(&args, "find");
            let replace = s(&args, "replace");
            let content = vault.read_note(&path).map_err(|e| e.to_string())?;
            let new_body = content.body.replace(&find, &replace);
            let count = content.body.matches(&find).count();
            vault.write_note(&path, &new_body).map_err(|e| e.to_string())?;
            Ok(json!({"replacements": count}).to_string())
        }
        _ => Err(format!("unknown tool: {}", name)),
    }
}
