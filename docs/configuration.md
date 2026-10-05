# Configuring keybindings

The optional configuration file is `~/.shtodo/config.toml`; shtodo does not
create it. When the file is missing, shtodo uses its compiled defaults.
Configuring an action replaces that action's defaults; omitted actions retain
their defaults, with the trash compatibility exception described below.

```toml
[keybindings.normal]
move_down = ["j", "down", "ctrl-n"]
move_up = ["k", "up", "ctrl-p"]
add_task = ["a"]
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

`Ctrl-C` is a fixed emergency quit key in all four modes.

See [Usage and keyboard controls] for the interaction guide.

[Usage and keyboard controls]: ./usage.md
