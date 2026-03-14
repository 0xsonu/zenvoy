# Zenvoy Web Architecture

> This is a complete rewrite of the original [ZenNotes](https://github.com/ZenNotes/zennotes) web architecture (Go server) in Rust (Axum).


Zenvoy runs as a self-hosted web app backed by a Rust (Axum) server. The same frontend that powers the Tauri desktop app is served by the standalone server binary, accessible from any browser.

## Goals

- Browser-accessible Zenvoy with full parity for editing, navigation, search, tasks, and rendering
- Self-hostable on a home server via Docker — one container, one volume, one port
- Vault is still plain Markdown files on a filesystem — no migration, no lock-in
- Same keyboard model: Vim mode, leader flows, command palette
- MCP server keeps working against the same vault

## Architecture

```
┌──────────────────────────┐         ┌──────────────────────────┐
│        Browser           │         │     Server host           │
│                          │         │                          │
│  ┌────────────────────┐  │  HTTP   │  ┌────────────────────┐  │
│  │   Zenvoy SPA     │◄─┼─────────┼─►│  zenvoy-server   │  │
│  │  (React + Vite)    │  │   WS    │  │  (Rust, Axum)      │  │
│  │                    │  │         │  │                    │  │
│  │  CodeMirror 6      │  │         │  │  Embedded frontend │  │
│  │  Zustand store     │  │         │  │  File watcher      │  │
│  │  HTTP bridge       │  │         │  │  Vault operations  │  │
│  └────────────────────┘  │         │  └─────────┬──────────┘  │
│                          │         │            │             │
└──────────────────────────┘         │  ┌─────────▼──────────┐  │
                                     │  │  Filesystem vault  │  │
                                     │  │  ~/ZenvoyVault/  │  │
                                     │  │    inbox/          │  │
                                     │  │    quick/          │  │
                                     │  │    archive/        │  │
                                     │  │    trash/          │  │
                                     │  └────────────────────┘  │
                                     └──────────────────────────┘
```

## Server

**Language**: Rust. Chosen for single-binary deployment, fast startup, low memory usage, and safe concurrency.

**Stack**:
- `axum` — HTTP framework
- `tokio` — async runtime
- `notify` — filesystem watcher
- `rust-embed` — embeds the built frontend into the binary
- `parking_lot` — fast synchronization primitives
- `serde` / `serde_json` — serialization

**Key properties**:
- Single static binary with embedded frontend (no external files needed)
- Cold-start in milliseconds
- Minimal memory footprint
- Docker image can be minimal (just the binary)

## HTTP API

All endpoints under `/api/`. Paths relative to the vault root.

| Method | Path | Purpose |
| --- | --- | --- |
| GET | `/api/healthz` | Liveness check |
| GET | `/api/version` | Server version |
| GET | `/api/capabilities` | Feature flags |
| GET | `/api/vault` | Vault info |
| GET | `/api/vault/settings` | Vault settings |
| POST | `/api/vault/settings` | Update settings |
| POST | `/api/vault/select` | Switch vault |
| GET | `/api/fs/browse` | Directory browsing |
| GET | `/api/notes` | List notes |
| GET | `/api/notes/read` | Read a note |
| POST | `/api/notes/write` | Write a note |
| POST | `/api/notes/create` | Create a note |
| POST | `/api/notes/rename` | Rename a note |
| POST | `/api/notes/delete` | Delete permanently |
| POST | `/api/notes/move` | Move a note |
| POST | `/api/notes/trash` | Soft delete |
| POST | `/api/notes/restore` | Restore from trash |
| POST | `/api/notes/archive` | Archive |
| POST | `/api/notes/unarchive` | Unarchive |
| GET | `/api/folders` | List folders |
| POST | `/api/folders/create` | Create folder |
| GET | `/api/assets` | List assets |
| GET | `/api/search/text` | Full-text search |
| GET | `/api/tasks` | All tasks |
| WS | `/api/watch` | Real-time file events |

## Auth

- **Loopback**: no auth required when bound to 127.0.0.1
- **Remote**: bearer token required, set via `ZENVOY_AUTH_TOKEN`
- Session-based: token is exchanged for a session cookie on login
- Rate-limited: 10 login attempts per 10 minutes per IP

## Client Bridge

The browser frontend uses `http-bridge.ts` which implements the same `ZenBridge` interface as the Tauri bridge but over HTTP/WebSocket:

- All vault operations go through `fetch()` to `/api/*`
- File change events stream over WebSocket at `/api/watch`
- Capabilities report web-specific feature flags

Runtime detection in `main.tsx`:
- If `window.__TAURI_INTERNALS__` exists → Tauri bridge (desktop)
- Otherwise → HTTP bridge (browser)

## Deployment

### Binary

Download, run, open browser. No runtime dependencies.

```bash
ZENVOY_VAULT_PATH=/path/to/vault ./zenvoy-server
```

### Docker

```yaml
services:
  zenvoy:
    build: .
    volumes:
      - ./vault:/vault
    environment:
      ZENVOY_VAULT_PATH: /vault
      ZENVOY_BIND: 0.0.0.0:7878
    ports:
      - "127.0.0.1:7878:7878"
```

### Security defaults

- Bind to loopback only by default
- Auth required for non-loopback bindings
- Security headers (CSP, X-Frame-Options, etc.)
- Path traversal prevention on all operations
- No shell execution

Recommended: run behind a reverse proxy with TLS for remote access.
