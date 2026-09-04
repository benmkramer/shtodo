# Changelog

All notable changes to shtodo are documented in this file.

## [Unreleased]

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

[Unreleased]: https://github.com/benmkramer/shtodo/compare/v0.1.0-beta.1...HEAD
[0.1.0-beta.1]: https://github.com/benmkramer/shtodo/releases/tag/v0.1.0-beta.1
