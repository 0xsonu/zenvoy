use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum NoteFolder {
    Inbox,
    Quick,
    Archive,
    Trash,
}

impl NoteFolder {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Quick => "quick",
            Self::Archive => "archive",
            Self::Trash => "trash",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "inbox" => Some(Self::Inbox),
            "quick" => Some(Self::Quick),
            "archive" => Some(Self::Archive),
            "trash" => Some(Self::Trash),
            _ => None,
        }
    }
}

impl std::fmt::Display for NoteFolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Default)]
pub struct VaultOptions {
    pub file_mode: u32,
    pub dir_mode: u32,
    pub max_asset_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSettings {
    pub primary_notes_location: String,
    pub daily_notes: DailyNotesSettings,
    pub weekly_notes: WeeklyNotesSettings,
    #[serde(default)]
    pub folder_icons: HashMap<String, String>,
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            primary_notes_location: "inbox".to_string(),
            daily_notes: DailyNotesSettings::default(),
            weekly_notes: WeeklyNotesSettings::default(),
            folder_icons: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyNotesSettings {
    pub enabled: bool,
    pub directory: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub template_id: String,
}

impl Default for DailyNotesSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            directory: "Daily Notes".to_string(),
            template_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyNotesSettings {
    pub enabled: bool,
    pub directory: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub template_id: String,
}

impl Default for WeeklyNotesSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            directory: "Weekly Notes".to_string(),
            template_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub root: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteMeta {
    pub path: String,
    pub title: String,
    pub folder: NoteFolder,
    pub sibling_order: i32,
    pub created_at: i64,
    pub updated_at: i64,
    pub size: i64,
    pub tags: Vec<String>,
    pub wikilinks: Vec<String>,
    pub has_attachments: bool,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteContent {
    #[serde(flatten)]
    pub meta: NoteMeta,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderEntry {
    pub folder: NoteFolder,
    pub subpath: String,
    pub sibling_order: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMeta {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub sibling_order: i32,
    pub size: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedAsset {
    pub name: String,
    pub path: String,
    pub markdown: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteComment {
    pub id: String,
    pub note_path: String,
    pub anchor_start: i32,
    pub anchor_end: i32,
    pub anchor_text: String,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSearchMatch {
    pub path: String,
    pub title: String,
    pub folder: NoteFolder,
    pub line_number: i32,
    pub offset: i32,
    pub line_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSearchCapabilities {
    pub ripgrep: bool,
    pub fzf: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultTask {
    pub id: String,
    pub source_path: String,
    pub note_title: String,
    pub note_folder: NoteFolder,
    pub line_number: i32,
    pub task_index: i32,
    pub raw_text: String,
    pub content: String,
    pub checked: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub due: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub priority: String,
    pub waiting: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultChangeEvent {
    pub kind: String,
    pub path: String,
    pub folder: NoteFolder,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedAsset {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultDemoTourResult {
    pub success: bool,
    pub paths: Vec<String>,
}
