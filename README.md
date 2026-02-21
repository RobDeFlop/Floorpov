# Floorpov

A desktop app for recording WoW gameplay with markers on important events. Supports full monitor or specific window capture with live preview.

## Features

- Monitor or window capture via Windows Graphics Capture API
- Live preview during recording
- Combat log markers (player deaths, kills)
- Manual markers via hotkey
- H.264/MP4 output

## Requirements

- Windows 10/11
- [Rust](https://rustup.rs/)
- [Node.js](https://nodejs.org/) 18+
- [Bun](https://bun.sh/)

## Setup

```bash
bun install
bun run tauri dev
```

## Development Commands

```bash
# Frontend only
bun run dev
bun run build

# Full Tauri app (frontend + backend)
bun run tauri dev
bun run tauri build

# Type checking
bunx tsc --noEmit

# Rust lint/format/check
cd src-tauri
cargo check
cargo clippy
cargo fmt --check
cargo test
```

## Tech Stack

| Layer | Technology |
|-------|------------|
| Frontend | React 19, TypeScript, Tailwind CSS, Vite |
| Backend | Tauri 2, Rust |
| Capture | Windows Graphics Capture API |
| Recording | H.264/MP4 |
| Hotkeys | tauri-plugin-global-shortcut |

## License

MIT
