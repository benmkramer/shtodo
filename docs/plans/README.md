# Implementation Plans

Execute plans in the order below unless their dependency notes say otherwise.
Each executor must read the complete plan, honor its STOP conditions, use the
specified verification gates, and update its status only after completion.

## Execution order and status

| Plan | Title | Priority | Effort | Depends on | Status |
| --- | --- | --- | --- | --- | --- |
| [001](./001-agent-shell-interface.md) | Add an agent-friendly list and soft-delete shell interface | P1 | M | none | DONE |

Status values: TODO, IN PROGRESS, DONE, BLOCKED with a short reason, or REJECTED
with a short rationale.

## Dependency notes

- Plan 001 has no dependencies.

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
  value. Shell handlers save only changes and keep the current lock architecture.
- Reconcile command dispatch, usage text, CLI tests, and shared documentation
  with the search/filtering and trash branches during integration. Benchmark
  timings remain historical; the added shell lifecycle commands are not timed.

## Findings considered and rejected

- JSON output is deferred because the agent-facing contract needs real usage
  before shtodo commits to a structured schema.
- List filters and search are deferred because agents can filter the initial
  explicit `open` and `done` rows themselves.
- Bulk deletion is deferred to keep retries and partial-failure behavior
  unambiguous.
- Permanent deletion is rejected for this slice because existing recoverable
  tombstones are safer and already integrated with TUI restoration.
