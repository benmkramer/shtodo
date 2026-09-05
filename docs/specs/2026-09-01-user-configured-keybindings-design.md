# User-Configured Keybindings Design

## Purpose

Add optional, user-configured keybindings to shtodo without allowing input
behavior, footer hints, empty-state guidance, and keyboard help to drift apart.
Users configure bindings in `~/.shtodo/config.toml`. The new `shtodo doctor`
command validates that file without starting the terminal interface or opening
task storage.

This feature preserves shtodo's small modal model. It changes how existing
semantic actions are bound, but it does not add macros, multi-key sequences,
plugins, live reload, per-project configuration, or configurable themes.

## User-facing behavior

### Optional configuration

The configuration file is:

```text
~/.shtodo/config.toml
```

On macOS and Linux, `~` uses the same nonempty `HOME` resolution as task
storage. Windows may use the existing `USERPROFILE` fallback. shtodo never
creates this file automatically.

When the file does not exist, interactive shtodo uses its compiled defaults.
A missing file is valid, including when checked by `shtodo doctor`.

When the file exists, each configured action replaces the complete default
key list for that action. Actions omitted from the file retain their defaults.
The order of each configured array is meaningful: the first key is the
preferred key shown in compact UI hints.

```toml
[keybindings.normal]
move_down = ["j", "down", "ctrl-n"]
move_up = ["k", "up", "ctrl-p"]
add_task = ["a"]
open_help = ["?"]

[keybindings.insert]
commit_edit = ["enter"]
cancel_edit = ["esc"]

[keybindings.help]
close_help = ["?", "esc"]
```

Empty tables are allowed. Empty action arrays are rejected because an action
that is explicitly configured must retain at least one binding.

### Configurable actions

The configuration schema exposes the existing fixed semantic actions under
their valid modes.

| Table | Action key | Current default keys |
| --- | --- | --- |
| `keybindings.normal` | `move_down` | `j`, `down` |
| `keybindings.normal` | `move_up` | `k`, `up` |
| `keybindings.normal` | `move_task_down` | `J` |
| `keybindings.normal` | `move_task_up` | `K` |
| `keybindings.normal` | `add_task` | `i` |
| `keybindings.normal` | `edit_task` | `e` |
| `keybindings.normal` | `toggle_complete` | `space` |
| `keybindings.normal` | `delete_task` | `d` |
| `keybindings.normal` | `restore_latest` | `u` |
| `keybindings.normal` | `open_help` | `?` |
| `keybindings.normal` | `quit` | `q` |
| `keybindings.insert` | `move_cursor_left` | `left` |
| `keybindings.insert` | `move_cursor_right` | `right` |
| `keybindings.insert` | `move_cursor_start` | `home` |
| `keybindings.insert` | `move_cursor_end` | `end` |
| `keybindings.insert` | `move_word_left` | `alt-left`, `alt-b` |
| `keybindings.insert` | `move_word_right` | `alt-right`, `alt-f` |
| `keybindings.insert` | `delete_before_cursor` | `backspace` |
| `keybindings.insert` | `delete_at_cursor` | `delete` |
| `keybindings.insert` | `delete_word_before_cursor` | `alt-backspace`, `ctrl-w` |
| `keybindings.insert` | `delete_word_at_cursor` | `alt-delete` |
| `keybindings.insert` | `commit_edit` | `enter` |
| `keybindings.insert` | `cancel_edit` | `esc` |
| `keybindings.help` | `close_help` | `?`, `esc` |

Printable characters in Insert mode remain text-entry fallback after fixed
bindings are checked. `InsertChar` is not a configurable action.

### Reserved emergency binding

`Ctrl-C` remains an implicit, non-removable quit binding in Normal, Insert,
and Help modes. It is displayed in keyboard help but not in the footer.
Configuration cannot replace it, remove it, or assign it to another action.
Any spelling that normalizes to `Ctrl-C` is rejected in an action array.

This is the only reserved binding. The Normal-mode `quit` action remains
configurable independently of the emergency binding.

### Key syntax

Each key is represented by one TOML string. Accepted forms are:

- One printable Unicode scalar value, such as `j`, `J`, `?`, or `é`.
- A named key: `up`, `down`, `left`, `right`, `home`, `end`, `page-up`,
  `page-down`, `tab`, `backtab`, `enter`, `esc`, `space`, `backspace`,
  `delete`, or `insert`.
- A printable character or named key prefixed by `ctrl-`, `alt-`, or both,
  such as `ctrl-n`, `alt-left`, or `ctrl-alt-x`.

Named keys and modifier names are ASCII case-insensitive. A single printable
character remains case-sensitive, so `j` and `J` are different bindings.
Users express shifted printable characters as the resulting character, such
as `J`, rather than `shift-j`. A `shift-` modifier is not part of the syntax.

Parsing produces one normalized key representation shared by conflict
detection and Crossterm event matching. ASCII letters combined with Ctrl or
Alt normalize case so terminal encodings such as `Alt-b` and `Alt-B` match the
same configured chord. Display labels are generated from normalized keys,
using forms such as `Down`, `Ctrl-n`, and `Alt-Left`.

## Validation and diagnostics

The configuration is valid only when all of these conditions hold:

- The TOML document is syntactically valid.
- Every table and field is recognized. Unknown fields are rejected so typos
  cannot silently leave defaults active.
- Every action appears only in the mode that supports it.
- Every configured key string uses the supported syntax.
- Every configured action array is nonempty.
- An action does not contain the same normalized key more than once.
- One normalized key is not assigned to different actions in the same mode.
- No configured key normalizes to reserved `Ctrl-C`.

The same key may be assigned in different modes because mode determines its
meaning. Replacement semantics may deliberately remove any non-reserved
default key.

Validation collects every independent issue it can determine from the parsed
document and reports them together in stable source order. A TOML syntax error
that prevents deserialization is reported with the parser's location and
context; semantic validation cannot continue when the document cannot be
parsed. File read failures other than Not Found are fatal and include the
configuration path.

Interactive startup validates configuration before opening task storage,
acquiring a list lock, or initializing Ratatui. An invalid file stops startup,
prints the path and diagnostics, and suggests `shtodo doctor`. It never falls
back silently to defaults.

The non-interactive `add`, `--help`, and `--version` paths do not consume a
keymap, so they do not load or validate the keybinding file. This keeps task
capture available even while a user repairs an invalid interactive config.

## `shtodo doctor`

The CLI adds exactly this command form:

```text
shtodo doctor
```

`doctor` does not accept `--local`, positional arguments, or additional
options. Unsupported combinations follow the existing usage-error behavior.
CLI help documents the command.

The command resolves `~/.shtodo/config.toml` and invokes the same loader,
parser, merger, and validator used by interactive startup. It does not open a
task store, create task directories, acquire a task lock, initialize the
terminal, or mutate the config file.

For a missing file, it exits zero and explains that compiled defaults are in
use:

```text
Config: /Users/ben/.shtodo/config.toml
OK: no config file; using defaults
```

For a valid file, it exits zero and summarizes the effective keymap:

```text
Config: /Users/ben/.shtodo/config.toml
OK: 24 configurable actions, 33 active bindings
```

The binding count includes the fixed `Ctrl-C` binding in each mode and changes
when user replacement arrays change the effective total.

For an invalid file, it exits nonzero and prints the path followed by all
available diagnostics:

```text
Config: /Users/ben/.shtodo/config.toml
Invalid configuration:
  keybindings.normal.move_down: unknown key "dn"
  keybindings.normal.add_task: "x" conflicts with delete_task
```

The doctor output does not suggest running doctor again. Interactive startup
wraps the shared diagnostics with that suggestion instead.

## Architecture

### Responsibility boundaries

Add `src/config.rs` with one responsibility: resolve the config path, read the
optional TOML file, deserialize the user-facing schema, apply overrides to the
compiled defaults, and return either a validated runtime keymap or structured
diagnostics.

Refactor `src/input.rs` around grouped semantic binding definitions:

- Compiled definitions retain mode, semantic action, description, footer
  eligibility and priority, and ordered default chords.
- A resolved binding retains that metadata plus its ordered effective chords.
- `Keymap` owns the resolved binding groups and exposes mode-filtered borrowed
  iteration and event mapping.
- The fixed emergency quit binding participates in matching and Help output
  but cannot be modified through configuration.
- Printable Insert-mode characters remain a fallback after resolved and fixed
  bindings are checked.

This grouped representation matches the configuration model and allows Help
to show all keys for one action on one row. Configuration code does not import
Ratatui. UI code does not deserialize TOML or merge bindings.

Update the existing callers so the resolved `Keymap` is explicit:

```text
config.toml + compiled defaults
              |
              v
       validated Keymap
          /         \
KeyEvent -> Action   footer, empty state, and Help
              |
              v
             App
```

The orchestration flow for interactive `shtodo` and `shtodo --local` is:

1. Parse CLI arguments.
2. Resolve the home directory and config path.
3. Load and validate the effective `Keymap`.
4. Resolve and open the selected task store.
5. Load application state.
6. Initialize the terminal.
7. Pass `&Keymap` to event mapping and rendering for the lifetime of the run.

Configuration is loaded exactly once. There is no file watching, caching,
background parsing, or live reload. The file and binding set are small, and
this one-time work is negligible compared with launching an interactive
terminal application.

### UI behavior

The UI renders only the effective keymap:

- The footer shows the first active key for each footer-eligible action.
- Footer ordering is based on semantic-action priority, never a literal key
  label, so rebinding `add_task` does not make its hint lose priority.
- Keyboard Help shows every active key grouped by action, for example
  `j / Down / Ctrl-n  move down`.
- The fixed `Ctrl-C` emergency binding remains visible in Help for each mode.
- The empty-state text derives its add-task and open-help keys from the same
  keymap instead of hardcoding `i` and `?`.
- Key labels come from normalized chords rather than user-entered casing.

The current responsive Help behavior remains: use two columns when content
fits and fall back to one column otherwise. This feature does not add Help
scrolling or a new layout mode.

## Dependencies and performance

Add one TOML deserialization dependency compatible with the project's current
Serde version and Rust 1.89 minimum. Do not add a configuration framework, CLI
framework, file watcher, cache, async runtime, or global mutable keymap.

Loading performs at most one metadata/open/read operation for a small local
file, one TOML deserialization, and validation over a few dozen bindings. It
occurs once before terminal initialization and adds no work to the steady-state
event or render loop.

## Testing strategy

### Configuration and key parsing

Unit tests cover:

- A missing config file produces defaults without creating the file.
- A valid partial config replaces only specified actions.
- Unspecified actions retain ordered defaults.
- Configured array order becomes preferred-key order.
- Printable, named, Ctrl, Alt, and combined-modifier forms normalize and
  generate canonical labels.
- Uppercase printable characters remain distinct from lowercase characters.
- Modified ASCII letter case normalizes for matching and conflict detection.
- Unknown tables, modes, and actions are rejected.
- Malformed keys, empty arrays, duplicate keys, same-mode conflicts, and
  reserved `Ctrl-C` are rejected.
- The same key in two modes is accepted.
- Multiple independent semantic errors are returned together in stable order.
- Syntax and non-Not-Found I/O errors include the config path and context.

Each unit test should describe and assert one behavior where practical.

### Input and UI integration

Input tests construct a custom `Keymap` and prove that old replaced keys stop
mapping, new keys map to the existing `Action`, fixed `Ctrl-C` still quits in
all modes, and printable Insert input still works after binding lookup.

Ratatui `TestBackend` tests prove that:

- A custom first key appears in the footer.
- Replaced default keys disappear from the footer and Help.
- Every effective alias is grouped into the matching Help row.
- Footer priority remains action-based after keys change.
- Empty-state instructions use the custom add-task and open-help keys.

Existing default-key and default-render tests remain as regression coverage
against `Keymap::default()`.

### CLI and orchestration

Parser tests cover the exact `doctor` form and rejected combinations.
Binary integration tests use isolated temporary home directories to prove:

- `shtodo doctor` succeeds for a missing file and does not create `.shtodo`.
- It succeeds and prints the effective summary for a valid file.
- It exits nonzero and prints all available issues for invalid configuration.
- It does not create global/project task storage or initialize the TUI.

An orchestration-level test verifies that interactive startup returns a
config diagnostic before task-store or terminal setup. The implementation
should expose a small internal seam for this test rather than mutate process
environment concurrently inside unit tests.

Final verification remains:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
```

## Documentation

Update the README to include:

- `shtodo doctor` in usage and command documentation.
- The optional `~/.shtodo/config.toml` path.
- A complete TOML example and the supported action names.
- Accepted key syntax and canonical label behavior.
- Replacement semantics and array ordering.
- The fixed `Ctrl-C` rule.
- Conflict and invalid-config behavior.
- The relationship between startup validation and `shtodo doctor`.
- Default binding tables clearly labeled as defaults.

Remove user-editable keybindings and configuration files from the version-one
limits section. Keep themes and other deferred configuration work explicitly
out of scope.

## Out of scope

- Creating or rewriting `config.toml` from shtodo.
- Live reload or a file watcher.
- Per-project, per-list, or command-line config paths.
- Multi-key sequences, chords with timing, macros, or command composition.
- Unbinding an action with an empty list.
- Removing or changing the fixed `Ctrl-C` emergency binding.
- Configurable descriptions, footer visibility, footer priority, or action
  names.
- Configurable themes or other visual settings.
- Help scrolling or a new Help navigation mode.
- Plugins, hooks, aliases for semantic actions, or runtime extensions.
