# AGENTS.md - Floorpov Project Guidelines

## Project Overview

Floorpov is a Tauri 2 desktop application with a React 19 + TypeScript frontend and Rust backend.

## Build Commands

```bash
# Frontend only
bun run dev          # Start Vite dev server on port 1420
bun run build        # TypeScript compile + Vite production build
bun run preview      # Preview production build

# Full Tauri app (frontend + backend)
bun run tauri dev        # Run Tauri in development mode
bun run tauri build      # Build production Tauri app
bun run tauri build -- --debug  # Build with debug symbols
```

### Running a Single Test

**No test framework is currently configured.** To add tests:

```bash
# Install Vitest for unit tests
bun add -D vitest @testing-library/react @testing-library/jest-dom jsdom

# Add to package.json scripts:
# "test": "vitest",
# "test:run": "vitest run",
# "test:ui": "vitest --ui"

# Run a single test file
npx vitest run src/App.test.tsx

# Run tests matching a pattern
npx vitest run --grep "greet"
```

For Rust tests:

```bash
cd src-tauri
cargo test              # Run all tests
cargo test greet        # Run tests matching "greet"
cargo test --lib        # Run library tests only
```

## Linting & Type Checking

```bash
# TypeScript type checking (part of build)
bunx tsc --noEmit        # Type check without emitting
bunx tsc --noEmit --watch # Watch mode

# Rust
cd src-tauri
cargo clippy            # Lint Rust code
cargo fmt --check       # Check formatting
cargo fmt              # Auto-format
```

## Code Style Guidelines

### TypeScript/JavaScript

**Formatting:**

- Use 2 spaces for indentation
- Use single quotes for strings
- Use semicolons
- Max line length: 100 characters
- Enable format on save in your editor

**Imports:**

- Order imports: external libs → internal modules → CSS/assets
- Use path aliases if configured (check tsconfig.json paths)
- Prefer named imports: `import { useState } from "react"`
- Default imports for components: `import App from "./App"`

**Types:**

- Use explicit types for function parameters and return values
- Use `interface` for object shapes, `type` for unions/aliases
- Avoid `any`, use `unknown` when type is truly unknown
- Enable strict mode in tsconfig.json

**Naming:**

- Components: PascalCase (e.g., `App.tsx`, `GreetingCard.tsx`)
- Functions/variables: camelCase
- Constants: UPPER_SNAKE_CASE
- Files: kebab-case (except components)

**React Patterns:**

- Never use class components - always use functional components with hooks
- Use async/await syntax for async operations
- Use functional components with hooks
- Destructure props: `function App({ title }: AppProps)`
- Use early returns for conditionals
- Avoid inline object styles, use CSS modules or classes

**Tailwind CSS:**

- Use utility classes instead of custom CSS
- Keep components small and focused to avoid deeply nested utility classes
- Use `@apply` directive sparingly - only for reusable patterns in CSS
- Extract repeated utility combinations into reusable components
- Use Tailwind's responsive prefixes (`sm:`, `md:`, `lg:`) for mobile-first design
- Use arbitrary values for one-off styles (e.g., `w-[300px]`) when needed
- Prefer semantic class names that describe content, not appearance when possible

**Error Handling:**

- Use try/catch for async operations
- Provide user-friendly error messages
- Log errors appropriately (console.error for dev, proper logging in production)

### Rust

**Formatting:**

- Run `cargo fmt` before committing
- Follow standard Rust formatting conventions
- 4 spaces for indentation

**Naming:**

- Functions/variables: snake_case
- Structs/Enums: PascalCase
- Traits: PascalCase (often with -able suffix)
- Constants: SCREAMING_SNAKE_CASE

**Async:**

- Prefer tokio primitives (e.g., `tokio::sync::RwLock`, `tokio::spawn`)
- Avoid blocking operations - use `tokio::fs` not `std::fs`
- Use `tokio::task::spawn_blocking` for CPU-intensive work

**Code Organization:**

- Keep functions small and focused
- Use modules to organize code
- Place tests in `tests/` directory or inline with `#[cfg(test)]`

**Error Handling:**

- Use `Result<T, E>` for fallible operations
- Avoid `.unwrap()` in production code
- Use `?` operator for error propagation
- Provide meaningful error messages

**Tauri Specific:**

- Tauri commands use `#[tauri::command]` attribute
- Use `invoke()` from `@tauri-apps/api/core` to call Rust commands
- Keep command handlers simple, delegate to service functions

## Project Structure

```
Floorpov/
├── src/                    # React frontend
│   ├── assets/            # Static assets (images, etc.)
│   ├── App.tsx            # Main app component
│   ├── main.tsx           # Entry point
│   └── *.css              # Stylesheets
├── src-tauri/             # Rust backend
│   ├── src/
│   │   ├── lib.rs         # Library entry, Tauri commands
│   │   └── main.rs        # Binary entry point
│   ├── Cargo.toml         # Rust dependencies
│   └── tauri.conf.json    # Tauri configuration
├── package.json           # Node dependencies
├── tsconfig.json          # TypeScript config
├── vite.config.ts         # Vite configuration
└── AGENTS.md              # This file
```

## Common Patterns

### Adding a New Tauri Command

1. Define command in `src-tauri/src/lib.rs`:

```rust
#[tauri::command]
fn my_command(arg: String) -> Result<String, String> {
    Ok(format!("Processed: {}", arg))
}
```

1. Add to invoke handler:

```rust
.invoke_handler(tauri::generate_handler![greet, my_command])
```

1. Call from TypeScript:

```typescript
import { invoke } from "@tauri-apps/api/core";
const result = await invoke<string>("my_command", { arg: "value" });
```

### Adding a New Component

1. Create `src/components/MyComponent.tsx`:

```typescript
interface MyComponentProps {
  title: string;
  onClick: () => void;
}

export function MyComponent({ title, onClick }: MyComponentProps) {
  return <button onClick={onClick}>{title}</button>;
}
```

1. Import and use in parent:

```typescript
import { MyComponent } from "./components/MyComponent";
```

## Editor Setup

**VS Code recommended extensions:**

- Tauri (official)
- ESLint
- Prettier
- Rust Analyzer (for src-tauri)

**Settings to add to .vscode/settings.json:**

```json
{
  "editor.formatOnSave": true,
  "editor.defaultFormatter": "esbenp.prettier-vscode",
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

## Developer Instructions

### Comments

- Explain **why**, never what the code does
- Write comments as permanent documentation, not changelog entries
- Avoid alarmist language (e.g., "CRITICAL:", "IMPORTANT FIX:", "NOTE:")
- No organizational comments (e.g., "// Section: imports", "// Helpers")
- No section dividers (e.g., "// ====", "// ---")
- Remove comments when code is removed during refactors
- If code needs a comment to be understood, refactor instead
- Never use placeholder comments ("for now", "TODO: extract this later")
- Never use markdown formatting (**bold**, _italic_) in code comments
- Never use comments explaining removed code during refactors

### General

- No speculative dead code
- No placeholder metadata
- No AI-filler patterns

### Writing Style

This applies to all documentation, code comments, and design documents.

Use clear, simple language. Write short, impactful sentences. Use active voice. Focus on practical, actionable information.

Address the reader directly with "you" and "your". Support claims with data and examples when possible.

**Avoid these constructions:**

- Em dashes (use commas or periods)
- "Not only this, but also this"
- Metaphors and cliches
- Generalizations
- Setup language like "in conclusion"
- Unnecessary adjectives and adverbs
- Emojis, hashtags, markdown formatting in prose

**Avoid these words:** comprehensive, delve, utilize, harness, realm, tapestry, unlock, revolutionary, groundbreaking, remarkable, pivotal

## Implementation Plan: WoW Gameplay Recorder

### Project Goal

Floorpov records WoW gameplay with markers on important events (player deaths, kills, etc.). Users can capture either the full monitor or a specific window, with live preview during recording.

### Tech Stack

| Layer | Technology |
|-------|------------|
| Capture | `windows-capture` 2.0 (Windows Graphics Capture API + DXGI Desktop Duplication) |
| Recording | `windows-capture::VideoEncoder` (H.264/MP4) |
| Preview Encoding | WIC JPEG via `ImageEncoder` from `windows-capture` |
| Hotkeys | `tauri-plugin-global-shortcut` (Phase 4) |
| Combat Log | File watcher + regex parsing (mocked initially) |
| Audio | WASAPI or `windows-record` (Phase 4 - later) |

### File Structure (Planned)

```
src-tauri/
├── Cargo.toml          # Add: windows-capture, tauri-plugin-global-shortcut (Phase 4)
├── src/
│   ├── lib.rs          # Tauri commands + module exports
│   ├── capture.rs      # Preview capture (JPEG frames)
│   ├── recording.rs    # Video recording (H.264, audio in Phase 4)
│   └── combat_log.rs   # Combat log parsing (mocked initially)

src/
├── contexts/
│   ├── VideoContext.tsx      # Existing - video playback
│   ├── RecordingContext.tsx  # NEW - recording state, preview frames
│   └── MarkerContext.tsx     # NEW - events/markers state
├── components/
│   ├── VideoPlayer.tsx       # Modified - canvas preview during recording
│   ├── Timeline.tsx          # Modified - use real markers
│   ├── RecordingControls.tsx # NEW - start/stop, source selector
│   └── Settings.tsx          # NEW - capture settings
├── hooks/
│   └── usePreview.ts         # NEW - canvas rendering logic
├── types/
│   └── events.ts             # NEW - GameEvent types
└── data/
    └── mockEvents.ts         # Keep for fallback
```

### Phase 1: Capture Infrastructure

**Backend Tasks:**

1. Add dependency to `Cargo.toml`:
   - `windows-capture = "2.0.0-alpha.7"`

2. Create `src/capture.rs`:
   - `PreviewCapture` struct implementing `GraphicsCaptureApiHandler`
   - `CaptureSource` enum: `Monitor` | `Window(String)`
   - `start_capture(source: CaptureSource)` command
   - `stop_capture()` command
   - `list_windows()` command - returns available window titles
   - Emit `"preview-frame"` with JPEG bytes on each frame
   - Emit `"capture-started"` with `{width, height, source}`

3. Create `src/recording.rs`:
   - `start_recording(source, outputPath, settings)` command
   - `stop_recording()` command
   - Use `VideoEncoder` from `windows-capture` for H.264/MP4
   - Emit `"recording-started"`, `"recording-stopped"` events
   - Audio support deferred to Phase 4

4. Update `lib.rs`:
   - Export capture and recording modules
   - Register Tauri commands

**Frontend Tasks:**

1. Create `src/contexts/RecordingContext.tsx`:

   ```typescript
   interface RecordingContextType {
     isRecording: boolean;
     isPreviewing: boolean;
     previewFrameUrl: string | null;
     captureSource: CaptureSource | null;
     availableWindows: string[];
     startPreview(source: CaptureSource): Promise<void>;
     stopPreview(): void;
     startRecording(): Promise<string>; // returns output path
     stopRecording(): Promise<void>;
   }
   ```

2. Create `src/components/RecordingControls.tsx`:
   - Source dropdown: "Primary Monitor" | window list
   - Start/Stop preview button
   - Start/Stop recording button
   - Recording timer display

3. Modify `VideoPlayer.tsx`:
   - Add `<canvas ref={canvasRef}>` element
   - Show canvas when `isRecording || isPreviewing`
   - Show video element when playing back recording
   - Paint JPEG frames to canvas via `usePreview` hook

4. Create `src/hooks/usePreview.ts`:
   - Accept `previewFrameUrl`
   - Draw to canvas on each new frame
   - Handle canvas sizing

### Phase 2: Settings & Polish

1. Create `src/components/Settings.tsx`:
   - Video quality dropdown (Low/Medium/High - maps to bitrate)
   - Frame rate (30/60)
   - Audio source toggles (system audio, microphone)
   - Output folder picker
   - Combat log path picker (for later)

2. Store settings in Tauri's app data via `tauri-plugin-store`

3. UI refinements:
   - Recording indicator in title bar
   - Capture border overlay (optional - highlight what's being captured)
   - Error handling for capture failures

### Phase 3: Markers System

**Backend Tasks:**

1. Create `src/combat_log.rs`:
   - `CombatEvent` struct: `{ timestamp, event_type, source?, target? }`
   - Mock implementation that generates random death/kill events at intervals
   - `start_combat_watch()` - begins mock event emission
   - `stop_combat_watch()`
   - Emit `"combat-event"` with `CombatEvent` payload
   - Later: real implementation parsing WoWCombatLog.txt

2. Add global hotkey:
   - `register_marker_hotkey()` command
   - On hotkey press, emit `"manual-marker"` with current timestamp

**Frontend Tasks:**

1. Create `src/contexts/MarkerContext.tsx`:

   ```typescript
   interface MarkerContextType {
     events: GameEvent[];
     addEvent(event: GameEvent): void;
     clearEvents(): void;
   }
   ```

2. Update `Timeline.tsx`:
   - Consume events from `MarkerContext` instead of `mockEvents`
   - Show live event markers during recording

3. Update `GameEvents.tsx`:
   - Same - use real events from context

### Event Flow

```
[Start Recording]
       │
       ▼
┌──────────────────┐
│  PreviewCapture  │
│  - JPEG preview  │
│  - VideoEncoder  │
└────────┬─────────┘
         │
         │ preview-frame event
         ▼
    [Frontend]
    Canvas draw
         │
         │ (parallel)
         ▼
┌──────────────────┐
│ Combat Log Watch │ (mocked)
│  - UNIT_DIED     │
│  - PARTY_KILL    │
└────────┬─────────┘
         │ combat-event
         ▼
    [MarkerContext]
    Timeline markers update
```

### Default Settings

| Setting | Value |
|---------|-------|
| Video codec | H.264 |
| Frame rate | 30 fps |
| Bitrate | 8 Mbps (High) |
| Container | MP4 |
| Audio | Phase 3 (deferred) |
| Preview FPS | 30 (same as recording) |
| Preview quality | JPEG 85% quality |
| Output folder | `%USERPROFILE%/Videos/Floorpov/` |

### Implementation Order

1. Backend: Capture module - `capture.rs` with preview-only functionality
2. Frontend: RecordingContext + usePreview - Show preview in canvas
3. Backend: Recording module - Add actual MP4 recording (video only)
4. Frontend: RecordingControls - UI for start/stop
5. Frontend: Settings - Quality/source configuration
6. Backend: Combat log mock - Emit fake events
7. Frontend: MarkerContext - Store and display events
8. Backend: Hotkeys - Manual marker via keyboard (Phase 4)
9. Backend: Audio - WASAPI or windows-record integration (Phase 4)

### Current Decisions

- Default capture source: Primary Monitor (user can select window later)
- Output folder: Default to `%USERPROFILE%/Videos/Floorpov/` (user can change later)
- Window picker: Dropdown list of available windows (simpler for phase 1)
- Combat log path: Mocked for now, real implementation later
