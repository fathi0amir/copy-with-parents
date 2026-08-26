# Copy with Parent Levels

A cross-platform desktop utility for copying files while preserving a configurable number of parent-directory levels in the destination.

## The Problem

You have files scattered across a deep directory structure and want to copy them to a new location — but you need to keep some of the original directory hierarchy intact. For example, copying `project/src/components/Button.tsx` with 2 parent levels preserves `src/components/Button.tsx` under the destination.

## Features

- **Configurable parent levels** — choose how many directory levels to preserve (0 = flat copy, 1 = immediate parent, 2 = two levels up, …)
- **Drag & drop** — drop files or folders directly onto the window (folders are recursively scanned)
- **Native file dialogs** — pick files and destination using your OS's native dialogs
- **Tree preview** — see the full destination directory structure before copying
- **Conflict handling** — choose what happens when a file already exists:
  - **Skip** — leave existing files untouched (default)
  - **Overwrite** — replace existing files
  - **Auto-rename** — append `(1)`, `(2)`, etc. to avoid conflicts
- **Zoom indicator** — live zoom percentage in the status bar (Ctrl+= / Ctrl+-)
- **Single executable** — no runtime dependencies, no installer needed

## Building

Requires [Rust](https://www.rust-lang.org/tools/install) (1.70+).

```bash
# Debug build
cargo build

# Release build (optimized, no console window on Windows)
cargo build --release
```

The binary is at `target/release/copy-with-parents.exe` (Windows) or `target/release/copy-with-parents` (macOS/Linux).

## How It Works

For a file at `/a/b/c/d/file.txt` with **parent levels = 2**:

1. The file's parent directory is `/a/b/c/`
2. Walking 2 levels up lands at `/a/`
3. The relative path from `/a/` to the file is `b/c/d/file.txt`
4. The file is copied to `<destination>/b/c/d/file.txt`

## Usage

1. **Select files** — click "Select files…" or drag & drop onto the window
2. **Set parent levels** — adjust the spinner (0–20)
3. **Choose destination** — click "Select…" and pick a folder
4. **Check the preview** — the tree view shows exactly what will be created
5. **Copy** — click "Copy files"

## Dependencies

| Crate | Purpose |
|-------|---------|
| [eframe](https://crates.io/crates/eframe) | Cross-platform native GUI framework (egui backend) |
| [rfd](https://crates.io/crates/rfd) | Native file dialogs |

Both are pulled in via Cargo — no system packages needed.

## License

MIT
