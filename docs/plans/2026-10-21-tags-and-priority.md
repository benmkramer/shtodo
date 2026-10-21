# Tags and priority: derived todo.txt tokens with TUI and shell filtering

Status: IN PROGRESS. Planning only; no runtime code has changed. Revised
after review cycle 1 (October 21, 2026).
Date: October 21, 2026. Inspected baseline: `1f2421c` (feat: add JSON output
for shell task commands, #16) on branch `feat/tags-and-priority`.
Priority: P1. Estimated effort: M, in stages. Depends on: concurrent usage
(DONE) and the Unreleased JSON output contract.

This plan is self-contained; a separate spec would duplicate its contracts.
Implementation begins only after the owner reviews this plan and answers or
accepts defaults for the open questions at the end.

## Recommendation

Keep priority and tags inside the task text and derive them on demand. A
leading `(A) ` through `(Z) ` is the priority. Whitespace-delimited tokens that
start with `+` or `@` are tags, after trailing `.,;:!?` is stripped, as long as
at least one character follows the sigil. Do not add
stored fields, change on-disk schema 1, or migrate beta.4 data. Old and new
binaries keep reading and writing the same `tasks.json`, so mixed-version use
from the concurrent-usage plan still works. A future todo.txt export stays
cheap because the text already uses todo.txt syntax.

Add one shared, allocation-free token module and one shared filter predicate.
`shtodo list` and the TUI both use them. Shell `list` gains composable
`--open`, `--done`, `--tag`, `--priority`, and `--search` filters, honored
identically by `--json`. JSON task objects gain two always-present additive
fields, `priority` and `tags`. `schema_version` stays `1`: the documented v1
contract already tells clients to tolerate additional fields. Later due dates
and parent IDs fit the same additive rule, so the planned slices need no
schema bump.

In the TUI, highlight priority and tags in place in each row. Add a
toggleable priority sort on `s` and a tag/priority filter editor on `f`. Both
compose with `/` search and All/Open/Done tabs. Both new defaults give way to
existing user bindings, following the search-key upgrade rule. While you type
in the filter editor, the last token matches tags by prefix, and a hint line
lists the tags present in the current view. That covers tag discovery. Agents
discover tags from the `tags` field in `list --json`; a separate `shtodo tags`
command is deferred.

## Evidence from the inspected baseline

| Boundary | Verified behavior | Consequence |
| --- | --- | --- |
| [`src/task.rs`](../../src/task.rs), `Task` and `TaskList` | `#[serde(deny_unknown_fields)]` on both; `SCHEMA_VERSION = 1`; fields are `id`, `text`, `completed`, `deletion_sequence` | Any new stored field makes beta.4 reject new snapshots, and an old TUI open on the same list would fail to refresh |
| [`src/storage.rs`](../../src/storage.rs), `parse_snapshot` | Probes `schema_version` and rejects anything except 1, then validates | Extracted fields would need schema 2 plus a migration, and older binaries would stop reading the list |
| [`src/task.rs`](../../src/task.rs), `validated_text` | Trimmed, non-empty, no `\n`/`\r`; any other characters allowed | Tokens can live in text with no validation change; a priority marker is always at byte 0 after trimming |
| [`src/projection.rs`](../../src/projection.rs) | `Projection { view, lowercase_query }`; `matches` checks tombstones, view, then a lowercase substring; `adjacent` walks the canonical order | The single place to add a tag/priority predicate shared by the App, reorder guards, and (new) the shell |
| [`src/mutation.rs`](../../src/mutation.rs), `MoveAdjacent` | Carries an owned `Projection`; adjacency is rechecked under the lock | The filter must become part of `Projection` so the latest-snapshot guard matches the displayed rows |
| [`src/app.rs`](../../src/app.rs), `App` | Transient `view`, `query`, `lowercase_query`, `search_session`; `visible_tasks` filters in canonical order; `reconcile` preserves them | Filter and sort state are more transient fields of the same kind; selection fallback logic is reused |
| [`src/ui.rs`](../../src/ui.rs), `task_item`, `render_view` | Row is marker plus one text span; `render_content` builds a `ListItem` for every visible task each frame; the status line is `· N matches · /query` | Highlighting adds spans per row; status line must show filter and sort state at 40 columns |
| [`src/input.rs`](../../src/input.rs), `DEFINITIONS`, `defaults_yield_to_overrides`, `resolve_open_trash_default` | Normal defaults use `j k Down Up J K i e Space d u / Tab Shift-Tab Esc t ? q`; Ctrl-C is fixed and reserved; conflicts are per mode | `s` and `f` are free Normal keys; new defaults must yield to explicit bindings so existing configs still load |
| [`src/config.rs`](../../src/config.rs), `doctor_report`; [`tests/cli.rs`](../../tests/cli.rs) | Reports configurable action and active binding counts; tests assert `35 configurable actions, 47 active bindings` | Two new actions change these counts; tests and docs must be updated |
| [`src/cli.rs`](../../src/cli.rs), `parse_invocation`/`parse_args` | `list` takes no arguments; `--json` is stripped before the command, right after it, and from the end past `positional_count` | `list` needs its own argument scanner so a value such as `--search --json` is not taken as a flag |
| [`src/lib.rs`](../../src/lib.rs), `list_tasks` | Read-only `load_read_only`, then prints or `json::write_list`; text-mode errors return `Err` from `main` (exit status 1); JSON errors return `ExitCode::FAILURE` (1) | Filtering is an iterator adapter on this read-only path; exit codes need no new mechanism |
| [`src/json.rs`](../../src/json.rs) | `Task<'a>` borrows `text`; `CliError` maps to `invalid_arguments`; JSON `SCHEMA_VERSION = 1` independent of storage | New fields can be borrowed `&str` slices; filter usage errors map to the existing code |
| [`CHANGELOG.md`](../../CHANGELOG.md) | JSON output is listed under Unreleased, not in beta.4 | No released client depends on the exact v1 task object yet; additive fields are low-risk |
| [`docs/plans/README.md`](./README.md), rejected findings | "Shell list filters and search are deferred because agents can filter … themselves" | This plan reverses that deferral at the owner's request; implementation must update that bullet |
| [`docs/benchmarks.md`](../benchmarks.md) | Repaired build: 10,000-task `list` median 12.77 ms, `add` 17.47 ms; TUI completion-to-frame 16.13 ms at 10,000 | Baselines for the performance gate |

## Alternatives for storing priority and tags

| Approach | Benefits | Costs and failure modes | Decision |
| --- | --- | --- | --- |
| Tokens stay in text; derive on demand | No schema or migration; beta.4 and new binaries share files both ways; edit/add/restore need no changes; text is already todo.txt syntax; one source of truth | Literal text can accidentally form a tag (`+1`, `@here`); a priority change means a text edit; every reader scans text when it needs the metadata | Recommend |
| Tokens stay in text plus an in-memory cache filled at load (`#[serde(skip)]` field) | Parses each task once per load | Pays the parse cost on every load, including plain `list` and every refresh, even when no feature uses it; adds cache-consistency code to `add`/`edit` | Reject for now; derive lazily (see Performance) |
| Extract into stored fields (`priority`, `tags`) and strip from text | Structured edits; no accidental tags | Schema 2 and a migration; `deny_unknown_fields` makes beta.4 fail on new files, and an old TUI stops refreshing; export must reassemble text; two sources of truth for edits | Reject |
| Stored fields while also keeping text | Fast structured reads | Same compatibility break as above, plus fields and text can disagree | Reject |

The derived design also covers later work. todo.txt due dates are `due:YYYY-MM-DD`
text tokens, so they can be derived the same way. Parent IDs may need a stored
field. That would be a separate, explicit storage migration decided then; it
does not affect this slice.

## Contracts

### Token grammar

All parsing is on canonical (already validated, trimmed) task text.

- **Priority**: the text's first four bytes are `(`, an ASCII uppercase letter
  `A` to `Z`, `)`, and an ASCII space. Text that is exactly `(A)`, a lowercase
  `(a) `, `(AA) `, or a marker anywhere other than the start is not a priority.
  This matches todo.txt.
- **Tags**: split the text on Unicode whitespace (`str::split_whitespace`). A
  token that starts with `+` (project) or `@` (context) has any trailing ASCII
  `.`, `,`, `;`, `:`, `!`, and `?` stripped. It is a tag if at least one
  character remains after the sigil. So `ship it +shtodo.` has the tag
  `+shtodo`, and `+.` has no tag. Punctuation inside a tag stays (`+a.b`).
  A `+` or `@` inside a token (`C++`, `bob@example.com`) does not start a tag.
- Stripping trailing punctuation goes beyond strict todo.txt, where `+shtodo.`
  is the project `shtodo.`. It is the default because people end sentences
  with a tag. It also matches what users already get from todo.sh: `todo.sh ls
  +shtodo` is a substring grep, so it matches `ship it +shtodo.` too. Reporting
  and matching use the stripped form. A later todo.txt export writes the text
  unchanged, so nothing is lost.
- Tags are reported in text order and deduplicated on exact equality, keeping
  the first occurrence. Accepted filters match exactly and case-sensitively
  (see open question 2). The only prefix matching is the Filter editor's live
  preview (see TUI).
- Completion state is still the `completed` field. A todo.txt `x ` prefix in
  text has no meaning. Completing a task does not remove its priority (todo.txt
  convention does remove it; export can handle that later).

Proposed module `src/metadata.rs`. These are pure functions on `&str` with no
allocation:

```rust
//! todo.txt-style tokens derived from canonical task text.

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Priority(u8);

impl Priority {
    pub(crate) fn letter(self) -> char {
        char::from(self.0)
    }
}

pub(crate) fn priority(text: &str) -> Option<Priority> {
    match text.as_bytes() {
        [b'(', letter @ b'A'..=b'Z', b')', b' ', ..] => Some(Priority(*letter)),
        _ => None,
    }
}

const TRAILING_PUNCTUATION: [char; 6] = ['.', ',', ';', ':', '!', '?'];

pub(crate) fn tags(text: &str) -> impl Iterator<Item = &str> {
    text.split_whitespace()
        .filter(|token| token.starts_with(['+', '@']))
        .map(|token| token.trim_end_matches(TRAILING_PUNCTUATION))
        .filter(|tag| tag.len() > 1)
}
```

The returned slices borrow the text, so row styling can highlight exactly the
tag and leave the stripped punctuation unstyled.

`Priority` sorts `A` before `Z`. Sorting by priority puts tasks with no
priority after all prioritized tasks (see TUI sort).

### Filter model shared by shell and TUI

Add to `src/metadata.rs`:

```rust
/// Bits 0..26 are A..Z; bit 26 is "no priority". Empty means any priority.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PrioritySet(u32);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TaskFilter {
    pub(crate) tags: Vec<String>,
    pub(crate) priorities: PrioritySet,
}
```

`TaskFilter::matches(&self, text: &str) -> bool` returns true when every
required tag is among `tags(text)` (logical AND) and, if `priorities` is
non-empty, `priority(text)` is in the set (logical OR). The empty filter does
not scan the text.

`projection::Projection` gains `pub(crate) filter: TaskFilter`.
`projection::matches` changes from `(task, view, lowercase_query)` to taking
`&Projection`. It checks, in this order: tombstone, view, filter, then the
existing lowercase substring search (the most expensive check last). The App,
`Mutation::MoveAdjacent`'s guard under the lock, and the shell `list` all call
this one predicate. Search still matches the full literal text, tokens
included.

One token parser serves both the shell flag values and the TUI filter editor:

| Input token | Meaning |
| --- | --- |
| `+x`, `@x` (normalized like derived tags: trailing `.,;:!?` stripped, then at least one character after the sigil) | Required tag, exact; `--tag +shtodo.` equals `--tag +shtodo` |
| `A` to `Z`, ASCII case-insensitive | Priority letter |
| `A-C` (start not after end, case-insensitive) | Priority range, inclusive |
| `none`, ASCII case-insensitive | Tasks without a priority |

Anything else is invalid: a bare `work`, `+`, `@`, `C-A`, `AA`, or a value
containing whitespace.

### Shell `list` filters

```text
shtodo [--local] list [--open | --done] [--tag <TAG>]... [--priority <P>]... [--search <TEXT>] [--json]
```

- Flags follow `list` in any order. Each value is the next argument taken
  exactly as given, even if it starts with `-`. So `list --search --json`
  searches for the text `--json`. An `--json` in a flag position selects JSON
  mode, as it does for the other commands. `--flag=value` forms are not
  supported and fail as unsupported arguments.
- `--open` shows only open tasks and `--done` only done tasks; they map to
  `TaskView::Open`/`Done`. With neither, all live tasks are shown (today's
  behavior).
- `--tag` repeats as AND: the task must have every listed tag.
  `--priority` repeats as OR: union of the listed letters, ranges, and `none`.
  Categories combine with AND.
- `--search` appears at most once. It is a literal substring match after
  Unicode lowercasing, the same as TUI `/`.
- Output order stays canonical; filtering never reorders. Text rows keep
  exactly `ID  open|done  text`. Tombstones are always omitted.
- A filter that matches nothing is success: empty stdout, or `"tasks":[]`,
  exit status 0. Reads remain side-effect free: no directories, lock, or
  config. Every argument error is detected before storage is touched.

Exit statuses and JSON error codes (no new codes are introduced):

| Case | Exit | JSON `error.code` | Message (text mode and `error.message`; usage follows) |
| --- | --- | --- | --- |
| Success, including no matches | 0 | none | none |
| Unknown flag, `--flag=value`, or stray positional (`list extra`, `list --wat`) | 1 | `invalid_arguments` | `unsupported shtodo arguments: "--wat"` (existing variant) |
| `--tag`, `--priority`, or `--search` as the last argument | 1 | `invalid_arguments` | `--tag requires a value` |
| `--tag work`, `--tag +`, `--tag +.`, `--tag "+a b"` | 1 | `invalid_arguments` | `invalid --tag "work"; expected +project or @context` |
| `--priority AA`, `--priority C-A`, `--priority ""` | 1 | `invalid_arguments` | `invalid --priority "AA"; expected a letter A-Z, a range such as A-C, or none` |
| `--search ""`, whitespace-only, or multiline | 1 | `invalid_arguments` | `--search requires non-empty single-line text` |
| `--open` with `--done` | 1 | `invalid_arguments` | `--open and --done cannot be combined` |
| A repeated `--open`, `--done`, or `--search` | 1 | `invalid_arguments` | `--search may only be specified once` |
| A repeated `--json` | 1 | `invalid_arguments` | existing `--json may only be specified once` |
| Non-UTF-8 filter value | 1 | `invalid_arguments` | `invalid --tag value; expected UTF-8` |
| Snapshot read or validation failure | 1 | `command_failed` | existing storage message |

Rejecting empty search and tag values is deliberate: a shell variable that is
unset must not quietly become "match everything" for an agent. New
`CliError` variants: `MissingFilterValue { flag }`, `InvalidFilterValue {
flag, value }`, `ConflictingStateFilters`, and `DuplicateListFlag { flag }`.
Because they are `CliError`s, `json::error_details` already maps them to
`invalid_arguments`, and `Display` already appends usage.

`cli::Command::List(ScopeChoice)` becomes `List(ScopeChoice, ListFilter)`,
where `ListFilter { view: TaskView, filter: TaskFilter, search: Option<String> }`.
`parse_invocation` hands everything after `list` to a dedicated
`parse_list_args` that consumes flag/value pairs and counts `--json` only in
flag positions. The generic trailing-`--json` loop must not run for `list`,
or it would eat a `--search --json` value. `lib::list_tasks` builds one
`Projection` and filters `visible_tasks()` through `projection::matches`.

### JSON output

Every task object, in `list` and in every mutation reply (including `delete`
and tombstones), gains two always-present fields:

```json
{"id":1,"text":"(A) Fix the bug +shtodo @laptop","state":"open","deleted":false,"priority":"A","tags":["+shtodo","@laptop"]}
```

| Field | Contract |
| --- | --- |
| `priority` | A one-character JSON string `"A"` to `"Z"`, or `null` when the text has no leading marker; never a number, never the `(A)` marker |
| `tags` | Array of normalized tag strings (sigil kept, trailing `.,;:!?` stripped), in text order, deduplicated; `[]` when none |

- `schema_version` stays `1`. The v1 contract already says "tolerate additional
  fields within that version", JSON is unreleased, and `text` keeps its exact
  meaning (still literal and still round-trips through `edit`).
- Schema policy, to be written in usage.md: new task metadata is always added
  as an always-present field, nullable or array, and is never inferred from an
  absent field. `schema_version` changes only if an existing field changes
  meaning, a field is removed, or an enum gains values clients must handle
  (such as a third `state`). The field names `due` (string or null) and
  `parent_id` (number or null) are reserved for planned slices. They are not
  emitted until implemented, which avoids committing their semantics now.
- `json::Task<'a>` gains `priority: Option<char>` and `tags: Vec<&'a str>`,
  borrowed from the task text. serde serializes `char` as a one-character
  string and `None` as `null`. A `src/json.rs` unit test pins this exact form. The list reply does not echo the filter (open
  question 5). An older binary given filter flags fails closed with
  `invalid_arguments`; it never returns unfiltered data.

### TUI

**Rows.** Keep the text exactly as typed and style the token spans in place:
priority marker bold, `+project` one color, `@context` another. Completed rows
keep today's dim/strikethrough on every span. No columns or reformatting, so
row width and the editor's byte cursor mapping do not change. Trash rows get
the same styling.

**Sort.** `TaskOrder { Manual, Priority }` is transient App state, starting at
Manual every session (like the view and query). Priority order is stable: `A`
to `Z`, then no priority, with canonical order breaking ties. Done and open
tasks are not separated; the Open tab already does that.
- `j`/`k` follow the displayed order. Selection keeps the task's identity when
  sort is toggled.
- While Priority order is on, `J`/`K` submit no mutation. They show
  `Reorder uses manual order; press s to switch`, with the key taken from the
  keymap. Manual order is the canonical vector, and a priority-sorted swap has
  no clear meaning. This keeps `Mutation::MoveAdjacent` and its storage guard
  unchanged except for the filter field.
- The `TaskOrder` lives on `App`, not `Projection`, because storage never needs
  it.

**Filter.** `f` opens `Mode::Filter`. It uses the shared text editor exactly as
Search does: `Keymap::bindings_for` maps Filter to Insert bindings and
`config::mode_name` maps it to `"insert"`. A `FilterSession` saves the
previous filter and selection. The buffer holds space-separated tokens from
the shared grammar, for example `+shtodo @laptop A-B`.

Live preview, chosen over Enter-only. With exact matching and a live preview,
typing `+sh` would show zero rows until `+shtodo` was complete, so the list
would blank on every keystroke. Enter-only avoids that but drops the
as-you-type feedback that `/` search already gives, and gives no way to
discover tags (below). The choice: while `Mode::Filter` is editing, the
trailing in-progress token matches by prefix, and everything else is exact.
- The trailing token is in progress while the buffer does not end in
  whitespace. If it is a tag token, it matches any of the task's tags that
  start with it (`+sh` matches `+shtodo` and `+shell`). A sigil alone (`+`,
  `@`) previews no tag constraint. An in-progress priority token that does not
  parse yet (`A-`) is left out of the preview without an error.
- Earlier, completed tokens are exact. If one of them is invalid, the last
  valid preview stays and the line shows the invalid token.
- The preview is App-only state: `TaskFilter` gains `tag_prefix:
  Option<String>`, set only in the Filter-mode preview. The shell never
  constructs it, and no `Mutation` projection carries it. No mutation can be
  submitted while the editor is open, and accepted filters always have
  `tag_prefix: None`.
- Enter accepts exactly what was typed; the prefix is not carried over. So
  accepting `+sh` filters by the exact tag `+sh`. To prevent a surprise empty
  list, the hint line shows the tags the prefix matches, so the user can finish
  the word. Tab completion is not in this slice (open question 9). An empty
  buffer clears the filter. Enter on a buffer with an invalid token stays in
  the editor and shows
  `Invalid filter "foo"; use +project, @context, A, A-C, or none`.
- Escape restores the previous filter and selection, as Search does.
- Covered by the `src/app.rs` test
  `filter_editor_should_preview_trailing_tag_by_prefix_and_accept_exactly`
  (see Test plan).

**Tag discovery.** While `Mode::Filter` is open, the line below the editor
lists the distinct tags in the rows the preview shows if the trailing token is
ignored. They are narrowed to tags starting with the trailing tag prefix, if
there is one. Order is by count, highest first, then by name. They are
clipped to the terminal width with `…` and shown with counts, for example
`+shtodo 12 · +shell 3 · @laptop 2`.
- Cost: one `tags` scan over the visible rows and a small `HashMap<&str, usize>`
  on each Filter-mode keystroke and refresh. There is no cost outside Filter
  mode and no stored index. At 10,000 rows this is one scan per keystroke, the
  same order of work as live `/` search, and it goes through the same TUI
  measurement gate.
- In this slice, as part of Stage 3c.
- Agents use the `tags` array already present in `list --json` (for example
  `shtodo --json list | jq '[.tasks[].tags[]] | group_by(.) | map({tag: .[0], count: length})'`).
  A read-only `shtodo [--local] tags [--json]` command is deferred (open
  question 10). It would need a new `Command` variant, argument parsing, usage
  text, a JSON reply (`{"schema_version":1,"command":"tags","scope":…,"tags":[{"tag":"+shtodo","count":12}]}`),
  and CLI and JSON tests: small, but more public surface than the stated goal
  of keeping shtodo small requires today.
- In Normal mode, Esc (`clear_search`) clears the accepted search first. If no
  search is active, it clears the filter. Each press clears one layer.

**Composition and state.** Tabs, search, filter, and sort combine. The status
line becomes `· N matches · /query · f:+shtodo A-B · by priority`; the filter
and sort segments appear only when active. Clipping at 40 columns removes
`by priority` first, then truncates the filter with `…`. The match count is
always kept. "No matching tasks" hints include the configured filter and
clear keys. Filter, sort, filter editor, and session survive refresh and
reconcile, and opening and closing Trash or Help. Trash still ignores the
view, search, filter, and sort. The hidden-task messages for add, edit,
restore, and completion now mention the filter too ("hidden by the current
view, search, or filter").

**Keybindings.**

| Mode | Config name | Default | Collision check |
| --- | --- | --- | --- |
| Normal | `toggle_sort` | `s` | Not a Normal default; Trash/Insert/Help conflicts are per mode |
| Normal | `edit_filter` | `f` | Not a Normal default; `f` appears only in config examples and tests as a user binding |

- Add both IDs to `BindingId::defaults_yield_to_overrides`. If an existing
  config binds `s` or `f` in Normal mode, the config still loads, the new
  default is left unbound, and Help shows `Unbound: toggle_sort (config)`.
  This follows the search-key upgrade rule. There is no implicit fallback
  chord like `open_trash` has; one precedent is enough.
- Ctrl-C stays fixed. An explicit binding of either action still goes through
  ordinary conflict and reserved-key validation.
- Existing fixtures already exercise the yield rule on these keys and must
  pass unchanged:
  - `src/config.rs`, `load_should_resolve_search_and_view_actions_and_shared_editor_keys`:
    `start_search = ["s"]`.
  - `src/config.rs`, `load_should_preserve_existing_normal_bindings_that_claim_new_search_defaults`:
    `toggle_complete = ["f"]` among other keys.
  - `tests/cli.rs`, `doctor_should_accept_existing_bindings_on_new_search_default_keys`:
    `toggle_complete = ["f"]`.
  - `src/input.rs`: `search_actions_should_support_overrides_and_detect_existing_key_conflicts`,
    `existing_explicit_bindings_should_claim_all_new_search_defaults`, and
    `explicit_search_and_legacy_bindings_should_still_conflict_in_either_source_order`.
    The last one still expects exactly one issue.
- With a config file, `shtodo doctor` reports 37 configurable actions. The
  `move_down = ["x"]` fixture in `tests/cli.rs` goes from 47 to 49 active
  bindings. The existing upgrade fixture `toggle_complete = ["f"]` must keep
  loading, with `edit_filter` unbound. No new doctor note is needed: unbound
  search actions already set the precedent of appearing in Help only.

### Effect on existing commands and behavior

| Area | Effect |
| --- | --- |
| `add` / `add --print-id` | Unchanged parsing and storage; tokens are text. `--print-id` still prints only the ID. JSON add reply includes `priority`/`tags` |
| `edit` | Priority and tag changes are text edits. Unchanged-text detection, TUI original-text conflict guards, and detached drafts are unchanged. Editing away a tag in a filtered view uses the existing "hidden" message path |
| `done` / `reopen` | No token changes; priority stays on completion |
| `delete` / `restore` / Trash | Tombstones keep their text, so metadata comes back on restore. Trash shows highlighted rows, ignores filter and sort, and keeps today's ordering |
| Concurrent refresh | Unchanged byte-token skip; no parsing on unchanged bytes. Metadata is not computed during refresh, only when rendering or matching. Filtered `MoveAdjacent` carries the filter, so latest-snapshot adjacency guards match the displayed rows |
| Mixed versions | beta.4 and new binaries share snapshots both ways. An old binary given filter flags returns `invalid_arguments` |
| Celebration | Unchanged; still counts every open task, including ones hidden by a filter |
| Agent skill | Use `--json list` with filter flags instead of filtering on the client; parse `priority`/`tags`; set priority or tags by editing text; on `invalid_arguments` from an older binary, fall back to an unfiltered list and filter client-side |

### Performance

- Plain `list` and `list --json` without filters skip the filter entirely. Text
  output does no token work. JSON does one `priority` byte check and one
  `split_whitespace` scan per task, with a small borrowed `Vec<&str>`. That is
  the only new cost on the unfiltered path.
- Filtering checks cheapest first: tombstone, view, priority (constant time),
  tags (one scan, and only if requested), then search (allocates a lowercase
  copy, as today).
- No load-time cache and no stored fields, so storage parsing and refresh cost
  stay the same.
- TUI Priority order: collect the visible IDs, then stable-sort by
  `(priority.is_none(), priority)`. Cache this `Vec<TaskId>` on `App` and
  invalidate it on `reconcile`, filter, view, query, or sort changes, so
  navigation and rendering do not re-sort every keypress.
- Row styling adds a few spans per row, and `render_content` builds an item for
  every visible task. If the 10,000-task TUI gate fails, restrict styled items
  to the viewport window before considering anything else.

Gates (release build, paired against `1f2421c` with
`scripts/benchmark_concurrency_comparison.py`, same fixtures and seeds):

- 10,000-task plain `list` median within 5% of control and within 0.5 ms
  absolute. `list --json` (add it as a case) within 10% and 1.5 ms.
- New cases for information only: `list --tag +x`, `list --priority A-C`, and
  `list --search x` at 1,000 and 10,000 tasks, with a fixture where about half
  the tasks carry tokens.
- TUI completion-to-frame at 10,000 tasks within 5% in Manual order; measure
  Priority order and report it.
- Idle refresh: zero parses and zero redraws on unchanged bytes still hold.

## Staged implementation and acceptance gates

### Stage 0: Review and freeze contracts

Owner reviews this plan, answers or accepts the open questions, and approves
implementation. Recheck drift from `1f2421c` in `cli.rs`, `json.rs`,
`projection.rs`, `app.rs`, and `input.rs`.
Acceptance: an agreed grammar, flag table, JSON fields, and keys.

### Stage 1: Token module and shared predicate

Write failing tests first. Add `src/metadata.rs` (`Priority`, `priority`,
`tags`, `PrioritySet`, `TaskFilter`, `parse_filter_token`) and extend
`Projection`/`projection::matches`. Update existing callers (App, mutation
reorder guard, session tests) to pass `TaskFilter::default()`. No behavior
change visible to users.
Acceptance: unit tests pass; all existing tests pass unchanged; Clippy strict.

### Stage 2: Shell filters and JSON fields

Add `parse_list_args`, the `CliError` variants, `Command::List(_, ListFilter)`,
filtered `list_tasks`, and the JSON `priority`/`tags` fields. Update `usage()`.
Acceptance: every row of the exit/error table has a CLI test in text and JSON
mode; no-storage-created holds for usage errors; existing exact list output
holds.

### Stage 3a: Styled rows and the two bindings

Add styled `task_item` (live and Trash rows), the `ToggleSort` and
`EditFilter` `BindingId`s, `Action`s, `DEFINITIONS` entries, config names,
`defaults_yield_to_overrides` membership, and Help/footer descriptions. Update
the doctor counts. Until 3b and 3c land, the actions show a `not available
yet` message and change nothing; no other behavior changes.
Acceptance: ui styling tests; input/config tests including the named
yield-rule fixtures (all pass unchanged and report `toggle_sort`/`edit_filter`
unbound where their keys are claimed); doctor count tests at 37/49; every
earlier test passes. A STOP may land here, and the tree stays green.

### Stage 3b: Priority sort

Add `TaskOrder`, the cached sorted `Vec<TaskId>` and when it is invalidated,
`j`/`k` in displayed order, selection identity across toggles, reconcile and
selection fallback in displayed order, the `by priority` status segment, and
`J`/`K` disabled with a keymap-derived message under Priority order.
Acceptance: app sort tests, ui status and clipping tests, and session tests
showing refresh invalidates the cached order and unchanged bytes cause no
parse or redraw while sorted. `Mutation::MoveAdjacent` is never submitted
while sorted. A STOP may land here.

### Stage 3c: Filter editor and tag discovery

Add `Mode::Filter`, `FilterSession`, live preview with the trailing-token
prefix, exact acceptance, the invalid-token message, the tag-discovery hint
line, layered Esc in Normal mode, the `f:` status segment, filter-aware
"hidden" messages and "No matching tasks" hints, and the filter carried in
`MoveAdjacent`'s `Projection`.
Acceptance: app filter tests including
`filter_editor_should_preview_trailing_tag_by_prefix_and_accept_exactly`;
input tests showing Filter mode uses Insert bindings; session tests for
editor buffer, cursor, and filter across refresh; mutation tests for the
filtered adjacency guard; the PTY smoke scenario passes.

### Stage 4: Measurements

Run the paired benchmarks above and record a short report under
`docs/benchmarks/` without overwriting earlier samples.
Acceptance: gates met, or STOP.

### Stage 5: Docs and final verification

Complete the documentation checklist, then run:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo build --release --locked
git diff --check
```

Acceptance: all gates pass; release-binary smoke uses a temporary home and an
exact temporary cwd. Mark this plan DONE and update the index row only after
the behavior and evidence are present.

## Test plan

| File | Cases |
| --- | --- |
| `src/metadata.rs` (unit) | Priority: `(A) x` yes; `(Z) x` yes; `(A)` alone, `(a) x`, `(AA) x`, `x (A) y`, `(A)x` no. Tags: `+p`, `@c`, `+` and `@` alone ignored, `C++` and `a@b.c` ignored, `ship it +shtodo.` gives `+shtodo`, each of `.,;:!?` and runs like `+x?!` stripped, `+.` no tag, inner punctuation `+a.b` kept, other trailing characters such as `)` kept, Unicode tags (`+東京!` gives `+東京`), tabs and other Unicode whitespace as separators, dedupe after normalization keeps first (`+x +x.` gives one tag). `--tag +shtodo.` normalizes to `+shtodo`. Filter tokens: letters (both cases), ranges, reversed range rejected, `none`, bare words rejected. `PrioritySet` membership including `none`; empty set matches any. `TaskFilter` AND over tags, OR over priorities |
| `src/projection.rs` (unit) | `matches` order and composition of view, filter, and search; `adjacent` skips filtered-out rows; hidden and tombstone slots keep their positions under filtered reorder |
| `src/mutation.rs` (unit) | `MoveAdjacent` with a filter: a neighbor that leaves the filter conflicts; a newly matching row between the pair conflicts; unrelated changes coexist |
| `src/cli.rs` (unit) | `parse_list_args`: every flag, any order, repeats, `--search --json` taken as a value, `--json` in flag positions in front/middle/end, every row of the error table, both scopes; `parse_invocation` keeps the format for list usage errors |
| `src/json.rs` (unit) | Task object includes `priority` serialized exactly as `"A"` or `null` (a one-character string, not a number or `"(A)"`) and `tags` (`[]` or normalized values) for live and deleted tasks |
| `tests/cli.rs` | Text `list` with each filter and combinations; canonical order kept; empty match exits 0 with empty stdout; usage errors exit 1 with usage text and create no storage; filters ignore invalid keybinding config; doctor counts 37 actions and 49 bindings for the `move_down` fixture; the `toggle_complete = ["f"]` fixture still loads; exact unfiltered output unchanged |
| `tests/json.rs` | `list --json` with filters; new fields on list and on add/edit/done/reopen/delete/restore replies; every error-table row returns `invalid_arguments`, empty stdout, exit 1; missing scope returns `[]` without creating storage; a filter read works while a writer holds the lock |
| `src/input.rs` (unit) | Defaults map `s`/`f` in Normal only; explicit `toggle_sort`/`edit_filter` conflicts and `ctrl-c` are rejected; Filter mode uses Insert bindings and treats `q`, `f`, `s`, `/`, Space as text. Existing tests pass unchanged, with added assertions: `search_actions_should_support_overrides_and_detect_existing_key_conflicts` and `existing_explicit_bindings_should_claim_all_new_search_defaults` report `EditFilter` in `unbound_actions_for(Mode::Normal)` because `f` is claimed; `explicit_search_and_legacy_bindings_should_still_conflict_in_either_source_order` still returns exactly one issue |
| `src/config.rs` (unit) | `toggle_sort`/`edit_filter` parse under `[keybindings.normal]` and are unknown in other modes; doctor report counts. `load_should_resolve_search_and_view_actions_and_shared_editor_keys` (`start_search = ["s"]`) still loads with `StartSearch` on `s` and `ToggleSort` reported unbound. `load_should_preserve_existing_normal_bindings_that_claim_new_search_defaults` still loads for every key; for `f` it reports `EditFilter` unbound and `ToggleComplete` on `f` |
| `src/app.rs` (unit) | `filter_editor_should_preview_trailing_tag_by_prefix_and_accept_exactly`: tasks with `+shtodo`, `+shell`, `+other`. Typing `+`, `+s`, `+sh`, through `+shtodo` never leaves the `+shtodo` task out of the preview; `+sh` previews both `+shtodo` and `+shell`; `+sh ` (trailing space) is exact and previews none; `@x +sh` keeps `@x` exact; Enter on `+sh` accepts the exact tag `+sh` with no prefix kept; the accepted `Projection` has `tag_prefix: None`. Also: the hint line lists matching tags with counts in the specified order; an invalid completed token keeps the last valid preview; an in-progress `A-` is left out of the preview without an error; Enter rejects an invalid buffer; Escape restores filter and selection; layered Esc clear; sort toggle keeps selection identity; `j`/`k` follow sorted order; `J`/`K` under Priority order submit nothing and show message; hidden-by-filter messages for add/edit/restore/complete; filter and sort survive Trash/Help round trips |
| `src/ui.rs` (unit) | Styled spans for priority/tags, including on completed rows and in Trash; status segments; 40-column clipping order; "No matching tasks" hints with configured keys; Filter editor cursor visible with a long Unicode buffer |
| `src/session.rs` (unit) | Refresh keeps the filter editor buffer/cursor, filter, and sort; external edit that adds or removes a tag reconciles selection; equal bytes cause no parse or redraw with filter and sort active; cached sort order is invalidated by refresh |
| `scripts/test_concurrency.py` (PTY) | One scenario: TUI with `f +x` and `s` active; shell `add "(A) new +x"` and `edit` appear on idle refresh in sorted position; shell `edit` removing `+x` hides the row |

## Documentation checklist

- [ ] `README.md`: Quick start examples (`list --open --tag +work`); essentials
  line adds `s sort · f filter`; Scope paragraph removes "priorities, tags" from
  the excluded list and adds the derived-token note.
- [ ] `docs/usage.md`: new "Priority and tags" section (grammar, examples,
  trailing-punctuation stripping and how it differs from strict todo.txt,
  edge cases); list filter syntax, composition, exit/error table. State that
  for `list`, an `--json` placed right after `--tag`, `--priority`, or
  `--search` is taken as that flag's value (`list --search --json` searches for
  `--json`), and that the way to avoid this is to put `--json` first
  (`shtodo --json list --search …`), as the agent skill already does. Also: JSON fields
  and schema policy with reserved `due`/`parent_id`; Normal-mode table rows for
  `s`/`f`; Filter mode; search-and-views section covering composition, layered
  Esc, and sorted reorder; Version-one limits removes "priorities, tags" and
  "shell search or filters", and keeps dates, notifications, and subtasks
  listed as deferred.
- [ ] `docs/configuration.md`: example config and Normal default table gain
  `toggle_sort`/`edit_filter`; upgrade section covers yield-to-override for
  `s`/`f`.
- [ ] `docs/agent-skill.md`: example prompt for filtered open tasks.
- [ ] `skills/shtodo/SKILL.md`: the "no filter flag" and "do not offer search
  or filters" text is replaced with filter flags; task fields include
  `priority` and `tags`; priority and tags are set by editing text; older-binary
  fallback on `invalid_arguments`.
- [ ] `CHANGELOG.md` Unreleased `### Added`: derived priority and tags, shell
  list filters with JSON, TUI highlight/sort/filter, new configurable actions.
- [ ] `docs/plans/README.md`: update the rejected-findings bullet on shell
  filters to note this plan supersedes it; status DONE at the end.
- [ ] `docs/benchmarks.md`: link the new paired report; update "Shell listing
  remains unfiltered".
- [ ] `cli::usage()` text and its tests.

## STOP conditions

Stop and report to the owner instead of continuing when:

- Any design requires a stored field, a `schema_version` change on disk, or a
  migration, or makes beta.4 unable to read a snapshot this branch wrote.
- A JSON change would alter the meaning of an existing v1 field or remove one,
  which needs a `schema_version` bump.
- The 10,000-task `list` or `list --json` gate fails after the viewport and
  cache mitigations above, or idle refresh starts parsing or redrawing
  unchanged bytes.
- A new default key breaks loading of any existing valid config, or needs a
  change to the reserved-key or conflict rules.
- Filtered or sorted reorder cannot be guarded under the lock with the
  existing `MoveAdjacent` contract.
- The owner asks for dates, notifications, subtasks, or a priority-setting
  command in this slice; replan rather than grow scope.
- Work would touch CI, workflows, secrets, releases, dependencies, or files
  outside this worktree, or would need a push or PR without explicit
  authorization.

## Open questions for the owner

1. **Grammar strictness.** Priority follows todo.txt exactly (`(X) ` with a
   trailing space at byte 0). Tags strip trailing ASCII `.,;:!?` for both
   reporting and matching, so `ship it +shtodo.` matches `--tag +shtodo`.
   Default: yes. This goes beyond strict todo.txt, but todo.sh's own
   `ls +shtodo` substring grep matches that text too. I found no reason
   against it: export writes text unchanged, and no other trailing characters
   are stripped (`+x)` stays `+x)`).
2. **Tag case.** Exact, case-sensitive matching (todo.txt, cheapest), or ASCII
   case-insensitive? Default: case-sensitive.
3. **`--priority` forms.** Letters, ranges `A-C`, and `none`, repeatable as
   union? Default: yes. A smaller option is single letters only.
4. **Shell sort.** Add `list --sort priority` now, or keep shell output in
   canonical order only? Default: canonical only, sort deferred.
5. **Filter echo.** Include a `filter` object in the JSON list reply so agents
   can confirm which filter applied? Default: no; an older binary already fails
   closed.
6. **Keys.** `s` for sort and `f` for filter, plus layered Esc (search first,
   then filter)? Default: yes.
7. **Priority-sorted reorder.** Disable `J`/`K` with a message under Priority
   order (default), or reorder within the same priority band?
8. **Parent IDs.** Accept that parent IDs will likely need a stored field and
   their own migration later, even though JSON stays additive?
9. **Tab completion in the Filter editor.** Should a key complete the trailing
   prefix to the top tag in the hint line? Default: deferred; the hint line is
   enough for this slice, and a completion key would need another Insert-mode
   binding.
10. **`shtodo tags [--json]`.** Add a read-only command that prints distinct
    tags with counts? Default: deferred, out of this slice. Agents can get the
    same from the `tags` field of `list --json`, and people from the Filter
    editor's hint line. Revisit if agents show they need it.
11. **Default sort order in config.** Sort resets to Manual every session.
    Should a config key such as `[view] default_order = "priority"` exist
    later? Default: not in this slice, and nothing here prevents it. It would
    be the first non-keybinding config section, so `config::parse_table` and
    doctor rules would need extending, and that is better decided on its own.

## Validation record

Planning inspection on October 21, 2026 at `1f2421c`. Read the README, the
plan index, the concurrent-usage plan, usage, agent-skill and configuration
docs, `skills/shtodo/SKILL.md`, the changelog, and `src/task.rs`, `json.rs`,
`projection.rs`, `cli.rs`, `storage.rs`, `lib.rs`, `mutation.rs`, `config.rs`,
`input.rs`, `app.rs`, `ui.rs`, and `session.rs`. Only this plan and one row in
`docs/plans/README.md` changed. No runtime code, tests, or other docs changed,
and the repository check command was not run for this documentation-only
change. The `metadata.rs` snippets were compiled and unit-tested on their own
with `rustc --edition 2024 --test` in a scratch directory, and `rustfmt
--check` passed on them.

Review cycle 1, October 21, 2026: chose a live preview with trailing-token
prefix matching and exact acceptance; added tag discovery through the Filter
editor's hint line (in this slice) and deferred `shtodo tags`; made
trailing-punctuation stripping the tag default; split Stage 3 into 3a, 3b,
and 3c; named the existing yield-rule fixtures; pinned the JSON `priority`
form; documented `--json` taken as a value after a filter flag; added open
questions 9 to 11. The revised `tags` snippet was recompiled, tested
(including `ship it +shtodo.`, `+.`, `+a.b`, and `+東京!`), and checked with
`rustfmt --check` in the scratch directory.
