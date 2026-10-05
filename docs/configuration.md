# Configuring keybindings

The optional configuration file is `~/.shtodo/config.toml`; shtodo does not
create it. When the file is missing, shtodo uses its compiled defaults.
Configuring an action replaces that action's defaults; omitted actions retain
their defaults subject to the upgrade compatibility rules below.

```toml
[keybindings.normal]
move_down = ["j", "down", "ctrl-n"]
move_up = ["k", "up", "ctrl-p"]
add_task = ["a"]
start_search = ["/"]
cycle_view = ["tab"]
previous_view = ["shift-tab"]
clear_search = ["esc"]
open_help = ["?"]
open_trash = ["t"]

[keybindings.trash]
restore_selected = ["r"]
close_trash = ["t", "esc"]

[keybindings.insert]
commit_edit = ["enter"]
cancel_edit = ["esc"]

[keybindings.help]
close_help = ["?", "esc"]
```

Array order matters: the first key is used in the footer and every key is
shown in Help. Accepted named keys are `up`, `down`, `left`, `right`, `home`,
`end`, `page-up`, `page-down`, `tab`, `backtab` (also `shift-tab`), `enter`,
`esc`, `space`, `backspace`, `delete`, and `insert`. Ctrl and Alt modifiers use
forms such as `ctrl-n`, `alt-left`, and `ctrl-alt-x`; shifted printable characters use the
resulting character such as `J`. Named keys and modifier names are ASCII
case-insensitive, while unmodified printable characters remain case-sensitive.
Modified ASCII letters normalize to lowercase for matching and conflict
detection. Help and diagnostics show canonical labels generated from that
normalized form, such as `Down`, `Ctrl-n`, and `Alt-Left`, regardless of the
casing used in the config file. `backtab` and `shift-tab` both display as
`Shift-Tab` and refer to the same key. Shift is supported explicitly for Tab;
other shifted printable characters use their resulting character.

`Ctrl-C` is fixed in all modes and cannot be configured. Invalid config stops
interactive startup and points to `shtodo doctor`. `shtodo doctor` checks the
same parser and validator without opening task storage or the TUI.

Shell task commands (`add`, `list`, `delete`, `done`, `reopen`, `edit`, and
`restore`) do not load keybinding configuration. They remain available when
the configuration is invalid; `add --print-id` has the same independence.

## Upgrading existing configurations

The new Normal-mode defaults for `start_search` (`/`), `cycle_view` (`tab`),
`previous_view` (`shift-tab`), and `clear_search` (`esc`) yield to explicit
bindings in the same mode. Existing valid configs that already use those keys
continue to load and keep their behavior. The conflicting new default becomes
unbound; shtodo does not rewrite your config or choose an alternative search
key automatically.

Help marks these actions as `Unbound: cycle_view (config)` or the corresponding
action name. The footer shows active bindings. To enable an unbound action,
assign it a free key, for example:

```toml
[keybindings.normal]
toggle_complete = ["tab"]
cycle_view = ["v"]
```

Conflicts between explicit bindings and conflicts with older defaults are
still configuration errors. `Ctrl-C` remains fixed. `shtodo doctor` reports
the number of supported configurable actions and the effective active keys,
including configs with unbound search actions.

### Existing bindings and the new trash default

Existing Normal bindings take priority over the new implicit `open_trash`
default. If your config already assigns `t` to another Normal action, trash
opens with `Ctrl-t`; the footer, Help, and empty-list hint show that fallback.
For example, this existing config continues to load and keeps `t` completing
tasks:

```toml
[keybindings.normal]
toggle_complete = ["t"]
```

If both `t` and `Ctrl-t` are already assigned, the config still loads and the
implicit trash opening binding is omitted. `shtodo doctor` reports that trash
needs an opening key. Set `open_trash = ["b"]` (or another unused Normal key)
to enable it. An explicit `open_trash` override still participates in ordinary
conflict validation. Other binding conflicts and reserved-key errors remain
invalid. Trash `close_trash` remains independent and defaults to `t` and Esc.

## Default actions

### Normal

| Action            | Default keys |
| ----------------- | ------------ |
| `move_down`       | `j`, `down`  |
| `move_up`         | `k`, `up`    |
| `move_task_down`  | `J`          |
| `move_task_up`    | `K`          |
| `add_task`        | `i`          |
| `edit_task`       | `e`          |
| `toggle_complete` | `space`      |
| `delete_task`     | `d`          |
| `restore_latest`  | `u`          |
| `start_search`    | `/`          |
| `cycle_view`      | `tab`        |
| `previous_view`   | `shift-tab`  |
| `clear_search`    | `esc`        |
| `open_trash`      | `t`          |
| `open_help`       | `?`          |
| `quit`            | `q`          |

### Insert

| Action                      | Default keys              |
| --------------------------- | ------------------------- |
| `move_cursor_left`          | `left`                    |
| `move_cursor_right`         | `right`                   |
| `move_cursor_start`         | `home`                    |
| `move_cursor_end`           | `end`                     |
| `move_word_left`            | `alt-left`, `alt-b`       |
| `move_word_right`           | `alt-right`, `alt-f`      |
| `delete_before_cursor`      | `backspace`               |
| `delete_at_cursor`          | `delete`                  |
| `delete_word_before_cursor` | `alt-backspace`, `ctrl-w` |
| `delete_word_at_cursor`     | `alt-delete`              |
| `commit_edit`               | `enter`                   |
| `cancel_edit`               | `esc`                     |

### Help

| Action       | Default keys |
| ------------ | ------------ |
| `close_help` | `?`, `esc`   |

### Search

Search shares every binding in `[keybindings.insert]`; there is no separate
`[keybindings.search]` table. In Search mode, `commit_edit` accepts the query
and returns to Normal, while `cancel_edit` restores the previous query and
selection. Cursor movement and deletion work the same way as in Insert.
Changing an Insert binding changes it for both editors, and Help and the
Search footer show the effective keys with search-specific accept/cancel hints.

In Normal mode, `start_search` opens the current query, `cycle_view` moves to
the next All/Open/Done tab, `previous_view` moves to the previous tab, and
`clear_search` clears only the query. Both tab controls wrap at either end.
To use a letter shortcut instead, configure `cycle_view = ["f"]` or another
free key in `[keybindings.normal]`. Search/view state is never stored in
configuration or task snapshots.

### Trash

| Action             | Default keys |
| ------------------ | ------------ |
| `move_down`        | `j`, `down`  |
| `move_up`          | `k`, `up`    |
| `restore_selected` | `r`          |
| `close_trash`      | `t`, `esc`   |
| `open_help`        | `?`          |
| `quit`             | `q`          |

Trash bindings are independent of Normal bindings. For example, changing
Normal `open_trash` does not change Trash `close_trash`; configure both to use
the same key if desired. Normal navigation, help, and quit overrides do not
carry into Trash. Duplicate keys conflict within a mode; the same key can be
used in different modes. Trash help and footer use the resolved Trash keymap.

Search and view controls apply only to the live list. Trash always shows all
tombstones; returning to the live list preserves its active tab and query.

`Ctrl-C` is a fixed emergency quit key in all five modes.

See [Usage and keyboard controls] for the interaction guide.

[Usage and keyboard controls]: ./usage.md
