# Concurrent usage: transactions and TUI refresh

Status: TODO. Design proposed for review; runtime implementation has not begun.
Inspected baseline: `cb2f95b77c381bfc5c70b21b86db2c31daba3416`, October 4, 2026.
Branch: `feat/concurrent-usage`. Priority: P1. Estimated effort: L, in stages.

This plan is self-contained; a separate spec would duplicate its contracts.
Present the recommendation and material choices to the user before runtime
implementation. This planning turn authorizes documentation only. Future work
must stay in the assigned worktree, accommodate the three parallel branches,
and must not push, open a PR, merge, publish, or release.

## Recommendation

Replace the session-long writer lock with short, synchronous, per-scope
read-modify-write transactions. Every committed action names task IDs and is
applied to a freshly validated snapshot while holding `tasks.lock`. The TUI's
snapshot is a display cache and must never become the input to a storage save.
Install the transaction result only after persistence succeeds.

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
cost of one snapshot read per second per idle TUI; it is a review choice, not a
claim of zero polling or measured performance.

## Evidence from the current checkout

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
a handle or reading a snapshot acquires no writer lock. Create directories and
the existing lock file only when a write transaction needs them.

The sole production mutation entry point should resemble
`Store::mutate(&MutationRequest, LockBudget) -> Result<MutationReply, TransactionError>`.
Names are provisional. Its sequence is mandatory:

1. Validate request syntax/text before creating storage when possible. Parse
   shell arguments and read stdin before waiting for a writer lock.
2. Open the existing, stable `tasks.lock` file and acquire its exclusive lock
   within the caller's acquisition budget.
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
| MoveAdjacent | ID, direction, and the neighbor ID observed in canonical live order, including an explicit boundary expectation | Current TUI; projected-view policy requires coordination |

A reply carries an owned `TaskList` plus an in-memory content token and an
outcome such as `Changed`, `AlreadyInState`, `NothingToRestore`, or `Conflict`.
Include outcome text/IDs from the transaction's latest data so shell messages
cannot accidentally describe stale state. Treat conflicts as typed expected
outcomes carrying the latest snapshot, distinct from storage corruption or
I/O failure. Do not expose the on-disk JSON as a new public output contract.

Use a small `TaskObservation` for destructive interactive expectations
(original text, completion, and deletion state/sequence). Use original text
alone for text edits, allowing unrelated completion/order changes to coexist.
Targeted trash restore should compare the observed deletion sequence so a
stale row cannot restore a newer deletion of the same ID.

Wrap the action in a small request carrying a scope-presence precondition:
`AllowMissing` for a never-created scope or a fresh shell invocation, and
`RequireExisting` for a TUI that has observed a canonical snapshot. Evaluate
this under the lock before treating NotFound as an empty list. This prevents
a racing disappearance from resetting an open TUI's scope/counters without
changing the existing missing-scope shell/list behavior. No cached TaskList is
part of the request.

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

For the search/filter branch, recommend disabling reorder whenever the view
excludes live tasks. This avoids deciding whether J/K crosses hidden matches
or moves relative to canonical neighbors. Navigation/selection still follows
that branch's projected IDs. If it already implements filtered reorder, review
and agree that contract before integration: a projected-neighbor transaction
must reevaluate the same projection on latest storage and validate the ID pair.
Never let filtered row numbers enter storage. This is a material integration
choice, not permission to override that branch's behavior.

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
- Keep storage-error state until a successful refresh/recovery. Do not reset a
  malformed snapshot to an empty list or save cached data over it.

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

On refresh, read canonical bytes once. Equal token means no JSON parse, model
validation, projection rebuild, or redraw. Changed bytes must validate before
installation; semantically identical pretty-print changes update the token
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
warning. A retained add draft must remember its already-allocated ID so Enter
cannot append it again. Recovery retries synchronization of the current canonical
file and directory under the lock, without repeating the action; it must check
latest data because another writer may have intervened. Navigation/cancel/quit
remain available while recovery fails. CLI exits nonzero and states that a
change may already be visible, directing the caller to inspect before retrying.

Successful no-ops and ordinary Busy/conflict results do not rewrite snapshots.
An explicit recovery sync does not change bytes. No command offers exactly-once
execution across a crash or failed stdout delivery; no request log is introduced.

## Integration ownership and order

Only this worktree owns concurrency implementation when authorized. The other
sessions retain ownership of their feature behavior. Do not edit their
checkouts or revert their work. Compare changes against this inspected baseline
before implementing, and coordinate shared contracts before combining branches.

| Branch/responsibility | Likely shared files | Contract to reconcile |
| --- | --- | --- |
| Concurrency | `storage.rs`, `terminal.rs`, mutation/result portions of `app.rs` and `lib.rs`; concurrency test harness | Sole writer boundary, refresh, typed conflicts, no cached-list save |
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
- Reinspect drift from `cb2f95b` in shared files, read parallel feature contracts,
  and agree on the active-view projection/eligibility interface.
- Keep the recommendation in schema 1 unless the user explicitly requires
  strict event-history conflict detection.

Acceptance: one agreed transaction/result contract and documented filtered
reorder policy; unresolved product choices have an explicit answer or accepted
default. Implementation begins only in a subsequent authorized turn.

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
snapshot bytes and file identity unchanged. Current CLI output/tests pass.
Run focused task/storage/lib/CLI tests and strict Clippy.

### Stage 2: TUI mutation submission and snapshot reconciliation

Refactor App transitions to emit intents, add result installation, store editor
base text/conflict state, and render detached drafts. Replace every
`save(app.tasks())` path. Keep keymap-driven hints, footer/help, and fixed Ctrl-C.
Install committed/no-op/conflict snapshots without reconstructing App wholesale.

Acceptance: two cached Apps can edit different tasks without loss; conflicting
edits preserve exact drafts/cursors; deleted/restored targets behave as above;
same desired completion does not reverse; stale reorder cannot undo another
swap; save/lock errors do not exit the TUI or claim success. All input/UI/editor/
selection and celebration regressions pass. At this stage the lifetime lock
may be removed only when every writer uses the transaction API.

### Stage 3: Idle refresh and redraw scheduling

Add snapshot byte tokens, absolute refresh deadlines, dirty redraw state, and
stable refresh error/recovery state to the synchronous event loop. Retain
animation timing without using it as the storage-read interval.

Acceptance: idle shell changes appear without a keypress; continuous input
cannot starve refresh; unchanged data causes no parsing or drawing; equal
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
| Domain | Independent field updates; same-value no-ops; eligibility forbids tombstone edit/complete; text guards; current latest and targeted restore; deletion-sequence guards; canonical adjacency/boundary guards; checked ID/sequence overflow |
| Storage | Fresh load after lock acquisition; lock budgets with controllable clock; independent scopes; no-write bytes/identity; invalid scope/schema/data leaves temp/canonical untouched; failure before replace; sync uncertainty after replace; ignored stale temp; initialized-file disappearance |
| App/UI | Selection retained by ID/fallback ordinal; empty-view arrivals; filter/query/help preservation; unchanged drafts close safely; exact UTF-8 buffer/cursor on edit/delete/restore conflict; detached edit row visible; keymap-derived recovery hints; local-only durable celebration |
| Loop | Fake-clock deadlines; key flood does not postpone refresh; one scheduled read/second; no catch-up burst; zero parses/draws for unchanged bytes; semantic no-change rewrite; resize redraw; animation/refresh separation; refresh errors back off without message churn |
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

## Decisions and review questions

Resolved within this proposal: use the existing per-scope OS lock for short
transactions; all writers reapply ID-based intents to latest storage; never save
App snapshots; preserve the atomic/sync path, schema 1, IDs, cwd semantics, and
read-only list contract; use explicit desired completion, guarded text edits and
canonical adjacent swaps; preserve drafts and view state; no coordinator/runtime/
database dependency.

Material defaults to review before implementation:

| Choice | Recommendation | When user input changes the plan |
| --- | --- | --- |
| Idle freshness/cost | One-second exact-byte polling with no unchanged parse/redraw | If zero periodic file reads or substantially faster updates are required, evaluate native watching and reconciliation |
| Contention | 1-second shell / 100-millisecond TUI acquisition budgets | If scripts need different waiting behavior, adjust budgets; no new config surface initially |
| Edit history | Guard current text/lifecycle values; observed deletion invalidates draft | If every unseen edit/delete/restore cycle must conflict, require task generations and a migration design |
| Filtered reorder | Disable when live rows are excluded until a projection-aware contract is agreed | User/feature owner chooses canonical-neighbor or projected-neighbor behavior if reorder must work in filtered views |
| Destructive interactive conflict | Guard delete against changed displayed task values | User may prefer ID-only delete; shell delete keeps its existing latest-by-ID semantics |

No additional user information is needed to complete this planning deliverable.
The one-second polling and conflict policies are proposed product defaults, not
previously approved behavior. Filtered reorder needs explicit reconciliation
with the parallel search/filter branch before combined implementation. Any
request for stricter history semantics or zero polling must be resolved before
silently broadening dependencies or changing schema.

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
platforms rather than assume identical handle semantics. Crossterm 0.29 requires
poll/read on the same thread in its
[event documentation](https://docs.rs/crossterm/0.29.0/crossterm/event/index.html).
[Notify's documentation](https://docs.rs/notify/8.2.0/notify/) describes editor
replacement events and notification limitations, supporting the need for
reconciliation if watching is later adopted. These API facts support this
design, not proof of its unimplemented behavior.

Planning verification on macOS with Rust 1.98.0, October 4, 2026:

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo test --locked`: passed, 125 unit tests and 24 CLI tests.
- `cargo build --release --locked`: passed.
- `git diff --check`: passed; local Markdown links and no-emdash constraint
  checked for both changed documents.
- Only this plan and `docs/plans/README.md` changed. Runtime code, dependencies,
  user usage/configuration docs, changelog, and historical benchmarks are intact.

The first Clippy attempt could not unpack a dependency into the read-only shared
Cargo cache. Clippy/tests/build then passed with an isolated `CARGO_HOME` under
this worktree's ignored `target/concurrency-cargo-home`; no shared-cache write or
real task-data smoke was needed. These are baseline checks, not concurrency
validation. Concurrency acceptance tests, platform evidence, and refresh
performance measurements remain to be implemented in the stages above.
