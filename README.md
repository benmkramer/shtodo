# shtodo 💩

For people who have **sh**it **todo** and want to track it in their terminal.
`shtodo` is a deliberately small, keyboard-first terminal todo list. It keeps
one global list by default and can keep an independent list for an exact
project directory.

## Installation

### Homebrew

```sh
brew install benmkramer/tap/shtodo
```

The tap distributes the current beta release for Apple Silicon macOS,
Intel macOS, and x86-64 Linux. Update an existing installation with
`brew update` followed by `brew upgrade shtodo`.

### Prebuilt binaries

Prebuilt archives for Apple Silicon macOS, Intel macOS, and x86-64 Linux are
attached to each [GitHub Release]. Each archive has a corresponding SHA-256
checksum.

Use an authenticated GitHub CLI session to download and run the installer:

```sh
gh release download --repo benmkramer/shtodo --pattern shtodo-installer.sh
SHTODO_GITHUB_TOKEN="$(gh auth token)" sh ./shtodo-installer.sh
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

## Quick start

```text
shtodo
shtodo --local
shtodo add "Fix the bug"
shtodo --local add "Run the tests"
shtodo list
shtodo --local list
shtodo delete 3
shtodo --local delete 3
shtodo done 3
shtodo reopen 3
shtodo edit 3 "Fix the remaining bug"
shtodo restore 3
shtodo add --print-id "Capture a task for a script"
shtodo doctor
```

Running `shtodo` opens the default global list. Running `shtodo --local` opens
a list for the exact directory from which it is run.

`shtodo list` prints each non-deleted task's scope-local ID, state, and text.
Use `done`, `reopen`, `edit`, `delete`, and `restore` with one scope-local ID
to manage a task from the shell. Deleted tasks retain their text and state;
restore them by ID, with the TUI's `u` action for the latest deletion, or with
`t` to browse trash and restore a specific task. Add `--local` before the
command to select the exact current-directory list. `add --print-id` prints
only the saved ID for scripts.

The essentials are:

```text
i add · e edit · Space complete · d delete · u restore latest · t trash · ? help · q quit
```

See [Usage and keyboard controls] for the full interaction guide.

## Agent skill

An optional [shtodo skill](./skills/shtodo/SKILL.md) helps LLM-based agents
use the CLI with the right scope, task IDs, and verification. With Node.js
and npm available, install it for your agent:

```sh
npx skills add benmkramer/shtodo --skill shtodo --global
```

The installer supports Codex, Claude Code, Cursor, and other compatible
agents. Install the shtodo binary separately using the instructions above.
See [Agent skill installation and usage] for manual installation and
example prompts.

## Documentation

- [Usage and keyboard controls]
- [Configuring keybindings]
- [Agent skill installation and usage]
- [Release process]
- [Benchmarks and CLI comparison]: results, methodology, feature differences,
  and rerun instructions. Also see the [benchmark harness] and [recorded samples].

## Scope

Version one is intentionally local and narrow. It does not include accounts,
synchronization, sharing, recurring tasks, reminders, dates, priorities, tags,
search, or multiple named lists. See [Version-one limits] for the complete
scope and deferred features.

See [Benchmarks and CLI comparison] for selected command latencies and feature
differences. `shtodo`'s narrow scope matters when interpreting those
results; the tools provide different capabilities and storage guarantees.

## License

Copyright (c) Ben Kramer <benmkramer@gmail.com>

This project is licensed under the MIT license ([LICENSE] or
<http://opensource.org/licenses/MIT>).

[LICENSE]: ./LICENSE
[GitHub Release]: https://github.com/benmkramer/shtodo/releases
[Usage and keyboard controls]: ./docs/usage.md
[Configuring keybindings]: ./docs/configuration.md
[Agent skill installation and usage]: ./docs/agent-skill.md
[Release process]: ./docs/releasing.md
[Benchmarks and CLI comparison]: ./docs/benchmarks.md
[benchmark harness]: ./scripts/benchmark.py
[recorded samples]: ./docs/benchmarks/
[Version-one limits]: ./docs/usage.md#version-one-limits
