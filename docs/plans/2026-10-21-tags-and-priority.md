# Tags and priority: derived todo.txt tokens with TUI and shell filtering

Status: READY FOR OWNER DECISION. Planning is complete after three review
cycles; no runtime code has changed. Implementation waits on the owner's
answers in [Decisions before Stage 0 exits](#decisions-before-stage-0-exits).
The plan index row stays IN PROGRESS until implementation is done.
Date: October 21, 2026. Inspected baseline: `1f2421c` (feat: add JSON output
for shell task commands, #16) on branch `feat/tags-and-priority`.
Priority: P1. Estimated effort: L, in stages and two PRs (see Review
boundaries). Depends on: concurrent usage (DONE) and the Unreleased JSON
output contract.

This plan is self-contained; a separate spec would duplicate its contracts.
Implementation begins only after the owner answers or accepts the defaults in
the decisions table below. The full text of each question is under Open
questions for the owner.

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
in the filter editor, the last token matches tags by prefix. Enter completes
a unique prefix and refuses an ambiguous one, and a hint line lists the tags
present in the current view. That covers tag discovery. Agents discover tags
from the `tags` field in `list --json`; a separate `shtodo tags` command is
deferred.

## Decisions before Stage 0 exits

Answer by number in one message; "defaults" accepts every default. A question
gates a PR if its answer changes that PR's contracts or scope.

| # | Question | Default | Gates |
| --- | --- | --- | --- |
| 1 | Tag grammar: strip trailing `.,;:!?`; priority strictly `(X) ` at start | Yes | PR 1 |
| 2 | Tag matching case | Case-sensitive, exact | PR 1 |
| 3 | `--priority` forms | Letters, `A-C` ranges, `none`; repeatable as union | PR 1 |
| 4 | Shell `list --sort priority` | Not now; canonical order only | PR 1 |
| 5 | Echo the applied filter in JSON list replies | No | PR 1 |
| 6 | TUI keys `s` sort, `f` filter, layered Esc | Yes | PR 2 |
| 7 | `J`/`K` while sorted by priority | Disabled with a message | PR 2 |
| 8 | Parent IDs may later need a stored field and migration | Accept; PR 1 only reserves the `parent_id` name in the schema policy | PR 1 |
| 9 | Filter editor key to cycle ambiguous matches | Deferred (Enter completion makes it mostly moot) | PR 2 |
| 10 | Read-only `shtodo tags [--json]` command | Deferred; would be its own follow-up PR | None |
| 11 | Config key for the default sort order | Not in this slice | PR 2 |

The mapping matches the reviewer's reading. Question 8 gates PR 1 only
through the reserved-name text in usage.md's schema policy.

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
  (see open question 2). The only prefix matching is inside the TUI Filter
  editor: the live preview of the trailing token, and Enter's completion of a
  unique prefix (see TUI). The shell never matches by prefix.
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
  string and `None` as `null`. A `src/json.rs` unit test pins this exact
  form. The list reply does not echo the filter (open question 5). An older binary given filter flags fails closed with
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
  whitespace. If it is a tag token, it is first normalized the same way
  `--tag` values and derived tags are: trailing `.,;:!?` is stripped. The
  result matches any of the task's tags that start with it (`+sh` matches
  `+shtodo` and `+shell`; `+shtodo.` normalizes to `+shtodo` and matches
  `+shtodo`). A sigil alone, including one left by normalization (`+`, `@`,
  `+.`), previews no tag constraint. An in-progress priority token that does
  not parse yet (`A-`) is left out of the preview without an error.
- Earlier, completed tokens are exact, normalized like `--tag` values. If one
  of them is invalid, the last valid preview stays and the footer message slot
  shows the invalid token.
- The prefix lives only in an App-side type, so the rule is enforced by the
  types rather than by convention. `TaskFilter` and `Projection` stay exactly
  what the shell and `Mutation::MoveAdjacent` carry, with no preview field:

  ```rust
  /// Filter-mode preview. It never leaves `App`, so no shell or `Mutation` value can carry it.
  struct FilterPreview {
      exact: TaskFilter,
      /// The trailing tag token with trailing `.,;:!?` stripped, as for `--tag` values.
      trailing_prefix: Option<String>,
  }

  impl FilterPreview {
      fn matches(&self, text: &str) -> bool {
          self.exact.matches(text)
              && self
                  .trailing_prefix
                  .as_deref()
                  .is_none_or(|prefix| metadata::tags(text).any(|tag| tag.starts_with(prefix)))
      }
  }
  ```

  `exact` holds the buffer's completed tokens. While `Mode::Filter` is open,
  `App::matches` builds a `Projection` from the view, the query, and
  `preview.exact`, then also applies `trailing_prefix`. Outside Filter mode,
  `App` holds only the accepted `TaskFilter`. No mutation can be submitted
  while the editor is open, and `App::move_task` builds its `Projection`
  from the accepted filter only.
- Enter resolves the normalized trailing token (the same form as
  `trailing_prefix`) against the tags in the current view. The current view
  means the rows that match the view, the search, and the buffer's completed
  tokens, which is the same set the hint line counts. The accepted filter
  always stores normalized tags, so Enter on `+shtodo.` accepts `+shtodo`.
  - If the trailing token equals a tag present in that view, accept it as
    typed.
  - Otherwise, if it prefix-matches exactly one distinct tag, replace the
    token with that tag and accept (`+sh` becomes `+shtodo` when only
    `+shtodo` starts with `+sh`).
  - If it prefix-matches several, stay in the editor, keep the buffer and
    cursor, and show `Finish the tag: +shtodo, +shell`. Tags are listed in
    hint-line order and clipped like the hint line.
  - If it matches none, accept exactly as typed, so a user can set up a filter
    for a tag that does not exist yet. The list may then be empty, which is
    what was asked for.
  - A sigil alone (`+`, `@`, or `+.` after normalization) is invalid on
    Enter, as in the shell.
  - Completed tokens are never changed. Priority tokens are never completed.
- An empty buffer clears the filter. Enter on a buffer with an invalid token
  stays in the editor and shows
  `Invalid filter "foo"; use +project, @context, A, A-C, or none`.
- Escape restores the previous filter and selection, as Search does.
- In Normal mode, Esc (`clear_search`) clears the accepted search first. If no
  search is active, it clears the filter. Each press clears one layer.
- Covered by the `src/app.rs` tests
  `filter_editor_should_preview_trailing_tag_by_prefix_and_accept_exactly` and
  `filter_editor_enter_should_complete_unique_prefix_refuse_ambiguous_and_accept_unknown`
  (see Test plan).

**Tag discovery.** While `Mode::Filter` is open, a hint line lists the
distinct tags in the rows the preview shows if the trailing token is ignored.
They are narrowed to tags starting with the normalized trailing tag prefix,
if there is one. Order is by count, highest first, then by name. They are
shown with counts, for example `+shtodo 12 · +shell 3 · @laptop 2`.
- Cost: one `tags` scan over the visible rows and a small `HashMap<&str, usize>`
  on each Filter-mode keystroke and refresh. There is no cost outside Filter
  mode and no stored index. At 10,000 rows this is one scan per keystroke, the
  same order of work as live `/` search, and it goes through the TUI
  measurement gate (see Performance).
- In this slice, as part of Stage 3c.
- Agents use the `tags` array already present in `list --json` (for example
  `shtodo --json list | jq '[.tasks[].tags[]] | group_by(.) | map({tag: .[0], count: length})'`).
  A read-only `shtodo [--local] tags [--json]` command is deferred (open
  question 10). It would need a new `Command` variant, argument parsing, usage
  text, a JSON reply (`{"schema_version":1,"command":"tags","scope":…,"tags":[{"tag":"+shtodo","count":12}]}`),
  and CLI and JSON tests: small, but more public surface than the stated goal
  of keeping shtodo small requires today.

Placement at the 40x8 minimum. Today `ui::render` splits the screen into row 0
for the header, row 1 for the tab strip and status, rows 2 to 6 for the list
(`Constraint::Min(1)`), and row 7 for the footer.
- While `Mode::Filter` is open, `render` adds a `Constraint::Length(1)`
  region between the list and the footer. At 8 rows high, the hint is row 6
  and the list is rows 2 to 5 (4 rows, which keeps at least 3). Outside
  Filter mode, that region has length 0 and the layout is unchanged.
- The tab strip stays on row 1, so the view, the match count, and the editor
  remain visible. The filter buffer is edited on row 1 in place of the `f:`
  segment, with the cursor windowed by `editor_window`, as Search edits
  `/query` there today.
- Messages such as `Finish the tag: …` and invalid-token errors use the
  existing footer message slot on row 7. The hint never displaces them.
- Clipping at 40 columns: whole `tag count` entries are dropped from the end,
  and `…` is appended when any were dropped, so no tag is shown cut in half. If
  even the first entry does not fit, it is cut to `width - 1` columns plus `…`.
  With no tags, the hint shows `No tags in this view`.
- Covered by the `src/ui.rs` test
  `filter_editor_should_render_hint_above_footer_and_keep_list_rows_at_minimum_size`.

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
- Each binding is registered in the stage that makes it work, so the doctor
  counts change in two steps. With a config file, `shtodo doctor` reports 36
  configurable actions after Stage 3b (`toggle_sort`) and 37 after Stage 3c
  (`edit_filter`). The `move_down = ["x"]` fixture in `tests/cli.rs` goes from
  47 to 48 to 49 active bindings. The existing upgrade fixture
  `toggle_complete = ["f"]` must keep loading in both steps, with
  `edit_filter` unbound after 3c. No new doctor note is needed: unbound
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
- Filter mode adds the preview predicate and the hint line's tag scan per
  keystroke; neither runs outside Filter mode.
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
  Priority order and report it. Measure and report Filter-mode
  keystroke-to-frame at 10,000 tasks; it must stay within 5% of `/` search
  keystroke-to-frame on the same fixture.
- The shell gates run in PR 1 and the TUI gates in PR 2 (see Review
  boundaries).
- Idle refresh: zero parses and zero redraws on unchanged bytes still hold.

## Staged implementation and acceptance gates

### Review boundaries

Effort is L, not M. Stages 3a to 3c, the hint row, the PTY scenario, and paired
benchmarks are each substantial. The work is split into two PRs, each
reviewable and releasable on its own:

| PR | Stages | Ships | Documentation checklist items |
| --- | --- | --- | --- |
| PR 1: shell and JSON | 1, 2, plus a shell-only Stage 4 subset (`list` and `list --json` gates and the filtered-list cases) | Derived tokens, shell `list` filters, JSON `priority`/`tags`. The agent-facing gain, usable without any TUI change | README Quick start and Scope; usage.md grammar, list filters and error table, the `--json`-as-value note, JSON fields and schema policy, Version-one limits for priorities, tags, and shell filters; agent-skill.md; SKILL.md; CHANGELOG entries for tokens, filters, and JSON; plans README rejected-findings bullet; benchmarks.md "Shell listing remains unfiltered" and a link to the shell paired report; `cli::usage()` |
| PR 2: TUI | 3a, 3b, 3c, then the TUI Stage 4 subset (completion-to-frame, idle refresh) | Row styling, priority sort, filter editor, tag discovery | README essentials line; usage.md Normal-mode keys, Filter mode, search-and-views composition, layered Esc, sorted reorder; configuration.md example, defaults, and upgrade rules for `s`/`f`; CHANGELOG TUI entries; benchmarks.md link to the TUI paired report; this plan to DONE and the index row |

PR 2 is branched from and rebased onto PR 1, and is reviewed only after PR 1
merges. Its Stage 1 dependencies (`metadata.rs`, `TaskFilter`, extended
`Projection`) come from PR 1 and must not be redone. Because each binding
registers with its working stage, no intermediate state of PR 2 exposes a
key that does nothing, so a release may be cut after PR 1 or after PR 2. No
release is cut from a partial PR 2.

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
Acceptance: the `metadata.rs`, `projection.rs`, and filtered `MoveAdjacent`
guard (`mutation.rs`) unit tests pass; all existing tests pass unchanged;
Clippy strict.

### Stage 2: Shell filters and JSON fields

Add `parse_list_args`, the `CliError` variants, `Command::List(_, ListFilter)`,
filtered `list_tasks`, and the JSON `priority`/`tags` fields. Update `usage()`.
Acceptance: every row of the exit/error table has a CLI test in text and JSON
mode; no-storage-created holds for usage errors; existing exact list output
holds.

### Stage 3a: Styled rows

Add styled `task_item` for live and Trash rows: the priority marker and tag
slices are styled, completed rows keep dim and strikethrough on every span,
and stripped trailing punctuation is left unstyled. No `BindingId`, `Action`,
config name, or doctor count changes, so no key exists that does nothing yet.
Acceptance: ui styling tests; every earlier test passes unchanged, including
doctor counts of 35 actions and 47 bindings. A STOP may land here, and the
tree stays green.

### Stage 3b: Priority sort

Register `BindingId::ToggleSort` and `Action::ToggleSort`: the `DEFINITIONS`
entry with default `s`, config name `toggle_sort`,
`defaults_yield_to_overrides` membership, and the Help and footer
description. Add `TaskOrder`, the cached sorted `Vec<TaskId>` and when it is
invalidated, `j`/`k` in displayed order, selection identity across toggles,
reconcile and selection fallback in displayed order, the `by priority` status
segment, and `J`/`K` disabled with a keymap-derived message under Priority
order.
Acceptance: app sort tests, ui status and clipping tests, and session tests
showing refresh invalidates the cached order and unchanged bytes cause no
parse or redraw while sorted. `Mutation::MoveAdjacent` is never submitted
while sorted. The `toggle_sort` input and config tests pass, including
`load_should_resolve_search_and_view_actions_and_shared_editor_keys` loading
with `ToggleSort` unbound. Doctor counts are 36 actions and 48 bindings. A STOP
may land here.

### Stage 3c: Filter editor and tag discovery

Register `BindingId::EditFilter` and `Action::EditFilter` (default `f`,
config name `edit_filter`, with yield-to-override). Add `Mode::Filter`,
`FilterSession`, the App-side `FilterPreview`, the live preview with the
trailing-token prefix, Enter resolution (complete a unique prefix, refuse an
ambiguous one, accept an unknown one), the invalid-token message, the
tag-discovery hint row above the footer, layered Esc in Normal mode, the `f:`
status segment, filter-aware "hidden" messages and "No matching tasks" hints,
and the accepted filter carried in `MoveAdjacent`'s `Projection`.
Acceptance: app filter tests including
`filter_editor_should_preview_trailing_tag_by_prefix_and_accept_exactly` and
`filter_editor_enter_should_complete_unique_prefix_refuse_ambiguous_and_accept_unknown`;
ui test
`filter_editor_should_render_hint_above_footer_and_keep_list_rows_at_minimum_size`;
input tests showing Filter mode uses Insert bindings; the `f` yield-rule
fixtures report `EditFilter` unbound; session tests for editor buffer,
cursor, and filter across refresh; mutation tests for the filtered adjacency
guard; the PTY smoke scenario passes. Doctor counts are 37 actions and 49
bindings.

### Stage 4: Measurements

Run the paired benchmarks above, the shell subset in PR 1 and the TUI subset
in PR 2, and record a short report for each under `docs/benchmarks/` without
overwriting earlier samples.
Acceptance: that PR's gates are met, or STOP.

### Stage 5: Docs and final verification

Complete the checklist items assigned to the PR being finished (see Review
boundaries), then run this for each PR:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
cargo build --release --locked
git diff --check
```

Acceptance: all gates pass; release-binary smoke uses a temporary home and an
exact temporary cwd. Mark this plan DONE and update the index row only at the
end of PR 2, once the behavior and evidence are present.

## Test plan

| File | Stage | Cases |
| --- | --- | --- |
| `src/metadata.rs` (unit) | 1 | Priority: `(A) x` yes; `(Z) x` yes; `(A)` alone, `(a) x`, `(AA) x`, `x (A) y`, `(A)x` no. Tags: `+p`, `@c`, `+` and `@` alone ignored, `C++` and `a@b.c` ignored, `ship it +shtodo.` gives `+shtodo`, each of `.,;:!?` and runs like `+x?!` stripped, `+.` no tag, inner punctuation `+a.b` kept, other trailing characters such as `)` kept, Unicode tags (`+東京!` gives `+東京`), tabs and other Unicode whitespace as separators, dedupe after normalization keeps first (`+x +x.` gives one tag). `--tag +shtodo.` normalizes to `+shtodo`. Filter tokens: letters (both cases), ranges, reversed range rejected, `none`, bare words rejected. `PrioritySet` membership including `none`; empty set matches any. `TaskFilter` AND over tags, OR over priorities |
| `src/projection.rs` (unit) | 1 | `matches` order and composition of view, filter, and search; `adjacent` skips filtered-out rows; hidden and tombstone slots keep their positions under filtered reorder |
| `src/mutation.rs` (unit) | 1 (guard), 3c (acceptance) | `MoveAdjacent` with a filter: a neighbor that leaves the filter conflicts; a newly matching row between the pair conflicts; unrelated changes coexist |
| `src/cli.rs` (unit) | 2 | `parse_list_args`: every flag, any order, repeats, `--search --json` taken as a value, `--json` in flag positions in front/middle/end, every row of the error table, both scopes; `parse_invocation` keeps the format for list usage errors |
| `src/json.rs` (unit) | 2 | Task object includes `priority` serialized exactly as `"A"` or `null` (a one-character string, not a number or `"(A)"`) and `tags` (`[]` or normalized values) for live and deleted tasks |
| `tests/cli.rs` | 2; doctor counts in 3b and 3c | Text `list` with each filter and combinations; canonical order kept; empty match exits 0 with empty stdout; usage errors exit 1 with usage text and create no storage; filters ignore invalid keybinding config; doctor counts for the `move_down` fixture are 36 and 48 after Stage 3b, then 37 and 49 after Stage 3c; the `toggle_complete = ["f"]` fixture still loads; exact unfiltered output unchanged |
| `tests/json.rs` | 2 | `list --json` with filters; new fields on list and on add/edit/done/reopen/delete/restore replies; every error-table row returns `invalid_arguments`, empty stdout, exit 1; missing scope returns `[]` without creating storage; a filter read works while a writer holds the lock |
| `src/input.rs` (unit) | 3b (`s`), 3c (`f`, Filter mode) | Defaults map `s`/`f` in Normal only; explicit `toggle_sort`/`edit_filter` conflicts and `ctrl-c` are rejected; Filter mode uses Insert bindings and treats `q`, `f`, `s`, `/`, Space as text. Existing tests pass unchanged, with added assertions: `search_actions_should_support_overrides_and_detect_existing_key_conflicts` and `existing_explicit_bindings_should_claim_all_new_search_defaults` report `EditFilter` in `unbound_actions_for(Mode::Normal)` because `f` is claimed; `explicit_search_and_legacy_bindings_should_still_conflict_in_either_source_order` still returns exactly one issue |
| `src/config.rs` (unit) | 3b (`toggle_sort`), 3c (`edit_filter`) | `toggle_sort`/`edit_filter` parse under `[keybindings.normal]` and are unknown in other modes; doctor report counts. `load_should_resolve_search_and_view_actions_and_shared_editor_keys` (`start_search = ["s"]`) still loads with `StartSearch` on `s` and `ToggleSort` reported unbound. `load_should_preserve_existing_normal_bindings_that_claim_new_search_defaults` still loads for every key; for `f` it reports `EditFilter` unbound and `ToggleComplete` on `f` |
| `src/app.rs` (unit) | 3b (sort), 3c (filter) | `filter_editor_should_preview_trailing_tag_by_prefix_and_accept_exactly`: tasks with `+shtodo`, `+shell`, `+other`. Typing `+`, `+s`, `+sh`, through `+shtodo` never leaves the `+shtodo` task out of the preview; `+sh` previews both `+shtodo` and `+shell`; `+shtodo.` (trailing punctuation) normalizes to the prefix `+shtodo` and previews the `+shtodo` task, and Enter on it accepts `+shtodo`; `+.` previews no tag constraint; `+sh ` (trailing space) is exact and previews none; `@x +sh` keeps `@x` exact; the preview is a `FilterPreview` with `exact` holding only `@x` and `trailing_prefix` `+sh`; after Enter, `App` holds only a plain `TaskFilter`, and a following `J`/`K` builds its `Projection` from it. `filter_editor_enter_should_complete_unique_prefix_refuse_ambiguous_and_accept_unknown`: with `+shtodo` and `+shell` present, Enter on `+sh` stays in the editor with the buffer and cursor kept and the message `Finish the tag: +shtodo, +shell`; Enter on `+sht` completes to `+shtodo` and accepts; Enter on `+shtodo` (exact and present) accepts unchanged; Enter on `+zzz` (no match) accepts `+zzz` exactly and the list is empty; Enter on `+` alone is invalid; `@x +sht` completes only the trailing token, and only among tags in rows that also match `@x`, the view, and the search; a priority token is never completed. Also: the hint line lists matching tags with counts in the specified order; an invalid completed token keeps the last valid preview; an in-progress `A-` is left out of the preview without an error; Enter rejects an invalid buffer; Escape restores filter and selection; layered Esc clear; sort toggle keeps selection identity; `j`/`k` follow sorted order; `J`/`K` under Priority order submit nothing and show message; hidden-by-filter messages for add/edit/restore/complete; filter and sort survive Trash/Help round trips |
| `src/ui.rs` (unit) | 3a (styling), 3b (sort segment), 3c (filter and hint) | Styled spans for priority/tags (stripped punctuation unstyled), including on completed rows and in Trash; status segments; 40-column clipping order; "No matching tasks" hints with configured keys. `filter_editor_should_render_hint_above_footer_and_keep_list_rows_at_minimum_size`, written like `search_editor_should_keep_long_unicode_query_cursor_visible_at_minimum_width` with `TestBackend::new(40, 8)`: in Filter mode with a long Unicode buffer, the cursor is on row 1 with `x < 40`; row 1 holds the buffer's tail; rows 2 to 5 hold list rows; row 6 holds the hint with whole entries and a trailing `…`; row 7 holds the Filter footer; with a message set, row 7 shows it and row 6 still shows the hint. A second case shows that in Normal mode the list again covers rows 2 to 6 |
| `src/session.rs` (unit) | 3b (sort), 3c (filter) | Refresh keeps the filter editor buffer/cursor, filter, and sort; external edit that adds or removes a tag reconciles selection; equal bytes cause no parse or redraw with filter and sort active; cached sort order is invalidated by refresh |
| `scripts/test_concurrency.py` (PTY) | 3c | One scenario: TUI with `f +x` and `s` active; shell `add "(A) new +x"` and `edit` appear on idle refresh in sorted position; shell `edit` removing `+x` hides the row |

## Documentation checklist

- [ ] `README.md`: Quick start examples (`list --open --tag +work`) and Scope
  paragraph, which removes "priorities, tags" from the excluded list and adds
  the derived-token note (PR 1); essentials line adds `s sort · f filter`
  (PR 2).
- [ ] `docs/usage.md`, PR 1: new "Priority and tags" section (grammar, examples,
  trailing-punctuation stripping and how it differs from strict todo.txt,
  edge cases); list filter syntax, composition, exit/error table. State that
  for `list`, an `--json` placed right after `--tag`, `--priority`, or
  `--search` is taken as that flag's value (`list --search --json` searches for
  `--json`), and that the way to avoid this is to put `--json` first
  (`shtodo --json list --search …`), as the agent skill already does. Also:
  JSON fields and schema policy with reserved `due`/`parent_id`; Version-one
  limits removes "priorities, tags" and "shell search or filters", and keeps
  dates, notifications, and subtasks listed as deferred.
- [ ] `docs/usage.md`, PR 2: Normal-mode table rows for `s`/`f`; Filter mode,
  including prefix preview, Enter completion, and the hint line;
  search-and-views section covering composition, layered Esc, and sorted
  reorder.
- [ ] `docs/configuration.md` (PR 2): example config and Normal default table gain
  `toggle_sort`/`edit_filter`; upgrade section covers yield-to-override for
  `s`/`f`.
- [ ] `docs/agent-skill.md` (PR 1): example prompt for filtered open tasks.
- [ ] `skills/shtodo/SKILL.md` (PR 1): the "no filter flag" and "do not offer search
  or filters" text is replaced with filter flags; task fields include
  `priority` and `tags`; priority and tags are set by editing text; older-binary
  fallback on `invalid_arguments`.
- [ ] `CHANGELOG.md` Unreleased `### Added`: derived priority and tags, shell
  list filters with JSON (PR 1); TUI highlighting, sort, filter, tag
  discovery, and the new configurable actions (PR 2).
- [ ] `docs/plans/README.md`: update the rejected-findings bullet on shell
  filters to note this plan supersedes it (PR 1); status DONE (end of PR 2).
- [ ] `docs/benchmarks.md`: update "Shell listing remains unfiltered" and
  link the shell paired report (PR 1); link the TUI paired report (PR 2).
- [ ] `cli::usage()` text and its tests (PR 1).

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
- The Filter preview cannot be built without adding preview or prefix state
  to `TaskFilter`, `Projection`, or any `Mutation`, or Enter resolution would
  need to change completed tokens or complete priority tokens.
- The hint row cannot keep at least 3 list rows at 40x8, would displace footer
  messages, or would remove the tab strip and match count while editing.
- Filter-mode keystroke-to-frame at 10,000 tasks misses its gate after the
  viewport mitigation.
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
   order, or reorder within the same priority band? Default: disable with a
   message.
8. **Parent IDs.** Accept that parent IDs will likely need a stored field and
   their own migration later, even though JSON stays additive? Default:
   accept. PR 1 only reserves the `parent_id` name in the schema policy.
9. **Cycling key in the Filter editor.** Mostly moot now that Enter
   completes a unique prefix and refuses an ambiguous one. What remains is a
   key, such as Tab, that cycles through ambiguous matches without typing. Default: deferred; it
   would need another Insert-mode binding.
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
prefix matching and exact acceptance (Enter resolution superseded in cycle 2); added tag discovery through the Filter
editor's hint line (in this slice) and deferred `shtodo tags`; made
trailing-punctuation stripping the tag default; split Stage 3 into 3a, 3b,
and 3c; named the existing yield-rule fixtures; pinned the JSON `priority`
form; documented `--json` taken as a value after a filter flag; added open
questions 9 to 11. The revised `tags` snippet was recompiled, tested
(including `ship it +shtodo.`, `+.`, `+a.b`, and `+東京!`), and checked with
`rustfmt --check` in the scratch directory.

Review cycle 2, October 21, 2026:
- Enter now completes a unique tag prefix, refuses an ambiguous one with
  `Finish the tag: …`, and accepts an unknown one as typed. Open question 9
  is now mostly moot.
- The preview prefix moved out of `TaskFilter` into an App-side
  `FilterPreview`, so `Projection` and `TaskFilter` stay structurally equal to
  what the shell and `MoveAdjacent` carry.
- Specified the hint row at 40x8 (row 6, above the footer, with the list on
  rows 2 to 5 and the tab strip kept), its clipping rules, and the ui test.
- Re-rated effort to L, defined the PR 1 (Stages 1 and 2) and PR 2 (Stages 3a
  to 3c and 4) boundaries with their checklist items, and stated that PR 2
  rebases on PR 1.
- Moved binding registration into 3b (`toggle_sort`) and 3c (`edit_filter`),
  so 3a is styling only and doctor counts step 35/47, then 36/48, then 37/49.
- Verified against `src/ui.rs` (`render` layout, `MIN_WIDTH`/`MIN_HEIGHT`
  40x8, and the search-cursor test at row 1 and footer at row 7). The
  `FilterPreview` snippet was compiled with stub `TaskFilter` and
  `metadata::tags`, unit-tested for prefix and no-prefix cases, and checked
  with `rustfmt --check` in the scratch directory.

Review cycle 3, October 21, 2026:
- The trailing token is normalized (trailing `.,;:!?` stripped) before prefix
  preview, hint narrowing, and Enter resolution, the same as `--tag` values.
  Added the `+shtodo.` and `+.` cases to
  `filter_editor_should_preview_trailing_tag_by_prefix_and_accept_exactly`.
- Added the "Decisions before Stage 0 exits" table under the Recommendation.
  It accepts the reviewer's PR mapping and notes that question 8 gates PR 1
  only through the reserved-name text.
- Status set to READY FOR OWNER DECISION; the index row stays IN PROGRESS.
- Consistency fixes from a full top-to-bottom re-read:
  - The header no longer says "revised after review cycle 1" and now points to
    the decisions table instead of "the open questions at the end".
  - The grammar section said the only prefix matching was the live preview;
    it now includes Enter completion and states that the shell never matches
    by prefix.
  - Moved the layered-Esc bullet out of Tag discovery into the Filter section,
    and moved the cost, slice, and agent bullets back under Tag discovery
    instead of under the 40x8 placement list.
  - "The line shows the invalid token" now names the footer message slot,
    matching the placement rules.
  - Reflowed an over-long line in the JSON section.
  - Performance now covers Filter-mode cost and adds a keystroke-to-frame
    gate. Stage 4 and Stage 5 now say which gates and reports go with which
    PR, and that DONE happens at the end of PR 2.
  - The PR table and checklist gave the benchmarks link only to PR 2 even
    though PR 1 runs the shell benchmarks; each PR now links its own report.
    Every checklist item now names its PR, and usage.md is split into PR 1 and
    PR 2 items, matching the PR table.
  - The test plan gained a Stage column, so every stage's acceptance line maps
    to rows: mutation guard tests land in Stage 1 and are part of 3c's
    acceptance; doctor counts land in 3b and 3c.
  - The STOP conditions now cover `FilterPreview` leaking into shared types,
    Enter resolution changing completed tokens, the hint row's 40x8 limits,
    and the Filter-mode performance gate.
  - Open questions 7 and 8 now state explicit defaults; question 9 is renamed
    from "Tab completion" to "cycling key", since Enter now completes.
  - The cycle 1 record now notes that its Enter behavior was superseded.
    I confirmed no other sentence describes Enter accepting a prefix as typed.
- Verified: the `FilterPreview` snippet with its new field comment was
  recompiled, unit-tested, and checked with `rustfmt --check` in the scratch
  directory. `git diff --check`, local Markdown links, and the no-em-dash check
  pass.
