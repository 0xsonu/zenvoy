use notify::{RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher, Event, EventKind};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::vault::types::{NoteFolder, VaultChangeEvent};
use crate::vault::safepath::folder_for_relative_path;

const INTERNAL_VAULT_DIR: &str = ".zenvoy";
const VAULT_SETTINGS_PATH: &str = ".zenvoy/vault.json";
const NOTE_COMMENTS_PREFIX: &str = ".zenvoy/comments/";
const NOTE_COMMENTS_SUFFIX: &str = ".comments.json";

pub struct VaultWatcher {
    root: PathBuf,
    _watcher: RecommendedWatcher,
    sender: broadcast::Sender<VaultChangeEvent>,
}

impl VaultWatcher {
    pub fn start(root: impl AsRef<Path>) -> notify::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let (sender, _) = broadcast::channel(256);
        let tx = sender.clone();
        let watch_root = root.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                handle_event(&watch_root, &event, &tx);
            }
        })?;

        watcher.watch(&root, RecursiveMode::Recursive)?;

        Ok(Self { root, _watcher: watcher, sender })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<VaultChangeEvent> {
        self.sender.subscribe()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn handle_event(root: &Path, event: &Event, tx: &broadcast::Sender<VaultChangeEvent>) {
    let kind = match event.kind {
        EventKind::Create(_) => "add",
        EventKind::Modify(_) => "change",
        EventKind::Remove(_) => "unlink",
        _ => return,
    };

    for path in &event.paths {
        let rel = match path.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        // Skip hidden files (except .zenvoy internals)
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') && !rel.starts_with(INTERNAL_VAULT_DIR) {
                continue;
            }
        }

        // Vault settings
        if rel == VAULT_SETTINGS_PATH {
            let _ = tx.send(VaultChangeEvent {
                kind: kind.to_string(),
                path: rel,
                folder: NoteFolder::Inbox,
                scope: "vault-settings".to_string(),
            });
            continue;
        }

        // Note comments
        if rel.starts_with(NOTE_COMMENTS_PREFIX) && rel.ends_with(NOTE_COMMENTS_SUFFIX) {
            let note_path = &rel[NOTE_COMMENTS_PREFIX.len()..rel.len() - NOTE_COMMENTS_SUFFIX.len()];
            let folder = folder_for_relative_path(note_path).unwrap_or(NoteFolder::Inbox);
            let _ = tx.send(VaultChangeEvent {
                kind: kind.to_string(),
                path: note_path.to_string(),
                folder,
                scope: "comments".to_string(),
            });
            continue;
        }

        // Skip all other .zenvoy internals
        if rel.starts_with('.') || rel.contains("/.") {
            continue;
        }

        let folder = match folder_for_relative_path(&rel) {
            Some(f) => f,
            None => continue,
        };

        let scope = if path.is_dir() { "folder" } else { "" };

        let _ = tx.send(VaultChangeEvent {
            kind: kind.to_string(),
            path: rel,
            folder,
            scope: scope.to_string(),
        });
    }
}
