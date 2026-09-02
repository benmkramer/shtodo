# User-Configured Keybindings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load optional keybinding overrides from `~/.shtodo/config.toml`, use the resolved keymap for input and every UI key hint, and add `shtodo doctor` for non-interactive validation.

**Architecture:** Replace the static per-key binding table with grouped semantic binding definitions and one owned, validated `Keymap`. `config.rs` parses ordered TOML overrides and merges them with compiled defaults before interactive startup; input mapping, the footer, the empty state, and Help all borrow that same keymap. `shtodo doctor` invokes the identical loader and validator without opening task storage or Ratatui.

**Tech Stack:** Rust 2024 with Rust 1.89 minimum, Crossterm 0.29.0, Ratatui 0.30.2, color-eyre 0.6.5, Serde 1.0, toml 1.1.4 with `preserve_order`, tempfile, and Ratatui `TestBackend`.

**Spec:** `docs/superpowers/specs/2026-09-01-user-configured-keybindings-design.md`

## Global Constraints

- Implement in an isolated worktree, not directly on `main`.
- Keep the event loop synchronous and blocking. Do not add threads, timers, channels, polling, file watchers, or an async runtime.
- Keep `Ctrl-C` as an implicit, non-removable quit binding in Normal, Insert, and Help modes.
- A configured action replaces all of that action's configurable defaults; an omitted action retains its ordered defaults.
- Reject empty arrays, malformed keys, duplicate normalized keys, same-mode conflicts, unknown fields, and attempts to configure `Ctrl-C`.
- Load the config once before interactive task storage or terminal initialization. Do not load it for `add`, `--help`, or `--version`.
- A missing `~/.shtodo/config.toml` is valid and must not create the file or `.shtodo` directory.
- Derive input behavior, the footer's preferred key, empty-state hints, and all Help keys from the same resolved `Keymap`.
- Keep task/application logic independent from Crossterm, Ratatui, TOML, and filesystem APIs.
- Production code must not use `unwrap`, `expect`, or panic-based recovery.
- Keep the existing Rust 1.89 minimum and macOS/Linux primary runtime support. Preserve practical Windows build compatibility.
- Do not add config generation, live reload, per-project config, multi-key sequences, macros, themes, plugins, configurable UI metadata, or Help scrolling.
- Each task uses strict RED then GREEN evidence and commits only after its focused tests and `cargo fmt --check` pass.
- Final verification commands are exactly:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
```

---

## File Structure

| Path | Final responsibility |
| --- | --- |
| `Cargo.toml` | Add `toml = { version = "1.1.4", features = ["preserve_order"] }` while retaining the Rust 1.89 floor. |
| `Cargo.lock` | Lock the TOML parser and ordered-map transitive dependency versions. |
| `src/input.rs` | Define normalized key chords, grouped compiled binding definitions, override application, conflict validation, runtime key mapping, UI metadata, and key labels. |
| `src/config.rs` | Resolve `~/.shtodo/config.toml`, parse ordered TOML tables, collect structural diagnostics, merge known overrides into `Keymap`, and format validation results. |
| `src/cli.rs` | Parse the exact `shtodo doctor` command and document it in CLI usage. |
| `src/lib.rs` | Orchestrate config loading before interactive storage, keep non-keymap commands independent, and print doctor reports. |
| `src/terminal.rs` | Borrow the resolved `Keymap` for event mapping and each draw. |
| `src/ui.rs` | Render footer, empty-state, and grouped Help labels from the borrowed `Keymap`. |
| `tests/cli.rs` | Verify doctor exit/output/filesystem behavior and interactive fail-fast ordering in isolated homes. |
| `README.md` | Document doctor, config location and schema, supported keys, replacement semantics, defaults, and validation behavior. |
| `CHANGELOG.md` | Record configurable keybindings and doctor under Unreleased. |

The `toml` 1.1.4 crate enables Serde parsing by default. Its documented `preserve_order` feature stores table entries in source order, and its Rust 1.85 MSRV is below this project's Rust 1.89 floor. See [toml features](https://docs.rs/crate/toml/1.1.4+spec-1.1.0/features) and [toml Table](https://docs.rs/toml/1.1.4+spec-1.1.0/toml/type.Table.html).

---

### Task 1: Replace the Static Binding Table with a Runtime Keymap

**Files:**
- Modify: `src/input.rs:1-654`
- Modify: `src/ui.rs:11-588`
- Modify: `src/terminal.rs:1-116`
- Modify: `src/lib.rs:1-44`

**Interfaces:**
- Produces: `BindingId::from_config(mode: Mode, name: &str) -> Option<BindingId>`, `BindingId::config_name(self) -> Option<&'static str>`, and `BindingId::mode(self) -> Mode`.
- Produces: `KeyChord::parse(value: &str) -> Result<KeyChord, KeyParseError>` and `KeyChord::label(&self) -> String`.
- Produces: `BindingOverride { order: usize, path: String, id: BindingId, keys: Vec<String> }`.
- Produces: `KeymapIssue { order: usize, path: String, message: String }`.
- Produces: `Keymap::defaults() -> Keymap`, `Keymap::with_overrides(overrides: &[BindingOverride]) -> Result<Keymap, Vec<KeymapIssue>>`, `Keymap::map_key(&self, mode: Mode, event: KeyEvent) -> Option<Action>`, `Keymap::bindings_for(&self, mode: Mode) -> impl Iterator<Item = &ResolvedBinding>`, `Keymap::configurable_action_count(&self) -> usize`, and `Keymap::active_binding_count(&self) -> usize`.
- Produces: `ResolvedBinding::id(&self) -> BindingId`, `preferred_label(&self) -> &str`, `labels(&self) -> impl Iterator<Item = &str>`, `action() -> Action`, `description() -> &'static str`, and `footer_priority() -> Option<u8>`.
- Changes: `ui::render(frame: &mut Frame<'_>, app: &App, keymap: &Keymap)`.
- Changes: `terminal::run(app: App, store: &Store, keymap: &Keymap) -> Result<()>`.
- Consumes: existing `Mode`, `Action`, Crossterm `KeyEvent`, and Ratatui rendering. It does not consume TOML or filesystem APIs.

- [ ] **Step 1: Write failing normalized-key and override tests**

Replace the old tests' calls to free `map_key` and `bindings_for` functions with `Keymap::defaults()`. Add focused tests in `src/input.rs`:

```rust
#[test]
fn key_chord_should_parse_supported_forms_and_generate_canonical_labels() {
    let cases = [
        ("J", "J"),
        ("down", "Down"),
        ("space", "Space"),
        ("ctrl-n", "Ctrl-n"),
        ("ALT-left", "Alt-Left"),
        ("ctrl-alt-x", "Ctrl-Alt-x"),
    ];

    for (source, expected) in cases {
        let chord = KeyChord::parse(source).unwrap();
        assert_eq!(chord.label(), expected);
    }
}

#[test]
fn with_overrides_should_replace_one_action_and_preserve_other_defaults() {
    let keymap = Keymap::with_overrides(&[BindingOverride {
        order: 0,
        path: "keybindings.normal.move_down".into(),
        id: BindingId::MoveDown,
        keys: vec!["x".into(), "ctrl-n".into()],
    }])
    .unwrap();

    assert_eq!(
        keymap.map_key(Mode::Normal, pressed(KeyCode::Char('x'), KeyModifiers::NONE)),
        Some(Action::MoveDown)
    );
    assert_eq!(
        keymap.map_key(Mode::Normal, pressed(KeyCode::Char('j'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(
        keymap.map_key(Mode::Normal, pressed(KeyCode::Char('k'), KeyModifiers::NONE)),
        Some(Action::MoveUp)
    );
}

#[test]
fn with_overrides_should_collect_empty_duplicate_conflict_and_reserved_issues() {
    let result = Keymap::with_overrides(&[
        BindingOverride {
            order: 0,
            path: "keybindings.normal.move_down".into(),
            id: BindingId::MoveDown,
            keys: Vec::new(),
        },
        BindingOverride {
            order: 1,
            path: "keybindings.normal.move_up".into(),
            id: BindingId::MoveUp,
            keys: vec!["x".into(), "x".into()],
        },
        BindingOverride {
            order: 2,
            path: "keybindings.normal.add_task".into(),
            id: BindingId::StartAdd,
            keys: vec!["d".into(), "ctrl-c".into()],
        },
    ]);
    let messages = result
        .unwrap_err()
        .into_iter()
        .map(|issue| issue.message)
        .collect::<Vec<_>>();

    assert_eq!(
        messages,
        vec![
            "must contain at least one key",
            "duplicate key \"x\"",
            "Ctrl-C is reserved and cannot be configured",
            "\"d\" conflicts with delete_task",
        ]
    );
}
```

Add a `pressed(code, modifiers)` test helper that calls `KeyEvent::new_with_kind(code, modifiers, KeyEventKind::Press)`. Keep the existing tests for release events, normal defaults, word-edit aliases, fixed `Ctrl-C`, and printable Insert fallback.

- [ ] **Step 2: Run the focused tests to verify RED**

```bash
cargo test --locked input::tests::key_chord_should_parse_supported_forms_and_generate_canonical_labels
cargo test --locked input::tests::with_overrides_should_replace_one_action_and_preserve_other_defaults
```

Expected: compilation fails because `BindingId`, `BindingOverride`, `KeyParseError`, and `Keymap` do not exist.

- [ ] **Step 3: Implement normalized chords and grouped definitions**

Replace `KeyCodePattern`, `Binding`, `BINDINGS`, and the free iterator/mapping functions with owned runtime types. Use this shape:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BindingId {
    MoveDown,
    MoveUp,
    MoveTaskDown,
    MoveTaskUp,
    StartAdd,
    StartEdit,
    ToggleComplete,
    Delete,
    RestoreLatest,
    OpenHelp,
    NormalQuit,
    MoveCursorLeft,
    MoveCursorRight,
    MoveCursorStart,
    MoveCursorEnd,
    MoveWordLeft,
    MoveWordRight,
    DeleteBeforeCursor,
    DeleteAtCursor,
    DeleteWordBeforeCursor,
    DeleteWordAtCursor,
    CommitEdit,
    CancelEdit,
    CloseHelp,
    InsertEmergencyQuit,
    HelpEmergencyQuit,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct KeyChord {
    code: KeyCode,
    modifiers: KeyModifiers,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BindingOverride {
    pub(crate) order: usize,
    pub(crate) path: String,
    pub(crate) id: BindingId,
    pub(crate) keys: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct KeymapIssue {
    pub(crate) order: usize,
    pub(crate) path: String,
    pub(crate) message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedBinding {
    id: BindingId,
    mode: Mode,
    action: Action,
    description: &'static str,
    footer_priority: Option<u8>,
    chords: Vec<KeyChord>,
    labels: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Keymap {
    bindings: Vec<ResolvedBinding>,
}
```

Define all groups in this exact order. The first list is configurable defaults; the second list is fixed keys appended after replacement:

| Mode | Config name | Action | Description | Footer priority | Defaults | Fixed |
| --- | --- | --- | --- | ---: | --- | --- |
| Normal | `move_down` | `MoveDown` | `move down` | 2 | `j`, `Down` | none |
| Normal | `move_up` | `MoveUp` | `move up` | 2 | `k`, `Up` | none |
| Normal | `move_task_down` | `MoveTaskDown` | `move task down` | 2 | `J` | none |
| Normal | `move_task_up` | `MoveTaskUp` | `move task up` | 2 | `K` | none |
| Normal | `add_task` | `StartAdd` | `add task` | 0 | `i` | none |
| Normal | `edit_task` | `StartEdit` | `edit task` | 2 | `e` | none |
| Normal | `toggle_complete` | `ToggleComplete` | `toggle complete` | 2 | `Space` | none |
| Normal | `delete_task` | `Delete` | `delete task` | 2 | `d` | none |
| Normal | `restore_latest` | `RestoreLatest` | `restore latest` | 2 | `u` | none |
| Normal | `open_help` | `OpenHelp` | `show help` | 1 | `?` | none |
| Normal | `quit` | `Quit` | `quit` | 2 | `q` | `Ctrl-C` |
| Insert | `move_cursor_left` | `MoveCursorLeft` | `move cursor left` | 2 | `Left` | none |
| Insert | `move_cursor_right` | `MoveCursorRight` | `move cursor right` | 2 | `Right` | none |
| Insert | `move_cursor_start` | `MoveCursorStart` | `move cursor start` | none | `Home` | none |
| Insert | `move_cursor_end` | `MoveCursorEnd` | `move cursor end` | none | `End` | none |
| Insert | `move_word_left` | `MoveWordLeft` | `move one word left` | none | `Alt-Left`, `Alt-b` | none |
| Insert | `move_word_right` | `MoveWordRight` | `move one word right` | none | `Alt-Right`, `Alt-f` | none |
| Insert | `delete_before_cursor` | `DeleteBeforeCursor` | `delete before cursor` | 2 | `Backspace` | none |
| Insert | `delete_at_cursor` | `DeleteAtCursor` | `delete at cursor` | none | `Delete` | none |
| Insert | `delete_word_before_cursor` | `DeleteWordBeforeCursor` | `delete previous word` | none | `Alt-Backspace`, `Ctrl-w` | none |
| Insert | `delete_word_at_cursor` | `DeleteWordAtCursor` | `delete next word` | none | `Alt-Delete` | none |
| Insert | `commit_edit` | `CommitEdit` | `save edit` | 0 | `Enter` | none |
| Insert | `cancel_edit` | `CancelEdit` | `cancel edit` | 1 | `Esc` | none |
| Insert | none | `Quit` | `quit` | none | none | `Ctrl-C` |
| Help | `close_help` | `CloseHelp` | `close help` | 0 | `?`, `Esc` | none |
| Help | none | `Quit` | `quit` | none | none | `Ctrl-C` |

Implement `BindingId::from_config` with exhaustive mode/name matching for the 24 configurable rows and return `None` for emergency-only groups. Implement `config_name` with the exact names above.

`KeyChord::parse` must:

1. Consume case-insensitive `ctrl-` and `alt-` prefixes in either order, rejecting duplicate modifiers or an empty remainder.
2. Match named keys case-insensitively: `up`, `down`, `left`, `right`, `home`, `end`, `page-up`, `page-down`, `tab`, `backtab`, `enter`, `esc`, `space`, `backspace`, `delete`, and `insert`.
3. Otherwise accept exactly one non-control Unicode scalar value.
4. Reject `shift-`, multi-character unknown names, and control characters.
5. Normalize modified ASCII letters to lowercase. Preserve unmodified character case.

`KeyChord::from_event` must remove Crossterm's Shift bit for character and BackTab events, preserve a character's resulting case, and apply the same modified-ASCII lowercase normalization. Map `KeyCode::BackTab` to the `backtab` named key. `label` must emit the modifier order `Ctrl-Alt-`, title-case named keys, `Space` for the space character, and the literal character otherwise.

`Keymap::with_overrides` must begin with all compiled definitions, replace only the configurable defaults for each supplied `BindingId`, append fixed chords, and return no keymap if any issue exists. Walk overrides by `order`; keep per-action duplicate diagnostics adjacent; then detect conflicts against the final effective map and attach each conflict to the later source-ordered override. Treat conflict with an omitted action's default as a conflict with that action. Sort issues stably by `order` before returning them.

`Keymap::map_key` must ignore events other than Press and Repeat, search the current mode's resolved groups in definition order, then apply the existing unmodified printable-character Insert fallback.

- [ ] **Step 4: Run input tests to verify GREEN**

```bash
cargo test --locked input::tests
cargo fmt --check
```

Expected: all default, parsing, override, conflict, reserved-key, event-kind, word-edit, and printable Insert tests pass.

- [ ] **Step 5: Write failing UI tests for custom hints**

Change the `render_app` test helper in `src/ui.rs` to call a second helper with `Keymap::defaults()`:

```rust
fn render_app_with_keymap(app: &App, keymap: &Keymap, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render(frame, app, keymap))
        .unwrap();
    terminal.backend().buffer().clone()
}
```

Add these tests:

```rust
#[test]
fn custom_keymap_should_drive_footer_help_and_empty_state() {
    let keymap = Keymap::with_overrides(&[
        override_for(BindingId::StartAdd, &["a"]),
        override_for(BindingId::OpenHelp, &["h", "?"]),
    ])
    .unwrap();
    let mut app = App::new(TaskList::new(ListScope::Global));

    let normal = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 12));
    assert!(normal.contains("Press a to add · h for help"));
    assert!(normal.contains("a add task"));
    assert!(normal.contains("h help"));
    assert!(!normal.contains("i add task"));

    app.apply(Action::OpenHelp).unwrap();
    let help = buffer_text(&render_app_with_keymap(&app, &keymap, 80, 24));
    assert!(help.contains("h / ? show help"));
}

#[test]
fn custom_footer_should_prioritize_actions_instead_of_literal_keys() {
    let keymap = Keymap::with_overrides(&[
        override_for(BindingId::StartAdd, &["z"]),
        override_for(BindingId::OpenHelp, &["h"]),
    ])
    .unwrap();
    let app = App::new(TaskList::new(ListScope::Global));
    let buffer = render_app_with_keymap(&app, &keymap, 40, 8);
    let footer = buffer_row(&buffer, 40, 7);

    assert!(footer.contains("z add task"));
    assert!(footer.contains("h help"));
}
```

Define `override_for(id, keys)` in the UI test module with deterministic `order: 0`, the matching `config_name` path, and owned strings.

- [ ] **Step 6: Run custom UI tests to verify RED**

```bash
cargo test --locked ui::tests::custom_keymap_should_drive_footer_help_and_empty_state
```

Expected: compilation fails because `ui::render` still has no `Keymap` parameter and existing rendering reads the removed static table.

- [ ] **Step 7: Thread `&Keymap` through rendering and the terminal loop**

Make these exact call-shape changes:

```rust
// src/ui.rs
pub(crate) fn render(frame: &mut Frame<'_>, app: &App, keymap: &Keymap)

// src/terminal.rs
fn action_for_event(keymap: &Keymap, mode: Mode, event: Event) -> Option<Action>
pub(crate) fn run(mut app: App, store: &Store, keymap: &Keymap) -> Result<()>

// temporary default wiring in src/lib.rs, replaced by config loading in Task 3
let keymap = input::Keymap::defaults();
terminal::run(app, &store, &keymap)?;
```

In `ui.rs`:

- Pass `keymap` into `render_content`, `render_footer`, and `render_help`.
- Derive empty-state add/help labels by finding `BindingId::StartAdd` and `BindingId::OpenHelp` and taking `preferred_label`. These groups always exist in a validated keymap.
- Filter footer groups by `footer_priority().is_some()`, sort by that semantic priority, and render only `preferred_label` plus the existing shortened description.
- Build one Help line per group as `labels.join(" / ") + " " + description`, including fixed Ctrl-C groups.
- Preserve the existing mode headers and responsive two-column/one-column calculation.
- Update all existing `TestBackend` helpers to pass `Keymap::defaults()` so default screenshots and visual contracts remain regression coverage.

Replace the old one-row-per-chord Help expectations with these exact grouped rows:

```text
Normal
j / Down move down
k / Up move up
J move task down
K move task up
i add task
e edit task
Space toggle complete
d delete task
u restore latest
? show help
q / Ctrl-C quit

Insert
Left move cursor left
Right move cursor right
Home move cursor start
End move cursor end
Alt-Left / Alt-b move one word left
Alt-Right / Alt-f move one word right
Backspace delete before cursor
Delete delete at cursor
Alt-Backspace / Ctrl-w delete previous word
Alt-Delete delete next word
Enter save edit
Esc cancel edit
Ctrl-C quit

Help
? / Esc close help
Ctrl-C quit
```

In `terminal.rs`, use `keymap.map_key(mode, key)` and pass the same keymap to every `ui::render` call. Update the non-key-event test to construct one default keymap.

- [ ] **Step 8: Run the complete runtime-keymap slice to verify GREEN**

```bash
cargo test --locked input::tests
cargo test --locked ui::tests
cargo test --locked terminal::tests
cargo fmt --check
```

Expected: all tests pass. Default behavior remains unchanged except Help now groups all active aliases and the Help footer shows only the first close-help key, as required by the approved spec.

- [ ] **Step 9: Commit the runtime keymap**

```bash
git add src/input.rs src/ui.rs src/terminal.rs src/lib.rs
git commit -m "refactor: resolve bindings through runtime keymap"
```

---

### Task 2: Load and Validate `config.toml`

**Files:**
- Modify: `Cargo.toml:11-19`
- Modify: `Cargo.lock`
- Create: `src/config.rs`
- Modify: `src/lib.rs:1-8`

**Interfaces:**
- Consumes: `input::BindingId`, `BindingOverride`, `Keymap`, and `KeymapIssue` from Task 1.
- Produces: `ConfigSource::{Defaults, File}`.
- Produces: `LoadedKeymap { path: PathBuf, source: ConfigSource, keymap: Keymap }` with borrowed accessors.
- Produces: `ConfigDiagnostic { order: usize, path: String, message: String }`.
- Produces: `ConfigError::{Read, Parse, Invalid}` implementing `Display` and `Error` without losing the config path.
- Produces: `config_path(home: &Path) -> PathBuf` and `load(home: &Path) -> Result<LoadedKeymap, ConfigError>`.
- Does not consume: task storage, Ratatui, CLI scope, or process-global environment.

- [ ] **Step 1: Add the TOML dependency and failing loader tests**

Add to `[dependencies]`:

```toml
toml = { version = "1.1.4", features = ["preserve_order"] }
```

Add `mod config;` to `src/lib.rs`. Create `src/config.rs` with tests using `tempfile::tempdir()`:

```rust
#[test]
fn load_should_use_defaults_without_creating_missing_config() {
    let home = tempfile::tempdir().unwrap();

    let loaded = load(home.path()).unwrap();

    assert_eq!(loaded.source(), ConfigSource::Defaults);
    assert_eq!(loaded.keymap().configurable_action_count(), 24);
    assert_eq!(loaded.keymap().active_binding_count(), 33);
    assert!(!home.path().join(".shtodo").exists());
}

#[test]
fn load_should_apply_partial_action_replacements() {
    let home = configured_home(
        r#"
[keybindings.normal]
move_down = ["x", "ctrl-n"]
"#,
    );

    let loaded = load(home.path()).unwrap();

    assert_eq!(loaded.source(), ConfigSource::File);
    assert_eq!(loaded.keymap().active_binding_count(), 33);
    assert_eq!(
        labels_for(loaded.keymap(), BindingId::MoveDown),
        vec!["x", "Ctrl-n"]
    );
    assert_eq!(
        labels_for(loaded.keymap(), BindingId::MoveUp),
        vec!["k", "Up"]
    );
}

#[test]
fn load_should_report_structural_and_keymap_issues_in_source_order() {
    let home = configured_home(
        r#"
[keybindings.normal]
move_down = ["dn"]
wat = ["x"]
add_task = ["d", "ctrl-c"]

[keybindings.unknown]
close_help = ["esc"]
"#,
    );

    let diagnostics = match load(home.path()).unwrap_err() {
        ConfigError::Invalid { diagnostics, .. } => diagnostics,
        error => panic!("expected invalid configuration, got {error}"),
    };

    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "keybindings.normal.move_down",
            "keybindings.normal.wat",
            "keybindings.normal.add_task",
            "keybindings.normal.add_task",
            "keybindings.unknown",
        ]
    );
}
```

The `configured_home` test helper creates `<temp>/.shtodo/config.toml` and writes the supplied fixture. The `labels_for` helper finds the requested binding group through `bindings_for(id.mode())` and returns its labels.

Also add one-behavior tests named:

- `load_should_accept_the_same_key_in_different_modes`
- `load_should_reject_unknown_root_field`
- `load_should_reject_non_table_keybindings`
- `load_should_reject_non_array_action_value`
- `load_should_reject_non_string_array_member`
- `load_should_include_path_for_toml_syntax_error`
- `load_should_include_path_for_non_not_found_read_error`

For the read-error test, create a directory at `.shtodo/config.toml`; reading it as a file must produce `ConfigError::Read` on supported test platforms.

- [ ] **Step 2: Run the focused loader test to verify RED**

```bash
cargo test --locked config::tests::load_should_use_defaults_without_creating_missing_config
```

Expected: compilation fails because the config module types and `load` do not exist.

- [ ] **Step 3: Implement ordered structural parsing and shared validation**

Use these top-level types:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConfigSource {
    Defaults,
    File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConfigDiagnostic {
    pub(crate) order: usize,
    pub(crate) path: String,
    pub(crate) message: String,
}

#[derive(Debug)]
pub(crate) enum ConfigError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Invalid {
        path: PathBuf,
        diagnostics: Vec<ConfigDiagnostic>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LoadedKeymap {
    path: PathBuf,
    source: ConfigSource,
    keymap: Keymap,
}
```

Implement `load` in this order:

```rust
pub(crate) fn load(home: &Path) -> Result<LoadedKeymap, ConfigError> {
    let path = config_path(home);
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(LoadedKeymap {
                path,
                source: ConfigSource::Defaults,
                keymap: Keymap::defaults(),
            });
        }
        Err(source) => return Err(ConfigError::Read { path, source }),
    };
    let table = toml::from_str::<toml::Table>(&source)
        .map_err(|source| ConfigError::Parse { path: path.clone(), source })?;
    let (overrides, mut diagnostics) = parse_table(&table);
    let keymap = match Keymap::with_overrides(&overrides) {
        Ok(keymap) => Some(keymap),
        Err(issues) => {
            diagnostics.extend(issues.into_iter().map(ConfigDiagnostic::from));
            None
        }
    };
    diagnostics.sort_by_key(|diagnostic| diagnostic.order);
    if !diagnostics.is_empty() {
        return Err(ConfigError::Invalid { path, diagnostics });
    }
    let keymap = keymap.ok_or_else(|| ConfigError::Invalid {
        path: path.clone(),
        diagnostics: vec![ConfigDiagnostic {
            order: usize::MAX,
            path: "keybindings".into(),
            message: "validation failed without a diagnostic".into(),
        }],
    })?;
    Ok(LoadedKeymap {
        path,
        source: ConfigSource::File,
        keymap,
    })
}
```

The defensive `ok_or_else` branch is unreachable when `Keymap::with_overrides` obeys its interface, but it preserves Result-based production error handling without `unwrap` or `expect`.

Parse the ordered `toml::Table` manually so unknown fields are diagnostics rather than silently ignored:

1. At root, accept only `keybindings`; diagnose every other key at its root path.
2. If absent, return no overrides. If present and not a table, diagnose `keybindings: expected a table`.
3. Inside `keybindings`, accept only `normal`, `insert`, and `help`; diagnose each other mode.
4. Require each mode value to be a table.
5. Resolve every action with `BindingId::from_config(mode, name)`; diagnose unknown action names.
6. Require action values to be arrays and every member to be a string. Diagnose a non-string member at `keybindings.<mode>.<action>[<index>]`.
7. Assign an incrementing source-order number to every encountered root field, mode, action, and array member. Give each valid `BindingOverride` its action's order. Convert `KeymapIssue.order` directly into `ConfigDiagnostic.order`, then use stable sorting.

Implement `Display` for invalid config as the path plus one indented `<path>: <message>` line per diagnostic. `Read` and `Parse` displays must also contain the path. Implement `Error::source` for Read and Parse and return `None` for Invalid.

- [ ] **Step 4: Run all config tests to verify GREEN**

```bash
cargo test --locked config::tests
cargo fmt --check
```

Expected: missing, valid, structural, semantic, source-order, syntax, and I/O tests pass. `Cargo.lock` contains toml 1.1.4 and its ordered-map dependencies.

- [ ] **Step 5: Commit config loading**

```bash
git add Cargo.toml Cargo.lock src/config.rs src/lib.rs
git commit -m "feat: load and validate keybinding config"
```

---

### Task 3: Add `shtodo doctor` and Fail Fast Before the TUI

**Files:**
- Modify: `src/cli.rs:4-98`
- Modify: `src/config.rs`
- Modify: `src/lib.rs:9-89`
- Modify: `tests/cli.rs:1-152`

**Interfaces:**
- Extends: `cli::Command` with `Doctor`.
- Produces: exact parser form `shtodo doctor`; rejects `shtodo --local doctor` and additional arguments.
- Consumes: `config::load`, `LoadedKeymap`, `ConfigSource`, and `ConfigError` from Task 2.
- Changes interactive orchestration to load config before `Store::open` and call `terminal::run(app, &store, loaded.keymap())`.
- Keeps `add`, Help, and Version independent from config loading.

- [ ] **Step 1: Write failing CLI parser and usage tests**

Add to `src/cli.rs` tests:

```rust
#[test]
fn parse_args_should_accept_only_the_exact_doctor_command() {
    assert_eq!(parse_args(args(&["doctor"])), Ok(Command::Doctor));
    assert!(parse_args(args(&["--local", "doctor"])).is_err());
    assert!(parse_args(args(&["doctor", "extra"])).is_err());
}
```

Extend the existing help test in `tests/cli.rs` to assert:

```rust
assert!(stdout.contains("shtodo doctor"));
assert!(stdout.contains("Validate ~/.shtodo/config.toml"));
```

- [ ] **Step 2: Run parser tests to verify RED**

```bash
cargo test --locked cli::tests::parse_args_should_accept_only_the_exact_doctor_command
```

Expected: compilation fails because `Command::Doctor` does not exist.

- [ ] **Step 3: Implement the doctor command contract**

Add `Doctor` to `Command` and match `[command] if command == "doctor"` before the generic error arm. Add these exact usage lines:

```text
  shtodo doctor

Commands:
  add <TASK>  Add one task without opening the terminal UI.
              When TASK is omitted, read it from standard input.
  doctor      Validate ~/.shtodo/config.toml without opening the terminal UI.
```

Keep `doctor` global because configuration itself is global; do not accept `--local`.

- [ ] **Step 4: Write failing doctor and startup integration tests**

Add helpers to `tests/cli.rs`:

```rust
fn write_config(home: &std::path::Path, source: &str) {
    let directory = home.join(".shtodo");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("config.toml"), source).unwrap();
}

fn run_with_home(home: &std::path::Path, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_shtodo"))
        .args(arguments)
        .env("HOME", home)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}
```

Add these tests:

```rust
#[test]
fn doctor_should_accept_missing_config_without_creating_storage() {
    let home = tempfile::tempdir().unwrap();

    let output = run_with_home(home.path(), &["doctor"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Config:"));
    assert!(stdout.contains("OK: no config file; using defaults"));
    assert!(!home.path().join(".shtodo").exists());
}

#[test]
fn doctor_should_summarize_valid_effective_keymap_without_task_storage() {
    let home = tempfile::tempdir().unwrap();
    write_config(
        home.path(),
        "[keybindings.normal]\nmove_down = [\"x\"]\n",
    );

    let output = run_with_home(home.path(), &["doctor"]);

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout)
        .contains("OK: 24 configurable actions, 32 active bindings"));
    assert!(!home.path().join(".shtodo/global").exists());
    assert!(!home.path().join(".shtodo/projects").exists());
}

#[test]
fn doctor_should_report_all_available_invalid_config_issues() {
    let home = tempfile::tempdir().unwrap();
    write_config(
        home.path(),
        "[keybindings.normal]\nmove_down = [\"dn\"]\nadd_task = [\"d\"]\n",
    );

    let output = run_with_home(home.path(), &["doctor"]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Invalid configuration:"));
    assert!(stderr.contains("keybindings.normal.move_down"));
    assert!(stderr.contains("unknown key \"dn\""));
    assert!(stderr.contains("keybindings.normal.add_task"));
    assert!(stderr.contains("conflicts with delete_task"));
    assert!(!home.path().join(".shtodo/global").exists());
}

#[test]
fn interactive_startup_should_reject_invalid_config_before_creating_task_storage() {
    let home = tempfile::tempdir().unwrap();
    write_config(
        home.path(),
        "[keybindings.normal]\nmove_down = [\"dn\"]\n",
    );

    let output = run_with_home(home.path(), &[]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown key \"dn\""));
    assert!(stderr.contains("Run `shtodo doctor`"));
    assert!(!home.path().join(".shtodo/global").exists());
}

#[test]
fn add_should_remain_available_while_interactive_config_is_invalid() {
    let home = tempfile::tempdir().unwrap();
    write_config(
        home.path(),
        "[keybindings.normal]\nmove_down = [\"dn\"]\n",
    );

    let output = run_with_home(home.path(), &["add", "repair config later"]);

    assert!(output.status.success());
    assert_eq!(stored_task_text(home.path(), "global"), "repair config later");
}
```

- [ ] **Step 5: Run integration tests to verify RED**

```bash
cargo test --locked --test cli doctor_should_accept_missing_config_without_creating_storage
cargo test --locked --test cli interactive_startup_should_reject_invalid_config_before_creating_task_storage
```

Expected: doctor fails because `run()` has no Doctor branch, and interactive startup still reaches task storage or the terminal without loading config.

- [ ] **Step 6: Integrate shared loading into doctor and interactive startup**

Add report methods without duplicating validation:

```rust
impl LoadedKeymap {
    pub(crate) fn doctor_report(&self) -> String {
        let result = match self.source() {
            ConfigSource::Defaults => "OK: no config file; using defaults".to_owned(),
            ConfigSource::File => format!(
                "OK: {} configurable actions, {} active bindings",
                self.keymap().configurable_action_count(),
                self.keymap().active_binding_count()
            ),
        };
        format!("Config: {}\n{result}\n", self.path().display())
    }
}

impl ConfigError {
    pub(crate) fn doctor_report(&self) -> String {
        match self {
            Self::Invalid { path, diagnostics } => {
                let details = diagnostics
                    .iter()
                    .map(|diagnostic| format!("  {}: {}", diagnostic.path, diagnostic.message))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "Config: {}\nInvalid configuration:\n{details}",
                    path.display()
                )
            }
            Self::Read { path, source } => {
                format!("Config: {}\nCould not read configuration: {source}", path.display())
            }
            Self::Parse { path, source } => {
                format!("Config: {}\nInvalid TOML: {source}", path.display())
            }
        }
    }
}
```

Wire `src/lib.rs` with this control flow:

```rust
cli::Command::Doctor => {
    let home = storage::home_from_environment()?;
    match config::load(&home) {
        Ok(loaded) => std::io::stdout()
            .lock()
            .write_all(loaded.doctor_report().as_bytes())?,
        Err(error) => return Err(eyre!("{}", error.doctor_report())),
    }
}
cli::Command::Run(choice) => {
    let home = storage::home_from_environment()?;
    let loaded = config::load(&home)
        .map_err(|error| eyre!("{}\nRun `shtodo doctor` for a focused config check.", error))?;
    let scope = storage::scope_from_environment(choice)?;
    let store = storage::Store::open(&home, scope)?;
    let app = app::App::new(store.load()?);
    terminal::run(app, &store, loaded.keymap())?;
}
```

Do not move config loading above the command match. Help, Version, Add, and Doctor must retain their independent paths. Doctor resolves home and config only. Interactive Run resolves config before scope, store creation, lock acquisition, task loading, or terminal initialization.

- [ ] **Step 7: Run CLI and orchestration tests to verify GREEN**

```bash
cargo test --locked cli::tests
cargo test --locked --test cli
cargo fmt --check
```

Expected: parser, help, doctor, fail-fast, config-bypass Add, and existing task-capture tests all pass.

- [ ] **Step 8: Commit doctor and startup integration**

```bash
git add src/cli.rs src/config.rs src/lib.rs tests/cli.rs
git commit -m "feat: validate keybindings with shtodo doctor"
```

---

### Task 4: Document Configuration and Run Final Gates

**Files:**
- Modify: `README.md:47-157`
- Modify: `CHANGELOG.md:5-15`

**Interfaces:**
- Documents: the exact runtime schema and behavior implemented by Tasks 1 through 3.
- Changes no Rust interface.

- [ ] **Step 1: Update usage and add the configuration guide**

Extend the README usage block to include the existing Add forms and Doctor:

```text
shtodo
shtodo --local
shtodo add "Fix the bug"
shtodo --local add "Run the tests"
shtodo doctor
shtodo --help
shtodo --version
```

After Keyboard Controls, add `## Configuring keybindings` with:

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

State all of these rules explicitly:

- The optional path is `~/.shtodo/config.toml`; shtodo does not create it.
- Missing config uses compiled defaults.
- Configuring an action replaces that action's defaults; omitted actions retain defaults.
- Array order matters: the first key is used in the footer and every key is shown in Help.
- Accepted named keys are `up`, `down`, `left`, `right`, `home`, `end`, `page-up`, `page-down`, `tab`, `backtab`, `enter`, `esc`, `space`, `backspace`, `delete`, and `insert`.
- Ctrl and Alt modifiers use forms such as `ctrl-n`, `alt-left`, and `ctrl-alt-x`; shifted printable characters use the resulting character such as `J`.
- `Ctrl-C` is fixed in all modes and cannot be configured.
- Invalid config stops interactive startup and points to `shtodo doctor`.
- `shtodo doctor` checks the same parser and validator without opening task storage or the TUI.

Add a default-action table using these exact mappings, grouped under Normal, Insert, and Help:

```text
Normal: move_down = j, down; move_up = k, up; move_task_down = J;
move_task_up = K; add_task = i; edit_task = e; toggle_complete = space;
delete_task = d; restore_latest = u; open_help = ?; quit = q.

Insert: move_cursor_left = left; move_cursor_right = right;
move_cursor_start = home; move_cursor_end = end;
move_word_left = alt-left, alt-b; move_word_right = alt-right, alt-f;
delete_before_cursor = backspace; delete_at_cursor = delete;
delete_word_before_cursor = alt-backspace, ctrl-w;
delete_word_at_cursor = alt-delete; commit_edit = enter; cancel_edit = esc.

Help: close_help = ?, esc.
```

Label `Ctrl-C` separately as a fixed emergency quit key in all three modes. Use the exact snake_case names above so README examples remain valid TOML schema documentation.

- [ ] **Step 2: Correct the version-one limits and changelog**

Remove `user-editable keybindings` and `configuration files` from the README's excluded features. Remove the deferred `~/.shtodo/config` keybinding-format sentence. Keep custom themes, trash, sidebar, plugins, search, and other existing limits.

Under `CHANGELOG.md`'s `[Unreleased]`, add:

```markdown
### Added

- User-configured keybindings from `~/.shtodo/config.toml`, reflected in input, footer hints, empty-state guidance, and keyboard help.
- `shtodo doctor` for validating keybinding syntax, reserved keys, and conflicts without opening task storage or the terminal UI.
```

- [ ] **Step 3: Verify documentation terms against code**

Run these searches and inspect every match:

```bash
rg -n "config\.toml|shtodo doctor|Ctrl-C|move_down|close_help|user-editable keybindings|configuration files" README.md CHANGELOG.md src tests
rg -n "~/.shtodo/config($|[^.]|/)|show_in_footer|show_in_help|static BINDINGS" README.md src tests
```

Expected: the first search finds consistent command, path, schema, and reserved-key language. The second finds no stale extensionless config path and no removed static-visibility/table implementation.

- [ ] **Step 4: Run the final verification suite**

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
git diff --check
```

Expected: every command exits zero. Confirm the test output includes input normalization/override tests, config loader/diagnostic tests, Ratatui custom-keymap tests, doctor binary tests, interactive fail-fast coverage, and all pre-existing tests.

- [ ] **Step 5: Commit documentation**

```bash
git add README.md CHANGELOG.md
git commit -m "docs: explain configurable keybindings"
```

- [ ] **Step 6: Record final evidence for review**

In the implementation handoff, report the actual worktree and branch, every commit ID, each task's observed RED and GREEN command output, and the exit result for all four final verification commands. Close with the unchanged exclusions: live reload, config generation, per-project config, key sequences, themes, plugins, configurable UI metadata, and Help scrolling. Do not claim completion from planned or unrun commands.
