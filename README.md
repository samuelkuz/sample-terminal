# Sample Terminal

Sample Terminal is a small native macOS terminal prototype written in Rust. It opens a PTY-backed shell inside an AppKit window and renders the terminal grid with Metal.

## What It Does

- Launches your login shell in a pseudoterminal.
- Renders terminal content with Metal instead of a web view or cross-platform UI toolkit.
- Handles terminal grid layout, ANSI parsing, scrollback, cursor blinking, and text selection in Rust.
- Maps common keyboard input, including arrow keys, to terminal escape sequences.

## Project Layout

- [`src/main.rs`](/Users/samkuz/Coding/sample-terminal/src/main.rs): process entry point
- [`src/app.rs`](/Users/samkuz/Coding/sample-terminal/src/app.rs): AppKit window setup, input handling, render loop
- [`src/session.rs`](/Users/samkuz/Coding/sample-terminal/src/session.rs): PTY session management and shell process I/O
- [`src/terminal_buffer.rs`](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer.rs): terminal state, ANSI parsing, scrollback, damage tracking
- [`src/renderer/cells.rs`](/Users/samkuz/Coding/sample-terminal/src/renderer/cells.rs): layout math and render geometry
- [`src/renderer/metal.rs`](/Users/samkuz/Coding/sample-terminal/src/renderer/metal.rs): Metal pipeline setup and frame submission
- [`src/renderer/atlas.rs`](/Users/samkuz/Coding/sample-terminal/src/renderer/atlas.rs): glyph atlas generation

## Requirements

- macOS
- Rust toolchain with Cargo
- A machine with Metal support

## Getting Started

Build the project:

```bash
cargo build
```

Run the app:

```bash
cargo run
```

Run the tests:

```bash
cargo test
```

## Development Notes

- This repository is currently macOS-specific because it depends on AppKit, CoreText, QuartzCore, and Metal bindings.
- The terminal session uses `$SHELL` when available and falls back to `/bin/zsh`.
- Formatting can be checked with `cargo fmt -- --check`.

## Agent Guidance

If you are working on this repository with a coding agent, see [`AGENTS.md`](/Users/samkuz/Coding/sample-terminal/AGENTS.md) for agent-focused build steps, validation notes, and editing conventions.
