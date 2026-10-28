# Performance of one-level subtasks in the TUI

October 28, 2026. This compares the PR 1 head `ac8f1ed` (the control: stored
subtasks, shell commands, flat TUI rows) with PR 2 of the
[subtasks plan](../plans/2026-10-22-subtasks.md) at `5efdd05`: indented rows,
progress, context rows, Trash grouping, the Help layout, and `A`, `>`, and
`<`. The plan names the PR 1 merge as the control; PR 1 is not merged yet, so
its head stands in. Both are `0.1.0-beta.4` release builds with the same
optimization profile and no new dependency.

## Result: one gate fails (STOP)

Completion-to-frame in Manual order on `nested` fails its bound in both runs:
+0.80 ms (+5.1%) and +0.89 ms (+5.7%), against 5% or 0.5 ms. Every other gate
passes. The cause is the size of the snapshot that every save writes, not
the TUI work (see [Cause](#cause)), so the mitigation the plan lists, per-group
counts cached on `App`, cannot reach it. Per the plan's STOP conditions this
is reported to the owner; no bound was changed.

## Gates

Medians in milliseconds at 10,000 records. A gate passes within its relative
bound or its absolute bound.

| Case | Reference | Run 1 ref | Run 1 current | Run 1 change | Run 2 ref | Run 2 current | Run 2 change | Bound | Result |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| Completion-to-frame, Manual, mixed | Control | 16.02 | 15.76 | -1.6% (-0.26) | 15.46 | 15.45 | -0.1% (-0.01) | 5% or 0.5 ms | Pass |
| Completion-to-frame, Manual, nested | Same build, `nested-flat` | 15.54 | 16.34 | +5.1% (+0.80) | 15.53 | 16.42 | +5.7% (+0.89) | 5% or 0.5 ms | **Fail** |
| Completion-to-frame, Priority, nested | Same build, Priority on `nested-flat` | 16.04 | 16.39 | +2.2% (+0.35) | 15.61 | 16.34 | +4.7% (+0.73) | 5% or 0.5 ms | Pass |
| `/` keystroke-to-frame, nested, context rows | Same build, `nested-flat` | 1.34 | 1.30 | -3.1% (-0.04) | 1.35 | 1.30 | -3.3% (-0.04) | 5% or 0.5 ms | Pass |
| Idle CPU, mixed (% of one core) | Control on mixed | 0.1237 | 0.1257 | +1.6% (+0.0020 pp) | 0.1336 | 0.1343 | +0.5% (+0.0007 pp) | 10% or 0.05 pp | Pass |
| Idle CPU, nested (% of one core) | Control on mixed | 0.1237 | 0.1334 | +7.8% (+0.0097 pp) | 0.1336 | 0.1452 | +8.7% (+0.0116 pp) | 10% or 0.05 pp | Pass |
| Idle output on unchanged bytes | n/a | 0 bytes in every sample, every scope | | | 0 bytes | | | Exactly zero | Pass |
| Release binary, `aarch64-apple-darwin` | Control, 1,366,672 bytes | | 1,366,704 | +32 bytes | | | | 2% or 32 KiB | Pass |

At 1,000 records every gate passes in both runs; the largest change is the
Priority nested toggle at +0.35 ms (+3.0%). Zero parses and zero redraws on
unchanged bytes are covered exactly by the session unit tests
(`unchanged_idle_refresh_should_not_parse_or_redraw_and_input_should_not_starve_it`
and the subtask refresh test), which pass.

## Cause

The `nested` snapshot is 1,890,274 bytes against 1,691,822 for `nested-flat`
(+198,452, +11.7%): each of the 8,000 subtasks carries a
`"parent_id": N` line. Every completion rewrites and syncs the whole
snapshot. Measured in-process at `5efdd05` with `Store::mutate` toggling task
6, 200 alternating samples each, medians:

| Fixture | Bytes | `mutate` median |
| --- | ---: | ---: |
| `nested` | 1,890,274 | 17.01 ms |
| `nested-flat` | 1,691,822 | 16.11 ms |
| `nested-flat` with subtask texts padded to the same size, still schema 1 | 1,890,274 | 17.06 ms |

A flat list padded to the same byte count costs the same as the nested one,
so the +0.9 ms is snapshot size, not tree work. The TUI's own work differs by
about 0.1 ms in the same profile: one displayed-row pass is 0.087 ms on
`nested` against 0.048 ms flat, `reconcile` (with the list clone) 0.32
against 0.27 ms, and a full render 0.20 against 0.19 ms. The PR 1 gate on
`add --parent` (10% or 1.0 ms) already paid the same size cost, at +0.46 to
+0.75 ms; the TUI gate's 0.5 ms bound is tighter than that cost.

## Information only

Medians / p95 in milliseconds, current binary unless noted.

| Records | Case | Run 1 | Run 2 |
| ---: | --- | ---: | ---: |
| 10,000 | `J` on a top-level group, nested | 16.56 / 18.17 | 16.53 / 17.48 |
| 10,000 | `>` on a subtask after an untimed `<`, nested | 16.01 / 17.13 | 15.98 / 17.72 |
| 10,000 | Help open, mixed, control / current | 3.42 / 3.50 | 3.58 / 3.45 |
| 10,000 | Search keystroke, mixed, control / current | 1.26 / 1.27 | 1.27 / 1.27 |
| 10,000 | Completion-to-frame, Priority, mixed | 15.81 / 17.15 | 15.52 / 16.29 |

## Method and provenance

Apple M4 Max, 48 GB RAM, macOS 27.0.1 arm64, local APFS storage, on AC power.
Both binaries were built before measuring from `git archive` extractions and
copied to a scratch directory. No other build, test, or benchmark ran during
the measurements.

| Binary | Source | SHA-256 |
| --- | --- | --- |
| Control | `ac8f1ed` (PR 1 head) | `9207cd393a907cb37ccd510c30a6d3a191a1e8f45c0c095ca590a0c872189a4e` |
| Current | `5efdd05` on `feat/subtasks-tui` | `deaeb524978a46d8f18df27a3149e993c0c25e1519e616ba43086adb75b5933e` |

The raw reports record `baseline_commit` as the local `origin/main` ref,
which is not the control here; the hashes above identify the binaries.

The [comparison harness](../../scripts/benchmark_concurrency_comparison.py)
at `a57cd15` ran `--mode tui --kinds mixed nested --sizes 1000 10000`, five
warmups and 50 measured rounds, then `--mode idle --kinds mixed nested`
(three 60-second samples after a five-second warmup). Run 1 used seed
20261005 and run 2 seed 20261028; runs started at 18:12 and 18:16 UTC. All
cases of a fixture size run in one TUI each and are interleaved in a shuffled
order every round. Each nested case pairs with `nested-flat` on the same
build. Timing ends at the acknowledged frame: the toggled marker on task 6,
the moved group, the indented row, or, for the nested search, the new match
count (the view row pads with spaces, so the query text alone cannot
acknowledge it). After every completion, move, and nest the saved list is
checked. The nested search types `/7`, then times a space: the matches are
the IDs ending in 7, all subtasks, whose parents become context rows.

Raw samples: [TUI run 1](./2026-10-28-subtasks-tui-comparison.json),
[TUI run 2](./2026-10-28-subtasks-tui-comparison-run2.json),
[idle run 1](./2026-10-28-subtasks-idle-comparison.json), and
[idle run 2](./2026-10-28-subtasks-idle-comparison-run2.json).
