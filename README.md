# shtodo

`shtodo` is a deliberately small, keyboard-first terminal todo list. It keeps
one global list by default and can keep an independent list for an exact
project directory.

## Install from source

`shtodo` requires Rust 1.89 or newer. From a checkout with its lockfile:

```sh
cargo install --path . --locked
```

To build an optimized local binary without installing it:

```sh
cargo build --release --locked
```

## Usage

```text
shtodo
shtodo --local
shtodo --help
shtodo --version
```

Running `shtodo` opens the default global list. Running `shtodo --local` opens
a list for the exact directory from which it is run. `--local` does not search
parent directories or use a repository root. Use `--help` (or `-h`) for usage
and `--version` (or `-V`) for the installed version.

The interface has Normal, Insert, and Help modes. Add or edit tasks in Insert
mode, then press Enter to save. Task text is trimmed, must be non-empty and
single-line, and Escape cancels an uncommitted add or edit. A terminal smaller
than 40 columns by 8 rows displays a resize message until it is large enough.

## Keyboard controls

### Normal mode

| Key | Action |
| --- | --- |
| `j` or Down | Move selection down |
| `k` or Up | Move selection up |
| `J` | Move the selected task down |
| `K` | Move the selected task up |
| `i` | Add a task |
| `e` | Edit the selected task |
| Space | Toggle the selected task complete or incomplete |
| `d` | Delete the selected task |
| `u` | Restore the most recently deleted task |
| `?` | Open keyboard help |
| `q` or Ctrl-C | Quit |

### Insert mode

Type to enter task text. `q` is ordinary text in this mode.

| Key | Action |
| --- | --- |
| Left or Right | Move the text cursor |
| Home or End | Move the text cursor to the start or end |
| Backspace | Delete before the cursor |
| Delete | Delete at the cursor |
| Enter | Save the add or edit |
| Esc | Cancel the add or edit |
| Ctrl-C | Quit |

### Help mode

| Key | Action |
| --- | --- |
| `?` or Esc | Close keyboard help |
| Ctrl-C | Quit |

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
different local-list identity, even if its name is unchanged.

## Version-one limits

Version one is intentionally local and narrow. It has no sync, accounts,
network service, sharing or collaboration, reminders, notifications, due dates,
recurrence, tags, projects beyond exact-directory lists, search, filtering,
import or export, mouse support, runtime plugins, custom themes, configurable
keybindings, or additional task-management modes. It also does not promise
cross-device conflict resolution or compatibility with task-manager formats.

## License

Copyright (c) Ben Kramer <benmkramer@gmail.com>

This project is licensed under the MIT license ([LICENSE] or
<http://opensource.org/licenses/MIT>)

[LICENSE]: ./LICENSE
