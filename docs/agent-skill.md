# Agent skill

The optional [shtodo skill] teaches an LLM-based agent to use the installed
CLI, select the right list, preserve task IDs, and verify changes. It also
explains which actions currently require the TUI. The skill supplements
`shtodo --help` and works without a checkout of shtodo's source code.

Install the [shtodo binary] separately. The agent needs shell access to that
binary and to your local task storage; installing the skill does not sync
tasks to a remote agent environment.

## Install

With Node.js and npm available, use the [skills CLI] to install the published
skill from GitHub:

```sh
npx skills add benmkramer/shtodo --skill shtodo --global
```

Select your agent in the installer. To target Codex directly, add
`--agent codex`. Omit `--global` for a project-level skill installation.
The skill's installation location is independent of shtodo's task scope;
`shtodo --local` still selects tasks for the exact working directory.

To try a local or unpublished checkout, run this from the repository root:

```sh
npx skills add ./skills/shtodo --skill shtodo --global
```

For a manual install, copy the entire `skills/shtodo` directory to your
agent's skill directory. For Codex, use `~/.agents/skills/shtodo`, as described
in the [Codex skill documentation]. For Claude Code, use
`~/.claude/skills/shtodo`, as described in the [Claude Code skill
documentation]. Restart the agent if the skill does not appear.

## Use

In Codex, invoke it as `$shtodo`, for example:

```text
Use $shtodo to show my global tasks.
Use $shtodo to add "Review the release notes" to my global list.
Use $shtodo to show the open tasks for this directory.
Use $shtodo to delete task 3 from this directory's list.
```

In Claude Code, invoke it as `/shtodo`, for example:

```text
/shtodo show my global tasks
```

Agents that support automatic skill selection can also discover it when
you ask to use shtodo. Installing the skill does not ask the agent to record
every coding plan as tasks.

[shtodo skill]: ../skills/shtodo/SKILL.md
[shtodo binary]: ../README.md#installation
[skills CLI]: https://github.com/vercel-labs/skills
[Codex skill documentation]: https://learn.chatgpt.com/docs/build-skills#where-codex-loads-local-skills
[Claude Code skill documentation]: https://code.claude.com/docs/en/skills
