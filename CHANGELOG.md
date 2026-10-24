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
- Due dates derived from a todo.txt `due:YYYY-MM-DD` token in the task text.
  Nothing new is stored. On add and edit, in the shell and the TUI,
  `due:today` and `due:tomorrow` are rewritten to the date in the process's
  time zone, so saved text always holds an absolute date.
- `shtodo list --due` with `today`, `tomorrow`, `overdue`, `none`, `any`, a
  date, or an inclusive `START..END` range, repeatable as OR, and
  `--sort due` (earliest first, then undated tasks).
- JSON task objects include `due`, a `"YYYY-MM-DD"` string or `null`.
- TUI rows style the priority marker and tags in place: bold priority, cyan
  `+project`, magenta `@context`.
- TUI `s` cycles manual, priority, and due order, sharing the orders of
  `list --sort priority` and `list --sort due`. `J`/`K` reorder only in manual
  order and otherwise show `Reorder needs manual order; s cycles`.
- TUI rows style the first valid due token in place: red and bold when an
  open task is overdue, yellow and bold when it is due today, blue otherwise.
  The status marker of an open overdue or due-today task takes the same color,
  so the state survives a clipped row. Done and Trash rows use blue only.
- TUI filter editor accepts `due:WHEN` tokens with every `--due` value, such
  as `due:overdue due:today`, ORed together and ANDed with tags and
  priorities. In-progress due tokens are left out of the preview, and due
  tokens are never completed by Enter.
- The TUI notices a new local date within a second (up to five while storage
  errors slow its refresh), restyles rows, and moves
  `today`, `tomorrow`, and `overdue` in the filter to it, with one redraw.
- TUI `f` opens a tag and priority filter editor with a live prefix preview,
  a hint line listing tags in the current rows with counts, and Enter
  completion of a unique tag prefix. The status line shows the accepted filter
  and sort.
- Subtasks, one level deep: `shtodo add --parent <ID>` adds one at the end of
  its parent's group, and `shtodo parent <ID> <PARENT_ID | none>` nests or
  promotes a task. Deleting a parent moves its live subtasks to trash with
  it, and restoring it (shell `restore` or TUI `u`) brings them back;
  `delete` and `restore` report `with N subtasks`.
- `shtodo list` indents a subtask under its printed parent, `--roots` and
  `--parent <ID>` list one level, and `--sort priority` and `--sort due`
  sort as a tree, keeping each group together.
- JSON task objects include `parent_id`, a number or `null`; `tasks` keeps
  each returned group together with its parent first. New error code
  `invalid_parent` for `add --parent` and `parent`.

### Changed

- TUI Esc in Normal mode clears one layer per press: the accepted search,
  then the filter.
- TUI `s` (`toggle_sort`) cycles three orders instead of toggling priority
  order, and its description is `cycle sort order`. The reorder notice now
  reads `Reorder needs manual order; s cycles`, naming the configured key, and
  an invalid filter token names `due:WHEN`. The config name is unchanged.
- The new `toggle_sort` (`s`) and `edit_filter` (`f`) defaults yield to
  existing Normal bindings on those keys; such configs load unchanged and the
  new action is unbound until given a free key.
- A list is saved as schema 2 while it has at least one subtask, including
  one in trash, and as byte-identical schema 1 otherwise. Older shtodo
  versions refuse schema 2 without writing it (`unsupported schema version 2`),
  and an older TUI already open blocks changes until the list is schema 1
  again. Promoting every subtask makes the next save schema 1.
- `add --parent` with nothing after it is now a usage error; it used to save
  the text `--parent`.
- TUI `J`/`K` move a task among its siblings: a top-level task moves with its
  subtasks, and a subtask stays in its group.
- `restore` of a subtask whose parent is in trash fails with `task_deleted`,
  whose `task_id` names the parent to restore first.

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
