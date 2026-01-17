pub mod commands;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "zen", about = "Zenvoy CLI", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    /// Vault name or path
    #[arg(long, global = true)]
    pub vault: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// List notes
    List,
    /// Read a note
    Read { path: String },
    /// Create a new note
    Create {
        #[arg(long)]
        folder: Option<String>,
        #[arg(long)]
        title: Option<String>,
    },
    /// Write note content
    Write { path: String },
    /// Append to a note
    Append { path: String },
    /// Prepend to a note
    Prepend { path: String },
    /// Rename a note
    Rename { path: String, title: String },
    /// Move a note
    Move { path: String, folder: String },
    /// Archive a note
    Archive { path: String },
    /// Unarchive a note
    Unarchive { path: String },
    /// Trash a note
    Trash { path: String },
    /// Restore a note from trash
    Restore { path: String },
    /// Permanently delete a note
    Delete { path: String },
    /// Duplicate a note
    Duplicate { path: String },
    /// Search notes by content
    Search { query: String },
    /// Search notes by title
    #[command(name = "search-title")]
    SearchTitle { query: String },
    /// Find notes linking to a target
    Backlinks { path: String },
    /// Folder operations
    Folder {
        #[command(subcommand)]
        action: FolderAction,
    },
    /// Task operations
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// Tag operations
    Tag {
        #[command(subcommand)]
        action: TagAction,
    },
    /// Vault operations
    Vault {
        #[command(subcommand)]
        action: VaultAction,
    },
    /// Quick capture
    Capture {
        #[arg(long)]
        body: Option<String>,
    },
    /// Open a note in the app
    Open { path: String },
    /// Start MCP server
    Mcp,
}

#[derive(Subcommand)]
pub enum FolderAction {
    List,
    Create { folder: String, subpath: String },
    Rename { folder: String, old: String, new: String },
    Delete { folder: String, subpath: String },
}

#[derive(Subcommand)]
pub enum TaskAction {
    List,
    Toggle { id: String },
}

#[derive(Subcommand)]
pub enum TagAction {
    List,
    Find { tag: String },
}

#[derive(Subcommand)]
pub enum VaultAction {
    Info,
    List,
}
