# Zenvoy

<p align="center">
  <img src="src-tauri/icons/icon.png" alt="Zenvoy app icon" width="160">
</p>

Zenvoy is a keyboard-first Markdown notes app with multiple runtimes:

- a desktop app built with Tauri
- a self-hosted web app backed by a Rust (Axum) server
- a `zen` CLI for terminal workflows
- a first-party MCP server for AI tool integration

Zenvoy keeps your notes as ordinary Markdown files on disk. It adds Vim-friendly editing, split and preview workflows, tasks, tags, archive/trash, diagrams, search, daily notes, CSV databases (Notion-style Table + Board views over plain `.csv` files), and MCP integration on top of the files you already own.

Website: [](https://)

## Install

Download the latest release from the [Releases page](https://github.com/0xsonu/zenvoy/releases).

### macOS

After installing, macOS may show "Zenvoy is damaged and can't be opened" because the app is not notarized with Apple. To fix this, run:

```bash
xattr -cr /Applications/Zenvoy.app
```

Or right-click the app → Open → Open to bypass Gatekeeper on first launch.

### Desktop (Build from source)

```bash
npm ci
npx tauri build
```

The built app is in `src-tauri/target/release/bundle/`.

### `zen` CLI

Build the CLI:

```bash
cd src-tauri && cargo build --bin zen --release
```

The binary is at `src-tauri/target/release/zen`. Copy it to your PATH.

Commands include: list, read, search, capture, edit, archive/trash notes, tasks, folders, and MCP.

### Self-hosted web app

Run the standalone Axum server:

```bash
cd src-tauri && cargo run --bin zenvoy-server --release
```

Then open [http://localhost:7878](http://localhost:7878).

Or use Docker:

```bash
docker compose up -d
```

## What Zenvoy is for

- writing and organizing plain-file Markdown notes without a database
- moving quickly with keyboard-first navigation and Vim motions
- working across edit, split, and preview modes without losing context
- keeping tasks, tags, search, archive, trash, and quick capture inside the same vault
- rendering math and diagrams directly from Markdown
- exposing the vault to MCP-capable tools through a first-party server
- searching and opening notes from terminal scripts
- self-hosting the app on your own machine or home server

## Product modes

- `desktop`: Tauri shell with native menus, updater, floating windows
- `self-hosted`: browser frontend plus Rust server, suitable for home servers and LAN use
- `cli`: terminal-based note management via the `zen` binary

## Core ideas

### Plain files first

Every note is a normal `.md` file inside a chosen vault. Zenvoy does not store note content in a hidden database.

### Keyboard-first by default

Zenvoy assumes you want to move fast:

- first-class Vim mode
- leader-key flows
- command palette
- pane and tab motion
- local ex commands
- built-in help

### Preview is part of the workflow

Zenvoy supports:

- edit mode
- preview mode
- split mode
- pinned reference panes
- detached note windows on desktop

### Shared vault, shared tooling

Zenvoy includes a first-party MCP server so tools can work on the same vault you do.

## Feature overview

### Notes, folders, and lifecycle

Zenvoy can:

- create, rename, duplicate, move, archive, unarchive, trash, restore, and reveal notes and folders
- watch the vault for external changes
- reopen your workspace layout with tabs and panes

System folders:

- `quick`, `archive`, and `trash` are built-in lifecycle areas
- the main notes area can be either `inbox/` or the vault root directly (Obsidian-style flat vaults)
- built-in folder labels are customizable in the UI

### Daily notes

Daily notes are optional and can be enabled from Settings.

- when enabled, Zenvoy can open or create today's note automatically
- the title is a simple ISO date like `2026-04-21`
- daily notes live in a dedicated directory under your primary notes area

### Editor and preview

The editor stack is CodeMirror 6 with a Markdown-oriented workflow:

- live preview behavior in the editor
- heading folding
- outline extraction and jumps
- configurable line numbers and line-height (position: next to text or editor edge)
- syntax highlighting for fenced code blocks
- wiki links, callouts, tables, footnotes, and local embeds
- ==highlight== syntax with multi-color highlighter (8 colors, right-click menu)
- Vim block cursor and keyboard navigation
- per-note undo history (Cmd+Z never crosses note boundaries)
- /slash-command autocomplete in the editor and quick capture
- WYSIWYG table navigation keys are remappable

Preview and split mode support:

- GitHub-flavored Markdown
- KaTeX math
- Mermaid
- TikZ
- JSXGraph
- function-plot
- callouts, footnotes, wiki links, and backlinks

### Search, tasks, tags, and built-in views

- note search by title and path
- vault-wide text search
- tags view (combine with AND/OR, match all/any toggle)
- tasks view (customizable label, reorder via Shift+J/K or drag, @waiting tasks show on calendar)
- archive view
- trash view
- quick notes view
- home view (landing page with greeting, quick-create, recent notes, and today's tasks)
- built-in help/manual

### Obsidian-friendly vault support

- primary notes can live at the vault root instead of requiring `inbox/`
- loose files anywhere in the vault are surfaced as files/assets
- embedded files like `![[image.png]]` resolve like Obsidian
- CSV databases are linkable via [[wikilinks]]
- Obsidian Excalidraw drawings can be imported to native `.excalidraw` format
- legacy `attachements/` and `_assets/` folders are recognized

### Files and local assets

- local images and files appear in the vault tree
- images, SVGs, videos, audio, PDFs open inside Zenvoy tabs
- watcher updates include non-Markdown file changes
- sidebar multi-select with Cmd/Ctrl-click and Shift-click
- virtualized note list for large vaults (5000+ notes)
- manual drag-to-reorder note ordering
- vault-root notice dismissable per vault

### Themes, fonts, and customization

- theme families and light/dark/auto modes (includes Kanagawa Wave/Dragon/Lotus)
- interface, text, and monospace font selection
- editor font size and line-height controls
- preview and editor width controls
- keymap overrides
- Vim toggles and leader hint behavior
- search backend selection
- vault layout and daily notes settings
- system-folder display labels
- portable config file (`config.toml`) — sync prefs across machines with git/stow/chezmoi
- grouped settings rail with sub-tabbed pages

## Architecture

```text
src/                    Frontend (React + TypeScript + Vite)
  app/                  App components, store, lib
  bridge/              Bridge adapters (Tauri IPC + HTTP)
  shared/              Domain types
src-tauri/
  src/
    main.rs            Tauri desktop entry point
    commands.rs        68 Tauri commands
    vault/             Vault operations (CRUD, trash, archive, search, etc.)
    config/            Configuration management
    watcher/           File system watcher (notify crate)
    server/            Axum HTTP server (routes, auth, middleware, WebSocket)
    cli/               zen CLI (clap)
    mcp/               MCP JSON-RPC server (26 tools)
    bin/
      server.rs        Standalone Axum server binary
      cli.rs           zen CLI binary
```

## Development

### Requirements

- Node.js 22+
- Rust (stable)
- npm

### Install dependencies

```bash
npm ci
```

### Run the desktop app

```bash
npx tauri dev
```

### Run the standalone server

```bash
cd src-tauri && cargo run --bin zenvoy-server
```

Environment variables:

- `ZENVOY_VAULT_PATH`: path to the vault directory (default: `~/ZenvoyVault`)
- `ZENVOY_BIND`: server bind address (default: `127.0.0.1:7878`)
- `ZENVOY_AUTH_TOKEN`: bearer token for non-loopback access

### Run the CLI

```bash
cd src-tauri && cargo run --bin zen -- --help
```

### Run tests

```bash
cd src-tauri && cargo test --lib
```

### Build for production

```bash
npx tauri build
```

## Self-hosting with Docker

### Start the self-hosted app

```bash
docker compose up -d
```

Then open [http://localhost:7878](http://localhost:7878).

### Default Docker mounts

- host `./vault` → container vault directory
- host `./data` → container `/data`

### Security defaults

- published port binds to `127.0.0.1` unless overridden
- auth token generated on first run and stored in `./data/auth-token`
- browser signs in with token once, then uses an `HttpOnly` session cookie
- container runs as local UID/GID with read-only root filesystem

### Choosing a different vault folder

```bash
CONTENT_ROOT="$HOME/Documents/MyVault" docker compose up -d
```

### Environment variables

- `ZENVOY_BIND`: server bind address
- `ZENVOY_VAULT_PATH`: hard-lock to a specific vault path
- `ZENVOY_AUTH_TOKEN`: set a specific auth token
- `ZENVOY_BROWSE_ROOTS`: limit what the web picker can browse
- `ZENVOY_ALLOWED_ORIGINS`: restrict which browser origins can connect
- `ALLOW_INSECURE_NOAUTH=1`: disable auth (loopback only recommended)

## MCP integration

Zenvoy ships a dedicated MCP server exposing 26 vault tools:

- reading, creating, moving, appending to notes
- listing notes, folders, and assets
- searching vault text
- toggling tasks
- managing templates and comments

Run the MCP server:

```bash
cd src-tauri && cargo run --bin zen -- mcp
```

## Web vault picker

The self-hosted web build includes a server-backed vault chooser:

- browses folders on the server, not the browser machine
- only browses configured allowed roots by default
- supports common shortcuts (iCloud Drive, home, documents)

## Current status

Zenvoy is actively evolving. The desktop app and self-hosted server share the same Rust backend. The `zen` CLI provides terminal access to all vault operations.

## Acknowledgments

This project is a complete rewrite of the original [ZenNotes](https://github.com/ZenNotes/zennotes) Electron + Go codebase in Rust (Tauri + Axum). Thanks to all the contributors of the original project whose work made this possible.

## License

MIT
