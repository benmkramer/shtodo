# shtodo Version One Design Specification

## Summary

shtodo version one is a local terminal todo application built with Rust, Ratatui, and Crossterm. It provides one focused, persistent task list with modal keyboard interaction, a global scope, and an exact-current-directory local scope.

The first release must be useful for daily task capture and maintenance, not merely demonstrate terminal rendering. It therefore includes creation, editing, completion, soft deletion, persistent undo, navigation, manual ordering, contextual help, durable local storage, and reliable terminal restoration.

## Goals

- Launch quickly into a global or project-specific list.
- Make every task operation available from the keyboard.
- Preserve committed changes across restarts.
- Provide clear modal state and discoverable bindings.
- Keep task behavior independent from terminal input and rendering.
- Restore the terminal after normal exit, propagated errors, and panic unwinding.
- Produce a native executable with a documented source installation path.
- Establish an internal keybinding model that can support user configuration later.

## Non-goals

- Accounts, synchronization, or network access.
- Recurring tasks, reminders, dates, priorities, tags, or multiple named lists.
- A trash view. Deleted records remain available for a later trash view.
- A sidebar. The focused layout leaves room for one without implementing sidebar abstractions.
- User-editable keybindings, themes, or configuration files.
- Runtime plugins or extensions.
- Mouse interaction.
- Git-root discovery for local scope.
- Automated release binaries, installers, or package-manager distribution.
- Full Windows runtime support. Windows should remain build-compatible where practical.

## Supported environment

- Rust edition: 2024.
- Minimum supported Rust version: 1.89.0.
- Primary runtime targets: macOS and Linux.
- Terminal backend: Crossterm 0.29.0.
- Rendering: Ratatui 0.30.2.
- Event loop: synchronous and blocking.

Rust 1.89.0 is the baseline because it provides stable standard-library file locking. This avoids a separate locking dependency while allowing a second process to be rejected safely.

## Command-line interface

shtodo supports exactly these version-one invocations:

```text
shtodo             Open the global list
shtodo --local     Open the list for the canonical current directory
shtodo --help      Print usage without initializing the terminal
shtodo -h          Print usage without initializing the terminal
shtodo --version   Print the package version without initializing the terminal
shtodo -V          Print the package version without initializing the terminal
```

Unknown options, positional arguments, and combinations of run/help/version options return a usage error without initializing the terminal. No CLI parsing dependency is required.

`--local` canonicalizes the exact current working directory. It does not search for a Git repository root. Different subdirectories therefore have different lists. A path that cannot be canonicalized or represented as UTF-8 returns a contextual error.

## Interaction model

### Modes

The application uses an explicit `Mode` enum with three variants:

- `Normal`: task navigation and commands.
- `Insert`: adding or editing one single-line task buffer.
- `Help`: a read-only overlay of all current bindings.

The current mode is always visible as text, not only as color.

### Normal-mode bindings

| Key | Semantic action | Behavior |
| --- | --- | --- |
| `j`, Down | `MoveDown` | Select the next visible task. |
| `k`, Up | `MoveUp` | Select the previous visible task. |
| `J` | `MoveTaskDown` | Swap the selected task with the next visible task. |
| `K` | `MoveTaskUp` | Swap the selected task with the previous visible task. |
| `i` | `StartAdd` | Open an empty editor for a task appended at commit. |
| `e` | `StartEdit` | Open the selected task text in the editor. |
| Space | `ToggleComplete` | Toggle completion without changing order or selection. |
| `d` | `Delete` | Soft-delete and hide the selected task. |
| `u` | `RestoreLatest` | Restore and select the most recently deleted task. |
| `?` | `OpenHelp` | Open the binding overlay. |
| `q` | `Quit` | Exit after the most recent successful save. |
| Ctrl-C | `Quit` | Exit from any mode. |

Navigation and reordering at a boundary are no-ops. Actions requiring a selected task are no-ops when the visible list is empty. Such no-ops do not write storage.

### Insert-mode bindings

Printable Unicode input inserts at the editor cursor. The editor preserves valid UTF-8 and moves or deletes at Rust `char` boundaries.

| Key | Semantic action | Behavior |
| --- | --- | --- |
| Left | `MoveCursorLeft` | Move one character left. |
| Right | `MoveCursorRight` | Move one character right. |
| Home | `MoveCursorStart` | Move to the start. |
| End | `MoveCursorEnd` | Move to the end. |
| Backspace | `DeleteBeforeCursor` | Delete the preceding character. |
| Delete | `DeleteAtCursor` | Delete the character under the cursor. |
| Enter | `CommitEdit` | Trim outer whitespace and commit non-empty text. |
| Escape | `CancelEdit` | Discard the buffer and return to Normal. |
| Ctrl-C | `Quit` | Exit without committing the current buffer. |

`q` is printable text in Insert mode. Committing blank or all-whitespace text keeps Insert mode active and displays a validation message. Editing does not mutate the task until commit, so cancellation is lossless.

### Help-mode bindings

The help overlay is generated from the same binding definitions used for input mapping.

- `?` and Escape close Help and return to Normal.
- Ctrl-C quits.
- Other task actions do not run through the overlay.

### Selection rules

Selection is represented by `Option<TaskId>`, not by a render-library state type or a vector index.

- Adding a task selects the new task.
- Editing and completion preserve selection.
- Deleting selects the next visible task, or the previous task when deleting the last visible task.
- Restoring selects the restored task.
- Reordering preserves the selected task identity.
- An empty visible list has no selection.

### Soft deletion and ordering

The persisted task array is the canonical order and includes hidden tombstones. Reordering finds the adjacent visible task and swaps the two full array entries. Tombstones between those entries keep their array positions.

Deleting assigns the next monotonically increasing deletion sequence. `u` finds the currently deleted task with the greatest sequence, clears its deletion sequence, and selects it. This behavior survives application restarts. Deleting a restored task later assigns a new sequence.

## Visual design

Version one uses the approved focused-list layout.

### Header

- Product name: `shtodo`.
- Active scope: `global` or the canonical current-directory display name.
- Counts for visible open and completed tasks.

### Content

- One scrollable list of visible tasks.
- A visible selection marker and completion glyph using standard Unicode symbols that do not require a Nerd Font.
- Completed text is dimmed and struck through while remaining selectable.
- Insert mode shows the edit buffer and visible cursor at the add/edit location.
- An empty state teaches `i` and `?`.

### Footer

- A textual mode badge.
- Context-sensitive essential bindings derived from the binding definitions.
- Informational or validation messages when present.
- Hints shorten or truncate safely as terminal width decreases.

### Help and constrained terminals

Help is a centered overlay containing every binding available in the current version. Below the defined minimum render area, the interface displays a resize instruction rather than malformed widgets. Quit input remains active in this state.

The future sidebar is a separate feature. Version one does not add navigation models, view registries, or sidebar-specific layout state.

## Architecture

The crate contains a thin binary and a small library with flat, responsibility-based modules:

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Install `color-eyre` and call the library entry point. |
| `src/lib.rs` | Orchestrate CLI outcome, storage setup, and terminal runtime. Expose only `run()`. |
| `src/cli.rs` | Parse supported arguments into a concrete command enum and format usage. |
| `src/action.rs` | Define semantic input actions. |
| `src/input.rs` | Define default bindings, mode-aware key mapping, and binding descriptions. |
| `src/task.rs` | Define persisted task/list data and ordered list operations. |
| `src/app.rs` | Define modes, selection, editor state, messages, and semantic state transitions. |
| `src/storage.rs` | Resolve paths, lock a scope, validate/load JSON, and save atomically. |
| `src/ui.rs` | Render borrowed application state with Ratatui. |
| `src/terminal.rs` | Own terminal setup/restoration and the blocking event loop. |

Modules remain private or `pub(crate)` except for the library `run()` entry point. The design uses concrete structs and functions. It does not introduce repository traits, dependency injection, generic frameworks, type-state machinery, threads, or async code.

### Dependency direction

```text
Crossterm KeyEvent -> input.rs -> Action -> app.rs -> task.rs
                                            |
                                            +-> storage.rs after committed mutation
                                            |
                                            +-> ui.rs reads borrowed state
```

`task.rs` and `app.rs` do not import Crossterm, Ratatui, or filesystem APIs. `ui.rs` renders but does not perform domain mutations. `storage.rs` handles persisted structures but does not know about terminal events or widgets.

### Actions and transitions

`Action` is an explicit enum. Parameterless variants are small value types. Character insertion carries a `char`. Application transitions report whether they:

- Changed only transient application state.
- Committed a persisted mutation that must be saved.
- Requested quit.

Fallible task operations return `Result`. Expected user conditions, such as an empty editor or missing selection, are represented in transition outcomes and messages rather than infrastructure errors.

The binding table maps mode, key code, and modifiers to semantic actions. Printable Insert-mode characters are a controlled fallback after reserved bindings are checked. Footer and help labels come from the same binding records. A future configuration loader can replace or extend normal-mode records without changing task actions or state transitions.

## Runtime flow

The runtime is synchronous:

1. Parse CLI options.
2. Print help/version and return when requested.
3. Resolve the selected storage scope.
4. Create the selected scope directory if required.
5. Open and exclusively lock its lock file with `std::fs::File::try_lock()`.
6. Load and validate `tasks.json`, or construct an empty list when it is absent.
7. Initialize the terminal restoration guard.
8. Draw the current state.
9. Block on `crossterm::event::read()`.
10. Map key press/repeat input to an action and apply it.
11. Atomically save when the transition committed a persisted mutation.
12. Repeat until quit or error.
13. Restore the terminal and release the list lock.

Key release and mouse events are ignored. Resize events cause the next draw to use the new frame size. There are no ticks, background jobs, timers, channels, or polling timeouts.

## Data storage

### Home resolution and layout

On macOS and Linux, `HOME` is required. Windows builds may fall back to `USERPROFILE`. A missing home variable returns an error before terminal initialization.

```text
~/.shtodo/
  global/
    tasks.json
    tasks.lock
  projects/
    <slug>-<fingerprint>/
      tasks.json
      tasks.lock
  config/                         Reserved for a later configuration feature
```

Only directories needed by the selected scope are created. The reserved `config` directory is not created in version one.

For a project scope:

- The slug comes from the final canonical path component.
- ASCII letters, digits, `.`, `_`, and `-` are retained.
- Other character runs become one `-`.
- Leading and trailing `-` are removed.
- An empty result becomes `project`.
- The slug is limited to 48 ASCII bytes.
- The fingerprint is 64-bit FNV-1a over the canonical UTF-8 path bytes, rendered as 16 lowercase hexadecimal digits.
- The folder is `<slug>-<fingerprint>`.

The snapshot stores the full canonical path. A fingerprint collision or scope mismatch returns an error instead of loading or replacing another list. Moving a directory creates a new local scope because exact canonical path is the selected identity.

### JSON schema

Snapshots use pretty-printed JSON with a trailing newline. The version-one logical shape is:

```json
{
  "schema_version": 1,
  "scope": {
    "kind": "project",
    "path": "/Users/example/code/project"
  },
  "next_task_id": 3,
  "next_deletion_sequence": 2,
  "tasks": [
    {
      "id": 1,
      "text": "Draft the release notes",
      "completed": false,
      "deletion_sequence": null
    },
    {
      "id": 2,
      "text": "Review terminal restoration",
      "completed": true,
      "deletion_sequence": 1
    }
  ]
}
```

Global snapshots use `{ "kind": "global" }` for scope. IDs and deletion sequences start at 1 and use checked incrementing `u64` values.

Loading validates:

- `schema_version` is exactly 1.
- The stored scope equals the requested scope.
- Task IDs are nonzero and unique.
- Deleted sequence values are nonzero and unique.
- Counters are greater than every value already present.
- Task text is non-empty, trimmed, and contains no line break.
- Unknown fields and unsupported schema versions are rejected rather than silently discarded.

No timestamps, dates, position numbers, priority fields, or configuration values are persisted.

### Locking and atomic saves

The process holds an exclusive lock on `tasks.lock` for the complete interactive session. `TryLockError::WouldBlock` becomes a clear message that another shtodo process is using the same scope. Global and different local scopes can run concurrently.

Every committed add, edit, completion toggle, delete, restore, or reorder saves immediately:

1. Serialize the validated state.
2. Write and truncate sibling `tasks.json.tmp` while the scope lock is held.
3. Flush and call `sync_all()` on the temporary file.
4. Rename it over `tasks.json` on the primary macOS/Linux targets.
5. Sync the containing directory.

A failure before rename leaves the last snapshot untouched. A stale temporary file is never treated as canonical and may be overwritten by the next locked save. Parse, schema, and scope failures never trigger an automatic save.

## Terminal lifecycle and error handling

`terminal.rs` owns a guard that enters raw mode and the alternate screen, contains the Ratatui terminal, and restores terminal state during explicit shutdown and `Drop`.

- Normal `q` and Ctrl-C perform explicit clean shutdown.
- Rendering, event-reading, and storage errors propagate through `Result`.
- `Drop` provides restoration during early returns and panic unwinding.
- Errors before terminal initialization print normally.
- A save error exits after restoration and leaves the last successfully saved snapshot canonical.
- Production code contains no `unwrap` or `expect`.

If explicit restoration itself fails, the returned error retains that context. `Drop` makes a best-effort fallback because destructors cannot return errors.

`color-eyre` remains the binary-level reporting layer. No additional error crate is required. Small domain/storage error enums implement `Display` and `Error` directly when callers must distinguish variants.

## Testing strategy

### Pure task and modal transitions

Unit tests in `task.rs` and `app.rs` cover:

- Empty-list invariants and no-op actions.
- Adding and selecting a task.
- Editing commit and exact cancellation.
- Blank input validation.
- Completion toggling in place.
- Selection repair after deletion.
- Latest-deletion restoration, including restored ordering.
- Visible-task reordering across hidden tombstones.
- Reordering boundaries.
- Checked ID and deletion-sequence exhaustion.
- Normal, Insert, and Help transitions.
- Quit behavior by mode.

### Input mapping

Unit tests in `input.rs` construct Crossterm `KeyEvent` values and verify:

- Every documented default binding.
- Press and repeat acceptance and release rejection.
- Mode-specific interpretation of `q`, Escape, `?`, and editing keys.
- Ctrl-C precedence in every mode.
- Printable-character fallback only in Insert mode.
- Footer/help descriptions originate from the binding definitions.

### Ratatui rendering

Unit tests in `ui.rs` use `ratatui::backend::TestBackend` and assert relevant buffer cells, symbols, styles, and positions for:

- Empty global list.
- Populated global and local lists.
- Selection and completion styles.
- Add and edit buffers with cursor placement.
- Validation and informational messages.
- Help overlay.
- Scrolled task list.
- Constrained terminal fallback.

No snapshot-testing dependency is added.

### CLI and persistence boundaries

Tests in `cli.rs` and `storage.rs` cover:

- Every accepted and rejected argument form.
- Home and scope path resolution.
- Deterministic project slug/fingerprint generation.
- Missing-file empty load.
- JSON round trip and trailing newline.
- Tombstone and deletion-sequence round trips.
- Malformed JSON, unknown fields, unsupported version, invalid invariants, and scope mismatch.
- Atomic replacement behavior and canonical-file preservation on pre-rename failure.
- Same-scope lock contention and different-scope independence.

`tempfile` is the only new development dependency and provides isolated filesystem roots.

## Dependencies

Runtime dependencies after version-one implementation:

- Existing `ratatui = "0.30.2"` for terminal UI rendering.
- Existing `crossterm = "0.29.0"` for terminal lifecycle and events.
- Existing `color-eyre = "0.6.5"` for top-level error reports.
- `serde` with `derive` for persisted data structures.
- `serde_json` for inspectable versioned snapshots.

Development dependencies:

- `tempfile` for filesystem-isolated tests.

The standard library supplies CLI parsing, home-variable access, path handling, FNV-1a implementation, and file locking. Clap, Tokio, a database, a configuration parser, a logging framework, a snapshot framework, and Unicode-editing crates are not required.

## Documentation

README documentation must cover:

- Product purpose and version-one limitations.
- Rust 1.89.0 or newer as a source-build requirement.
- `cargo install --path . --locked` and `cargo build --release --locked`.
- Global and `--local` invocation.
- All bindings and modal behavior.
- Global and project storage layout.
- Soft deletion and persistent latest-delete undo.
- The fact that moving a project directory changes its exact-path local scope.
- The one-process-per-scope lock behavior.
- Deferred trash, sidebar, configuration, release automation, and package-manager work.

## Acceptance criteria

Version one is complete when all of the following are true:

1. `shtodo` launches the global list and `shtodo --local` launches an isolated exact-current-directory list.
2. A user can add, edit, cancel, navigate, toggle completion, soft-delete, restore, and reorder tasks using the documented keys.
3. Every committed mutation survives restart.
4. Completion preserves position and selection.
5. Soft deletion hides the task, and `u` restores the latest deletion after restart.
6. Help, contextual hints, mode labels, empty state, scrolling, and constrained-terminal behavior match this specification.
7. Invalid, future-version, or wrong-scope data is reported and never overwritten.
8. A second process cannot silently write the same scope.
9. Normal quit, Ctrl-C, propagated errors, and panic unwinding restore the terminal.
10. Production code contains no `unwrap` or `expect` and introduces no threads or async runtime.
11. The README documents source installation, usage, bindings, storage, and deferrals.
12. `cargo build --release --locked` produces the native executable.
13. These verification commands pass exactly:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
```

14. A manual smoke test in a temporary home directory confirms both scopes, cross-restart persistence, lock rejection, and terminal restoration after normal quit and a forced persistence error.

## Deferred follow-up work

The following work is intentionally separate from version one:

- A trash view that lists, restores, and permanently removes tombstones.
- A sidebar for navigating global, project, trash, and later views.
- A `~/.shtodo/config` format for keybindings and themes.
- Config validation, conflict reporting, and fallback behavior.
- Grapheme-cluster-aware editing if scalar-value editing proves insufficient.
- Automated GitHub release binaries.
- Homebrew or other package-manager distribution.
- Broader Windows runtime testing and support.
