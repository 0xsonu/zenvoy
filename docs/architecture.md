# Zenvoy Architecture

> This is a complete rewrite of the original [ZenNotes](https://github.com/ZenNotes/zennotes) (Electron + Go) in Rust.


Zenvoy uses a single codebase where the Tauri desktop app, standalone Axum server, CLI, and MCP server all share the same Rust backend library.

## Layout

```text
src/                        Frontend (React + TypeScript + Vite)
  app/
    App.tsx                 Main application component
    store.ts               Zustand state store
    components/            60+ React components
    lib/                   128+ utility modules
    styles/                Tailwind CSS
  bridge/
    contract.ts            ZenBridge interface definition
    tauri-bridge.ts        Tauri IPC adapter (desktop)
    http-bridge.ts         HTTP/WebSocket adapter (browser)
  shared/                  Domain types (15 modules)

src-tauri/
  src/
    main.rs                Tauri desktop entry point
    commands.rs            68 Tauri commands
    vault/
      mod.rs               Vault operations (~1800 lines)
      types.rs             Domain types
      parse.rs             Tag/wikilink/task extraction
      safepath.rs          Path traversal prevention
    config/mod.rs          Configuration with env vars + file persistence
    watcher/mod.rs         File watcher (notify crate)
    server/
      mod.rs               Axum router setup
      routes.rs            30+ HTTP handlers
      auth.rs              Session auth + rate limiting
      middleware.rs        Security headers + CORS
    cli/mod.rs             Clap CLI (25+ commands)
    mcp/
      mod.rs               JSON-RPC over stdio
      tools.rs             26 MCP tools
      instructions.rs      Custom instructions store
    bin/
      server.rs            Standalone Axum server binary
      cli.rs               zen CLI binary
```

## Bridge Contract

The frontend depends on a typed `ZenBridge` interface defined in `src/bridge/contract.ts`.

Each runtime installs its own implementation:

- **Tauri desktop**: `tauri-bridge.ts` — calls Rust commands via `@tauri-apps/api invoke()`
- **Browser/web**: `http-bridge.ts` — calls the Axum server via `fetch` + WebSocket

The bridge covers:

- note and folder CRUD
- search, tasks, archive, trash, tags
- asset operations
- watcher/subscription events
- remote workspace connection
- capability flags for platform-specific features

## Binaries

The Rust crate produces three binaries:

1. **zenvoy** (default) — Tauri desktop app
2. **zenvoy-server** — standalone Axum HTTP server with embedded frontend
3. **zen** — CLI for terminal workflows and MCP server

All three share the same `vault`, `config`, `watcher`, and `server` modules.

## Server

The Axum server provides:

- Embedded static file serving (SPA fallback from `dist/`)
- REST API under `/api/` for all vault operations
- WebSocket at `/api/watch` for real-time file change events
- Session-based auth with rate-limited login
- Security headers and CORS middleware
- Directory browsing for vault selection

## Desktop

The Tauri app provides:

- Native window management (main, floating notes, quick capture)
- 68 registered commands for direct IPC
- File watcher integration
- All vault operations available locally without a network
