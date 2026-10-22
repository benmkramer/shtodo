# Changelog

All notable changes to shtodo are documented in this file.

## [Unreleased]

### Added

- Opt-in `--json` output for listing and all shell task mutations, with a
  versioned schema, explicit scope, saved task data, and mutation change status.
- Structured JSON errors with stable codes, including lock contention and
  visible changes whose durability is unconfirmed.
- Priority and tags derived from todo.txt tokens in the task text: a leading
  `(A) ` to `(Z) ` and `+project`/`@context` words. Nothing new is stored, so
  existing lists and older binaries keep working.
- `shtodo list` filters `--open`/`--done`, repeatable `--tag` and
  `--priority` (letters, ranges such as `A-C`, or `none`), and `--search`,
  plus `--sort priority`. Invalid filters are usage errors detected before
  storage is read.
- JSON task objects include `priority` and `tags`, and JSON listing honors
  every filter and `--sort priority`.
- TUI rows style the priority marker and tags in place: bold priority, cyan
  `+project`, magenta `@context`.
- TUI `s` toggles priority order, sharing the order of `list --sort priority`.
  `J`/`K` reorder only in manual order and explain how to switch.
- TUI `f` opens a tag and priority filter editor with a live prefix preview,
  a hint line listing tags in the current rows with counts, and Enter
  completion of a unique tag prefix. The status line shows the accepted filter
  and sort.

### Changed

- TUI Esc in Normal mode clears one layer per press: the accepted search,
  then the filter.
- The new `toggle_sort` (`s`) and `edit_filter` (`f`) defaults yield to
  existing Normal bindings on those keys; such configs load unchanged and the
  new action is unbound until given a free key.

## [0.1.0-beta.4] - 2026-10-05

### Added

- Concurrent shell commands and TUIs for the same list, with short writer
  transactions, one-second refresh, and preserved drafts on conflicts or errors.
- Bounded lock waits and synchronization recovery that does not repeat a
  change after an uncertain save.

### Changed

- Refresh benchmark tables and charts with measurements from the concurrent
  CLI/TUI implementation, including its idle cost and interaction latency.

### Fixed

- Recreate absent lock files for writes and synchronization recovery while
  still requiring initialized task data to exist.
- Concurrent-writer regressions now verify bounded lock timeouts leave no
  task or consumed ID behind, and retries preserve every successful addition.

## [0.1.0-beta.3] - 2026-10-05

### Added

- Transient TUI text search with `/` and All/Open/Done tabs with Tab and
  Shift-Tab, with configurable controls and matching counts. Search composes
  with status views; navigation and task actions preserve identity, and filtered reordering leaves
  hidden rows in place.
- A labeled TUI trash view with stable task IDs, completion states, newest-first
  deletion ordering, and selective restoration. Configurable Trash controls
  appear in the footer and contextual keyboard help.
- Shell `done`, `reopen`, `edit`, and `restore` commands for one scope-local
  task ID, with no-write success for already-matching states, unchanged text,
  and already-live restoration.
- Optional `add --print-id` output for scripts, including single-task stdin.
- Homebrew installation through `benmkramer/tap/shtodo`, with automatic formula
  updates from the release workflow.
- An installable agent skill for scoped task management, updated for the full
  shell task lifecycle and script-friendly task ID output.
- A reproducible CLI benchmark harness and documented comparison results.

### Fixed

- Preserve existing explicit Normal-mode bindings that use `/`, Tab,
  Shift-Tab, or `Esc` when loading the new search controls. Claimed search defaults become unbound
  and appear in Help instead of rejecting a previously valid configuration.
- Preserve the active live search/view and valid task selection when returning
  from trash, including selective restoration of tasks hidden by that view.
- Existing Normal `t` bindings continue to load when upgrading to the trash
  feature. Implicit `open_trash` falls back to `Ctrl-t`; if both keys are taken,
  it stays unbound and `shtodo doctor` explains how to configure an opening key.
  Explicit `open_trash` conflicts still fail validation.

## [0.1.0-beta.2] - 2026-09-05

### Added

- Plain-text `list` commands with scope-local task IDs and open/done states.
- Recoverable, idempotent task deletion by ID from global or exact-directory scopes.

## [0.1.0-beta.1] - 2026-09-02

### Added

- A keyboard-first terminal todo list with global and exact-directory scopes.
- Persistent tasks, completion, reordering, soft deletion, and restoration.
- Normal, Insert, and Help modes with Vim-style navigation and editing.
- Command-line task capture with `shtodo add` and `shtodo --local add`.
- A centered poop-framed celebration when the final open task is completed.
- User-configured keybindings from `~/.shtodo/config.toml`, reflected in input, footer hints, empty-state guidance, and keyboard help.
- `shtodo doctor` for validating keybinding syntax, reserved keys, and conflicts without opening task storage or the terminal UI.

[Unreleased]: https://github.com/benmkramer/shtodo/compare/v0.1.0-beta.4...HEAD
[0.1.0-beta.4]: https://github.com/benmkramer/shtodo/compare/v0.1.0-beta.3...v0.1.0-beta.4
[0.1.0-beta.3]: https://github.com/benmkramer/shtodo/compare/v0.1.0-beta.2...v0.1.0-beta.3
[0.1.0-beta.2]: https://github.com/benmkramer/shtodo/compare/v0.1.0-beta.1...v0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/benmkramer/shtodo/releases/tag/v0.1.0-beta.1
