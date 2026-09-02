# shtodo

`shtodo` is a deliberately small, keyboard-first terminal todo list. It keeps
one global list by default and can keep an independent list for an exact
project directory.

## Installation

### Prebuilt binaries

Prebuilt archives for Apple Silicon macOS, Intel macOS, and x86-64 Linux are
attached to each [GitHub Release]. Each archive has a corresponding SHA-256
checksum.

The repository is currently private, so downloading a release requires GitHub
read access. An authenticated GitHub CLI session can download and run the
installer:

```sh
gh release download --repo benmkramer/shtodo --pattern shtodo-installer.sh
SHTODO_GITHUB_TOKEN="$(gh auth token)" sh ./shtodo-installer.sh
```

If the repository becomes public, the shell installer can be run directly and
will select the correct archive and install `shtodo` into Cargo's binary
directory:

```sh
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/benmkramer/shtodo/releases/latest/download/shtodo-installer.sh | sh
```

### Install from source

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
shtodo add "Fix the bug"
shtodo --local add "Run the tests"
shtodo doctor
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
Pressing Enter with blank or all-whitespace text keeps the editor in Insert
mode, saves nothing, and shows `Task text cannot be empty`.

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
| Alt-Left or Alt-b | Move to the start of the previous word |
| Alt-Right or Alt-f | Move to the end of the next word |
| Home or End | Move the text cursor to the start or end |
| Backspace | Delete before the cursor |
| Alt-Backspace or Ctrl-w | Delete the previous word |
| Delete | Delete at the cursor |
| Alt-Delete | Delete the next word |
| Enter | Save the add or edit |
| Esc | Cancel the add or edit |
| Ctrl-C | Quit |

On macOS, terminals report Option as Alt when Option is configured as an
Escape/Meta key. The common Meta-b and Meta-f encodings are supported by the
Alt-b and Alt-f aliases above.

### Help mode

| Key | Action |
| --- | --- |
| `?` or Esc | Close keyboard help |
| Ctrl-C | Quit |

## Configuring keybindings

The optional configuration file is `~/.shtodo/config.toml`; shtodo does not
create it. When the file is missing, shtodo uses its compiled defaults.
Configuring an action replaces that action's defaults; omitted actions retain
their defaults.

```toml
[keybindings.normal]
move_down = ["j", "down", "ctrl-n"]
move_up = ["k", "up", "ctrl-p"]
add_task = ["a"]
open_help = ["?"]

[keybindings.insert]
commit_edit = ["enter"]
cancel_edit = ["esc"]

[keybindings.help]
close_help = ["?", "esc"]
```

Array order matters: the first key is used in the footer and every key is
shown in Help. Accepted named keys are `up`, `down`, `left`, `right`, `home`,
`end`, `page-up`, `page-down`, `tab`, `backtab`, `enter`, `esc`, `space`,
`backspace`, `delete`, and `insert`. Ctrl and Alt modifiers use forms such as
`ctrl-n`, `alt-left`, and `ctrl-alt-x`; shifted printable characters use the
resulting character such as `J`. Named keys and modifier names are ASCII
case-insensitive, while unmodified printable characters remain case-sensitive.
Modified ASCII letters normalize to lowercase for matching and conflict
detection. Help and diagnostics show canonical labels generated from that
normalized form, such as `Down`, `Ctrl-n`, and `Alt-Left`, regardless of the
casing used in the config file.

`Ctrl-C` is fixed in all modes and cannot be configured. Invalid config stops
interactive startup and points to `shtodo doctor`. `shtodo doctor` checks the
same parser and validator without opening task storage or the TUI.

### Default actions

#### Normal

| Action | Default keys |
| --- | --- |
| `move_down` | `j`, `down` |
| `move_up` | `k`, `up` |
| `move_task_down` | `J` |
| `move_task_up` | `K` |
| `add_task` | `i` |
| `edit_task` | `e` |
| `toggle_complete` | `space` |
| `delete_task` | `d` |
| `restore_latest` | `u` |
| `open_help` | `?` |
| `quit` | `q` |

#### Insert

| Action | Default keys |
| --- | --- |
| `move_cursor_left` | `left` |
| `move_cursor_right` | `right` |
| `move_cursor_start` | `home` |
| `move_cursor_end` | `end` |
| `move_word_left` | `alt-left`, `alt-b` |
| `move_word_right` | `alt-right`, `alt-f` |
| `delete_before_cursor` | `backspace` |
| `delete_at_cursor` | `delete` |
| `delete_word_before_cursor` | `alt-backspace`, `ctrl-w` |
| `delete_word_at_cursor` | `alt-delete` |
| `commit_edit` | `enter` |
| `cancel_edit` | `esc` |

#### Help

| Action | Default keys |
| --- | --- |
| `close_help` | `?`, `esc` |

`Ctrl-C` is a fixed emergency quit key in all three modes.

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

Version one is intentionally local and narrow. It does not include accounts,
synchronization, network access, sharing or collaboration, recurring tasks,
reminders, notifications, dates or due dates, priorities, tags, or multiple
named lists. It has no trash view, sidebar, mouse interaction, Git-root
discovery for local scope, runtime plugins or extensions, custom themes,
search, filtering, import, export, or additional task-management modes.

The following work is explicitly deferred: a trash view that lists, restores,
and permanently removes tombstones; a sidebar for global, project, trash, and
later views. Editing is scalar-value-based, so grapheme-cluster-aware editing
is deferred if it becomes necessary. Homebrew and other package-manager
distribution, plus broader Windows runtime testing and support, are also
deferred. Windows is kept build-compatible where practical, but full Windows
runtime support is not a version-one promise.

Version one also does not promise cross-device conflict resolution or
compatibility with task-manager formats.

## Releasing

Release preparation happens in a normal pull request. Update the version in
`Cargo.toml`, refresh `Cargo.lock`, move the completed notes from `Unreleased`
into a versioned `CHANGELOG.md` section, and merge only after CI passes.

To rehearse a release, open the `Release` workflow in GitHub Actions, select
`main`, leave the tag as `dry-run`, and run the workflow. To publish, run the
same workflow from `main` with a tag matching the package version, such as
`v0.1.0`. The workflow creates the tag and GitHub Release only after all target
artifacts build successfully.

## License

Copyright (c) Ben Kramer <benmkramer@gmail.com>

This project is licensed under the MIT license ([LICENSE] or
<http://opensource.org/licenses/MIT>)

[LICENSE]: ./LICENSE
[GitHub Release]: https://github.com/benmkramer/shtodo/releases
