# AGENTS.md

This file is for coding agents working in this repository. It complements project docs by collecting the implementation details, validation steps, and repo conventions that are useful during automated edits.

## Purpose

`AGENTS.md` complements human-facing docs by containing the extra, sometimes detailed context coding agents need: build steps, tests, and conventions that would clutter a README or are not especially relevant to human contributors.

We keep it separate to:

- Give agents a clear, predictable place for instructions.
- Keep READMEs concise and focused on human contributors.
- Provide precise, agent-focused guidance that complements existing README and docs.

## Project Summary

- This repo is a native macOS terminal prototype written in Rust.
- The app uses `objc2` bindings to drive AppKit and Metal directly. It is not a cross-platform TUI crate.
- The executable entry point is [`src/main.rs`](/Users/samkuz/Coding/sample-terminal/src/main.rs), which delegates to [`src/app.rs`](/Users/samkuz/Coding/sample-terminal/src/app.rs).
- Terminal I/O is backed by a PTY session in [`src/session.rs`](/Users/samkuz/Coding/sample-terminal/src/session.rs).
- ANSI parsing, screen state, scrollback, and damage tracking live in [`src/terminal_buffer.rs`](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer.rs).
- Layout and render data generation live in [`src/renderer/cells.rs`](/Users/samkuz/Coding/sample-terminal/src/renderer/cells.rs).
- Metal setup and frame submission live in [`src/renderer/metal.rs`](/Users/samkuz/Coding/sample-terminal/src/renderer/metal.rs).
- Glyph atlas generation lives in [`src/renderer/atlas.rs`](/Users/samkuz/Coding/sample-terminal/src/renderer/atlas.rs).

## Environment Assumptions

- Assume macOS. This code depends on AppKit, CoreText, QuartzCore, and Metal.
- Expect graphical behavior to require launching the app, not just unit tests.
- The spawned shell defaults to `$SHELL` and falls back to `/bin/zsh`.
- Keep changes compatible with the current crate edition in [`Cargo.toml`](/Users/samkuz/Coding/sample-terminal/Cargo.toml): Rust 2024.

## Common Commands

- Build: `cargo build`
- Run the app: `cargo run`
- Run tests: `cargo test`
- Check formatting: `cargo fmt -- --check`
- Apply standard formatting: `cargo fmt`

## Current Validation Notes

- `cargo test` currently passes.
- `cargo fmt -- --check` currently fails on the existing worktree because the repo is not fully rustfmt-clean. Do not assume a formatting failure means your change is wrong; inspect whether the diff is pre-existing.

## Editing Conventions

- Preserve the separation of responsibilities:
  - App lifecycle, timers, input translation, and selection state in `app.rs`.
  - PTY process management in `session.rs`.
  - Terminal parsing and screen mutation in `terminal_buffer.rs`.
  - Geometry and render cache logic in `renderer/cells.rs`.
  - GPU resource setup and draw submission in `renderer/metal.rs`.
- Keep terminal behavior changes covered by focused unit tests in the same module when practical. Existing tests in `terminal_buffer.rs`, `renderer/cells.rs`, `renderer/atlas.rs`, and `app.rs` show the expected style.
- Prefer small, local changes. This codebase already has a clear module split; avoid moving logic across layers unless the task requires it.
- Be careful with resize behavior. Grid size, PTY window size, render caches, and terminal buffer dimensions are linked.
- Be careful with input handling. `app.rs` translates AppKit events into terminal byte sequences; regressions here can silently break shell interaction.
- Be careful with damage tracking. Rendering tries to avoid unnecessary rebuilds, so state changes should mark the appropriate rows or global flags.

## Practical Guidance For Agents

- Read the relevant module before editing; similar behavior is often already implemented nearby.
- If you change ANSI parsing or buffer mutation logic, run `cargo test` before finishing.
- If you change layout or rendering code, prefer running both `cargo test` and `cargo run` on macOS if the environment allows it.
- If you touch formatting, decide whether to format only the changed files or the whole repo. A full `cargo fmt` may rewrite many unrelated lines.
- Do not claim cross-platform support unless you actually add it. The current implementation is macOS-specific.

## Suggested Workflow

1. Identify the owning module for the requested change.
2. Read nearby tests and existing helper functions before editing.
3. Make the smallest change that fits the current architecture.
4. Run `cargo test` at minimum.
5. Run formatting checks when relevant, but report clearly if failures are pre-existing.
