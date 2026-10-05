# Implementation Plans

Execute plans in the order below unless their dependency notes say otherwise.
Each executor must read the complete plan, honor its STOP conditions, use the
specified verification gates, and update its status only after completion.

## Execution order and status

| Plan | Title | Priority | Effort | Depends on | Status |
| --- | --- | --- | --- | --- | --- |
| [001](./001-agent-shell-interface.md) | Add an agent-friendly list and soft-delete shell interface | P1 | M | none | DONE |
| [004](./004-browsable-trash.md) | Add browsable TUI trash and selective restoration | P1 | M | 001 | DONE |
| [Concurrent usage](./concurrent-usage.md) | Short storage transactions and safe TUI refresh | P1 | L | Plan 001; see integration order | DONE |

Status values: TODO, IN PROGRESS, DONE, BLOCKED with a short reason, or REJECTED
with a short rationale.

## Dependency notes

- Plan 001 has no dependencies.
- Feature 004 uses the existing persisted tombstones and scope-local IDs. Its
  record includes the shared restoration contract and parallel integration notes.
- Concurrent usage was approved for this checkout's existing CLI and TUI.
  Short transactions, snapshot/token consistency, retained drafts, and sync-only
  recovery are implemented and verified locally, with separate measurements.
  The latest `origin/main` shell lifecycle, search/filter, and trash features
  are integrated with the same transactions and reconciliation, including
  guarded selective restoration and projected reorder.

## Completed historical plans

- [Shtodo version-one implementation](./2026-09-01-shtodo-v1.md)
- [User-configured keybindings](./2026-09-01-user-configured-keybindings.md)

## Parallel integration notes

- The shell lifecycle and browsable-trash branches share
  `pub(crate) fn restore(&mut self, id: TaskId) -> Result<bool, ListError>` in
  `src/task.rs`. Keep one implementation when combining them: clear only
  `deletion_sequence`, return `true` for a restored tombstone, `false` for an
  already-live task, and `TaskNotFound` for an unknown ID. Preserve text,
  completion, ID, canonical position, and existing `restore_latest` behavior.
- Shell lifecycle adds `set_completed` and changes `TaskList::edit` to return
  whether the live task changed. Existing TUI callers may discard the return
  value. Every shell handler now uses the short transaction boundary and saves
  only changes. TUI restoration and projected reorder use that same boundary.
- Reconcile command dispatch, usage text, CLI tests, and shared documentation
  with the search/filtering and trash branches during integration. Historical
  benchmarks are retained. The [October 5 paired comparison](../benchmarks/concurrent-usage-comparison.md)
  now measures the combined code and shell lifecycle commands against beta.3.

## Findings considered and rejected

- JSON output is deferred because the agent-facing contract needs real usage
  before shtodo commits to a structured schema.
- Shell list filters and search are deferred because agents can filter the
  explicit `open` and `done` rows themselves. Transient TUI search and status
  views are implemented separately and leave that shell contract unchanged.
- Bulk deletion is deferred to keep retries and partial-failure behavior
  unambiguous.
- Permanent deletion is rejected for this slice because existing recoverable
  tombstones are safer and already integrated with TUI restoration.
