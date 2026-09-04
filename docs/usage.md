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
shtodo doctor
shtodo --help
shtodo --version
```

Running `shtodo` opens the default global list. Running `shtodo --local` opens
a list for the exact directory from which it is run. `--local` does not search
parent directories or use a repository root. Use `--help` (or `-h`) for usage
and `--version` (or `-V`) for the installed version.

### Shell listing and deletion

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

Deleted tasks retain their tombstones and can be restored with `u` in the TUI.
An unknown ID fails with `task 3 was not found`. Missing, zero, signed,
nonnumeric, and extra IDs are usage errors. Deletion uses the selected scope's
writer lock and atomic save path, so it fails if another writer holds that
lock. Invalid keybinding configuration does not block either shell command.

The interface has Normal, Insert, and Help modes. Add or edit tasks in Insert
mode, then press Enter to save. Task text is trimmed, must be non-empty and
single-line, and Escape cancels an uncommitted add or edit. A terminal smaller
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

| Key        | Action              |
| ---------- | ------------------- |
| `?` or Esc | Close keyboard help |
| Ctrl-C     | Quit                |

To change these controls, see [Configuring keybindings].

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
task even after quitting and relaunching. If no tombstone is available, `u`
shows `Nothing to restore` and leaves the snapshot unchanged.

Each list scope has its own process lock. A second `shtodo` process for the
same global or local list is rejected while the first holds the lock; a global
and a different local list can be open independently. Local-list identity is
the canonical absolute directory path. Moving a directory therefore creates a
different local-list identity, even if its name is unchanged. The read-only
`list` command does not acquire this writer lock.

## Version-one limits

Version one is intentionally local and narrow. It does not include accounts,
synchronization, network access, sharing or collaboration, recurring tasks,
reminders, notifications, dates or due dates, priorities, tags, or multiple
named lists. It has no trash view, sidebar, mouse interaction, Git-root
discovery for local scope, runtime plugins or extensions, custom themes,
search, filtering, import, export, structured JSON output, bulk commands,
permanent deletion, or additional task-management modes.

The following work is explicitly deferred: a trash view that lists, restores,
and permanently removes tombstones; a sidebar for global, project, trash, and
later views. Editing is scalar-value-based, so grapheme-cluster-aware editing
is deferred if it becomes necessary. Homebrew and other package-manager
distribution, plus broader Windows runtime testing and support, are also
deferred. Windows is kept build-compatible where practical, but full Windows
runtime support is not a version-one promise.

Version one also does not promise cross-device conflict resolution or
compatibility with task-manager formats.

[Configuring keybindings]: ./configuration.md
