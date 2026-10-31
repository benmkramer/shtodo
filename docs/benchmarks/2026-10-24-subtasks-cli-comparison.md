# Performance of one-level subtasks in shell commands

October 24, 2026. This compares `74747a0` (the control, the due-dates release
on `main`) with PR 1 of the [subtasks plan](../plans/2026-10-22-subtasks.md):
the stored parent link, schema 2 only while a list has a subtask, group-aware
delete and restore, `add --parent`, `parent`, `list --roots` and
`--parent`, tree sort, and the JSON `parent_id` field. Both are
`0.1.0-beta.4` release builds with the same optimization profile and no new
dependency. Exact binary hashes distinguish them.

## Gates

The plan's PR 1 gates, at 10,000 records. Each passes if the change is within
the relative bound or the absolute bound. Two full runs with different seeds
are shown; times are medians in milliseconds. The control cannot read a list
with subtasks, so the nested cases compare the `nested` fixture against its
`nested-flat` twin on the current build.

| Case | Run 1 reference | Run 1 current | Run 1 change | Run 2 reference | Run 2 current | Run 2 change | Bound |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `list`, plain | 12.32 | 12.51 | +1.5% (+0.19) | 12.30 | 12.37 | +0.6% (+0.07) | 5% or 0.5 ms |
| `list`, mixed | 10.86 | 11.14 | +2.6% (+0.28) | 11.01 | 11.07 | +0.5% (+0.06) | 5% or 0.5 ms |
| `list`, tagged | 12.76 | 12.86 | +0.8% (+0.11) | 12.67 | 12.76 | +0.7% (+0.09) | 5% or 0.5 ms |
| `list`, dated | 12.83 | 13.02 | +1.5% (+0.19) | 12.87 | 12.98 | +0.8% (+0.11) | 5% or 0.5 ms |
| `list --json`, plain | 7.03 | 7.38 | +4.9% (+0.34) | 6.92 | 7.24 | +4.6% (+0.32) | 10% or 1.5 ms |
| `list --json`, mixed | 7.42 | 7.65 | +3.0% (+0.22) | 7.39 | 7.71 | +4.3% (+0.32) | 10% or 1.5 ms |
| `list --json`, tagged | 7.76 | 8.28 | +6.7% (+0.52) | 7.96 | 8.12 | +2.1% (+0.16) | 10% or 1.5 ms |
| `list --json`, dated | 8.45 | 8.80 | +4.1% (+0.35) | 8.62 | 8.78 | +1.9% (+0.16) | 10% or 1.5 ms |
| `list`, nested, against the same build's `nested-flat` | 11.06 | 11.33 | +2.5% (+0.27) | 11.11 | 11.38 | +2.5% (+0.27) | 5% or 0.5 ms |
| `list --json`, nested, against `nested-flat` | 7.68 | 7.91 | +3.0% (+0.23) | 7.65 | 8.01 | +4.8% (+0.36) | 10% or 1.5 ms |
| `list --sort priority`, nested, against `nested-flat` | 11.12 | 11.35 | +2.1% (+0.24) | 11.19 | 11.37 | +1.5% (+0.17) | 10% or 1.0 ms |
| `list --sort due`, nested, against `nested-flat` | 11.17 | 11.47 | +2.7% (+0.30) | 11.03 | 11.41 | +3.5% (+0.38) | 10% or 1.0 ms |
| `add "plain text"`, mixed | 18.71 | 19.15 | +2.3% (+0.43) | 19.24 | 19.53 | +1.5% (+0.29) | 5% or 0.5 ms |
| `add --parent 6 "plain text"`, nested, against `add` on `nested-flat` | 18.53 | 19.27 | +4.0% (+0.75) | 19.36 | 19.82 | +2.4% (+0.46) | 10% or 1.0 ms |

Every gate passes in both runs, each within its relative bound. The largest
change is `add --parent` in run 1 (+4.0%, +0.75 ms); it was +2.4% in run 2.

The `list --json` cost is output size. Every task object now carries the
always-present `parent_id` field, 17 bytes as `,"parent_id":null`. At 10,000
records the output grows from 1,128,968 to 1,298,968 bytes (+15.1%) on the
plain fixture and from 1,018,496 to 1,131,818 bytes (+11.1%) on the mixed
fixture, which has 6,666 live tasks. That is +0.16 to +0.52 ms, inside the
plan's estimate of 0.4 to 1.0 ms.

The nested cases pay for a larger snapshot, not for tree work. The nested
fixture's `"parent_id": N` lines make it 1,890,274 bytes against 1,691,822
for `nested-flat` (+11.7%), and every command parses the whole snapshot. Text
`list` streams with one ID of state and prints 10,332 more bytes of
indentation. `--sort priority` and `--sort due` add the group key to each row's
existing key; both are within 0.4 ms of the flat sort. `add --parent` inserts
the new record after the second group, so it shifts almost the whole
`Vec`, the worst case for position, and then writes the larger snapshot.

Text `list` on the four control fixtures does no new work: it gained one
`Option<TaskId>` of state and a structure check. The +0.06 to +0.28 ms
differences there are within the run-to-run spread of the control itself
(for example, 10.86 against 11.01 ms on mixed).

## Binary size

| Target | Control `74747a0` | Current `cebb8e0` | Change | Bound |
| --- | ---: | ---: | ---: | --- |
| `aarch64-apple-darwin` (gated) | 1,350,144 | 1,366,672 | +16,528 bytes (16.1 KiB, +1.22%) | 2% or 32 KiB |
| `x86_64-unknown-linux-musl` (reported) | 1,568,024 | 1,584,408 | +16,384 bytes (16.0 KiB, +1.04%) | none |

The macOS pair was built on the host (rustc 1.98.0) from `git archive`
extractions of both commits, sizes from `stat -f %z`. The musl pair was built
from the same two trees in Docker, `rust:1.98-alpine` pinned by digest
`sha256:7cc1c22d77d9432f7fe012a70e6d3e555af54c2a6832700ed7d553f1769ae89f`
(rustc 1.98.1), `--platform linux/amd64`, with `CARGO_TARGET_DIR=target-musl`,
sizes from `stat -c %s` inside the container. musl SHA-256: control
`c560df3c39ad90467e7d6db906a5409ad730abcfc2e7b0bd84caad2d38bb781b`, current
`670dbff0d019ba9b608937b3315545a5abee4d5e1f7d4a1459e34ecf15067eae`.

## Subtask commands, for information

The control has no `--roots`, `--parent`, or `parent`, so these run on the
current binary only, on the nested fixture. Cells are median / p95 in
milliseconds. The last column is the same build's unfiltered `list` on the
nested fixture, from run 1.

| Records | Command | Run 1 | Run 2 | Unfiltered `list` median |
| ---: | --- | ---: | ---: | ---: |
| 1,000 | `list --roots` | 3.32 / 4.18 | 3.27 / 4.25 | 3.66 |
| 10,000 | `list --roots` | 7.66 / 9.14 | 7.76 / 8.73 | 11.33 |
| 1,000 | `list --parent 6` | 3.20 / 4.21 | 3.23 / 4.32 | 3.66 |
| 10,000 | `list --parent 6` | 6.60 / 7.37 | 6.62 / 7.65 | 11.33 |
| 1,000 | `list --json --sort due` | 3.45 / 4.57 | 3.36 / 4.37 | 3.66 |
| 10,000 | `list --json --sort due` | 8.23 / 10.17 | 8.12 / 9.48 | 11.33 |
| 1,000 | `parent 8 none` | 13.09 / 14.63 | 12.62 / 14.84 | 3.66 |
| 10,000 | `parent 8 none` | 19.37 / 21.83 | 20.15 / 23.74 | 11.33 |

The structure filters print fewer rows, so they are faster than unfiltered
listing. `parent 8 none` is a locked save, priced like `add`.

## Method and provenance

Apple M4 Max, 48 GB RAM, macOS 27.0.1 arm64, local APFS storage. Both runs
were on battery power, as the raw reports record; earlier reports ran on AC
power, so compare absolute times across reports with care. Both sides of
every pair ran in the same interleaved rounds.
Both binaries were built before measuring from `git archive` extractions and
copied to a scratch directory so later builds could not change them. No other
build, test, or benchmark ran during the measurements. Other activity on the
user's computer is not controlled. These are warm-cache host measurements,
not a cross-platform claim or a throughput ceiling. No user task data was
accessed.

| Binary | Source | SHA-256 |
| --- | --- | --- |
| Control | `74747a0`, extracted with `git archive` | `0d6ae4261ab2546034c435240a766ff588f610a260e298aa5e87c8102cffe081` |
| Current | `cebb8e0` on `feat/subtasks`, extracted with `git archive` | `883249c18827e210d3a08bbcd19a23dbb317f91c9917dee88e20ff878d187a05` |

The raw reports record `baseline_commit` as `74747a0`, the local
`origin/main` ref at run time, which matches the control. Their
`current_git_status` shows the uncommitted harness and docs changes of this
stage; the current binary was built from the clean `cebb8e0` tree.

The [comparison harness](../../scripts/benchmark_concurrency_comparison.py)
ran with `--mode cli --sizes 0 100 1000 10000 --kinds plain mixed tagged
dated nested nested-flat`, the operations `list list_json add add_parent
list_sort list_sort_due list_roots list_parent list_json_sort_due
parent_none`, five warmups, and 50 measured samples per case. Run 1 used the
default seed 20261005 and run 2 used seed 20261024 for the randomized order of
cases each round. Runs started at 14:33 and 14:34 UTC. p95 is the
nearest-rank sample percentile. Fixture reset and correctness checks are
outside the timers. A preflight checks every case's output against a Python
model of the contract, including indentation, tree sort, and the JSON
`parent_id` values; the control is checked without that field. Every timed
read also checks that the snapshot bytes are unchanged, and every timed
mutation checks the saved list against the model.

Raw samples: [run 1](./2026-10-24-subtasks-cli-comparison.json) and
[run 2](./2026-10-24-subtasks-cli-comparison-run2.json).

Fixtures (sizes count all stored records):

- Plain, mixed, tagged, and dated: as in the
  [due dates shell comparison](./2026-10-21-due-dates-cli-comparison.md).
- Nested: the mixed fixture's texts and completion states, in groups of one
  top-level task and four subtasks, written as schema 2. It has the same
  tombstone count as mixed (3,334 at 10,000 records): every fourth group is
  deleted whole with one shared sequence, and single last subtasks of the
  last live groups were deleted earlier with lower sequences. The second
  group (task 6 and subtasks 7 to 10) stays live and whole for the
  `--parent 6`, `add --parent 6`, and `parent 8 none` cases.
- Nested-flat: the same records with every `parent_id` removed, written as
  schema 1. Schema 1 requires unique deletion sequences, so its tombstones
  are renumbered in storage order; the count and positions are unchanged.
