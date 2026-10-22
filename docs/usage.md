# Usage and keyboard controls

## Commands

```text
shtodo
shtodo --local
shtodo add "Fix the bug"
shtodo --local add "Run the tests"
shtodo list
shtodo --local list
shtodo [--local] list [--open | --done] [--tag <TAG>]... [--priority <P>]... [--search <TEXT>] [--sort priority]
shtodo delete 3
shtodo --local delete 3
shtodo [--local] done 3
shtodo [--local] reopen 3
shtodo [--local] edit 3 "New text"
shtodo [--local] restore 3
shtodo [--local] add --print-id "Fix the bug"
shtodo list --json
shtodo --local --json add "Fix the bug"
shtodo doctor
shtodo --help
shtodo --version
```

Running `shtodo` opens the default global list. Running `shtodo --local` opens
a list for the exact directory from which it is run. `--local` does not search
parent directories or use a repository root. Use `--help` (or `-h`) for usage
and `--version` (or `-V`) for the installed version.

In command syntax, `[--local]` means an optional flag before the command;
do not type the brackets.

### Adding from the shell

`shtodo [--local] add "Task text"` adds one trimmed, non-empty, single-line
task. Omitting the text reads one task from standard input, including an
optional trailing newline. Multiple input lines are rejected, and missing or
blank input produces an error with usage examples. By default, success prints
`Added: Task text` after the task has been saved.

For scripts, use exactly `shtodo [--local] add --print-id [<TASK>]`. Place
`--print-id` immediately after `add`, before optional text. This mode prints
only the persisted positive decimal ID and a newline after a successful save.
It uses the same text validation and single-task stdin behavior:

```sh
task_id=$(shtodo add --print-id "Fix the bug")
shtodo done "$task_id"
printf 'Run the tests\n' | shtodo --local add --print-id
```

### Shell listing and task lifecycle

`shtodo list` prints every non-deleted global task in canonical order. Use
`shtodo --local list` for the exact current-directory scope. Each task occupies
one line, with two ASCII spaces between its persisted ID, lowercase state, and
text:

```text
1  open  Fix the bug
2  done  Run the tests
```

There is no heading or summary. An empty or never-created list prints nothing
and exits successfully. Listing is a side-effect-free reader: it does not
create `~/.shtodo`, a scope directory, or a lock file, and it can read the last
complete atomically saved snapshot while another process holds the writer
lock. Snapshot validation errors still fail the command without changing the
snapshot. This readable text is a display contract, not a serialization
format.

### Priority and tags

Priority and tags are written in the task text with
[todo.txt](https://github.com/todotxt/todo.txt) syntax. shtodo derives them
whenever it needs them; nothing extra is stored, so lists written by earlier
versions work unchanged and older binaries still read new lists.

```sh
shtodo add "(A) Fix the bug +shtodo @laptop"
shtodo edit 3 "(B) Fix the bug +shtodo"   # change the priority, drop @laptop
```

- **Priority** is a leading `(A) ` through `(Z) `: an opening parenthesis, one
  uppercase ASCII letter, a closing parenthesis, and a space, at the very start
  of the text. `(A)` alone, `(a) x`, `(AA) x`, `(A)x`, and a marker later in
  the text are not priorities. `A` is the highest.
- **Tags** are words that start with `+` (a project) or `@` (a context), split
  on Unicode whitespace. Trailing `.`, `,`, `;`, `:`, `!`, and `?` are not part
  of the tag, so `Ship it +shtodo.` has the tag `+shtodo`. At least one
  character must remain after the sigil: `+`, `@`, and `+.` are not tags.
  Other punctuation stays (`+a.b`, `+x)`), and a sigil inside a word
  (`C++`, `bob@example.com`) does not start a tag. Tags are case-sensitive.
- Stripping trailing punctuation goes beyond strict todo.txt, where
  `+shtodo.` is the project `shtodo.`. It matches what `todo.sh ls +shtodo`
  already finds, since that is a substring search. The text itself is never
  changed.
- Completion is still the task's state, not a todo.txt `x ` prefix, and
  completing a task keeps its priority.

To change a priority or tag, edit the text. There is no separate priority
command.

### Due dates

A due date is also written in the task text, as the todo.txt token
`due:YYYY-MM-DD`. Like priority and tags, it is derived when needed and never
stored separately, so older binaries read the same lists.

```sh
shtodo add "(A) Pay rent due:2026-11-01 +home"
shtodo add "Call the bank due:today"        # saved as due:2026-10-21 on that day
shtodo edit 3 "Pay rent due:2026-12-01"     # change the date by editing the text
```

- A **due token** is a word that starts with the lowercase key `due:`,
  anywhere in the text, followed by exactly `YYYY-MM-DD` in ASCII digits
  naming a real calendar day (years `0000` to `9999`, with leap years).
  Trailing `.`, `,`, `;`, `:`, `!`, and `?` are not part of the token, as for
  tags, so `Pay due:2026-11-01.` is due November 1.
- The task's due date is the **first** valid due token. Later due tokens,
  valid or not, are ignored.
- Anything else is plain text and gives no due date: `due:` alone,
  `due: 2026-11-01`, `Due:` or `DUE:`, `overdue:2026-11-01`,
  `+due:2026-11-01`, `due:2026-1-5`, `due:26-10-21`, `due:2026/10/21`,
  `due:2026-02-30`, `due:2026-11-01T09:00`, and `due:2026-11-01)`.
- **Relative words.** When a task is added or edited, in the shell or the
  TUI, `due:today` and `due:tomorrow` (any ASCII case, with optional trailing
  punctuation) are rewritten to the date, so saved text always holds an
  absolute date that never changes meaning. Every relative token is
  rewritten, while only the first valid token is the due date:
  `x due:today due:tomorrow` is saved as `x due:2026-10-21 due:2026-10-22`
  and is due 2026-10-21. Other words such as `due:fri` or `due:+3d` stay plain
  text. An edit whose rewritten text equals the current text reports
  `Unchanged` and writes nothing. Older shtodo versions do not rewrite these
  words; a `due:today` they save stays plain text and never becomes a date.
- **Today** is the calendar date in the time zone of the shtodo process. It
  follows `TZ` (a zone name such as `America/New_York`, `:America/New_York`,
  a zone file path, or a POSIX rule such as `<-05>5`) and otherwise the
  system zone. An unknown or empty `TZ` uses UTC. The TUI reads the date when
  it starts and again each second with its refresh, and uses the date it has
  when you press Enter.
- Completing a task keeps its due token. A done task is never overdue.
- Search still matches the full literal text, so `/due:2026-11` finds those
  tokens as text.

### Filtering and sorting the list

`shtodo list` accepts filters after `list`, in any order. They combine with
AND across kinds:

| Flag | Shows |
| --- | --- |
| `--open` or `--done` | Only open or only done tasks; at most one of them |
| `--tag <TAG>` | Tasks with that exact `+project` or `@context` tag. Repeat to require every listed tag |
| `--priority <P>` | Tasks whose priority is the letter `A` to `Z`, in the inclusive range `A-C`, or `none` for no priority. ASCII case-insensitive (`a-c`, `NONE`). Repeat to accept any of them |
| `--due <WHEN>` | Tasks by [due date](#due-dates); WHEN is listed below. Repeat to accept any of them |
| `--search <TEXT>` | Tasks whose full text contains TEXT, ignoring case, like TUI `/` search. At most once |
| `--sort priority` | All matching tasks ordered `A` to `Z`, then tasks without a priority. At most one `--sort` |
| `--sort due` | All matching tasks by due date, earliest first, then tasks without one. Equal dates are ordered by priority, then saved order. At most one `--sort` |

`--sort` values are case-sensitive: only `priority` and `due` are accepted.
`--due` takes one of these values, with ASCII case-insensitive words:

| WHEN | Shows |
| --- | --- |
| `today`, `tomorrow` | Tasks due on that local date |
| `overdue` | Open tasks due before today; never done tasks |
| `YYYY-MM-DD` | Tasks due on that date |
| `START..END` | Tasks due from START to END, inclusive. Each bound is a date, `today`, or `tomorrow`, and one may be omitted: `..2026-10-31`, `today..` |
| `none` | Tasks without a valid due token |
| `any` | Tasks with a valid due token |

There is no `week` value; write the range you mean, such as
`--due today..2026-10-28`. A range whose order is known to be reversed, such as
`2026-10-22..2026-10-21` or `tomorrow..today`, is an error. A range that mixes
a date and a relative word, such as `2026-10-25..today`, is accepted and
matches nothing on days when it is reversed. `--open --due overdue` is the
same as `--due overdue`, and `--done --due overdue` is always empty; use
`--done --due ..2026-10-20` for done tasks that were due by a date.

```sh
shtodo list --open --tag +shtodo
shtodo list --tag +shtodo --tag @laptop --priority A-B
shtodo --local list --priority none --search "release notes"
shtodo list --open --sort priority
shtodo list --open --due overdue --due today --sort due
shtodo list --due 2026-10-01..2026-10-31 --tag +home
```

Without `--sort`, filtered output keeps the saved order and the same
`ID  state  text` lines. `--sort priority` filters first and then sorts; tasks
with the same priority, and tasks without one, keep their saved order.
`--sort due` works the same way with dates. These are the only sorts;
reordering stays in the TUI. Only relative `--due` values (`today`,
`tomorrow`, `overdue`, or a range with one of them) read the date; absolute
dates, `none`, `any`, and `--sort due` never do. A `--tag` value is normalized like
a derived tag, so `--tag +shtodo.` means `--tag +shtodo`. A filter that
matches nothing prints nothing and exits 0.

Each flag value is the next argument, exactly as given, even if it starts with
`-`. So `shtodo list --search --json` searches for the text `--json`, and
`shtodo list --sort --json` and `shtodo list --due --json` fail because
`--json` is not a value they accept. Put `--json`
before `list` (`shtodo --json list --search ...`) to avoid this.
`--flag=value` forms are not supported.

Filter errors are usage errors. They exit with status 1, print the message
and usage, and are detected before any storage is read. With `--json`, the
error code is `invalid_arguments`:

| Case | Message |
| --- | --- |
| Unknown flag, `--flag=value`, or extra argument | `unsupported shtodo arguments: "--wat"` |
| `--tag`, `--priority`, `--due`, or `--search` with no value | `--tag requires a value` |
| `--tag work`, `--tag +`, `--tag "+a b"` | `invalid --tag "work"; expected +project or @context` |
| `--priority AA`, `--priority C-A`, `--priority ""` | `invalid --priority "AA"; expected a letter A-Z, a range such as A-C, or none` |
| Empty, whitespace-only, or multiline `--search` | `--search requires non-empty single-line text` |
| `--open` with `--done` | `--open and --done cannot be combined` |
| `--due soon`, `--due week`, `--due ""`, `--due ..`, `--due 2026-02-30`, `--due due:today`, `--due --json`, `--due tomorrow..today` | `invalid --due "soon"; expected today, tomorrow, overdue, none, any, YYYY-MM-DD, or START..END` |
| `--sort` with no value | `--sort requires a value; expected priority or due` |
| `--sort manual`, `--sort Due`, `--sort ""`, `--sort --json` | `invalid --sort "manual"; expected priority or due` |
| A repeated `--open`, `--done`, `--search`, or `--sort`, including `--sort priority --sort due` | `--search may only be specified once` |
| A filter value that is not UTF-8 | `invalid --tag value; expected UTF-8` |

Empty values are rejected on purpose, so an unset shell variable cannot
quietly turn a filter into "everything". An older shtodo that does not know a
filter flag also fails with a usage error rather than listing every task:
a version without due dates rejects `--due` with
`unsupported shtodo arguments: "--due"` and `--sort due` with
`invalid --sort "due"; expected priority`. A repeated identical `--due` value
is accepted and counted once.

`shtodo delete <ID>` and `shtodo --local delete <ID>` soft-delete exactly one
task from the selected scope. IDs are positive integers local to their scope,
so global task `3` and local task `3` may be different tasks. A successful
delete prints:

```text
Deleted 3: Fix the bug
```

Repeating the command is an idempotent success that does not rewrite the
snapshot:

```text
Already deleted 3: Fix the bug
```

Deleted tasks retain their tombstones and can be restored by ID from the
shell, with `u` in the TUI, or individually in the trash view opened with `t`.
An unknown ID fails with `task 3 was not found`. Missing, zero, signed,
nonnumeric, and extra IDs are usage errors. Deletion uses the selected scope's
writer lock and atomic save path, so it fails if another writer holds that
lock beyond the one-second acquisition budget. Invalid keybinding configuration
does not block any shell task command.

The rest of the shell lifecycle accepts the same positive scope-local IDs:

| Command | Behavior | Changed-task success | Unchanged-task success |
| --- | --- | --- | --- |
| `shtodo [--local] done <ID>` | Set a live task complete | `Completed 3: Fix the bug` | `Already done 3: Fix the bug` |
| `shtodo [--local] reopen <ID>` | Set a live task incomplete | `Reopened 3: Fix the bug` | `Already open 3: Fix the bug` |
| `shtodo [--local] edit <ID> "New text"` | Replace a live task's text | `Edited 3: New text` | `Unchanged 3: New text` |
| `shtodo [--local] restore <ID>` | Clear a task's tombstone | `Restored 3: Fix the bug` | `Already live 3: Fix the bug` |

`done` and `reopen` set explicit states, so retries never toggle a task.
Unchanged edits compare the trimmed, validated text. These no-op successes,
including restoring an already-live task, do not rewrite the snapshot.
Editing requires exactly one text argument; it does not read stdin. Blank or
multiline text fails with usage help. Completion and editing preserve IDs and
canonical ordering. Restoration preserves the original text, completion, ID,
and canonical position, and does not change how TUI `u` selects the latest
remaining tombstone.

`done`, `reopen`, and `edit` reject deleted IDs and point to the scoped
`restore` command to run first. Unknown IDs fail. All shell mutations use the
selected scope's existing writer lock, save changes atomically, and report
success only after persistence succeeds. Even no-op mutations fail while
another writer holds that lock. Invalid keybinding configuration does not
block `add`, `list`, `delete`, `done`, `reopen`, `edit`, or `restore`.

### JSON output for agents

Use `--json` with `add`, `list`, `delete`, `done`, `reopen`, `edit`, or
`restore`. Put it before the command, immediately after the command, or after
all its positional arguments. For `list`, it may also go between filters, but
not right after a flag that takes a value. `--local` still goes before the
command:

```sh
shtodo list --json
shtodo --json list --open --tag +shtodo --sort priority
shtodo --local --json add 'Fix the bug'
shtodo --local done --json 3
shtodo --local edit 3 'Updated text' --json
printf '%s\n' 'Run the tests' | shtodo --local add --json
```

Success writes one compact JSON object and a newline to stdout, with empty
stderr and exit status 0. Listing an empty or never-created scope returns an
empty `tasks` array and creates no storage. JSON listing has the same saved
order, inclusion of open and done tasks, and omission of tombstones as text
listing. List filters and `--sort priority` select and order the same task
objects, as do `--due` and `--sort due`; the reply does not repeat the filter:

```json
{"schema_version":1,"command":"list","scope":{"kind":"global"},"tasks":[{"id":1,"text":"(A) Fix the bug +shtodo @laptop due:2026-11-01","state":"open","deleted":false,"priority":"A","tags":["+shtodo","@laptop"],"due":"2026-11-01"},{"id":2,"text":"Run the tests","state":"open","deleted":false,"priority":null,"tags":[],"due":null}]}
```

Mutation results contain the task from the saved transaction and a `changed`
boolean. Repeated state-setting commands, unchanged edits, repeated deletion,
and restoration of live tasks return `changed: false` without rewriting the
snapshot. An add always creates a new task:

```json
{"schema_version":1,"command":"done","scope":{"kind":"global"},"task":{"id":1,"text":"Fix the bug","state":"done","deleted":false,"priority":null,"tags":[],"due":null},"changed":true}
```

The output schema is independent of the on-disk snapshot schema. Its fields
are:

| Field | Contract |
| --- | --- |
| `schema_version` | Output schema version, currently `1`. |
| `command` | The shell command that produced the successful result. |
| `scope` | `{"kind":"global"}` or `{"kind":"project","path":"<canonical absolute directory>"}`. |
| `tasks` | Array of non-deleted tasks for `list`, in saved order. |
| `task` | Resulting task for a mutation, including deleted tasks after `delete`. |
| `changed` | Whether the mutation changed the saved task list. |

Each task has a positive numeric `id`, literal `text`, `state` (`open` or
`done`), boolean `deleted`, `priority`, `tags`, and `due`. Deleting a task preserves
its completion state. Retain scope alongside IDs. Task text is data, including
quotes, backslashes, Unicode, and tabs; parse it with a JSON library. Clients
should check `schema_version` and tolerate additional fields within that
version.

| Task field | Contract |
| --- | --- |
| `priority` | The [derived priority](#priority-and-tags) as a one-character string `"A"` to `"Z"`, or `null`. Never the `(A)` marker or a number. |
| `tags` | The derived tags as strings with their sigil and without trailing `.,;:!?`, in text order, without duplicates; `[]` when there are none. |
| `due` | The [derived due date](#due-dates) as a `"YYYY-MM-DD"` string, or `null` when the text has no valid due token. Never the `due:` key, a relative word, or a time. |

All three are always present on every task object, including deleted tasks in
mutation replies, and `text` still holds the full literal text, tokens
included. An `add` or `edit` reply shows the saved text after `due:today` and
`due:tomorrow` are rewritten, with its `due`, so a client can check the date
it wrote; `"due": null` after writing a `due:` token means the token was not a
valid date. There is no `overdue` field and no echoed "today", because both
would depend on the reader's clock and zone: compare `due` with your own
date, or ask shtodo with `--due overdue`. To find the tags in use, read them from `list`, for example
`shtodo --json list | jq '[.tasks[].tags[]] | group_by(.) | map({tag: .[0], count: length})'`.

Schema policy: new task metadata is always added as an always-present field,
nullable or an array, and never signaled by an absent field. Adding such a
field does not change `schema_version`; `due` was added this way in version
1. The version changes only when an existing field changes meaning, a field is
removed, or an enumerated value gains members clients must handle, such as a
third `state`. The field name `parent_id` (a number or `null`) is reserved for
a planned feature and is not emitted yet.

Failures write one JSON object and a newline to stderr, leave stdout empty,
and exit nonzero. Error messages are for people; branch on `error.code`:

```json
{"schema_version":1,"error":{"code":"task_not_found","message":"task 42 was not found","task_id":42}}
```

Codes are `invalid_arguments`, `invalid_task_text`, `task_not_found`,
`task_deleted`, `busy`, `durability_unconfirmed`, and `command_failed` for
other storage, environment, or I/O failures. `task_id` is included when an
error identifies a task. `durability_unconfirmed` means the change is already
visible but synchronization failed; inspect the same scope before retrying,
especially for adds. A lost response can also leave an add's outcome
uncertain, so JSON does not make repeated adds idempotent.

`--json` cannot be combined with `--print-id`, repeated, or used with the
TUI, `doctor`, help, or version commands. Immediately after `add`, `--json`
selects JSON mode; to add a task whose entire text is `--json`, pass the text
through stdin. After `edit <ID>`, a sole `--json` argument is literal task
text; select JSON mode before the command when editing to that text.

### Interactive editing

The interface has Normal, Insert, Search, Filter, Help, and Trash modes. Add or edit
tasks in Insert mode, then press Enter to save. Task text is trimmed, must be non-empty
and single-line, and Escape cancels an uncommitted add or edit. A terminal smaller
than 40 columns by 8 rows displays a resize message until it is large enough.
Pressing Enter with blank or all-whitespace text keeps the editor in Insert
mode, saves nothing, and shows `Task text cannot be empty`.

## Keyboard controls

### Normal mode

| Key           | Action                                          |
| ------------- | ----------------------------------------------- |
| `j` or Down   | Move selection down                             |
| `k` or Up     | Move selection up                               |
| `J`           | Move the selected task down (manual order only) |
| `K`           | Move the selected task up (manual order only)   |
| `i`           | Add a task                                      |
| `e`           | Edit the selected task                          |
| Space         | Toggle the selected task complete or incomplete |
| `d`           | Delete the selected task                        |
| `u`           | Restore the most recently deleted task          |
| `/`           | Enter or refine text search                     |
| `f`           | Edit the tag and priority filter                |
| `s`           | Toggle priority order and manual order          |
| Tab           | Move to the next All, Open, or Done tab          |
| Shift-Tab     | Move to the previous view tab                    |
| Esc           | Clear the accepted search; with none, the filter |
| `t`           | Open trash for the current scope                |
| `?`           | Open keyboard help                              |
| `q` or Ctrl-C | Quit                                            |

### Insert mode

Type to enter task text. `q` is ordinary text in this mode.

| Key                     | Action                                   |
| ----------------------- | ---------------------------------------- |
| Left or Right           | Move the text cursor                     |
| Alt-Left or Alt-b       | Move to the start of the previous word   |
| Alt-Right or Alt-f      | Move to the end of the next word         |
| Home or End             | Move the text cursor to the start or end |
| Backspace               | Delete before the cursor                 |
| Alt-Backspace or Ctrl-w | Delete the previous word                 |
| Delete                  | Delete at the cursor                     |
| Alt-Delete              | Delete the next word                     |
| Enter                   | Save the add or edit                     |
| Esc                     | Cancel the add or edit                   |
| Ctrl-C                  | Quit                                     |

On macOS, terminals report Option as Alt when Option is configured as an
Escape/Meta key. The common Meta-b and Meta-f encodings are supported by the
Alt-b and Alt-f aliases above.

### Filter mode

Press `f` to edit the filter. Filter mode uses the Insert mode text editing
keys above, and `q`, `s`, `f`, `/`, and Space are text. The buffer holds
space-separated tokens with the same grammar as the shell `--tag` and
`--priority` values: `+project`, `@context`, a priority letter `A`, a range
`A-C`, or `none`. For example, `+shtodo @laptop A-B` shows tasks with both tags
and priority `A` or `B`. Tags are combined with AND, priorities with OR. See
[Priority and tags](#priority-and-tags) and
[Filtering and sorting the list](#filtering-and-sorting-the-list).

| Key    | Action                                               |
| ------ | ---------------------------------------------------- |
| Enter  | Accept the filter, completing a unique tag prefix    |
| Esc    | Cancel and restore the previous filter and selection |
| Ctrl-C | Quit                                                 |

The list previews the filter as you type. Completed tokens match exactly. The
last token, while you are still typing it, matches any tag that starts with
it, so `+sh` previews both `+shtodo` and `+shell`. A sigil alone (`+` or `@`)
previews no tag constraint, and a priority token that does not parse yet
(`A-`) is left out of the preview.

A hint line above the footer lists the tags in the rows matching the view,
the search, and the completed tokens, with row counts, highest first:
`+shtodo 12 · +shell 3 · @laptop 2`. While you type a tag, it lists only the
tags that start with it. `No tags in this view` means those rows have no tags.

Enter resolves the last tag token against the tags in the hint line:

- A tag that is present is accepted as typed. Trailing `.,;:!?` is removed,
  as for `--tag`.
- A prefix of exactly one tag is completed to that tag, so `+sh` becomes
  `+shtodo` when no other tag starts with `+sh`.
- A prefix of several tags keeps the editor open and shows
  `Finish the tag: +shtodo, +shell`.
- A tag that matches nothing is accepted as typed, so you can filter for a tag
  no task has yet. The list is then empty.

Earlier tokens and priority tokens are never completed. An invalid token, such
as `foo` or a sigil alone, keeps the editor open with
`Invalid filter "foo"; use +project, @context, A, A-C, or none`. Accepting an
empty buffer clears the filter.

### Help mode

Help shows the current view's bindings. From the normal list it also shows
Insert controls; from trash it shows Trash controls. Closing help returns to
the view and selection from which it was opened.

| Key        | Action              |
| ---------- | ------------------- |
| `?` or Esc | Close keyboard help |
| Ctrl-C     | Quit                |

### Trash mode

Press `t` from the normal list to browse deleted tasks in the current global
or exact-directory scope. If an existing config uses Normal `t` for another
action, the implicit opening key falls back to `Ctrl-t`. When both are taken,
configure `open_trash` with an unused Normal key; `shtodo doctor` reports the
missing opening binding. See [Configuring keybindings] for the details.
The header and footer label this view `TRASH`.
Rows show each task's stable scope-local ID, `open` or `done` completion
state, and text, including tasks deleted with the shell `delete` command.
Trash is ordered by persisted `deletion_sequence`, most recently deleted
first, and opens with the newest deletion selected. No timestamps are added.

| Key           | Action                         |
| ------------- | ------------------------------ |
| `j` or Down   | Select the next older deletion |
| `k` or Up     | Select the next newer deletion |
| `r`           | Restore the selected task      |
| `t` or Esc    | Return to the normal list      |
| `?`           | Open trash keyboard help       |
| `q` or Ctrl-C | Quit                           |

Restoration saves immediately and leaves trash open. Selection moves to the
next older deletion, or to the preceding newer deletion if there is no older
one. Restoring the final tombstone clears selection and shows `Trash is empty`.
Pressing `r` in empty trash shows `Nothing to restore` without saving.
Navigation stops at either end of the list.

Trash lists every tombstone, independently of the live view and search query.
Opening and closing trash preserves that view and query. Returning to the
normal list selects the most recent restored task that matches them. If no
restored task matches, it preserves the prior matching selection; an empty
live view has no selection. Restoring a hidden task shows an explanatory message.
Reopening trash starts again at the newest remaining deletion. Restoration
only clears the deletion marker; text, completion, ID, and canonical position
are preserved. The normal list's `u` still restores the latest remaining
deletion, including after a restart.

Normal mutation, search, filter, and sort keys (`i`, `e`, Space, `d`, `J`,
`K`, `u`, `/`, `f`, `s`, Tab, and Shift-Tab) are inactive in trash. Tombstones cannot be edited,
completed, reordered, or permanently
removed from this view. Browse and help actions never save the snapshot.

To change these controls, see [Configuring keybindings].

## Search, filters, and task views

Each TUI session starts in All with an empty search. The tab strip highlights
and brackets the active view, such as `[All]`. Tab moves through All, Open,
Done, then All again; Shift-Tab moves backward, wrapping from All to Done.
These controls work in Normal mode; accept or cancel an edit before switching
views. Press `/` to edit the current query. Matches update as you type and
combine with the active view. Search uses a literal substring of task text
after Unicode lowercasing; spaces and punctuation are literal, and there are
no regular expressions or accent normalization. Search treats `+project` and
`@context` as plain text; use the filter for exact tags.

Search mode uses the same text editing bindings as Insert mode. Enter accepts
the query and returns to the list, including an empty query. Escape cancels
search editing and restores the previous query and selected task. From Normal
mode, Escape clears an accepted query while keeping Open/Done filtering. `q`,
`f`, `s`, `/`, and Space are text while editing a query; Ctrl-C still quits. All of
these keys follow the configured bindings described in [Configuring keybindings].

Press `f` to filter by tag and priority (see [Filter mode](#filter-mode)). The
view, the search, and the filter combine: a task is shown only if it matches
all three. For example, Open with `/bug` and `f +shtodo A-B` shows open tasks
containing `bug`, tagged `+shtodo`, with priority `A` or `B`. In Normal mode
Escape clears one layer per press: the accepted search first, then the filter.
The view tab is never cleared by Escape.

The tab strip and status line show the active view, matching count, and
`/query`, including an empty query. An accepted filter adds `f:` and its
tokens, and priority order adds `by priority`, for example
`· 3 matches · /bug · f:+shtodo A-B · by priority`. When a filter or sort is
shown, an empty query is left out. In a narrow terminal the sort label is
dropped first, then the filter is shortened with `…`; the count is always
shown. The header's open/done counts cover all
non-deleted tasks in the scope. An empty list shows `No tasks yet`; a populated
list whose current view, search, or filter matches nothing shows
`No matching tasks`. Search, filter, sort, and view changes are transient,
start fresh in every session, do not save anything, and do not affect
`shtodo list` output.

Rows keep their text exactly as typed. A priority marker such as `(A)` is
bold, `+project` tags are cyan, and `@context` tags are magenta. Completed
rows are dimmed and struck through, including those spans. Trash rows use the
same styling.

Matching tasks retain their manual, canonical order unless priority order is
on. Press `s` to toggle priority order: `A` to `Z`, then tasks without a
priority, with manual order breaking ties, the same as
`shtodo list --sort priority`. Open and done tasks are not separated; use the
Open tab for that. Selection keeps the same task when the order changes.
Navigation and task actions operate on the displayed rows. Selection keeps its task identity while
that task still matches. If completion, reopening, editing, or a view change
hides it, selection moves to the next matching task in display order, then
the previous one. Deletion also selects the next match, then the previous one.
No matches means no selected task, so navigation, reordering, editing,
completion, and deletion safely do nothing.

`J`/`K` reorder only in manual order. In priority order they change nothing
and show `Reorder uses manual order; press s to switch`, naming the configured
`toggle_sort` key. In manual order, `J`/`K` swap the selected task with its
next/previous matching neighbor in their existing canonical slots. With a
filter, the neighbor is the next or previous task that matches the view,
search, and filter. If another process changes the list so that neighbor no
longer matches, the move is refused with `Task order changed; review and
retry`. Hidden tasks and tombstones stay in their
slots. This preserves manual ordering without shifting hidden rows. A new or
restored task is selected if it matches; otherwise the current selection is
kept and a message explains that the task is hidden by the current view,
search, or filter. `u`
still restores the most recently deleted task even if it will be hidden.
The completion celebration occurs only when the final actual open task is
completed, including open tasks hidden by search.

## Storage and project lists

The global list is stored at `~/.shtodo/global/tasks.json`. A local list is
stored beneath `~/.shtodo/projects/` in a readable directory name plus a stable
fingerprint of that list's canonical absolute directory. Each snapshot records
which scope it belongs to, so a global snapshot cannot be opened as a project
snapshot, or vice versa.

Changes are saved immediately after a successful add, edit, completion toggle,
reorder, deletion, or restoration. Snapshots are written through a temporary
file and atomically replace the previous canonical snapshot. Deletions are
tombstones rather than immediate erasure, so `u` restores the latest deleted
task and `t` lists deletions for selective restoration, even after quitting
and relaunching. If no tombstone is available, `u` shows `Nothing to restore`
and leaves the snapshot unchanged.

Each list scope has its own writer lock, held only while applying and saving a
change. Local-list identity is the canonical absolute directory path. Moving a
directory therefore creates a different local-list identity, even if its name
is unchanged. The read-only `list` command does not acquire this writer lock.

## Concurrent processes

Several TUIs and shell commands can share the same global or exact-directory
list. Every change reloads the latest snapshot under its scope's writer lock,
so unrelated additions, edits, and other updates survive. An idle TUI checks
for changes once per second. Selection stays on the same task when possible;
draft text, cursor position, search/filter state, Trash, and Help remain open
during refresh. Opening or
refreshing a never-created list creates no task storage.

Shell mutations wait up to one second to acquire the lock; interactive
mutations wait up to 100 milliseconds. When that budget expires, the command
reports a lock timeout or the TUI shows the error and keeps its draft. Retry
after the other writer finishes. These budgets limit lock acquisition, not the
disk work after acquisition. Different scopes use independent locks. Older
versions hold the lock for their entire TUI session: close that older session
to let new mutations proceed. New TUIs and `list` can still read its last
complete snapshot.

Interactive actions use the task and values displayed when you press the key.
Completion sets the intended state, so two stale completion actions cannot
toggle each other back. Reordering checks the observed adjacent task in the
active view/search before swapping, retaining hidden canonical slots. Selective
trash restoration checks the observed deletion sequence, so an unseen
restore/delete cycle needs review before another restoration. Deletion checks
that the displayed task has not changed. An edit
checks its original text, allowing unrelated completion or order changes to
coexist. If another writer changes the text, a conflicting draft stays visible
with a conflict hint. A draft that already matches the current text can close
without writing. Use the configured cancel key, Escape by default, then reopen
the edit to review the current text. A deleted edit target keeps a detached
draft and cannot be resurrected by saving it. Once refresh observes that
deletion, restoring the task does not make the old draft eligible again.

If refresh cannot validate or read storage, the TUI keeps its last good list
and draft, blocks mutations, and retries with a delay capped at five seconds.
A snapshot that disappears after the TUI has opened it is an error; the TUI
does not recreate it with reset IDs. Restore the valid file to resume.
An absent `tasks.lock`, such as after restoring only `tasks.json` from a backup,
is recreated by the next write or sync recovery. Opening and refreshing the
list remain read-only.

An error after atomic replacement can mean the change is already visible but
its durability is unconfirmed. The TUI retains a warning, freezes the committed
draft, and retries synchronization of the latest snapshot without repeating
the action. Enter also retries synchronization; Escape can close the draft
but leaves the warning and new writes blocked until synchronization succeeds.
A successful refresh alone cannot clear that warning. Shell commands report
the visible task ID in this case; inspect `shtodo list` before repeating an add.
Losing a process response does not provide exactly-once execution.

Conflict checks compare current values in schema 1. An unseen text change that
returns to the original value, or an unseen deletion followed by restoration,
may allow an edit. This is not a record of every intervening task event.

## Version-one limits

Version one is intentionally local and narrow. It does not include accounts,
synchronization, network access, sharing or collaboration, recurring tasks,
reminders, notifications, times of day, start or threshold dates, subtasks,
or multiple named lists. Priorities, tags, and due dates exist only as
todo.txt tokens in the task text ([priority and tags](#priority-and-tags),
[due dates](#due-dates)), with shell `list` filters, `--sort priority`, and
`--sort due`. Due dates are calendar days; relative input is limited to
`due:today` and `due:tomorrow`.
It has no sidebar, mouse interaction, Git-root
discovery for local scope, runtime plugins or extensions, custom themes,
import, export, bulk commands, permanent deletion, or bulk restoration.

The following work is explicitly deferred: permanent deletion and automatic
purging of tombstones; a sidebar for global, project, trash, and later views.
Editing is scalar-value-based, so grapheme-cluster-aware editing
is deferred if it becomes necessary. Distribution through package managers
other than Homebrew, plus broader Windows runtime testing and support, are
also deferred. Windows is kept build-compatible where practical, but full
Windows runtime support is not a version-one promise.

Version one also does not promise cross-device conflict resolution or
compatibility with task-manager formats.

[Configuring keybindings]: ./configuration.md
