# Performance of derived priority and tags in shell listing

October 21, 2026. This compares the JSON-output baseline at `1f2421c` (the
control) with PR 1 of the [tags and priority plan](../plans/2026-10-21-tags-and-priority.md):
derived priority and tags, shell `list` filters and `--sort priority`, and the
JSON `priority` and `tags` fields. Both are `0.1.0-beta.4` release builds with
the same Cargo manifest, lockfile, Rust 1.98.0, and optimization profile.
Exact binary hashes distinguish them.

## Gates

The plan's shell gates, at 10,000 records against the control:

| Gate | Fixture | Control | Current | Change | Limit | Result |
| --- | --- | ---: | ---: | ---: | --- | --- |
| `list` | Plain | 11.55 ms | 11.43 ms | -1.0% (-0.12 ms) | 5% and 0.5 ms | Pass |
| `list --json` | Plain | 5.65 ms | 6.15 ms | +8.7% (+0.49 ms) | 10% and 1.5 ms | Pass |
| `list` | Mixed | 10.24 ms | 10.19 ms | -0.5% (-0.05 ms) | 5% and 0.5 ms | Pass |
| `list --json` | Mixed | 6.37 ms | 6.72 ms | +5.5% (+0.35 ms) | 10% and 1.5 ms | Pass |

The gates use the harness's existing plain and mixed fixtures. The new tagged
fixture, where every other task has a priority and two tags, was added for the
information-only cases. Unfiltered `list --json` on it is
6.10 to 7.03 ms (+15.2%, +0.93 ms): within the
1.5 ms bound but over 10%. Text `list` on the same fixture is
-0.6%.

The JSON cost comes from the new always-present fields. At 10,000 records the
`list --json` output grows from 758,968 to 1,018,968 bytes (+34.3%) on the
plain fixture and from 843,968 to 1,178,968 bytes (+39.7%) on the tagged
fixture. Tagged tasks also pay one whitespace split. A first full run, before
any tuning, measured plain `list --json` at +11.0%, over the gate. A byte
pre-scan was then added so texts without `+` or `@` skip the split; a
30-sample probe measured +7.8%, and the final run above +8.7%. Serializing
tags without collecting them did not measurably help in a second probe and
was reverted. This report replaces the uncommitted first run. Text `list`
does no token work, so its medians are unchanged.

## Method and provenance

Apple M4 Max, 48 GB RAM, macOS 27.0.1 arm64, AC power, local APFS storage.
Both binaries were built before measuring. Other activity on the user's
computer is not controlled. These are warm-cache host measurements, not a
cross-platform claim or a throughput ceiling. No user task data was accessed.

| Binary | Source | SHA-256 |
| --- | --- | --- |
| Control | `1f2421c073a615623b60de1bda840ee07367430c`, extracted with `git archive` | `3db9d018cc75892730de3688b9732c191caa2d36e780c62cda41e78dd94d412d` |
| Current | `c5791b2` on `feat/tags-and-priority`, clean tree | `82e8d0742d0eee3ec59c576224e363e6329b99879f095274f152b2f0ef467214` |

The [comparison harness](../../scripts/benchmark_concurrency_comparison.py)
ran with `--mode cli --sizes 0 100 1000 10000 --kinds plain mixed tagged`,
the list operations, five warmups, 50 measured samples per case, and the
default seed for the randomized order of cases each round. p95 is the
nearest-rank sample percentile. Fixture reset and correctness checks are
outside the timers. A preflight checks every case's output against a Python
model of the contract, including the JSON `priority` and `tags` values; the
control is checked without those two fields. Every timed read also checks
that the snapshot bytes are unchanged.

Fixtures:

- Plain: the historical short open-task descriptions.
- Mixed: Unicode descriptions with 40 extra characters, alternating
  completion, and about one-third tombstones. Sizes count all stored records.
- Tagged: every other task is `(A) ` to `(D) ` followed by the plain text,
  `+projN`, and `@ctxN`; every fourth task is done.

The [raw report](./2026-10-21-tags-and-priority-cli-comparison.json) retains
2,800 measured invocations across 56 cases, the randomized
schedule, exact commands, fixture byte sizes, binary hashes, and host
metadata. Wall time includes process launch and exit; stdout goes to the null
device.

## Unfiltered listing, median milliseconds

Plain fixture:

| Records | Command | Control | Current | Change | Change (ms) | Control p95 | Current p95 |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | `list` | 2.44 | 2.41 | -1.3% | -0.03 | 2.92 | 2.92 |
| 0 | `list --json` | 2.39 | 2.42 | +0.9% | +0.02 | 2.86 | 2.71 |
| 100 | `list` | 2.51 | 2.53 | +0.6% | +0.01 | 2.75 | 2.87 |
| 100 | `list --json` | 2.49 | 2.51 | +0.8% | +0.02 | 2.96 | 2.88 |
| 1,000 | `list` | 3.40 | 3.30 | -2.9% | -0.10 | 3.86 | 3.62 |
| 1,000 | `list --json` | 2.78 | 2.81 | +1.0% | +0.03 | 3.16 | 3.32 |
| 10,000 | `list` | 11.55 | 11.43 | -1.0% | -0.12 | 12.05 | 12.04 |
| 10,000 | `list --json` | 5.65 | 6.15 | +8.7% | +0.49 | 6.03 | 6.65 |

Mixed fixture:

| Records | Command | Control | Current | Change | Change (ms) | Control p95 | Current p95 |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | `list` | 2.45 | 2.43 | -0.8% | -0.02 | 2.75 | 3.04 |
| 0 | `list --json` | 2.43 | 2.42 | -0.3% | -0.01 | 2.78 | 2.60 |
| 100 | `list` | 2.53 | 2.51 | -0.6% | -0.01 | 2.94 | 2.88 |
| 100 | `list --json` | 2.47 | 2.48 | +0.5% | +0.01 | 2.91 | 2.83 |
| 1,000 | `list` | 3.23 | 3.22 | -0.2% | -0.01 | 3.66 | 3.62 |
| 1,000 | `list --json` | 2.84 | 2.94 | +3.5% | +0.10 | 3.19 | 3.29 |
| 10,000 | `list` | 10.24 | 10.19 | -0.5% | -0.05 | 10.72 | 10.86 |
| 10,000 | `list --json` | 6.37 | 6.72 | +5.5% | +0.35 | 6.89 | 7.09 |

Tagged fixture:

| Records | Command | Control | Current | Change | Change (ms) | Control p95 | Current p95 |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | `list` | 2.44 | 2.41 | -1.0% | -0.02 | 2.81 | 3.05 |
| 0 | `list --json` | 2.44 | 2.38 | -2.5% | -0.06 | 2.77 | 2.71 |
| 100 | `list` | 2.52 | 2.54 | +1.0% | +0.02 | 2.76 | 2.99 |
| 100 | `list --json` | 2.45 | 2.53 | +3.0% | +0.07 | 2.65 | 3.01 |
| 1,000 | `list` | 3.42 | 3.36 | -2.0% | -0.07 | 3.75 | 4.00 |
| 1,000 | `list --json` | 2.81 | 2.95 | +5.2% | +0.14 | 3.30 | 3.31 |
| 10,000 | `list` | 11.85 | 11.78 | -0.6% | -0.08 | 12.57 | 12.34 |
| 10,000 | `list --json` | 6.10 | 7.03 | +15.2% | +0.93 | 6.79 | 7.59 |

## Filtered and sorted listing, for information

The control has no filter flags, so these run on the current binary only, on
the tagged fixture. The last column is the same binary's unfiltered `list`, or
`list --json` for the JSON case, on that fixture.

| Records | Command | Median | p95 | Unfiltered median |
| ---: | --- | ---: | ---: | ---: |
| 1,000 | `list --tag +proj1` | 2.82 | 3.45 | 3.36 |
| 10,000 | `list --tag +proj1` | 6.16 | 6.75 | 11.78 |
| 1,000 | `list --priority A-C` | 2.95 | 3.33 | 3.36 |
| 10,000 | `list --priority A-C` | 7.58 | 7.92 | 11.78 |
| 1,000 | `list --search x` | 3.07 | 3.54 | 3.36 |
| 10,000 | `list --search x` | 8.71 | 9.81 | 11.78 |
| 10,000 | `list --sort priority` | 11.81 | 12.56 | 11.78 |
| 10,000 | `list --json --sort priority` | 7.03 | 7.48 | 7.03 |

Filters print fewer rows, so they can be faster than unfiltered text listing.
`--search` lowercases each text, as TUI search does. `--sort priority`
collects the filtered rows and stable-sorts them by a constant-time priority
check.
