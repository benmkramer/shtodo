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

## Findings considered and rejected

- JSON output is deferred because the agent-facing contract needs real usage
  before shtodo commits to a structured schema.
- List filters and search are deferred because agents can filter the initial
  explicit `open` and `done` rows themselves.
- Bulk deletion is deferred to keep retries and partial-failure behavior
  unambiguous.
- Permanent deletion is rejected for this slice because existing recoverable
  tombstones are safer and already integrated with TUI restoration.
