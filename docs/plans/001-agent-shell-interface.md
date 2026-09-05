# Plan 001: Add an agent-friendly list and soft-delete shell interface

> **Executor instructions**: Follow this plan step by step and use tests first
> for every behavior change. Run every verification command and confirm the
> expected result before moving to the next step. If a STOP condition occurs,
> stop and report it instead of improvising. When finished, update the status
> row in `docs/plans/README.md` unless a reviewer says they maintain the index.
>
> **Drift check (run first)**:
> `git diff --stat 492e13d..HEAD -- src/cli.rs src/lib.rs src/storage.rs src/task.rs tests/cli.rs README.md docs/usage.md CHANGELOG.md`
>
> If an in-scope file changed since this plan was written, compare the current
> state excerpts below with the live code. Stop if the documented boundaries or
> assumptions no longer hold.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: direction
- **Planned at**: commit `492e13d`, 2026-09-03

## Why this matters

`shtodo add` lets people and agents capture work without opening the TUI, but
there is no corresponding way to inspect or remove a task from a shell. Agents
must launch an interactive terminal or read the private persistence format,
neither of which is a suitable product interface. This plan adds a small,
documented shell contract with stable scope-local IDs, readable task state, and
recoverable deletion while preserving shtodo's deliberately narrow model.

The interface is intentionally not a general task-management API. It adds only
`list` and single-ID `delete`; JSON, filters, search, bulk operations, task
completion, editing, import, and export remain deferred until real agent usage
shows which contracts are worth stabilizing.

## Approved command contract

The implementation must support exactly these new forms:

```text
shtodo list
shtodo --local list
shtodo delete <ID>
shtodo --local delete <ID>
```

Global scope remains the default. `--local` continues to mean the exact
canonical current directory, never a Git root or parent directory.

### `list` output

`list` prints every non-deleted task in canonical order. Each task occupies one
line with two ASCII spaces between fields:

```text
1  open  Fix the bug
2  done  Run the tests
```

The fields are:

1. The persisted positive numeric task ID.
2. The lowercase state `open` or `done`.
3. The stored task text.

There is no heading, summary, color, or terminal-dependent formatting. An empty
or never-created list produces empty stdout and exits successfully. The text
format is a documented display contract for this release, but it is not a
serialization format or a promise that future releases will never add a
separate structured-output option.

`list` must not load keybinding configuration, create `~/.shtodo`, create a
scope directory, or create a lock file. It may run while the TUI or another
writer holds the selected scope lock. Because writers replace snapshots
atomically, it reads either the complete prior snapshot or the complete new
snapshot. Malformed, unsupported-version, invalid, or wrong-scope snapshots
remain errors and must not be changed.

### `delete` behavior

`delete` accepts exactly one positive decimal task ID. IDs are local to the
selected scope. The same number in global and local scopes may identify
different tasks.

For an active task:

```text
Deleted 3: Fix the bug
```

The command must use the existing tombstone and deletion-sequence behavior,
save through `Store::save`, and exit successfully. The TUI's existing `u`
action must be able to restore the task later.

For an already-deleted task:

```text
Already deleted 3: Fix the bug
```

This is an idempotent success. It must not increment the deletion sequence or
rewrite `tasks.json`.

An ID that never existed in the selected scope fails with nonzero status and a
stderr message containing `task 3 was not found`. Missing IDs, zero, negative
numbers, nonnumeric values, and extra arguments are usage errors. A scope held
by another writer fails through the existing `Store::open` lock error. No
successful or failed `delete` invocation permanently removes a task.

Both new commands bypass `~/.shtodo/config.toml`, matching the existing
non-interactive `add` command. Invalid keybinding configuration must not prevent
an agent from listing or deleting tasks.

## Current state

### Relevant files

- `src/cli.rs`: handwritten argument parser, command enum, and static help.
- `src/lib.rs`: top-level dispatch plus the existing non-interactive `add`
  handler.
- `src/task.rs`: stable `TaskId`, task getters, canonical ordering, tombstones,
  and latest-delete restoration.
- `src/storage.rs`: scope resolution, path construction, validated snapshot
  loading, exclusive writer lock, and atomic save.
- `tests/cli.rs`: process-level command tests with isolated temporary homes.
- `README.md`: quick-start command surface.
- `docs/usage.md`: detailed command, scope, storage, and limits documentation.
- `CHANGELOG.md`: release-facing record of user-visible changes.

### Existing CLI shape

At `src/cli.rs:9-16`, the command model has no inspection or ID-based mutation:

```rust
pub(crate) enum Command {
    Run(ScopeChoice),
    Add(ScopeChoice, Option<OsString>),
    Doctor,
    Help,
    Version,
}
```

At `src/cli.rs:40-60`, the parser matches complete argument slices. Preserve
this explicit style rather than adding a CLI framework. At `src/cli.rs:63-95`,
help is built with `concat!`; retain that form because significant indentation
has regressed with continued string literals before.

At `src/lib.rs:24-60`, `run()` dispatches non-interactive commands before TUI
initialization. `add` does not load keybinding configuration:

```rust
cli::Command::Add(choice, argument) => {
    add_task(choice, argument)?;
}
```

At `src/lib.rs:64-83`, `add_task` establishes the mutation pattern to follow:

```rust
let home = storage::home_from_environment()?;
let scope = storage::scope_from_environment(choice)?;
let store = storage::Store::open(&home, scope)?;
let mut tasks = store.load()?;
tasks.add(&text)?;
store.save(&tasks)?;
```

### Existing task model

At `src/task.rs:7-15`, `TaskId` is a persisted `u64` with a getter but no
constructor for a parsed shell value:

```rust
pub(crate) struct TaskId(u64);

impl TaskId {
    pub(crate) fn get(self) -> u64 {
        self.0
    }
}
```

At `src/task.rs:33-48`, task text, completion, and tombstone state already have
crate-visible getters. At `src/task.rs:112-120`, `visible_tasks()` excludes
tombstones and preserves canonical vector order, while `task(id)` can find both
active and deleted tasks. At `src/task.rs:162-190`, `delete` assigns the next
deletion sequence and `restore_latest` clears the latest tombstone.

Do not change the persistence schema or ID allocation. Add only the smallest
typed conversion needed to turn a validated positive CLI integer into a
`TaskId`.

### Existing storage boundaries

At `src/storage.rs:36-71`, `Store::open` creates the scope directory and lock
file, then acquires an exclusive writer lock. This remains mandatory for
`delete`.

At `src/storage.rs:227-247`, `paths_for_home` computes all storage paths without
creating them. At `src/storage.rs:249-280`, `load_snapshot` returns an empty
`TaskList` for a missing file and otherwise validates schema version, JSON,
list invariants, and scope. These functions already provide the correct
building blocks for a side-effect-free read path.

At `src/storage.rs:86-145`, saving validates the list and scope, writes and
syncs a sibling temporary file, renames it over the canonical snapshot, and
syncs the directory on Unix. Do not create a second persistence path for CLI
deletion.

### Repository conventions

- Rust 2024 with minimum Rust 1.89.
- Small flat modules and explicit concrete types.
- `Result` errors gain path and operation context through `color-eyre`.
- No production `unwrap` or `expect`.
- No async runtime, threads, database, CLI framework, or new dependency for
  this feature.
- Unit tests live beside their module. Process behavior lives in
  `tests/cli.rs` and uses `tempfile` homes.
- Commit messages use Conventional Commit style, for example
  `feat: add tasks from the command line` and `docs: add poop emoji to readme`.

## Commands you will need

| Purpose | Command | Expected on success |
| --- | --- | --- |
| Parser tests | `cargo test --locked cli::tests` | all matching unit tests pass |
| Task tests | `cargo test --locked task::tests` | all matching unit tests pass |
| Storage tests | `cargo test --locked storage::tests` | all matching unit tests pass |
| CLI integration | `cargo test --locked --test cli` | all CLI integration tests pass |
| Full tests | `cargo test --locked` | all unit, integration, and doc tests pass |
| Formatting | `cargo fmt --check` | exit 0 with no diff |
| Lint | `cargo clippy --all-targets --all-features --locked -- -D warnings` | exit 0 with no warnings |
| Release build | `cargo build --release --locked` | exit 0 and `target/release/shtodo` exists |
| Patch hygiene | `git diff --check` | exit 0 with no output |

The baseline at commit `492e13d` passes all commands above. `cargo test
--locked` reports 113 library tests and 12 CLI integration tests, 125 total.

## Scope

**In scope**:

- `src/cli.rs`
- `src/lib.rs`
- `src/storage.rs`
- `src/task.rs`
- `tests/cli.rs`
- `README.md`
- `docs/usage.md`
- `CHANGELOG.md`
- `docs/plans/README.md`, status update only after implementation

**Out of scope**:

- `src/app.rs`, `src/action.rs`, `src/input.rs`, `src/terminal.rs`, and
  `src/ui.rs`; TUI behavior and keybindings do not change.
- `src/config.rs`; shell commands do not consume interactive keybinding config.
- `Cargo.toml` and `Cargo.lock`; add no dependencies and do not bump the
  version.
- JSON, TSV, CSV, or other machine serialization.
- `--open`, `--done`, search, pagination, counts, headings, or list filters.
- Bulk deletion or more than one ID per invocation.
- CLI completion, reopening, editing, restoration, permanent deletion, trash,
  import, or export.
- Changes to exact-current-directory identity or project folder naming.
- Publishing, tagging, pushing, release creation, or package-manager work.

## Git workflow

- Do not implement directly on `main`. Create an isolated worktree on branch
  `feat/agent-shell-interface` from the current `origin/main` after confirming
  the drift check.
- Use test-first RED/GREEN commits or small logical commits. Never commit a
  deliberately failing intermediate state.
- Suggested commit sequence:
  1. `feat: add task listing commands`
  2. `feat: delete tasks from the command line`
  3. `docs: document the agent shell interface`
- Do not push, open a pull request, merge, publish, or release unless the
  operator explicitly authorizes that action.

## Steps

### Step 1: Extend the typed CLI contract and help

In `src/cli.rs`, first add parser tests for:

- `list` and `--local list`.
- `delete 3` and `--local delete 3`.
- Missing delete ID.
- Zero, negative, nonnumeric, and non-UTF-8 delete IDs.
- Extra delete arguments and misplaced `--local`.
- Existing commands continuing to parse exactly as before.

Run the focused test and capture RED evidence because `Command::List` and
`Command::Delete` do not exist yet.

Then add:

```rust
List(ScopeChoice),
Delete(ScopeChoice, u64),
```

Keep raw numeric parsing at the CLI boundary. Accept only a nonzero decimal
`u64`; no signs, hexadecimal, whitespace normalization, or multiple IDs.
Replace the one-shape `CliError` only as much as needed to distinguish:

- Unsupported argument combinations.
- Missing task ID.
- Invalid task ID, including the offending value and `expected a positive
  integer`.

Every usage error must include the full help text. Extend `usage()` with the
four new invocation forms, concise command descriptions, and examples. Keep
`concat!` and its tested significant whitespace.

**Verify**: `cargo test --locked cli::tests` exits 0 and all old plus new parser
tests pass.

### Step 2: Add a side-effect-free validated snapshot reader

In `src/storage.rs`, first add tests proving a new read-only entry point:

- Returns an empty list for a missing snapshot without creating `.shtodo`.
- Reads a valid saved snapshot while a `Store` still holds the scope's
  exclusive lock.
- Preserves existing errors for malformed, future-version, invalid, and
  wrong-scope data.
- Resolves global and exact local paths through the existing path functions.

Run the focused storage test and capture RED evidence before adding the API.

Add a small function such as:

```rust
pub(crate) fn load_read_only(home: &Path, scope: &ListScope) -> Result<TaskList>
```

It must call `paths_for_home` and `load_snapshot` directly. It must not call
`Store::open`, `create_dir_all`, or `OpenOptions`, and it must not inspect the
temporary snapshot. Do not weaken `Store` locking or alter the writer path.

**Verify**: `cargo test --locked storage::tests` exits 0 and the new no-create
and lock-bypass tests pass.

### Step 3: Implement exact plain-text listing through process tests

In `tests/cli.rs`, first add process-level tests covering:

- A never-created global list: exit 0, empty stdout/stderr, and no `.shtodo`
  directory created.
- Active open and completed tasks: exact lines `<ID>  <STATE>  <TEXT>\n` in
  canonical order.
- Tombstones omitted without renumbering remaining IDs.
- Global and local lists remain isolated, with `--local` using the test
  process's exact current directory.
- A malformed or wrong-scope snapshot fails without modifying its bytes.
- An invalid `config.toml` does not affect `list`.

Use the existing `run_with_home`, `write_config`, and JSON snapshot helpers as
patterns. It is acceptable for tests to edit a valid snapshot fixture to mark
a task completed because no CLI completion command exists. Assert exact stdout
where the output contract matters.

Run `cargo test --locked --test cli` and capture RED evidence before wiring the
handler.

In `src/lib.rs`, add `Command::List` dispatch before the interactive run path.
Resolve home and scope, call the new side-effect-free loader, and write each
visible task directly to locked stdout. Use the task's persisted ID and getters:

```text
<id><two spaces><open|done><two spaces><text><newline>
```

Do not load config, initialize Ratatui, detect whether stdout is a terminal,
sort tasks, renumber IDs, or print a success summary.

**Verify**: `cargo test --locked --test cli` exits 0 and every list contract
test passes.

### Step 4: Implement safe, idempotent single-task deletion

In `src/task.rs`, first add a focused test for converting a positive shell
integer into `TaskId` while rejecting zero. Add the smallest crate-visible
constructor or conversion needed by `src/lib.rs`. Do not expose TaskId publicly
outside the crate and do not change serialization.

In `tests/cli.rs`, then add RED process tests for:

- Deleting an active global task prints the exact confirmation, persists a
  tombstone, and leaves other tasks unchanged.
- Repeating deletion for that ID prints `Already deleted ...`, exits 0, and
  leaves the complete `tasks.json` bytes unchanged.
- An unknown positive ID fails nonzero with `task <ID> was not found`, writes
  no stdout, and leaves an existing snapshot unchanged.
- Invalid IDs fail during parsing before storage is created or touched.
- `--local delete <ID>` changes only the exact-directory local scope.
- A task deleted by the CLI retains a deletion sequence and can be restored by
  the existing `restore_latest` model operation after a storage round trip.
- Invalid keybinding config does not block deletion.

Implement `Command::Delete` dispatch in `src/lib.rs` using the existing writer
path:

1. Resolve home and scope.
2. Open `Store` to acquire the exclusive lock.
3. Load the validated `TaskList`.
4. Convert the validated raw ID to `TaskId`.
5. Look up the task with `TaskList::task`, which includes tombstones.
6. Copy the task text for the confirmation before mutating the list.
7. If it is already deleted, print the idempotent confirmation and return
   without calling `save`.
8. Otherwise call the existing `TaskList::delete`, save through `Store::save`,
   and only then print the successful confirmation.

The output must never claim deletion before persistence succeeds. Do not add a
second delete algorithm or manually mutate `deletion_sequence` in `src/lib.rs`.
The existing `Store::open` lock test and the direct use of `Store::open` in the
handler provide the lock-failure contract; do not add a fragile PTY test solely
to exercise it.

**Verify**:

- `cargo test --locked task::tests` exits 0.
- `cargo test --locked --test cli` exits 0 and all delete contract tests pass.
- `cargo test --locked storage::tests::latest_delete_should_restore_after_storage_round_trip`
  exits 0.

### Step 5: Document the shell interface and its stability boundary

Update documentation only after behavior tests pass:

- `README.md`: add `list` and `delete` to Quick start and explain in one short
  paragraph that IDs are scope-local and deletion is recoverable.
- `docs/usage.md`: document exact invocations, row format, empty-list behavior,
  both scopes, idempotent delete responses, errors, and the fact that list is a
  side-effect-free concurrent reader.
- `docs/usage.md`: remove list/delete from any limitation statement while
  retaining JSON, filters, search, bulk commands, and permanent deletion as
  explicit non-goals.
- `CHANGELOG.md`: add concise `Unreleased` entries for plain-text listing and
  ID-based recoverable deletion.
- `src/cli.rs`: ensure `--help` descriptions and examples agree exactly with
  the detailed documentation.

Do not describe the plain-text output as JSON-safe, shell-escaped, or a stable
serialization format. Do not document future commands as though they exist.

**Verify**:

- `cargo test --locked cli::tests` exits 0.
- `cargo test --locked --test cli help_should_exit_successfully_without_starting_tui`
  exits 0.
- `rg -n "shtodo (list|delete)|--local (list|delete)" README.md docs/usage.md src/cli.rs`
  shows all four supported forms in the detailed help/docs and no unsupported
  forms.

### Step 6: Run the complete release-quality gate and review scope

Run the complete commands from the table without weakening flags or excluding
platform-neutral tests. Review the final diff for accidental TUI, schema,
dependency, version, generated-workflow, or release changes.

Perform a temporary-home smoke check with the release binary:

1. Add two global tasks and one exact-directory local task.
2. Run global and local `list`; verify IDs, states, text, and isolation.
3. Delete one global ID twice; verify the first and idempotent messages.
4. Run global `list`; verify the tombstone is absent and remaining IDs are not
   renumbered.
5. Confirm no JSON or filtering flag is accepted.

**Verify**:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo build --release --locked
git diff --check
git status --short
```

Expected: every command exits 0; all tests pass; the release binary exists;
`git diff --check` prints nothing; `git status --short` lists only the approved
in-scope implementation, documentation, and plan-status files.

## Test plan summary

### `src/cli.rs`

- Parse global/local list.
- Parse one positive delete ID.
- Reject missing, zero, signed, nonnumeric, non-UTF-8, extra, and misplaced
  arguments with actionable usage.
- Preserve every existing command form.

### `src/storage.rs`

- Read missing data without filesystem creation.
- Read a validated snapshot while the writer lock is held.
- Preserve validation and scope errors.

### `src/task.rs`

- Construct only nonzero `TaskId` values from the CLI boundary.
- Preserve existing deletion sequence and restoration behavior.

### `tests/cli.rs`

- Exact list output and empty output.
- Open/done states, canonical order, tombstone exclusion, and stable IDs.
- Global/local isolation and no list side effects.
- Active, repeated, unknown, invalid, and local deletion.
- No snapshot rewrite for idempotent or failed deletion.
- Config independence and snapshot validation.

## Done criteria

All of the following must hold:

- [ ] The four approved command forms appear in `shtodo --help` and work.
- [ ] `list` prints exact ID/state/text rows for all non-deleted tasks in
      canonical order.
- [ ] An empty `list` prints nothing and creates no filesystem entries.
- [ ] `list` succeeds while the selected scope's writer lock is held.
- [ ] Active delete persists one tombstone and prints only after a successful
      save.
- [ ] Repeated delete is a no-write success with the approved message.
- [ ] Unknown and invalid IDs fail without changing an existing snapshot.
- [ ] CLI-deleted tasks remain restorable through existing model semantics.
- [ ] Invalid keybinding config does not block list or delete.
- [ ] No persistence schema, dependencies, version, TUI behavior, or release
      configuration changed.
- [ ] `cargo fmt --check` passes.
- [ ] Strict Clippy passes with warnings denied.
- [ ] `cargo test --locked` passes with all new tests.
- [ ] `cargo build --release --locked` passes.
- [ ] `git diff --check` passes.
- [ ] Only in-scope files are modified.
- [ ] `docs/plans/README.md` marks Plan 001 DONE only after every criterion passes.

## STOP conditions

Stop and report instead of improvising if:

- Any in-scope source file has drifted from the documented command, storage,
  task, or test boundaries.
- Correct concurrent reads require weakening writer locking or replacing the
  atomic save design.
- Stable IDs cannot be exposed without a persistence migration.
- Idempotent deletion cannot be implemented without changing TUI delete or
  restore behavior.
- Correct behavior requires a new dependency, async runtime, background
  process, or database.
- The implementation requires changes to an out-of-scope file.
- A focused or full verification command fails twice after a reasonable fix
  attempt.
- Cross-platform behavior contradicts the primary macOS/Linux contract and
  cannot be isolated without broadening the feature.

## Maintenance notes

- Reviewers should scrutinize the difference between the read-only path and
  `Store::open`: `list` must not create or lock, while `delete` must lock and
  save atomically.
- Task IDs are stable only within one scope. Future agent tooling must carry
  global/local scope alongside any ID it retains.
- The plain-text format is deliberately small and readable. When structured
  output is added, design and version that contract separately rather than
  exposing the on-disk snapshot.
- Future completion, edit, restore, or bulk commands should reuse the same
  typed ID parsing and writer-lock boundary, but they are not authorized by
  this plan.
- A future list filter must preserve canonical ordering and must not silently
  change the current all-active-tasks default.
