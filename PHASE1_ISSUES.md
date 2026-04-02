# Phase 1 Issue Backlog

This document breaks Phase 1 into GitHub-style issues focused on moving this repository from a terminal prototype toward a more complete terminal core.

## Status Legend

- `Done`: implemented in the current worktree
- `Partial`: started, but acceptance criteria are not fully complete yet
- `Pending`: not implemented yet

Scope for Phase 1:

- strengthen terminal emulation correctness
- improve input compatibility for common shell/TUI workflows
- add a minimal terminal response path
- improve test coverage around terminal semantics
- add basic terminal identity and title plumbing

Non-goals for Phase 1:

- Unicode grapheme-cluster redesign
- font fallback and shaping
- major renderer refactors
- tabs, splits, settings UI, search, clipboard UX polish

## Issue 1: Add explicit terminal mode/state storage

**Status**

`Done`

**Summary**

Introduce explicit terminal mode fields so parser and ops logic can stop encoding behavior implicitly in ad hoc state.

**Why**

Upcoming work for cursor visibility, bracketed paste, origin mode, and application cursor keys needs a stable home in the terminal model.

**Primary files**

- [src/terminal_buffer/types.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/types.rs)
- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs)
- [src/app_state.rs](/Users/samkuz/Coding/sample-terminal/src/app_state.rs)

**Proposed changes**

- Add a `TerminalModes` struct or equivalent fields for:
  - `cursor_visible`
  - `bracketed_paste`
  - `application_cursor`
  - `origin_mode`
- Decide whether modes live on `TerminalBuffer` or per-screen on `ScreenBuffer`.
- Add basic accessors for input and render paths.

**Acceptance criteria**

- Terminal modes are represented explicitly in the terminal model.
- Cursor rendering can distinguish blink visibility from terminal-controlled visibility.
- Input translation can query terminal input modes without reaching into parser internals.

**Estimated file diffs**

- [src/terminal_buffer/types.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/types.rs): moderate
- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs): small to moderate
- [src/app_state.rs](/Users/samkuz/Coding/sample-terminal/src/app_state.rs): small

## Issue 2: Implement DEC cursor visibility mode (`CSI ? 25 h/l`)

**Status**

`Done`

**Summary**

Support terminal-controlled cursor show/hide mode.

**Why**

Shells and TUIs use cursor visibility control constantly. Your current cursor visibility is blink-driven only.

**Primary files**

- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs)
- [src/terminal_buffer/ops.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/ops.rs)
- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs)
- [src/app_state.rs](/Users/samkuz/Coding/sample-terminal/src/app_state.rs)

**Proposed changes**

- Extend DEC private mode parsing for `?25`.
- Persist cursor visible/hidden state in terminal modes.
- Combine terminal cursor state with blink state during `render_snapshot()`.

**Acceptance criteria**

- `\x1b[?25l` hides the cursor in rendered output.
- `\x1b[?25h` shows the cursor again.
- Hidden cursor stays hidden regardless of blink timer state.
- Unit tests cover hide/show transitions.

**Estimated file diffs**

- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs): small
- [src/terminal_buffer/ops.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/ops.rs): small
- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs): small
- [src/app_state.rs](/Users/samkuz/Coding/sample-terminal/src/app_state.rs): small

## Issue 3: Implement bracketed paste mode (`CSI ? 2004 h/l`)

**Status**

`Done`

**Summary**

Support bracketed paste mode in the terminal model and input path.

**Why**

Modern shells and full-screen apps rely on bracketed paste to treat pasted text safely.

**Primary files**

- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs)
- [src/terminal_buffer/ops.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/ops.rs)
- [src/input.rs](/Users/samkuz/Coding/sample-terminal/src/input.rs)
- [src/app.rs](/Users/samkuz/Coding/sample-terminal/src/app.rs)
- [src/app_state.rs](/Users/samkuz/Coding/sample-terminal/src/app_state.rs)

**Proposed changes**

- Extend DEC private mode parsing for `?2004`.
- Store paste mode in terminal state.
- Add a paste path that wraps payloads in:
  - `\x1b[200~`
  - pasted bytes
  - `\x1b[201~`
- Keep plain paste behavior when bracketed paste is disabled.

**Acceptance criteria**

- Terminal can toggle bracketed paste mode on and off.
- Paste input is wrapped only when the mode is enabled.
- Existing key input behavior is unchanged for non-paste events.
- Tests cover mode toggling and encoded paste output.

**Estimated file diffs**

- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs): small
- [src/terminal_buffer/ops.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/ops.rs): small
- [src/input.rs](/Users/samkuz/Coding/sample-terminal/src/input.rs): moderate
- [src/app.rs](/Users/samkuz/Coding/sample-terminal/src/app.rs): moderate
- [src/app_state.rs](/Users/samkuz/Coding/sample-terminal/src/app_state.rs): small

## Issue 4: Expand special-key input translation

**Status**

`Partial`

**Summary**

Add support for more non-text keyboard input commonly expected by shells, pagers, and editors.

**Why**

The current input path only handles arrows and plain text. That will break normal usage quickly.

**Primary files**

- [src/input.rs](/Users/samkuz/Coding/sample-terminal/src/input.rs)
- [src/app.rs](/Users/samkuz/Coding/sample-terminal/src/app.rs)

**Proposed changes**

- Add mappings for:
  - Home
  - End
  - Page Up
  - Page Down
  - Delete / forward delete
  - F1-F12
- Capture modifier flags from `NSEvent`.
- Decide whether input translation should move from raw text-only API to a typed key event helper.

**Acceptance criteria**

- Common navigation keys generate terminal sequences instead of being ignored.
- Function keys produce expected escape sequences.
- Modifier state is available to the translator even if only partially used in Phase 1.
- Tests cover the new key translations.

**Current state**

- Implemented:
  - Home
  - End
  - Page Up
  - Page Down
  - Delete / forward delete
  - F1-F12
  - application-cursor-mode-aware arrows, Home, and End
  - modifier capture from `NSEvent`
- Still pending for full completion:
  - modifier-aware escape sequences for navigation/function keys
  - broader input protocol coverage beyond the current fixed mappings

**Estimated file diffs**

- [src/input.rs](/Users/samkuz/Coding/sample-terminal/src/input.rs): moderate to large
- [src/app.rs](/Users/samkuz/Coding/sample-terminal/src/app.rs): small to moderate

## Issue 5: Implement origin mode (`CSI ? 6 h/l`) and tighten scroll-region semantics

**Status**

`Partial`

**Summary**

Add DEC origin mode and make cursor addressing honor the current scroll region.

**Why**

This is a common correctness gap that affects full-screen applications and complex redraw behavior.

**Primary files**

- [src/terminal_buffer/types.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/types.rs)
- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs)
- [src/terminal_buffer/ops.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/ops.rs)

**Proposed changes**

- Store origin mode in terminal state.
- Update cursor positioning helpers to interpret row addressing relative to the scroll region when origin mode is enabled.
- Re-check `DECSTBM` behavior and cursor reset semantics.

**Acceptance criteria**

- `CSI ? 6 h` enables region-relative addressing.
- `CSI ? 6 l` restores absolute addressing.
- `CUP`, `VPA`, and related movement behave correctly with a non-default scroll region.
- Tests cover cursor placement at top/bottom of scroll region with origin mode both enabled and disabled.

**Current state**

- Implemented:
  - terminal mode storage for `origin_mode`
  - DEC private mode toggling for `?6`
- Still pending:
  - actual origin-mode-aware cursor addressing semantics
  - scroll-region-relative cursor behavior
  - acceptance tests for movement semantics

**Estimated file diffs**

- [src/terminal_buffer/types.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/types.rs): small
- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs): small
- [src/terminal_buffer/ops.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/ops.rs): moderate

## Issue 6: Add explicit scroll commands (`CSI S` / `CSI T`)

**Status**

`Pending`

**Summary**

Implement explicit scroll up and scroll down control sequences.

**Why**

You already have internal scroll operations. These sequences expose them directly and improve compatibility with terminal-aware apps.

**Primary files**

- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs)
- [src/terminal_buffer/ops.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs)

**Proposed changes**

- Dispatch `CSI S` to scroll up.
- Dispatch `CSI T` to scroll down.
- Reuse existing region-aware scrolling operations.

**Acceptance criteria**

- `CSI S` scrolls the active region upward.
- `CSI T` scrolls the active region downward.
- Full-screen primary-screen scrolling still routes lines into scrollback where appropriate.
- Tests cover both default and custom scroll regions.

**Estimated file diffs**

- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs): small
- [src/terminal_buffer/ops.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/ops.rs): small

## Issue 7: Parse and apply OSC window title updates (`OSC 0` / `OSC 2`)

**Status**

`Pending`

**Summary**

Parse OSC title-setting sequences and reflect them in the application window title.

**Why**

This is a small but visible milestone that proves the terminal can carry non-grid state from PTY output into the app UI.

**Primary files**

- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs)
- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs)
- [src/app_state.rs](/Users/samkuz/Coding/sample-terminal/src/app_state.rs)
- [src/app.rs](/Users/samkuz/Coding/sample-terminal/src/app.rs)

**Proposed changes**

- Replace ignore-only OSC handling with payload accumulation.
- Parse at least commands `0` and `2`.
- Store terminal title state.
- Update the app window title when title state changes.

**Acceptance criteria**

- `OSC 0 ; title ST` updates the window title.
- `OSC 2 ; title ST` updates the window title.
- Unrecognized OSC sequences are ignored without corrupting parser state.
- Tests cover both BEL and `ESC \` OSC termination.

**Estimated file diffs**

- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs): moderate
- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs): small to moderate
- [src/app_state.rs](/Users/samkuz/Coding/sample-terminal/src/app_state.rs): small
- [src/app.rs](/Users/samkuz/Coding/sample-terminal/src/app.rs): small to moderate

## Issue 8: Add a terminal-generated output/response queue

**Status**

`Pending`

**Summary**

Allow the terminal core to emit bytes back to the PTY in response to control queries.

**Why**

The current architecture is receive-only plus direct user input. Some terminal behaviors require automated responses.

**Primary files**

- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs)
- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs)
- [src/app_state.rs](/Users/samkuz/Coding/sample-terminal/src/app_state.rs)
- [src/session.rs](/Users/samkuz/Coding/sample-terminal/src/session.rs)

**Proposed changes**

- Add a pending-output queue to `TerminalBuffer`.
- Add `drain_pending_output()` or equivalent.
- Let parser/ops push generated terminal responses into the queue.
- Flush pending output after PTY reads or render-cycle polling.

**Acceptance criteria**

- Terminal core can enqueue bytes for transmission to the PTY.
- App state drains and writes queued responses without blocking normal input.
- The output queue is testable without AppKit.

**Estimated file diffs**

- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs): moderate
- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs): small to moderate
- [src/app_state.rs](/Users/samkuz/Coding/sample-terminal/src/app_state.rs): small to moderate
- [src/session.rs](/Users/samkuz/Coding/sample-terminal/src/session.rs): small

## Issue 9: Implement a minimal status/device response path

**Status**

`Pending`

**Summary**

Use the response queue to answer a minimal set of terminal queries.

**Why**

A response path is less useful if nothing uses it. Start with a small, defensible subset.

**Primary files**

- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs)
- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs)

**Proposed changes**

- Support a minimal `DSR` response set such as cursor position reporting if you are comfortable wiring it correctly.
- Keep the surface intentionally small in Phase 1.
- Document unsupported queries by ignoring them safely.

**Acceptance criteria**

- At least one terminal query results in a correct generated response.
- Unsupported queries do not leave the parser in a bad state.
- Tests verify exact bytes emitted.

**Estimated file diffs**

- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs): small to moderate
- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs): small

## Issue 10: Set explicit terminal identity in spawned session

**Status**

`Pending`

**Summary**

Set an explicit `TERM` value when spawning the shell.

**Why**

Right now the terminal process identity is implicit. A stable terminal identity is important for capability negotiation.

**Primary files**

- [src/session.rs](/Users/samkuz/Coding/sample-terminal/src/session.rs)

**Proposed changes**

- Change shell exec setup so `TERM` is set explicitly.
- Prefer a compatibility-first value for now rather than inventing a custom entry before terminfo exists.
- Keep the environment setup contained to session spawn logic.

**Acceptance criteria**

- Spawned shell sees a deterministic `TERM` value.
- Existing shell startup behavior continues to work.
- Session spawn still respects `$SHELL` for executable selection.

**Estimated file diffs**

- [src/session.rs](/Users/samkuz/Coding/sample-terminal/src/session.rs): moderate

## Issue 11: Reorganize terminal tests into a clearer emulator test corpus

**Status**

`Partial`

**Summary**

Restructure and expand terminal tests so protocol behavior is easier to extend safely.

**Why**

Phase 1 adds protocol surface quickly. Without organized tests, regressions will accumulate.

**Primary files**

- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs)
- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs)
- [src/terminal_buffer/ops.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/ops.rs)
- optionally a new [src/terminal_buffer/tests.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/tests.rs)

**Proposed changes**

- Group tests by behavior instead of only by whichever file currently contains them.
- Add helpers for:
  - creating a buffer
  - feeding byte transcripts
  - asserting visible lines
  - asserting cursor state
  - asserting pending terminal output
- Add dedicated tests for:
  - cursor visibility
  - bracketed paste mode
  - origin mode
  - explicit scroll commands
  - OSC title parsing
  - response queue behavior

**Acceptance criteria**

- New Phase 1 behaviors are covered by tests.
- Test helpers reduce repeated boilerplate.
- `cargo test` remains the primary validation command for terminal-core changes.

**Current state**

- Implemented:
  - tests for terminal mode defaults
  - tests for cursor visibility mode
  - tests for bracketed paste mode
  - tests for application cursor/origin mode toggles
  - tests for expanded key translation and paste encoding
- Still pending:
  - broader reorganization into a clearer emulator test corpus
  - shared transcript-style helpers
  - dedicated response/output queue tests

**Estimated file diffs**

- [src/terminal_buffer/mod.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/mod.rs): moderate
- [src/terminal_buffer/parser.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/parser.rs): small to moderate
- [src/terminal_buffer/ops.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/ops.rs): small to moderate
- [src/terminal_buffer/tests.rs](/Users/samkuz/Coding/sample-terminal/src/terminal_buffer/tests.rs): new file, moderate

## Issue 12: Add minimal Phase 1 validation matrix

**Status**

`Pending`

**Summary**

Document and validate a small set of real-world interactive behaviors after the protocol changes land.

**Why**

Unit tests are necessary but not sufficient. You also need a small manual validation list tied to this repo’s current architecture.

**Primary files**

- [AGENTS.md](/Users/samkuz/Coding/sample-terminal/AGENTS.md)
- optionally [README.md](/Users/samkuz/Coding/sample-terminal/README.md)
- optionally a new validation note in `docs/` if you introduce a docs folder later

**Proposed changes**

- Add a manual verification checklist for:
  - shell prompt redraw
  - title updates
  - bracketed paste in shell
  - `less`
  - `vim`
  - alternate screen transitions
  - cursor hide/show behavior
- Keep it short and runnable by a single developer on macOS.

**Acceptance criteria**

- Manual validation steps are documented.
- Terminal-core changes have both automated and manual validation guidance.
- The checklist is realistic for the current app.

**Estimated file diffs**

- [AGENTS.md](/Users/samkuz/Coding/sample-terminal/AGENTS.md): small
- [README.md](/Users/samkuz/Coding/sample-terminal/README.md): optional, small

## Suggested execution order

Recommended build order for the actual work:

1. Issue 1: terminal mode/state storage
2. Issue 2: cursor visibility
3. Issue 3: bracketed paste mode
4. Issue 4: expanded special-key translation
5. Issue 5: origin mode and scroll-region semantics
6. Issue 6: explicit scroll commands
7. Issue 8: terminal-generated output queue
8. Issue 9: minimal response path
9. Issue 7: OSC title parsing and app title plumbing
10. Issue 10: explicit `TERM`
11. Issue 11: test corpus cleanup and expansion
12. Issue 12: manual validation matrix

## Suggested milestones

### Milestone A: Input and terminal mode correctness

Includes:

- Issue 1
- Issue 2
- Issue 3
- Issue 4

**Outcome**

The terminal starts behaving more like a serious shell host for interactive input.

### Milestone B: Screen semantics correctness

Includes:

- Issue 5
- Issue 6
- Issue 11

**Outcome**

Cursor movement, scroll regions, and alternate-screen-adjacent behavior become more predictable and testable.

### Milestone C: Terminal/application feedback loop

Includes:

- Issue 7
- Issue 8
- Issue 9
- Issue 10
- Issue 12

**Outcome**

The terminal can carry state and responses in both directions, and the app begins reflecting terminal metadata.
