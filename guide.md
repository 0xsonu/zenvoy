# Zenvoy Quick Run Guide


> A complete Rust rewrite of [ZenNotes](https://github.com/ZenNotes/zennotes). Thanks to all contributors of the original Electron + Go codebase.

This guide is for people who just cloned the repo and want to run Zenvoy quickly.

## Choose your path

- **Desktop app** on your own machine → [Desktop](#1-run-the-desktop-app)
- **Browser access** on a home server → [Self-hosted with Docker](#2-self-hosted-with-docker)
- **Browser access** from source → [Self-hosted from source](#3-self-hosted-from-source)
- **Terminal workflows** → [CLI](#4-zen-cli)

## 1. Run the desktop app

### Requirements

- Node.js 22+
- Rust (stable)
- npm

### Steps

```bash
npm ci
npx tauri dev
```

### Build the desktop app

```bash
npx tauri build
```

The built app is in `src-tauri/target/release/bundle/`.

## 2. Self-hosted with Docker

### Requirements

- Docker
- Docker Compose

### Steps

```bash
docker compose up -d
```

Then open [http://localhost:7878](http://localhost:7878).

Important:

- Docker binds to `127.0.0.1` by default
- on first run, Zenvoy creates a bootstrap auth token at `./data/auth-token`
- the browser asks for that token once, then uses a secure session cookie

To disable auth for a trusted local setup:

```bash
ALLOW_INSECURE_NOAUTH=1 docker compose up -d
```

### Use a different vault folder

```bash
CONTENT_ROOT="$HOME/Documents/MyVault" docker compose up -d
```

### Stop

```bash
docker compose down
```

## 3. Self-hosted from source

### Requirements

- Node.js 22+
- Rust (stable)
- npm

### Steps

Build the frontend:

```bash
npm ci
npx vite build
```

Run the server:

```bash
cd src-tauri && cargo run --bin zenvoy-server
```

Then open [http://localhost:7878](http://localhost:7878).

The server embeds the built frontend from `dist/`, so you only need the one process.

### Environment variables

- `ZENVOY_VAULT_PATH` — vault directory (default: `~/ZenvoyVault`)
- `ZENVOY_BIND` — bind address (default: `127.0.0.1:7878`)
- `ZENVOY_AUTH_TOKEN` — bearer token for non-loopback access

## 4. zen CLI

Build and run:

```bash
cd src-tauri && cargo run --bin zen -- --help
```

Available commands: list, read, write, search, create, rename, move, trash, restore, archive, unarchive, folders, tasks, mcp, and more.

Install to PATH:

```bash
cd src-tauri && cargo build --bin zen --release
cp target/release/zen /usr/local/bin/
```

## 5. Choose a vault in the web version

When you first open the browser version, Zenvoy loads the vault configured via `ZENVOY_VAULT_PATH` or uses the default vault picker.

In the web version:

- you are browsing the **server's filesystem**, not the browser machine's
- by default, only configured allowed roots can be browsed
- if auth is enabled, the browser prompts for the bootstrap token first

## 6. Common problems

### "Cannot read properties of undefined (reading 'invoke')"

You're accessing the web version but the app is trying to use the Tauri bridge. Make sure you rebuilt after the latest changes:

```bash
npx vite build
```

Then restart the server.

### "The web app opens, but I can't do anything"

Make sure the server is running:

```bash
cd src-tauri && cargo run --bin zenvoy-server
```

### "I want to use iCloud Drive"

For Docker, mount the iCloud folder:

```bash
CONTENT_ROOT="$HOME/Library/Mobile Documents/com~apple~CloudDocs" docker compose up -d
```

For source runs, set the env var:

```bash
ZENVOY_VAULT_PATH="$HOME/Library/Mobile Documents/com~apple~CloudDocs" cargo run --bin zenvoy-server
```

## 7. Handy commands

| Command | Purpose |
| --- | --- |
| `npx tauri dev` | Run desktop app in dev mode |
| `npx vite build` | Build frontend |
| `cargo run --bin zenvoy-server` | Run standalone server |
| `cargo run --bin zen -- --help` | CLI help |
| `cargo test --lib` | Run Rust tests |
| `docker compose up -d` | Start Docker stack |
| `docker compose down` | Stop Docker stack |
| `docker compose logs -f` | Follow logs |
