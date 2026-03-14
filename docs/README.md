# Zenvoy Docs

This directory documents Zenvoy: a Tauri desktop app and self-hosted web app backed by a Rust (Axum) server, sharing one frontend and one backend codebase.

## Start here

If you are new to Zenvoy:

1. Read the [Quick Run Guide](../guide.md) to get started
2. Read the [README](../README.md) for the full feature overview
3. Read the architecture docs below for how the system is built

## Architecture

- [Project Architecture](./architecture.md) — how the Tauri app, Axum server, CLI, and MCP server fit together
- [Web Architecture](./web-architecture.md) — how the browser-based self-hosted mode works

## Security

See [SECURITY.md](../SECURITY.md) for the security model and vulnerability reporting.

## Acknowledgments

This project is a complete rewrite of the original [ZenNotes](https://github.com/ZenNotes/zennotes) Electron + Go codebase in Rust (Tauri + Axum). Thanks to all the contributors of the original project.
