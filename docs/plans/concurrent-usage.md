# Concurrent usage: transactions and TUI refresh

Status: DONE. The approved baseline and integration with `origin/main` at
`707b0ec` are implemented and verified locally. The original release-build
measurements are retained; Windows/Linux CI verification remains separate work.
Inspected baseline: `74d049e7944547126d81a902c8f2ff1852d1643d`, October 4, 2026.
The evidence table below describes that baseline before implementation.
Planning branch: `worktree/brave-forest-b998`; existing feature branch:
`feat/concurrent-usage`. Priority: P1. Estimated effort: L, in stages.

This plan is self-contained; a separate spec would duplicate its contracts.
The user approved implementation on October 4, 2026 after the planning update.
The initial scope was add/list/delete and the Normal/Insert/Help TUI. The user
subsequently authorized updating from `origin/main` and resolving conflicts,
bringing its search/filter, shell lifecycle, and trash behavior under the
contracts below. The user authorized committing and opening a PR on October 5,
2026. Stay in the assigned worktree and publish this implementation for review.
The baseline and integration records distinguish the two validation rounds.

## Recommendation

Replace the session-long writer lock with short, synchronous, per-scope
read-modify-write transactions. Task-level actions name stable IDs; Add and
RestoreLatest resolve their IDs under the lock. Apply each action to a freshly
validated snapshot while holding `tasks.lock`. The TUI's
snapshot is a display cache and must never become the input to a storage save.
Finish the user action only after persistence succeeds; a replaced snapshot
with unconfirmed durability enters the recovery state defined below.

Use one deadline-driven refresh per second, reading and comparing canonical
snapshot bytes. Parse only changed bytes and redraw only changed display state.
Keep schema version 1, the current paths, atomic replacement, and Unix directory
sync. Use value-based, field-specific conflict checks for interactive edits,
guarded deletion, and adjacent reordering. A persisted revision, daemon,
database, async runtime, or filesystem watcher is not needed for the first
implementation.

This allows scripts and two TUIs to share a list without overwriting unrelated
changes. It does not provide historical conflict detection or exactly-once
execution after a process loses its response. The refresh strategy has a real
cost of one snapshot read per second per idle TUI. The implementation evidence
below records that cost separately from historical CLI benchmarks.

## Evidence from the inspected baseline

| Boundary | Verified behavior | Consequence |
| --- | --- | --- |
| [`src/storage.rs`](../../src/storage.rs), `Store::open` | Creates the scope directory, opens `tasks.lock`, calls `File::try_lock`, and stores the handle in `_lock` | Lock ownership lasts as long as `Store`; a second writer fails immediately |
| [`src/lib.rs`](../../src/lib.rs), interactive dispatch | Creates one `Store`, loads one `TaskList`, and passes both to `terminal::run` | An open TUI owns the lock for its whole session |
| [`src/lib.rs`](../../src/lib.rs), add/delete | Loads, mutates, and saves under that same writer lock; success output follows save | Safe shell mutations today, but blocked by an open TUI |
| [`src/app.rs`](../../src/app.rs), `App::apply` | Mutates its owned list before returning `Transition::Persisted`; edit commit clears the editor | Releasing the lock alone permits stale whole-list replacement and loses drafts on save failure |
| [`src/terminal.rs`](../../src/terminal.rs), `run` | Saves `app.tasks()` on `Persisted`, draws at every loop entry, blocks in `event::read` when idle | Needs mutation intent/result handling, refresh deadlines, and explicit redraw state |
| [`src/task.rs`](../../src/task.rs) | Schema 1 rejects unknown fields; IDs and deletion sequences use checked monotonic counters; canonical vector retains tombstones | Adding a revision field breaks old readers; current counters and ordering must survive transactions |
| Task mutation helpers | `edit` and `toggle_complete` find tombstones too; `delete` requires a live task; `restore_latest` chooses the largest deletion sequence | The shared mutation boundary must enforce lifecycle eligibility rather than rely on these helpers alone |
| App/editor/UI | Selection already uses `TaskId`; editor stores target ID, buffer, and UTF-8 byte cursor; UI renders an edit only in its live row | Preserve identity and buffers; render a detached draft if its row disappears |
| [`tests/cli.rs`](../../tests/cli.rs) | Isolated homes; exact list/delete output; idempotent delete does not rewrite; config-independent shell commands | Retain these contracts and extend them with real concurrent processes |
| [`docs/benchmarks.md`](../benchmarks.md) | Existing numbers measure whole CLI invocations with single-process synthetic data, including full-snapshot persistence | They support retaining the simple store, but establish neither concurrent throughput nor idle refresh cost |

Read alongside [usage](../usage.md), [configuration](../configuration.md),
[README](../../README.md), and the [plan index](./README.md). No project or
ancestor `AGENTS.md` was found during inspection; the supplied session
instructions still apply.

## Alternatives and why this one is smallest

| Approach | Benefits | Costs and failure modes | Decision |
| --- | --- | --- | --- |
| Short locked transactions plus refresh and checks | Reuses the current lock, validated model, and save format; serializes independent processes; no service lifecycle | Each write parses/serializes the full list; callers need explicit intents and conflict handling | Recommend |
| Single-writer coordinator with local IPC | Can push refresh events and cache the canonical list; one serialization point | Requires startup/election, socket naming and permissions, protocol/versioning, per-scope routing, crash/reconnection handling, acknowledgement/durability rules, and mixed-version fallback | Defer until measurements show transaction contention or snapshot costs justify it |
| A writer thread inside each TUI | Can move disk work off its rendering thread | Does not serialize separate TUIs or CLI processes; still needs the file transaction protocol | Unnecessary initially |
| Release lock and save App snapshots | Very small change | Overwrites additions, edits, deletion sequences, and order from other writers | Reject |
| Whole-snapshot revision check and reject every stale write | Avoids lost updates if checked under a lock | Unrelated actions conflict unnecessarily; retry still requires reapplying an intent | Use per-action expectations instead |
| Revision check without a lock | Appears to avoid lock waiting | Check and replacement race; two writers can both pass | Reject |
| Refresh only on input/manual refresh | No idle storage polling | Does not show shell changes in an idle TUI | Does not satisfy the proposed behavior |
| Native directory watcher | Avoids regular file reads and can notify quickly | New dependency and event/lifetime coordination; replacement events must be coalesced; dropped/unsupported events still need reconciliation | Revisit if the measured polling cost or requested freshness warrants it |

The OS lock is already the single-writer mechanism during each transaction.
Adding a coordinator moves that mechanism into a service without evidence of a
current need. JSON writes remain O(total live tasks plus tombstones); do not
infer a throughput ceiling from the existing command benchmarks.

## Storage and transaction contract

Separate a lightweight `Store` containing resolved paths and scope from a
private `LockedStore`/transaction guard containing the lock handle. Constructing
a handle or reading a snapshot acquires no writer lock. Create storage
directories only for writes allowed to initialize a scope, and create the lock
file when a write or sync recovery needs it.

The sole production mutation entry point should resemble
`Store::mutate(&MutationRequest, LockBudget) -> Result<MutationReply, TransactionError>`.
Names are provisional. Its sequence is mandatory:

1. Validate request syntax/text before creating storage when possible. Parse
   shell arguments and read stdin before waiting for a writer lock.
2. Open the stable `tasks.lock` file, creating it if absent, and acquire its
   exclusive lock within the caller's acquisition budget.
3. Load and validate the latest canonical `tasks.json`, including schema,
   scope, text, IDs, and deletion-sequence invariants.
4. Evaluate the request's eligibility and expectations against that list.
   A conflict or domain error writes nothing and consumes no IDs/sequences.
5. Apply exactly one domain action to that owned latest list. Allocate IDs and
   deletion sequences here, never in `App` before the lock is acquired.
6. If the action is a no-op, return the latest snapshot and outcome without
   touching the temporary or canonical snapshot. Missing-list no-ops do not
   create `tasks.json`.
7. Otherwise validate and serialize the result as today, write the sibling
   `tasks.json.tmp`, sync the file, close its handle, atomically replace
   `tasks.json`, then sync the scope directory on Unix.
8. Return the resulting snapshot, action outcome, and any created/restored ID.
   Drop the lock before rendering, printing confirmations, or reading input.

Make the whole-list save helper private to the guard. Neither a normal `Store`
nor `App` should expose a production path for saving a cached list. A closure
transaction is acceptable internally, but it must receive only the freshly
loaded list and must not capture an App snapshot for replacement. Prefer one
typed mutation dispatcher shared by shell and TUI to duplicated eligibility
checks. Keep domain/application code independent of filesystem and terminal
APIs; use small typed outcomes/errors and the existing report wrapper for I/O
context. No general-purpose transaction framework or dependency is required.

Keep `tasks.lock` at the same path and never delete or replace it for cleanup:
unlinking a held lock allows another process to lock a different inode. Do not
lock `tasks.json`, which changes identity on each save. Keep the fixed sibling
temporary filename: only the transaction guard can write it, and old binaries
use the same lock. Do not spawn children, duplicate lock handles, or wait for
user input while the guard is alive.

### Shared mutation API for the parallel branches

Use a crate-private request enum with these capabilities. This branch implements
only the operations needed by existing commands/TUI; future variants and
helpers are an integration contract, not authorization to implement new UI or
shell features here.

| Request | Parameters and semantics | Consumer |
| --- | --- | --- |
| Add | Validated text; append to latest canonical vector; return allocated ID | Current shell/TUI |
| SetCompleted | ID and explicit desired boolean; require live task; same value is a no-write success | TUI and shell lifecycle branch |
| EditText | ID and text, with optional original-text expectation; require live task; preserve current completion/order | TUI guarded; shell lifecycle branch can issue an explicit replacement |
| Delete | ID, with optional observed task values for guarded interactive deletion; already-deleted is a no-write success | Current shell/TUI |
| RestoreById | ID, optional expected deletion sequence; clear that tombstone only, retain ID/text/completion/canonical position | Shell lifecycle and trash branches |
| RestoreLatest | Choose largest deletion sequence from latest storage at transaction time; no tombstone is a no-write outcome | Existing TUI and future shell |
| MoveAdjacent | ID, direction, observed adjacent ID or explicit boundary, and the applicable pure projection descriptor | Current TUI; preserve projected swaps when search/filter is integrated |

A reply carries an owned `TaskList` plus an in-memory content token and an
outcome such as `Changed`, `AlreadyInState`, `NothingToRestore`, or `Conflict`.
Include outcome text/IDs from the transaction's latest data so shell messages
cannot accidentally describe stale state. Treat conflicts as typed expected
outcomes carrying the latest snapshot, distinct from storage corruption or
I/O failure. Do not expose the on-disk JSON as a new public output contract.

Keep a snapshot and its token together in one owned `Snapshot` value. For a
changed reply, retain the exact serialized bytes that were replaced under the
lock; for no-op/conflict replies, retain the exact bytes loaded under the lock.
Never reread after unlocking to manufacture a reply token: another writer could
commit between those steps, pairing newer bytes with an older model and causing
refresh to skip that newer change. A reply may already be stale when installed;
its internally consistent token ensures the next refresh detects that fact.

Use one pure request evaluator shared by App-facing and shell transactions,
preferably in a small crate-private `mutation.rs` module. It owns eligibility,
expectations, and outcome construction; storage owns locking and persistence.
Reuse the model's text normalization/validation for both early validation and
under-lock application. Compare canonical proposed text before deciding whether
an edit changed. Valid-snapshot rejections, including a missing/deleted target,
carry that snapshot for TUI reconciliation; read/validation failures retain the
App's last good state. Do not add a generic command bus or new error dependency.

Use a small `TaskObservation` for destructive interactive expectations
(original text, completion, and deletion state/sequence). Use original text
alone for text edits, allowing unrelated completion/order changes to coexist.
Targeted trash restore should compare the observed deletion sequence so a
stale row cannot restore a newer deletion of the same ID.

Wrap the action in a small request carrying a scope-presence precondition:
`AllowMissing` for a never-created scope or a fresh shell invocation, and
`RequireExisting` for a TUI that has observed a canonical snapshot. Evaluate
this on the canonical data read under the lock before treating NotFound as an
empty list. This prevents a racing disappearance from resetting an open TUI's
scope/counters without changing the existing missing-scope shell/list behavior.
No cached TaskList is part of the request. Once a TUI has observed any present
valid snapshot, including its own first add's reply, `RequireExisting` stays set for the session;
read errors or a missing token must never downgrade it to `AllowMissing`.
Both presence modes create an absent lock file when acquiring a write or
recovery lock, so a restored snapshot remains writable without a shell command.
Read-only startup and refresh do not create it.

### Contention and bounded errors

Proposed acquisition budgets: 1 second for shell commands and 100 milliseconds
for an interactive mutation. Use `Instant` and `try_lock`, with short waits
starting at 10 milliseconds and capped at 50 milliseconds or remaining budget.
Only `WouldBlock` is retryable; permission, lock, validation, and other I/O
errors return immediately. Never use unbounded `File::lock`.

These budgets bound lock acquisition, not filesystem read/write/sync latency.
The OS provides no FIFO/fairness guarantee. A saturated scope may return Busy;
it must not spin, silently drop an action, or promise every contender succeeds.
A different scope remains independent. Shell Busy exits nonzero with no
success stdout and an actionable scope-specific message. Interactive Busy
keeps the TUI open and the editor/cursor intact; show a message and let the user
retry. Do not queue stale actions indefinitely or replay after a save error.

## Conflict and user-action semantics

Capture the selected ID and its relevant expectations from the displayed App
state before refreshing or issuing a transaction. Never refresh selection and
then reinterpret the same key against a replacement row. All writes use latest
storage, even when the idle refresh has not run yet.

| Situation | Recommended behavior |
| --- | --- |
| Two TUIs add from the same cached state | Both transactions append; distinct scope-local IDs; both additions survive |
| Two TUIs complete the same displayed open task | Space requests `SetCompleted(true)`, not toggle-latest; first changes it, second returns already-done without reversing it |
| Completion races with a text edit | Apply the desired completion to the live ID and preserve the latest text |
| Interactive text edit sees a different original text | Reject replacement, install latest tasks, keep the draft/cursor/base text; show that the task changed and explain cancel/reopen |
| Latest text already equals the proposed edit | Converged no-write success, unless the draft was invalidated by an observed deletion |
| User commits an unchanged draft | Close it as a local no-change action; refresh current data; never overwrite an external edit |
| Edit target is externally deleted or missing | Keep the draft as a detached editor; reject save; do not silently resurrect or retarget it |
| An invalidated edit target is subsequently restored | Preserve the draft and conflict state; require cancel/reopen before replacing text |
| Guarded TUI delete sees changed task text/completion | Reject with latest data and message; a new explicit delete can act on the refreshed row |
| Shell delete sees edited live task | Delete that stable ID's latest value; existing shell command has no observation token |
| Any delete sees an existing tombstone | Already-deleted no-write success; no new deletion sequence |
| Edit/complete/reorder sees a tombstone | No mutation; report the target is no longer active; never modify it through a live-task command |
| Targeted restore sees a newer deletion sequence | Conflict for the guarded trash request; unguarded shell restore applies to current tombstone |
| Targeted restore sees an already-live ID | Idempotent no-write success; unknown IDs still fail |
| Two `restore_latest` actions serialize | Each acts on the current latest tombstone, potentially restoring two different tasks; this is a list-level action, not undo of the caller's own delete |

On interactive conflicts do not overwrite automatically or rebase the edit's
original text just because refresh happened. Keep the conflict indicator
separate from transient footer messages so typing cannot erase its meaning.
The existing configurable CancelEdit action abandons the retained draft; start
editing again to use current text. Recovery hints must use the resolved keymap,
not hard-coded Escape/Enter keys. Do not add a force-save binding in this slice.

These checks compare current values, not every intermediate event. With schema
1, an unobserved text A -> B -> A or delete -> restore cycle can be equivalent
to the original observation and is allowed if latest eligibility/text match.
An observed deletion invalidates the open draft even after restoration. Strict
detection of all unobserved cycles needs persisted task generations and a
schema migration; it is explicitly outside the recommendation.

### Reorder semantics

Keep today's canonical vector and swap behavior, including hidden tombstone
positions. In the ordinary all-live view, J/K requests a swap of the selected
ID with its observed adjacent live ID in a specified direction. Under the lock,
require both to remain live and that adjacency/direction still hold; otherwise
refresh and report changed order. An observed boundary remains a no-write
boundary only if it is still a boundary. Do not replay an old whole-vector order
or blindly swap with a newly added/restored neighbor.

Two TUIs making the same swap from the same display do not undo each other:
the first commits, the second fails its adjacency expectation. Independent
swaps may both succeed if their expectations remain true. Other task text,
completion, counters, and tombstones must remain unchanged.

The inspected search/filter branch already implements projected-neighbor swaps
and tests that hidden live rows and tombstones retain their canonical slots.
Preserve this behavior when integrating it. Extend `MoveAdjacent` with an owned,
pure projection descriptor containing the status view and normalized query.
Baseline requests use all live tasks; add filtered descriptors only when that
feature is integrated. Extract and reuse the same matching predicate for App
and mutation evaluation, including the branch's lowercase substring semantics.

Under the lock, require the selected ID to remain in that projection and the
observed neighbor to remain adjacent in the requested direction. A newly
matching row between the pair, a neighbor leaving the projection, or a changed
boundary conflicts. Swap only the two canonical slots, preserving hidden rows,
tombstones, current task fields, and counters. Unrelated changes that leave
projected adjacency intact can coexist. Never pass cached projected ID vectors,
row numbers, or a predicate capturing App state into storage.

## TUI state and idle refresh

Replace `Persisted` with a pending mutation transition (or equivalent) and a
separate result-application method. Pure/transient actions still update App
immediately. Storage-changing actions describe intent without mutating its
list, clearing an editor, moving selection, or starting a celebration yet.
After a durable reply, install the returned list and finish the local action:
select an added/restored task when visible, close a saved editor, or choose the
next row after deletion. No-op replies still reconcile current data.

For any refresh/result installation:

- Preserve the selected ID if it remains in the current projected view. If it
  leaves, choose the row at its old visible ordinal, clamped to the final row;
  choose None when the view is empty. This retains next-then-previous deletion
  behavior. A formerly empty view selects its first newly visible task.
- With trash integrated, reconcile its selection against newest-first
  tombstones independently of the saved Normal selection and live projection.
  Preserve the active view and Help return mode. External restore removes that
  trash row and uses the same ordinal fallback; external deletion can add a
  row without stealing an existing selection. A successful local targeted
  restore remembers its ID for returning to Normal only if it matches that
  live projection, otherwise reconcile the saved Normal selection.
- Retain mode, help overlay, filter/search query and input, and any view state
  introduced by the other branches. Refresh only the underlying task snapshot
  and recompute projected IDs through their shared projection entry point.
- Preserve editor kind/ID, original text, draft bytes, UTF-8 cursor, and conflict
  flag. A filtered-away/deleted target needs a dedicated visible draft row or
  editor area; it is not reintroduced as a live task. Add drafts also survive.
- Passive external completion does not start a celebration. Start one only
  after this TUI durably changes an open task to done and the transaction's
  before/after state crosses the last-open-task boundary. External reopening
  or a newly open task dismisses an obsolete celebration.
- Keep read-error state until a successful refresh, and durability uncertainty
  until successful synchronization recovery. Reading valid bytes cannot prove
  that an earlier replacement was durably synced. Do not reset a malformed
  snapshot to an empty list or save cached data over it.

### Revision/token and refresh loop

Use exact bytes from a single opened canonical file as an in-memory snapshot
token; represent a missing snapshot distinctly. The token is not an on-disk
revision, persisted field, hash with collision assumptions, or part of CLI
output. Share parsing/validation between transactions and side-effect-free
reads. Metadata timestamps/length alone are not reliable revision checks:
same-size edits, coarse timestamps, and replacement races must be tested.

At startup read once without a writer lock. Use an absolute next-refresh
deadline, initially one second later. `event::poll` waits until the next refresh
or existing 50-millisecond celebration deadline, whichever is sooner; read only
after poll says an event is ready. Poll and read remain on the same thread.
Check due deadlines even during continuous key traffic so refresh cannot starve.
Advance the deadline from now after a slow read; do not catch up with a burst of
missed reads. Transaction results update the cached token/deadline immediately.

When input and a deadline are both ready, map and apply that input against the
currently displayed App before servicing due refresh/recovery work. Derive any
mutation's ID/expectations at that point; do not install a refresh between key
mapping and intent capture. Service due deadlines in the same iteration, then
draw changed state before reading the next event. This also protects StartEdit
and guarded deletion from silently acting on a row substituted by refresh.

On refresh, read canonical bytes once. Equal token means no JSON parse, model
validation, or projection rebuild; draw only if a status change such as clearing
a read error requires it. Changed bytes must validate before installation;
semantically identical pretty-print changes update the token
without a redraw. Reuse read buffers where practical; do not clone the full list
on every timeout. A poll timeout itself is never a reason to draw. Initial
frame, resize, changed transient state, installed display changes, and active
animation frames set the redraw flag; ignored/release/no-op events do not.

Under normal successful local I/O, external changes become visible within one
refresh interval plus read/render time. A refresh racing a writer may see the
complete old snapshot and reconcile on the next interval. Writers are always
correct regardless of token/freshness because they reload under the lock.
There is no storage polling at the animation's 20 Hz rate and no read on every
keystroke. One necessary file read per second is the initial cost ceiling for
idle reconciliation, not a zero-I/O claim.

Keep the last good snapshot and all drafts on read/validation errors. Show one
stable error rather than redrawing repeated identical messages; retry refresh
with backoff capped at five seconds, and reset on success. After a previously
present canonical file disappears, report unavailable storage and inhibit
mutations rather than silently reset counters; a never-created empty scope is
normal. Every mutation still checks latest storage, and the TUI must not use a
stale store handle to recreate a disappeared initialized list without review.

Measure release-build idle CPU, bytes read, parse count, and redraw count for
0, 1,000, and 10,000 total records, including substantial tombstones and Unicode
text. Existing CLI timings are not refresh measurements. If this cost is
unacceptable, reconsider a native directory watcher before expanding the
implementation. Watch the containing directory rather than a replaced inode,
coalesce events, reread after registration, reconcile on overflow/error, and
retain a fallback for unavailable notifications. Do not add it speculatively.

Use a five-second warmup and three 60-second samples per fixture, first with
one idle TUI and then two sharing a scope. Record platform/filesystem, snapshot
byte size, CPU time as a fraction of one core, and p50/p95/max read durations.
After initial rendering, stable-data samples must show no parses or redraws
and at most one scheduled read per second per TUI, allowing the sample boundary
read. Record input-to-frame and mutation latency separately under a modest
concurrent writer and a contended burst, including Busy counts. Use private
test/measurement hooks, not a runtime CLI flag or product metrics UI. Keep raw
samples with the resulting report; see the implementation evidence below.

## Read-only, platform, compatibility, and recovery

`list` must retain its current independent loader: no directories, lock file,
temporary file, writer/shared lock, config load, or terminal startup. Keep
default rows exactly `ID  open|done  text` in canonical order, omitting tombstones;
empty/missing scope is empty stdout and success. It observes an atomic point-in-
time snapshot, possibly the prior commit, rather than promising the latest
concurrent commit. Refresh likewise reads only canonical data, never the temp.

Keep the sibling temp on the same filesystem. Close its handle before rename
to avoid unnecessary platform-dependent sharing failures. On Unix a reader
opened before replacement can finish reading the old inode; this is allowed.
Keep Rust's replacement operation and existing Unix directory sync; never add
delete-then-rename or truncate canonical data as a fallback. Windows replacement
and handle sharing must be tested, including a reader held open across save.
Directory fsync is currently Unix-only; do not claim a new Windows power-loss
guarantee. The primary runtime promise remains macOS/Linux, with Windows build
compatibility and the existing test matrix preserved. Remote/network filesystems
have additional lock/cache/durability behavior and are not newly certified.

No migration: preserve schema 1, scope identity, exact canonical cwd resolution,
slug/hash paths, positive scope-local IDs, counters, tombstones, and file format.
Adding even an optional revision would break old `deny_unknown_fields` readers.
New/old processes share `tasks.lock`, so old shell transactions remain safe. An
old TUI still holds it for its lifetime: new readers/TUIs can view the last saved
snapshot, but mutations return Busy until that old session exits. Restart old
TUIs to obtain concurrent writes. No downgrade conversion or stale-lock removal
is necessary.

| Failure point | Required result/recovery |
| --- | --- |
| Lock acquisition, load, validation, or conflict | No canonical/temp mutation; release guard; show typed error/latest valid state |
| Temp write, temp sync, or replacement fails | Canonical snapshot remains previous complete version; no success output; keep TUI draft; stale temp is never loaded |
| Process crashes before replacement | OS releases the lock; canonical stays previous; next guarded write can overwrite the unused temp |
| Process crashes after replacement | Canonical is a complete snapshot; power-loss durability depends on whether directory sync completed |
| Directory sync fails after successful replacement | The change may be visible but durability is unconfirmed; do not report ordinary success or assume rollback |
| Canonical data is corrupt/unsupported/wrong-scope | Fail closed, retain original bytes and TUI last-good state, do not promote temp or initialize empty storage |

Distinguish a pre-replacement save failure from `ReplacedButNotSynced`, carrying
the applied outcome/ID and visible snapshot in the latter case. Do not retry
the mutation automatically: Add, RestoreLatest, and reorder are not safely
replayable after ambiguous persistence. Reconcile and display a clear durability
warning. CLI exits nonzero and states that a change may already be visible,
directing the caller to inspect before retrying.

### TUI durability recovery state

Record `Ready` or `AwaitingSync` separately from transient messages and read
errors. `AwaitingSync` retains the applied outcome/ID and snapshot token, plus
any committed editor buffer/cursor. Install that visible snapshot immediately
without claiming durable success or starting a celebration. Freeze the retained
buffer/cursor and inhibit new mutations or editor starts until recovery succeeds.
Existing cancel/quit controls stay available; cancel returns to Normal, where
navigation, view, and help controls continue to work with the resolved keymap.
Cancel abandons the draft, not the already-visible task change or the durability
warning. No recovery-specific keybinding is introduced.

Use a separate `Store::recover_sync(LockBudget)` operation. Acquire the same
stable scope lock with the interactive budget, require existing canonical data,
load and validate it, sync the current canonical file and Unix directory, and
return that latest snapshot/token. Do not create/rewrite canonical or temporary
data, recreate missing directories, allocate counters, repeat the original
action, or restore a cached result. Another writer may have edited or deleted
the originally affected task; recover the current valid snapshot without
undoing that later action. Create the scope lock file if absent; its absence
does not make an existing canonical snapshot unavailable.

Set a recovery deadline one second after entering `AwaitingSync`, with failures
backing off to at most five seconds. Keep it independent of the read-refresh
deadline and include it in the event-loop poll deadline; a successful read must
not reset its backoff. An editor commit key while awaiting sync may attempt
recovery immediately; it never resubmits Add/Edit. Ordinary successful reads,
typing, and no-op outcomes never clear `AwaitingSync`. On successful recovery,
install the latest data, clear the warning, close any retained committed draft,
and return to normal action handling. Do not replay delayed selection changes
or celebrations over that latest state. Busy, missing/corrupt data, and repeated
sync failures retain the pending state. This is an in-session replay guard;
there is no persisted receipt or exactly-once guarantee after restarting.

Successful no-ops and ordinary Busy/conflict results do not rewrite snapshots.
An explicit recovery sync does not change bytes. No command offers exactly-once
execution across a crash or failed stdout delivery; no request log is introduced.

## Integration ownership and order

Only this worktree owns concurrency implementation when authorized. The other
sessions retain ownership of their feature behavior. Do not edit their
checkouts or revert their work. Compare changes against this inspected baseline
before implementing, and coordinate shared contracts before combining branches.

Read-only feature inspection on October 4, 2026 found the following contracts.
These refs are snapshots for integration review, not evidence that their tests
were rerun here or that any branch is merged into this worktree.

| Feature ref inspected | Existing behavior to preserve |
| --- | --- |
| `feat/search-filters` at `a8fe2a1` | `App::visible_tasks` projects All/Open/Done plus lowercase substring search; filtered reorder swaps matching slots and leaves hidden rows fixed; search input has its own mode/session |
| `feat/shell-lifecycle` at `2e0c47d` | `set_completed`, idempotent canonical-text `edit`, and `restore(id)` already exist; all shell handlers still use locked load/save; latest-data messages distinguish changed/already-in-state outcomes |
| `feat/trash-browser` at `b57543c` | Trash has a separate newest-first selection and Help context; targeted restore retains task values; Normal mutations are inactive in Trash; storage still uses the lifetime lock |

Reuse the lifecycle helpers and one copy of `restore(id)` when combining these
branches. Convert `mutate_task_in_store` and trash restore alongside existing
writers. Reconcile active Search/Trash/Help modes and their separate selection
state through snapshot installation; recreating App would discard them.

| Branch/responsibility | Likely shared files | Contract to reconcile |
| --- | --- | --- |
| Concurrency | `mutation.rs`, `storage.rs`, `terminal.rs`, mutation/result portions of `app.rs` and `lib.rs`; concurrency test harness | Sole writer boundary, snapshot/token pairing, refresh, typed conflicts, sync-only recovery, no cached-list save |
| Search/filter | `app.rs`, `ui.rs`, `action.rs`, `input.rs`, CLI/list paths and tests, usage/config docs | Shared projected-ID access, selection fallback, retained query state, filtered reorder policy |
| Shell done/reopen/edit/restore | `task.rs`, `lib.rs`, `cli.rs`, `tests/cli.rs`, usage/changelog | Typed ID actions, explicit desired states, latest-data output, no-write idempotence, shared transactions |
| Trash browsing | `task.rs`, `app.rs`, `ui.rs`, actions/keymap and tests, usage/config docs | Tombstone eligibility, guarded restore by ID/sequence, view preservation, detached editor rendering |

Recommended integration sequence:

1. Agree on mutation eligibility/outcomes, projected-ID access, and reorder
   policy. Parallel feature branches may keep their currently safe lifetime-lock
   behavior while finishing; they must not release it independently.
2. Land the transaction API and convert all existing TUI/shell writers as one
   coherent change. Do not integrate an intermediate state with an unlocked
   cached-snapshot save path.
3. Adapt shell lifecycle handlers to this API. Their new model helpers may be
   integrated earlier; convert every new writer before concurrency is enabled.
4. Integrate search/filter projection and selection contracts, then trash view
   and targeted restore. If those features arrive first, stage 2 must adapt all
   of their writers rather than assume only baseline actions exist.
5. Run combined regression and multi-process suites, reconcile usage/config/
   changelog changes, and verify no production caller can save an App snapshot.

Do not implement the other features here or introduce unused API variants solely
for them. Expect textual conflicts in App transitions, TUI rendering, shell
dispatch, and documentation; resolve by preserving their feature tests and
moving their mutation intent into the sole storage boundary.

## Staged implementation and acceptance gates

### Stage 0: Review and freeze the contracts

- Review this plan and the material choices below before any runtime edit.
- Reinspect drift from `74d049e` in shared files and the feature refs above,
  read parallel feature contracts, and agree on the active-view
  projection/eligibility interface.
- Keep the recommendation in schema 1 unless the user explicitly requires
  strict event-history conflict detection.

Acceptance: one agreed transaction/result/recovery contract and preservation of
the inspected projected-reorder behavior; unresolved product choices have an
explicit answer or accepted default. Implementation begins only in a subsequent
authorized turn.

### Stage 1: Domain intents and guarded storage transactions

Add meaningful failing behavior tests before implementation for lost-update
prevention, stale action rejection, no-write outcomes, and allocation safety.
Introduce typed intents/outcomes and the private lock guard. Keep the existing
save guarantees. Convert current shell add/delete handlers and their unit
helper to transaction replies; do not add new shell commands.

Until stage 2 converts the terminal, retain its existing session guard and
locked save behavior. Do not change its handle to an unlocked Store while
`Transition::Persisted` still saves `app.tasks()`. Either keep these stages in
one coherent implementation commit or retain a safe temporary legacy guard.

Acceptance: competing handles can exist; at most one transaction writes a
scope; each action reloads latest data; unrelated changes survive; checked
counters are allocated under lock; Busy is bounded; no-write outcomes leave
snapshot bytes and file identity unchanged; every returned token matches its
owned model, including after a second writer commits. Current CLI output/tests
pass. Run focused task/storage/lib/CLI tests and strict Clippy.

### Stage 2: TUI mutation submission and snapshot reconciliation

Refactor App transitions to emit intents, add result installation, store editor
base text/conflict state, and render detached drafts. Replace every
`save(app.tasks())` path. Keep keymap-driven hints, footer/help, and fixed Ctrl-C.
Install committed/no-op/conflict snapshots without reconstructing App wholesale.
Implement `AwaitingSync` and the sync-only recovery path here, before allowing
interactive retries after any persistence error.

Acceptance: two cached Apps can edit different tasks without loss; conflicting
edits preserve exact drafts/cursors; deleted/restored targets behave as above;
same desired completion does not reverse; stale reorder cannot undo another
swap; save/lock errors do not exit the TUI or claim success. All input/UI/editor/
selection and celebration regressions pass; repeated commit keys during sync
uncertainty cannot duplicate an add or replay restore/reorder. At this stage
the lifetime lock may be removed only when every writer uses the transaction API.

### Stage 3: Idle refresh and redraw scheduling

Add snapshot byte tokens, absolute refresh deadlines, dirty redraw state, and
stable refresh error/recovery state to the synchronous event loop. Retain
animation timing without using it as the storage-read interval.

Acceptance: idle shell changes appear without a keypress; continuous input
cannot starve refresh; unchanged valid data/status causes no parsing or drawing; equal
metadata does not hide changed bytes; edits/queries/help survive refresh;
errors retain the last-good list; missing initialized storage does not reset
IDs. Run deterministic clock/event tests and release-build idle measurements.

### Stage 4: Process races, crash paths, and combined feature integration

Run the multi-process cases below against isolated storage, plus narrow PTY
smokes for actual terminal startup/idle refresh/two TUIs. Use a Unix-only test
harness/dev dependency if needed for PTYs; do not add a production dependency
or test-only runtime CLI switch. All child waits require timeouts and cleanup.

Acceptance: repeatable process tests demonstrate serialization and preservation
of successful mutations; readers see complete snapshots; killed writers release
locks; pre/post-replacement errors are distinguishable; combined branch feature
tests pass. Run Windows storage/CLI tests through the existing CI matrix when
integration is authorized; local macOS testing is not Windows/Linux proof.

### Stage 5: User docs, performance evidence, and final verification

After behavior passes, update `docs/usage.md` concurrent-process guidance,
conflict/draft recovery, bounded Busy, mixed old/new sessions, and reorder
semantics. Update README only as needed for overview. `docs/configuration.md`
should retain existing controls and keymap derivation; document any integrated
new action in its owning feature, with no concurrency-specific binding/config
setting unless justified. Add accurate Unreleased changelog behavior entries.
Do not describe this plan as a shipped feature in current usage/changelog.

Record separate refresh/concurrency measurements if useful; do not overwrite
historical benchmark samples or portray them as concurrent benchmarks. Review
every production write call site and the combined diff, then run:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo build --release --locked
git diff --check
```

Acceptance: all gates pass; release-binary smokes use a temporary home and exact
temporary cwd; stable IDs, scope isolation, soft deletion/restoration, plain
list output, config-independent CLI, and keymap-based UI still hold. Mark this
plan DONE only after the complete behavior and evidence are present.

## Regression matrix

| Layer | Required cases |
| --- | --- |
| Domain | Independent field updates; same-value no-ops; eligibility forbids tombstone edit/complete; canonical-text guards; current latest and targeted restore; deletion-sequence guards; canonical/projected adjacency and boundary guards; changed query membership; hidden slots preserved; checked ID/sequence overflow |
| Storage | Fresh load after lock acquisition; lock budgets with controllable clock; independent scopes; no-write bytes/identity; invalid scope/schema/data leaves temp/canonical untouched; reply model/token consistency after a later commit; failure before replace; sync uncertainty after replace; sync-only recovery preserves intervening writes; ignored stale temp; initialized-file disappearance |
| App/UI | Selection retained by ID/fallback ordinal; independent live/trash selections; empty-view arrivals; filter/query/search-input/help preservation; unchanged drafts close safely; exact UTF-8 buffer/cursor on edit/delete/restore conflict; detached edit row visible; keymap-derived recovery hints; repeated Enter cannot replay an uncertain action; successful reads cannot clear durability warnings; local-only durable celebration |
| Loop | Fake-clock deadlines; input targets captured before due refresh; key flood does not postpone refresh/recovery; one scheduled read/second; no catch-up burst; zero parses/draws for unchanged bytes/status; read-error clearance redraws even with equal bytes; semantic no-change rewrite; resize redraw; animation/refresh separation; independent read/recovery backoff without message churn |
| CLI | Existing exact stdout/error/no-create/config bypass contracts; transaction-backed add/delete while TUI lives; lifecycle branch commands use the same boundary; unknown/deleted/no-op cases; global/exact-local isolation |
| Platform | Reader held open while replacement happens; temp handle closed; complete old/new reads; scope lock exclusion and release; Windows replacement/sharing behavior with existing directory-sync limitation |

### Multi-process cases and deterministic orchestration

Use actual child processes, not only threads or two handles in one process.
Reuse the Rust test executable's ignored child-helper test pattern inside the
storage/session modules so crate-private APIs remain private. Exchange ready/
proceed acknowledgements over pipes or isolated rendezvous files, and assert
children are ready before releasing a barrier. Use bounded waits, terminate and
reap on failure, capture stderr, and never assume a race from a fixed sleep.
All homes/cwds/rendezvous paths are fresh temporary directories. Do not point
any child, PTY, smoke, or benchmark at real `~/.shtodo`.

1. Start two stale client sessions, let each see the same initial list, then
   release distinct add/edit intents together. Both successful updates survive;
   added IDs are unique and counters validate. This must fail the unsafe
   unlock-and-save-cached-snapshot implementation.
2. Start several shell add children from one barrier. Assert every successful
   child's task is present exactly once, all IDs are unique, and every failure
   is an explicit bounded Busy with no success output. A smaller controlled
   pair must both succeed within the shell budget on normal local I/O.
3. Hold a transaction after load with a ready acknowledgement; a contender
   times out without touching canonical/temp data. Release the holder and show
   the contender succeeds. Repeat with a distinct scope to prove isolation.
4. Launch an actual TUI in a PTY and wait for its first frame. Run shell add and
   delete in the same scope; assert shell success and refreshed terminal content
   without typing. Repeat for exact-local scope and a separate subdirectory.
5. Launch two actual TUIs, keep both alive, mutate through each, and verify both
   converge and unrelated changes survive. Combine deterministic session-helper
   assertions with PTY output checks rather than rely only on ANSI text matching.
6. Two editors with the same base text: first saves, second conflicts while
   preserving its draft. Repeat with external deletion, observed restoration,
   and unrelated completion. Also test the documented unobserved value-cycle
   behavior, so expectations do not imply stronger history detection.
7. Two stale same-completion intents end done, not open. Two stale same-reorder
   intents commit once and conflict once. Delete/restore races follow serialized
   lifecycle outcomes, with unique deletion sequences and retained tombstones.
8. Loop read-only `list` during controlled replacements. Every successful output
   is one complete old/new list, never partial or malformed; readers create no
   storage/locks in a missing scope. Include same-length changed snapshots with
   deliberately equal timestamps to exercise content tokens.
9. Pause writer before replacement, terminate it, and assert the old canonical
   remains, the lock becomes acquirable, and stale temp is ignored. Pause after
   replacement, terminate, and assert a complete canonical snapshot loads. Use
   private injectable save steps for write/sync/rename errors; kill tests do not
   prove power-loss durability or simulate a filesystem losing cached writes.
10. Exercise mixed-version behavior using a helper that holds the legacy lock:
    readers still work, mutations return bounded Busy, and subsequent writes
    succeed after release. Keep side-effect-free list tests independent of TUI.
11. Capture writer A's reply, let writer B commit before A installs it, and show
    A's next refresh detects B. Exercise changed/no-op/conflict replies; none
    may combine A's list with bytes reread from B's snapshot.
12. Inject directory-sync failure after replacement. Repeat editor commit and
    successful read refreshes; exactly one added task remains and the warning
    persists. Let another process edit/delete it, then recover synchronization;
    its change survives, no temp/canonical rewrite occurs, and the retained
    committed draft closes. Repeat for RestoreLatest and reorder to prove their
    outcomes are not replayed; Busy/corruption during recovery remains bounded.

## Decisions and review questions

Resolved within this proposal: use the existing per-scope OS lock for short
transactions; all writers reapply ID-based intents to latest storage; never save
App snapshots; preserve the atomic/sync path, schema 1, IDs, cwd semantics, and
read-only list contract; use explicit desired completion, guarded text edits and
canonical adjacent swaps; preserve drafts and view state; no coordinator/runtime/
database dependency.

Material defaults accepted by the user's implementation approval:

| Choice | Recommendation | When user input changes the plan |
| --- | --- | --- |
| Idle freshness/cost | One-second exact-byte polling with no unchanged parse/redraw | If zero periodic file reads or substantially faster updates are required, evaluate native watching and reconciliation |
| Contention | 1-second shell / 100-millisecond TUI acquisition budgets | If scripts need different waiting behavior, adjust budgets; no new config surface initially |
| Edit history | Guard current text/lifecycle values; observed deletion invalidates draft | If every unseen edit/delete/restore cycle must conflict, require task generations and a migration design |
| Filtered reorder | Preserve the inspected branch's projected-neighbor slot swaps with latest-data adjacency checks | Replan only if the feature contract changes or the user requests a different reorder policy |
| Destructive interactive conflict | Guard delete against changed displayed task values | User may prefer ID-only delete; shell delete keeps its existing latest-by-ID semantics |
| Durability uncertainty | Freeze the committed draft, inhibit new writes, and retry synchronization only | Replan if continued editing or writes during uncertain durability are required; no implicit action replay |

The approved baseline uses these polling, conflict, and durability-recovery
defaults. Filtered reorder has a concrete preservation contract from the
inspected search/filter branch; verify that ref for drift before combined
integration. A request for stricter history semantics or zero polling requires
replanning before broadening dependencies or changing schema.

## Stop/replan conditions

- A writer still accepts an unlocked cached snapshot, including a newly added
  writer from another branch: do not enable concurrent usage until converted.
- Correctness requires weakening snapshot validation, atomic replacement, sync,
  ID/scope stability, or read-only no-create/no-lock behavior.
- Platform testing shows replacement/locking fails with simultaneous readers:
  retain safe failure behavior and investigate before promising support.
- Measurements show refresh or synchronous transactions make the TUI
  unresponsive: use the evidence to reconsider scheduling/watchers/coordinator.
- Strict task-history conflicts, unsupported filesystem guarantees, a public
  machine-readable protocol, or a schema migration become requirements.
- Shared feature contracts disagree about projected reorder, trash eligibility,
  or editor rendering: resolve the contract rather than discard feature edits.

## Technical references and validation record

Rust documents `try_lock` contention and release when the handle closes in
[File locking](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock).
Replacement has platform-specific behavior in
[fs::rename](https://doc.rust-lang.org/std/fs/fn.rename.html); test the supported
platforms rather than assume identical handle semantics. Rust's
[sync_all](https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all)
attempts to flush file content and metadata; a successful read is not such a
synchronization attempt. The recovery state and replay guard are this plan's
application policy. Crossterm 0.29 requires poll/read on the same thread in its
[event documentation](https://docs.rs/crossterm/0.29.0/crossterm/event/index.html).
[Notify's documentation](https://docs.rs/notify/8.2.0/notify/) describes editor
replacement events and notification limitations, supporting the need for
reconciliation if watching is later adopted. These API facts support this
design; runtime evidence is recorded separately below.

Prior planning verification recorded in the original plan, on macOS with Rust
1.98.0, October 4, 2026. These runtime gates were not rerun for this documentation
revision; they are historical checks before the approved implementation:

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --locked`: passed, 125 unit tests and 24 CLI tests.
- `cargo build --release --locked`: passed.
- `git diff --check`: passed; local Markdown links and no-emdash constraint
  checked for both changed documents.
- Only this plan and `docs/plans/README.md` changed. Runtime code, dependencies,
  user usage/configuration docs, changelog, and historical benchmarks were intact.

The first Clippy attempt could not unpack a dependency into the read-only shared
Cargo cache. Clippy/tests/build then passed with an isolated `CARGO_HOME` under
the original planning worktree's ignored `target/concurrency-cargo-home`; no
shared-cache write or real task-data smoke was needed. These are baseline checks, not concurrency
validation. Current implementation evidence follows below. Cross-platform CI
results and combined integration with the other branches remain future work.

Further planning review on October 4, 2026:

- Rechecked `74d049e` and confirmed no runtime-source, manifest, or CI changes
  since `cb2f95b`; the current planning branch is recorded above.
- Inspected the three feature refs above through Git without editing their
  worktrees; reconciled projected reorder, lifecycle helpers, and trash state.
- Checked the primary Rust/Crossterm references, including synchronization and
  same-thread poll/read constraints.
- `git diff --check`, local Markdown link checks, and the no-emdash check passed
  for this plan and the plan index. Only those two documents changed in this
  review; no runtime tests were added or executed for the documentation changes.

### Approved implementation: October 4, 2026

The baseline portions of stages 0 through 5 are complete in
`worktree/brave-forest-b998`. Search/filter, shell lifecycle, and trash behavior
were inspected for their integration contracts and were not merged or added.
The Windows/Linux CI matrix remains configured but was not triggered here;
all executed runtime evidence is macOS arm64 on APFS.

- [`src/mutation.rs`](../../src/mutation.rs) defines typed intents and outcomes
  for existing actions, including desired completion, guarded text/deletion,
  and observed-neighbor reorder. No unused feature variants were introduced.
- [`src/storage.rs`](../../src/storage.rs) resolves handles without locking or
  creating storage. A private short-lived guard owns replacement; transactions
  validate/load latest data under the stable scope lock, preserve no-write
  outcomes, and return matching model/byte tokens. Acquisition budgets are one
  second for shell commands and 100 milliseconds for the TUI. Atomic replacement,
  temporary-file sync, Unix directory sync, schema 1, and counters are retained.
  Cached-list `save` exists only for test fixture construction.
- [`src/app.rs`](../../src/app.rs), [`src/session.rs`](../../src/session.rs),
  and [`src/terminal.rs`](../../src/terminal.rs) submit intents before refresh,
  finish durable outcomes, reconcile selected IDs and retained drafts, and use
  independent refresh/recovery deadlines based on completed I/O. Idle equality
  skips parsing/redraw; read errors retain last-good data and back off to five
  seconds. Initialized missing storage fails closed. Detached/conflicting drafts
  remain visible with configured cancel-key hints in [`src/ui.rs`](../../src/ui.rs).
- Replacement with unconfirmed directory sync carries the visible snapshot and
  ID. The TUI inhibits new writes, retains the committed draft, and synchronizes
  the latest canonical snapshot without repeating the action. Later writes
  survive recovery; successful reads alone do not clear the warning.
- Shell add/delete now use transaction replies; the existing plain list output,
  side-effect-free reader, config bypass, exact local scope, and recoverable
  idempotent deletion still pass their regressions. Dependencies, lockfile,
  public commands, and keybinding configuration are unchanged.

Two focused tests first failed against the original implementation: submitting
delete changed displayed data before persistence, and opening two idle handles
was blocked by the lifetime lock. Both now pass. Additional tests cover stale
independent editors, exact draft/cursor retention, observed and unobserved
lifecycle cycles, completion/reorder races, unique deletion sequences, no-write
file identity, equal-metadata changes, paired replies after later commits,
read/parse/redraw scheduling, stable errors, allocation overflow, and save/sync
failures. Sync uncertainty is exercised for add, restore, and reorder with
intervening writes, proving recovery does not replay them.

[`src/storage_concurrency_tests.rs`](../../src/storage_concurrency_tests.rs)
uses actual child processes with ready/proceed acknowledgements, bounded waits,
and kill/reap cleanup for stale clients, exclusion/isolation, lifecycle races,
replacement readers, and writers killed before/after replacement.
[`scripts/test_concurrency.py`](../../scripts/test_concurrency.py) adds eight
actual PTY/shell scenarios, invoked from the Unix CLI suite. It reconstructs
terminal cells to verify idle convergence and conflict/detached-draft behavior.
Unix test runs require Python 3; Windows retains the Rust tests and skips PTYs.
These crash/error tests do not simulate power loss.

Verification completed with Rust 1.98.0 and an isolated ignored
`target/concurrency-cargo-home`:

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --locked`: passed, 163 unit tests and 25 CLI tests. Two unit tests
  are intentionally ignored: the child helper invoked by process tests and the
  long release measurement, which was run explicitly and passed.
- `cargo build --release --locked`: passed. Eight release-binary PTY scenarios
  also passed against isolated homes/cwds.
- Python harness compilation, `git diff --check`, local Markdown link checks,
  and the no-emdash constraint passed.
- Production write call sites were reviewed: shell/TUI actions use `mutate`;
  only the private locked replacement capability writes canonical snapshots.

The separate [concurrency measurement report](../benchmarks/concurrent-usage.md)
and three raw JSON artifacts retain the exact binary hash and samples. Five
seconds of warmup and three 60-second samples were run for 0/1,000/10,000
Unicode/tombstone records with one and two TUIs. Every idle sample had zero
terminal output, and the instrumented refresh path had zero parses/redraw
requests and 59 or 60 reads per TUI per minute. Native CPU was approximately
0.028% to 0.114% of one core per TUI on this host; polling 10,000 records still
reads about 1.7 MB per second per TUI. The report separately records paced
writer/input latency, an eight-writer burst, and a bounded legacy-lock Busy.
These measurements support the current scheduling choice for this workload,
with the host interference and platform limits stated in the report.

[Usage](../usage.md#concurrent-processes), the README, and Unreleased changelog
now describe the implemented behavior. Historical benchmark data is intact.
No commit, push, PR, merge, deployment, or release was performed.

### Integration with origin/main: October 4, 2026

The user requested synchronization after merging other features to main.
Fetched `origin/main` and fast-forwarded this worktree from `74d049e` to
`0982c5e`, including shell lifecycle (#10), browsable trash (#11), and
search/All/Open/Done tabs (#12). A final fetch found `707b0ec`, the beta.3 release
preparation (#13); the worktree was fast-forwarded again and that version,
lockfile, release notes, and updated agent skill were preserved. The uncommitted concurrency work was
checkpointed and reapplied, preserving all upstream feature tests and behavior.

Resolved conflicts in App, CLI dispatch, CLI tests, usage, changelog, and the
plan index. Every new writer now uses the same short transaction API:

- Shell done/reopen/edit/restore preserve their exact success/no-op/error
  contracts, deleted-task restore guidance, config bypass, exact local scope,
  and no-write outcomes. Add preserves the `--print-id` output contract.
- [`src/projection.rs`](../../src/projection.rs) shares status/query matching
  and adjacency between the App and latest-snapshot mutation checks. Reorder
  captures that projection and swaps only its observed neighboring task slots,
  retaining filtered-out and tombstone slots. Changed membership or adjacency
  conflicts before writing.
- Selective trash restore captures the target ID and observed deletion
  sequence; a newer deletion conflicts, while an already-live target is a
  no-write success. Restoring from Trash retains its mode, ordered selection,
  and the active live filter/query.
- Snapshot reconciliation independently retains/falls back live and trash
  selections, including arrivals into empty views. Search buffers/cursors,
  tab/query state, and contextual Help survive refresh. Edits hidden by
  external completion retain detached drafts and can preserve that completion
  when committing. Hidden-result messages remain consistent with main.
- Read errors and sync recovery inhibit writes while allowing transient search
  controls. Recovery retains a newly opened Search editor and never replays
  selective restoration. Storage errors take footer priority over draft
  conflict hints and restore the configured conflict hint after recovery.

Combined verification passed on macOS with Rust 1.98.0:

- `cargo fmt --check`, strict all-target/all-feature Clippy, and release build.
- `cargo test --locked`: 243 unit tests and 41 CLI tests pass, with the two
  intentional helper/long-measurement tests ignored as described above.
- The PTY suite now covers 12 scenarios, adding lifecycle/print-ID operations
  with two TUIs, live search refresh plus filtered reorder, external trash
  changes, and a live draft hidden by completion. The terminal restoration
  round-trip test uses the actual Session transaction path.
- New domain/session tests cover projected membership guards, deletion-sequence
  guards, search/filter/trash/help preservation, independent selections, and
  selective-restore sync recovery with intervening writes.
- Release-binary PTY validation also passes all 12 scenarios. The manifests
  remain identical to main at `0.1.0-beta.3`, and concurrency notes stay under
  Unreleased. Agent skill lock/retry guidance reflects short transactions.
- All conflict markers and unmerged index entries are cleared; the changes
  remain uncommitted on the original worktree branch. No push or PR was created.

The three original performance artifacts and their binary hashes remain
historical evidence for the pre-integration concurrency binary. Long idle
measurements had not been rerun for the combined code at the end of that
integration validation. Windows/Linux CI has not been triggered from this
worktree.

### Performance comparison: October 5, 2026

The user requested fresh benchmarks to check the performance impact of the
concurrency change. Built an unchanged `707b0ec` release control in an ignored
directory and compared it with the current combined implementation, retaining
exact binary hashes and all raw samples in the
[paired comparison report](../benchmarks/concurrent-usage-comparison.md).
The Cargo manifest, lockfile, compiler, and optimization profile match.
Benchmark batches ran sequentially on this macOS/APFS host.

- 4,100 paired shell invocations, with five warmups and 50 randomized measured
  rounds per case. Plain fixtures reproduce the original all-open short-task
  workload; mixed fixtures include Unicode, completion states, and one-third
  tombstones. Exact output, mutations, and no-op byte/mtime checks pass.
  At 10,000 plain records, add is 17.32 to 17.42 ms and list is 13.08 to
  13.23 ms. Mixed-record lifecycle mutation medians remain within about 2%
  of the control.
- 600 paired TUI actions at 100/1,000/10,000 mixed records. Completion-to-frame
  rises from 12.90 to 16.21 ms at 10,000 records, a 3.31 ms or 25.6% increase;
  Help input remains similar. Every completion is checked in storage.
- Fresh native idle measurements: three 60-second windows after five-second
  warmup, for both binaries with one TUI and for the current binary with two
  TUIs sharing each scope. Every window emits zero terminal output. At 10,000
  records one-TUI CPU rises from 0.0055% to 0.1167% of one core; two current
  TUIs together use about 0.2318%. Logical snapshot reads are about 1.7 MB/s
  per TUI. CPU includes initialization/warmup; physical disk traffic and
  battery impact are not measured.
- Three fresh live-TUI load repetitions retain all 114 successful additions,
  expected texts, consecutive unique IDs, and counters. Paced-add median/p95
  is 27.95/29.47 ms; Help input is 4.00/7.58 ms; eight-writer burst is
  136.56/334.39 ms. Three deliberate held-lock attempts return Busy after
  about 1.006 to 1.008 seconds. The unchanged control rejects a live-TUI
  shell addition, so that workload has no equivalent successful baseline.
- The new reusable paired harness covers CLI/TUI/idle; the native idle harness
  can select one/two clients. The load harness now uses the shared median and
  nearest-rank p95 helper and verifies exact added tasks/counters. All older
  raw reports remain intact. No Rust runtime behavior changed for this audit.

The measured tradeoff is additional large-list TUI save work and continuous
idle byte reads. Any future optimization should retain latest-snapshot writes
under the lock and existing conflict/durability semantics. These measurements
do not isolate internal storage steps or establish other-platform performance.

### Primary benchmark refresh: October 5, 2026

The user requested that the original benchmark page reflect the concurrency
implementation. Reran the five-app comparison through 1,000 tasks and the
10,000-task topydo comparison, with the same compiler/profile and unchanged
comparison-tool binaries. All 3,500 measured invocations and correctness checks
passed. The same Taskbook 0.3.0 10,000-task listing failure was reproduced and
excluded from timing.

[The primary benchmark page](../benchmarks.md) now uses the fresh tables and
PNG/SVG chart from `2026-10-05-expanded` and `2026-10-05-expanded-10000` reports.
The plotting helper includes the measurement date, CLI version, and binary
hash prefix on the chart. October 4 reports/charts remain unchanged under the
historical links. These cross-tool batches remain separate from the paired
before/after concurrency measurements above. No Rust runtime code changed.

### Missing-lock review repair: October 5, 2026

Reproduced the review finding with the measured release binary: removing
`tasks.lock` while retaining `tasks.json` causes a TUI completion to fail with
`could not open lock file`, leaving the task unchanged. The new session write
and sync-recovery regressions also failed before the fix.

Lock acquisition now creates an absent lock file for both presence modes.
`RequireExisting` still applies to the canonical data read under that lock;
missing initialized data is never replaced with an empty list. Read-only
startup and refresh do not create locks, and recovery does not replay a change.
Updated the storage/recovery contract and usage documentation accordingly.

Verification passed on macOS: 244 unit tests (two intentional helper/measurement
tests remain ignored), 41 CLI tests, all 13 release-binary PTY scenarios,
formatting, strict all-target/all-feature Clippy, and the release build.
Coverage includes an absent lock before TUI startup and after a successful
write, sync recovery after an injected post-replacement sync failure with an
intervening deletion, and rejection of missing initialized data in mutation
and recovery paths.

The repaired release binary has SHA-256
`0210c031885d6990e1df4a119cd14f2b0851e9e37a4b161eca1bc2e492a47281`.
Benchmark reports and raw samples retain their measured hashes and numbers;
the primary and paired benchmark pages now identify the samples as predating
this repair. At the end of repair validation, performance benchmarks had not
been rerun after it. Changes remained uncommitted in this worktree.

### Benchmarks after the missing-lock repair: October 5, 2026

The user requested fresh measurements of the repaired release binary. Reran
the full five-app comparison through 1,000 tasks, the shtodo/topydo 10,000-task
extension, paired CLI/TUI comparisons against unchanged beta.3 at `707b0ec`,
one/two-TUI idle measurements, and three concurrent-writer load repetitions.
Timed batches ran sequentially, retaining the original sample counts, warmups,
fixture families, and randomized seeds. The repaired binary's SHA-256 remains
`0210c031885d6990e1df4a119cd14f2b0851e9e37a4b161eca1bc2e492a47281`;
comparison-tool and baseline executable hashes are unchanged.

- All 3,500 shared-workflow CLI invocations, 4,100 paired CLI invocations, and
  600 TUI actions passed correctness checks. At 1,000 tasks, primary shtodo
  list/add/delete medians are 3.73/13.04/13.13 ms; at 10,000 tasks they are
  12.77/17.47/17.43 ms. The Taskbook 10,000-task preflight failure was reproduced
  twice and remains excluded from timings.
- The paired 10,000-record TUI completion median is 12.69 to 16.13 ms, a
  3.44 ms or 27.1% increase. Paired plain-list/add medians are 13.28/17.66 ms
  for the control and 13.53/17.58 ms for the repaired concurrency build.
- All three 60-second idle windows after five-second warmup emitted zero
  terminal bytes per process. At 10,000 records, one-TUI CPU is 0.0062% for
  the control and 0.1246% for the repaired binary, measured over each process's
  lifetime including startup/warmup. Two repaired TUIs together use 0.2559%
  of one core. Logical snapshot reads remain about 1.7 MB/s per TUI.
- All 114 concurrent additions survived with expected text, consecutive IDs,
  and counters. Three deliberate held-lock attempts returned Busy without
  adding a task. Raw load samples and aggregate medians/p95s are retained.

The [primary benchmark page](../benchmarks.md), PNG/SVG chart, and
[paired report](../benchmarks/concurrent-usage-comparison.md) now use
`2026-10-05-repaired` artifacts. Earlier raw samples, tables, and charts remain
intact, and the earlier paired report is archived separately. Fresh batches
are not pooled with earlier samples and do not isolate the missing-lock
repair's performance contribution. No runtime code changed for this refresh.
