# Usage and keyboard controls

## Commands

```text
shtodo
shtodo --local
shtodo add "Fix the bug"
shtodo --local add "Run the tests"
shtodo list
shtodo --local list
shtodo delete 3
shtodo --local delete 3
shtodo [--local] done 3
shtodo [--local] reopen 3
shtodo [--local] edit 3 "New text"
shtodo [--local] restore 3
shtodo [--local] add --print-id "Fix the bug"
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

### Interactive editing

The interface has Normal, Insert, Search, Help, and Trash modes. Add or edit
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
| `J`           | Move the selected task down                     |
| `K`           | Move the selected task up                       |
| `i`           | Add a task                                      |
| `e`           | Edit the selected task                          |
| Space         | Toggle the selected task complete or incomplete |
| `d`           | Delete the selected task                        |
| `u`           | Restore the most recently deleted task          |
| `/`           | Enter or refine text search                     |
| Tab           | Move to the next All, Open, or Done tab          |
| Shift-Tab     | Move to the previous view tab                    |
| Esc           | Clear the accepted search, keeping the view     |
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

Normal mutation and search keys (`i`, `e`, Space, `d`, `J`, `K`, `u`, `/`,
Tab, and Shift-Tab) are inactive in trash. Tombstones cannot be edited,
completed, reordered, or permanently
removed from this view. Browse and help actions never save the snapshot.

To change these controls, see [Configuring keybindings].

## Search and task views

Each TUI session starts in All with an empty search. The tab strip highlights
and brackets the active view, such as `[All]`. Tab moves through All, Open,
Done, then All again; Shift-Tab moves backward, wrapping from All to Done.
These controls work in Normal mode; accept or cancel an edit before switching
views. Press `/` to edit the current query. Matches update as you type and
combine with the active view. Search uses a literal substring of task text
after Unicode lowercasing; spaces and punctuation are literal, and there are
no regular expressions, accent normalization, or tags.

Search mode uses the same text editing bindings as Insert mode. Enter accepts
the query and returns to the list, including an empty query. Escape cancels
search editing and restores the previous query and selected task. From Normal
mode, Escape clears an accepted query while keeping Open/Done filtering. `q`,
`f`, `/`, and Space are text while editing a query; Ctrl-C still quits. All of
these keys follow the configured bindings described in [Configuring keybindings].

The tab strip and status line show the active view, matching count, and
`/query`, including an empty query. The header's open/done counts cover all
non-deleted tasks in the scope. An empty list shows `No tasks yet`; a populated
list whose current view/search matches nothing shows `No matching tasks`. Search and view changes
are transient, do not save anything, and do not affect `shtodo list` output.

Matching tasks retain their manual, canonical order. Navigation and task
actions operate on those same rows. Selection keeps its task identity while
that task still matches. If completion, reopening, editing, or a view change
hides it, selection moves to the next matching task in canonical order, then
the previous one. Deletion also selects the next match, then the previous one.
No matches means no selected task, so navigation, reordering, editing,
completion, and deletion safely do nothing.

`J`/`K` swap the selected task with its next/previous matching neighbor in
their existing canonical slots. Hidden tasks and tombstones stay in their
slots. This preserves manual ordering without shifting hidden rows. A new or
restored task is selected if it matches; otherwise the current selection is
kept and a message explains that the task is hidden by the view/search. `u`
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
reminders, notifications, dates or due dates, priorities, tags, or multiple
named lists. It has no sidebar, mouse interaction, Git-root
discovery for local scope, runtime plugins or extensions, custom themes,
shell search or filters, import, export, structured JSON output, bulk commands,
permanent deletion, or bulk restoration.

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
