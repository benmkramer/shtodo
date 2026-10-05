# Browsable TUI trash and selective restoration

Status: DONE

This feature adds a Trash mode over existing tombstones. It preserves schema
version 1, canonical ordering, scope-local IDs, exact-directory scope identity,
and the existing writer lock and atomic durable save path. It adds no shell
commands, search, permanent deletion, bulk restoration, or automatic purging.

## Behavior

- Normal `t` opens trash, ordered by descending `deletion_sequence`, with the
  newest deletion selected. Rows show ID, open/done state, and task text.
- Trash `j`/`k` and arrows select older/newer deletions without wrapping.
- Trash `r` restores only the selected task and saves through the existing
  `Transition::Persisted` handling. Trash remains open, selecting the next older
  deletion or, at the end, the preceding newer deletion.
- Empty trash has no selection and displays `Trash is empty`. Restore in empty
  trash reports `Nothing to restore` and does not save.
- Trash `t` or Escape returns to Normal, selecting the last restored task or
  preserving the previous Normal selection if nothing was restored. Each new
  visit starts at the newest remaining deletion.
- Help opened from trash displays Trash bindings and returns to that selection.
  Normal add/edit/complete/delete/reorder/restore-latest actions are ignored in
  trash. Normal `u` continues to restore the latest remaining deletion across
  restarts.
- `[keybindings.normal].open_trash` and the independent `[keybindings.trash]`
  actions drive input, contextual help, and footer hints. Ctrl-C remains fixed.
- For upgrade compatibility, an implicit Normal `open_trash` uses `Ctrl-t`
  when an existing action owns `t`. If both are taken, that opening binding is
  omitted and Doctor explains how to configure it. Explicit overrides still
  undergo normal conflict validation. This exception applies only to the new
  implicit opening action, preserving previous validation for all other keys.

## Shared restoration contract

The shell lifecycle branch may independently add this method in `src/task.rs`:

```rust
pub(crate) fn restore(&mut self, id: TaskId) -> Result<bool, ListError>
```

It returns `true` after clearing a tombstone's `deletion_sequence`, `false` for
an already live task, and `ListError::TaskNotFound(id)` for an unknown ID. It
preserves text, completion, ID, canonical position, and both counters. Keep one
copy when integrating the branches. `restore_latest` is unchanged.

`TaskList::deleted_tasks()` returns references ordered newest first, leaving the
underlying canonical vector untouched. A shell trash reader can reuse it.

## Integration overlaps

- Search/filtering also touches `app`, `action`, `input`, `config`, `ui`, and
  usage/configuration docs. Trash uses its own `TrashState` selection and
  `render_trash_content`; the normal `visible_tasks()` projection is unchanged.
  `/` and `f` remain available for the search/filter branch. Preserve the active
  normal filter/query when opening and closing trash. Integration must reconcile
  normal selection if the last restored task falls outside that projection;
  trash itself lists all tombstones in the scope.
- The keymap now locates definitions by BindingId rather than hard-coded table
  offsets, so added search actions do not require renumbering those offsets.
  Resolve conflicts in BindingId/Action/Mode matches and recompute keymap counts
  in config, input, and CLI doctor tests after combining branches.
- Shell lifecycle overlaps the `restore(id)` domain method, docs, and changelog.
  Default plain-text shell listing and shell argument parsing are unchanged.
- Concurrency planning may describe storage coordination. This feature uses
  the current Store API unchanged and does not change concurrent-writer behavior.
- This record and `docs/plans/README.md` may overlap other planning work. Merge
  their index entries without replacing independently owned plans.

## Verification

Regression coverage includes deletion-order enumeration and re-deletion,
idempotent/unknown-ID restoration, full task/counter preservation, neighboring
selection, empty trash, returning to Normal, inactive mutations, help return,
custom keymaps and validation, displayed IDs/states, narrow footer hints,
scrolling, and a temporary-home persistence round trip followed by Normal `u`.

All required gates passed:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features --locked -- -D warnings`
- `cargo test --locked`: 146 unit tests and 25 CLI tests, including 22 new
  behavior/regression tests over the 125-unit/24-CLI baseline.
- `cargo build --release --locked`
- `git diff --check`

Fifteen release-binary smoke checks passed using pseudo-terminals, temporary
homes, and exact-directory fixtures. These exercised shell-deleted tasks,
completion preservation, newest-first trash rows, inactive normal mutation
keys, selective durable restoration, contextual help, quit/relaunch with Normal
`u`, unchanged plain-text shell listing, absence of sibling temporary snapshots,
custom Trash bindings, and global/parent/child scope isolation.

Upgrade regressions additionally cover an existing `toggle_complete = ["t"]`
config, preserving both legacy task bindings, explicit `open_trash` conflicts,
the resolved fallback in footer/help/empty-state hints, and Doctor accepting
these existing configs without creating task storage. Ten additional
release-binary upgrade smoke checks passed in a temporary home: both existing
configs start, legacy keys still complete tasks, the `Ctrl-t` fallback opens
trash and allows restoration, and Doctor explains the fully occupied case.

No standalone feature decisions remain open. The filtered-selection integration
case above must be resolved when combining this branch with search/filtering.
