# shtodo 💩

For people who have **sh**it **todo** and want to track it in their terminal.
`shtodo` is a deliberately small, keyboard-first terminal todo list. It keeps
one global list by default and can keep an independent list for an exact
project directory.

## Installation

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
shtodo doctor
```

Running `shtodo` opens the default global list. Running `shtodo --local` opens
a list for the exact directory from which it is run.

`shtodo list` prints each non-deleted task's scope-local ID, state, and text.
`shtodo delete <ID>` recoverably deletes one task from the selected scope, so
the TUI's `u` action can restore it.

The essentials are:

```text
i add · e edit · Space complete · d delete · u restore · ? help · q quit
```

See [Usage and keyboard controls] for the full interaction guide.

## Documentation

- [Usage and keyboard controls]
- [Configuring keybindings]
- [Release process]

## Scope

Version one is intentionally local and narrow. It does not include accounts,
synchronization, sharing, recurring tasks, reminders, dates, priorities, tags,
search, or multiple named lists. See [Version-one limits] for the complete
scope and deferred features.

## License

Copyright (c) Ben Kramer <benmkramer@gmail.com>

This project is licensed under the MIT license ([LICENSE] or
<http://opensource.org/licenses/MIT>).

[LICENSE]: ./LICENSE
[GitHub Release]: https://github.com/benmkramer/shtodo/releases
[Usage and keyboard controls]: ./docs/usage.md
[Configuring keybindings]: ./docs/configuration.md
[Release process]: ./docs/releasing.md
[Version-one limits]: ./docs/usage.md#version-one-limits
