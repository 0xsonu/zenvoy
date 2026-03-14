# Contributing to Zenvoy

Thanks for your interest in improving Zenvoy — a keyboard-first, Markdown-first
notes app built on Tauri, Rust, React, TypeScript, and CodeMirror 6.

## Before you start

- For **anything non-trivial**, open an issue first so we can agree on scope
  and approach before you spend time building.
- For **small fixes** (typos, obvious bugs, dependency bumps), open a PR
  directly.
- For **security issues**, do not open a public issue — follow
  [SECURITY.md](./SECURITY.md).

## Getting set up

```bash
git clone <repo-url>
cd zenvoy
npm ci
npx tauri dev
```

Useful commands:

- `npx tauri dev` — run the desktop app with hot reload
- `cd src-tauri && cargo test --lib` — run the Rust test suite
- `npx vite build` — produce a production frontend build
- `npx tauri build` — build the full desktop app

## Working on a change

1. Fork the repo and create a feature branch from `main`.
2. Keep commits focused. A clear commit message beats a long PR description.
3. Add or update tests when you change behavior.
4. Make sure `cargo test --lib` and `npx vite build` pass locally.
5. Open a pull request against `main`.

## Pull request requirements

- **Pull request required** — no direct pushes to main
- **Green CI** — Rust tests and frontend build must pass
- **All review comments resolved** before merging

## Style and scope

- Match the style of the surrounding code — don't introduce new patterns
  mid-file.
- Keep the scope of each PR tight. Refactors, feature work, and formatting
  changes belong in separate PRs.
- Zenvoy is keyboard-first. Every new user-facing feature should ship with a
  keybinding or leader flow.
- Rust code follows standard `rustfmt` and `clippy` conventions.
- TypeScript/React code follows the existing patterns in `src/app/`.

## Background

This project is a complete rewrite of the original [ZenNotes](https://github.com/ZenNotes/zennotes) Electron + Go codebase in Rust. Thanks to all contributors of the original project.

## Code of conduct

Be kind, assume good faith, focus on the work.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](./LICENSE).
