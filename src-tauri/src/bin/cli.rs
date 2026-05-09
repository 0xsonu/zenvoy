use clap::Parser;
use zenvoy_lib::cli::{Cli, Commands, FolderAction, TaskAction, TagAction, VaultAction};
use zenvoy_lib::config::Config;
use zenvoy_lib::vault::{Vault, VaultOptions, NoteFolder};

fn main() {
    let cli = Cli::parse();
    let config = Config::load();

    // Resolve vault root from --vault flag or config
    let vault_path = cli.vault.unwrap_or(config.vault_path);

    // Some commands don't need a vault
    match &cli.command {
        Commands::Mcp => { println!("MCP server starting..."); return; }
        _ => {}
    }

    let vault = match Vault::new(&vault_path, VaultOptions::default()) {
        Ok(v) => v,
        Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
    };

    if let Err(e) = run_command(&vault, cli.command) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn run_command(vault: &Vault, cmd: Commands) -> Result<(), String> {
    match cmd {
        Commands::List => {
            let notes = vault.list_notes().map_err(|e| e.to_string())?;
            for note in notes {
                println!("{}\t{}\t{}", note.path, note.title, note.folder);
            }
        }
        Commands::Read { path } => {
            let content = vault.read_note(&path).map_err(|e| e.to_string())?;
            print!("{}", content.body);
        }
        Commands::Create { folder, title } => {
            let f = NoteFolder::from_str(&folder.unwrap_or("inbox".to_string())).ok_or("invalid folder")?;
            let meta = vault.create_note(&f, title.as_deref(), None).map_err(|e| e.to_string())?;
            println!("{}", meta.path);
        }
        Commands::Write { path } => {
            let mut body = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut body).map_err(|e| e.to_string())?;
            let meta = vault.write_note(&path, &body).map_err(|e| e.to_string())?;
            println!("{}", meta.path);
        }
        Commands::Append { path } => {
            let mut body = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut body).map_err(|e| e.to_string())?;
            vault.append_to_note(&path, &body, "end").map_err(|e| e.to_string())?;
            println!("ok");
        }
        Commands::Prepend { path } => {
            let mut body = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut body).map_err(|e| e.to_string())?;
            vault.append_to_note(&path, &body, "start").map_err(|e| e.to_string())?;
            println!("ok");
        }
        Commands::Rename { path, title } => {
            let meta = vault.rename_note(&path, &title).map_err(|e| e.to_string())?;
            println!("{}", meta.path);
        }
        Commands::Move { path, folder } => {
            let f = NoteFolder::from_str(&folder).ok_or("invalid folder")?;
            let meta = vault.move_note(&path, &f, None).map_err(|e| e.to_string())?;
            println!("{}", meta.path);
        }
        Commands::Archive { path } => {
            let meta = vault.archive_note(&path).map_err(|e| e.to_string())?;
            println!("{}", meta.path);
        }
        Commands::Unarchive { path } => {
            let meta = vault.unarchive_note(&path).map_err(|e| e.to_string())?;
            println!("{}", meta.path);
        }
        Commands::Trash { path } => {
            let meta = vault.move_to_trash(&path).map_err(|e| e.to_string())?;
            println!("{}", meta.path);
        }
        Commands::Restore { path } => {
            let meta = vault.restore_from_trash(&path).map_err(|e| e.to_string())?;
            println!("{}", meta.path);
        }
        Commands::Delete { path } => {
            vault.delete_note(&path).map_err(|e| e.to_string())?;
            println!("deleted");
        }
        Commands::Duplicate { path } => {
            let meta = vault.duplicate_note(&path).map_err(|e| e.to_string())?;
            println!("{}", meta.path);
        }
        Commands::Search { query } => {
            let results = vault.search_vault_text(&query, None).map_err(|e| e.to_string())?;
            for r in results {
                println!("{}:{}:{}", r.path, r.line_number, r.line_text);
            }
        }
        Commands::SearchTitle { query } => {
            let notes = vault.list_notes().map_err(|e| e.to_string())?;
            let q = query.to_lowercase();
            for note in notes.iter().filter(|n| n.title.to_lowercase().contains(&q)) {
                println!("{}\t{}", note.path, note.title);
            }
        }
        Commands::Backlinks { path } => {
            let notes = vault.list_notes().map_err(|e| e.to_string())?;
            let target_title = notes.iter().find(|n| n.path == path).map(|n| n.title.clone()).unwrap_or_default();
            for note in &notes {
                if note.wikilinks.iter().any(|w| w.eq_ignore_ascii_case(&target_title)) {
                    println!("{}\t{}", note.path, note.title);
                }
            }
        }
        Commands::Folder { action } => match action {
            FolderAction::List => {
                let folders = vault.list_folders().map_err(|e| e.to_string())?;
                for f in folders { println!("{}\t{}", f.folder, f.subpath); }
            }
            FolderAction::Create { folder, subpath } => {
                let f = NoteFolder::from_str(&folder).ok_or("invalid folder")?;
                vault.create_folder(&f, &subpath).map_err(|e| e.to_string())?;
                println!("ok");
            }
            FolderAction::Rename { folder, old, new } => {
                let f = NoteFolder::from_str(&folder).ok_or("invalid folder")?;
                let result = vault.rename_folder(&f, &old, &new).map_err(|e| e.to_string())?;
                println!("{}", result);
            }
            FolderAction::Delete { folder, subpath } => {
                let f = NoteFolder::from_str(&folder).ok_or("invalid folder")?;
                vault.delete_folder(&f, &subpath).map_err(|e| e.to_string())?;
                println!("ok");
            }
        }
        Commands::Task { action } => match action {
            TaskAction::List => {
                let tasks = vault.scan_tasks().map_err(|e| e.to_string())?;
                for t in tasks {
                    let status = if t.checked { "x" } else { " " };
                    println!("[{}] {} ({})", status, t.content, t.source_path);
                }
            }
            TaskAction::Toggle { id } => {
                println!("toggle {}", id);
            }
        }
        Commands::Tag { action } => match action {
            TagAction::List => {
                let notes = vault.list_notes().map_err(|e| e.to_string())?;
                let mut tags: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
                for note in &notes { for tag in &note.tags { tags.insert(tag.clone()); } }
                for tag in tags { println!("#{}", tag); }
            }
            TagAction::Find { tag } => {
                let notes = vault.list_notes().map_err(|e| e.to_string())?;
                for note in notes.iter().filter(|n| n.tags.contains(&tag)) {
                    println!("{}\t{}", note.path, note.title);
                }
            }
        }
        Commands::Vault { action } => match action {
            VaultAction::Info => {
                let info = vault.info();
                println!("name: {}\nroot: {}", info.name, info.root);
            }
            VaultAction::List => {
                println!("{}", vault.info().root);
            }
        }
        Commands::Capture { body } => {
            let content = body.unwrap_or_default();
            let meta = vault.create_note(&NoteFolder::Quick, None, None).map_err(|e| e.to_string())?;
            if !content.is_empty() {
                vault.write_note(&meta.path, &content).map_err(|e| e.to_string())?;
            }
            println!("{}", meta.path);
        }
        Commands::Open { path } => {
            println!("opening {}...", path);
        }
        Commands::Mcp => { /* handled above */ }
    }
    Ok(())
}
