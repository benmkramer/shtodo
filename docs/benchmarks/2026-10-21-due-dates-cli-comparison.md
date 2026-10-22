# Performance of derived due dates in shell commands

October 21, 2026. This compares `a33b687` (the control, the tags and priority
release on `main`) with PR 1 of the [due dates plan](../plans/2026-10-21-due-dates.md):
derived due dates, `list --due`, `list --sort due`, the JSON `due` field, and
`due:today` and `due:tomorrow` rewriting on add and edit. Both are
`0.1.0-beta.4` release builds with the same optimization profile; the current
build adds the `jiff` dependency. Exact binary hashes distinguish them.

## Gates

The plan's PR 1 gates, at 10,000 records. Each passes if the change is within
the relative bound or the absolute bound. Two full runs with different seeds
are shown; times are medians in milliseconds.

| Case | Run 1 reference | Run 1 current | Run 1 change | Run 2 reference | Run 2 current | Run 2 change | Bound |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `list`, plain | 11.42 | 11.47 | +0.5% (+0.05) | 11.47 | 11.37 | -0.9% (-0.10) | 5% or 0.5 ms |
| `list`, mixed | 10.27 | 10.27 | -0.0% (-0.00) | 10.22 | 10.21 | -0.1% (-0.01) | 5% or 0.5 ms |
| `list`, dated | 11.92 | 11.93 | +0.0% (+0.00) | 12.00 | 11.88 | -1.0% (-0.12) | 5% or 0.5 ms |
| `list --json`, plain | 6.17 | 6.46 | +4.6% (+0.28) | 6.16 | 6.50 | +5.7% (+0.35) | 10% or 1.5 ms |
| `list --json`, mixed | 6.82 | 7.00 | +2.7% (+0.19) | 6.76 | 7.00 | +3.5% (+0.24) | 10% or 1.5 ms |
| `list --json`, tagged | 6.98 | 7.26 | +3.9% (+0.28) | 7.00 | 7.35 | +5.0% (+0.35) | 10% or 1.5 ms |
| `list --json`, dated | 7.27 | 7.93 | +9.0% (+0.65) | 7.22 | 7.98 | +10.6% (+0.76) | 10% or 1.5 ms |
| `add "plain text"`, mixed | 18.22 | 18.05 | -0.9% (-0.17) | 18.20 | 18.20 | -0.0% (-0.00) | 5% or 0.5 ms |
| `add "x due:today"`, mixed, against the same build's `add` | 18.05 | 18.32 | +1.5% (+0.27) | 18.20 | 17.99 | -1.1% (-0.20) | 10% or 1.5 ms |
| `list --due today`, dated, against the same build's `--due DATE` | 5.69 | 5.64 | -0.9% (-0.05) | 5.66 | 5.67 | +0.1% (+0.00) | 10% or 1.5 ms |

Every gate passes in both runs. The dated `list --json` case is within 10% in
run 1 and passes on its absolute bound in run 2 (+0.76 ms against 1.5 ms).
Text `list` does no due work and is unchanged within noise. The two
first-clock-read gates show no measurable cost: reading the zone file and the
clock is tens of microseconds, below the run-to-run spread of `add`.

The `list --json` cost is output size. Every task object now carries the
always-present `due` field. At 10,000 records the output grows from 1,018,968
to 1,128,968 bytes (+10.8%) on the plain fixture, from 1,178,968 to 1,288,968
bytes (+9.3%) on the tagged fixture, and from 1,253,968 to 1,403,968 bytes
(+12.0%) on the dated fixture. The dated fixture also pays one token scan per
dated text.

The binary size gate ran at the start of Stage 1 and is recorded in the plan's
validation record.

Note added after review: these timings measure `7283b8c`. Three later review
fixes were not re-timed. Two change only `clock::today`, which now converts
`SystemTime` with `Timestamp::try_from` instead of calling `Timestamp::now()`
and clamps an out-of-range clock instead of panicking. The third makes the TUI
read the date on its own deadline. Dropping `Timestamp::now()` and its panic
path also made the binary smaller. At `52e817f` the macOS release binary is
1,350,144 bytes, +99,504 bytes (97.2 KiB, +8.0%) over the control's
1,250,640, down from +215,376 bytes at `7283b8c`. The
`x86_64-unknown-linux-musl` binary is 1,563,928 bytes, +94,304 bytes
(92.1 KiB, +6.4%) over 1,469,624. Clean-archive builds of `7283b8c`,
`f321ba8`, and `d13dcf8` on the same host gave 1,466,016, 1,350,144, and
1,350,144 bytes, so the drop came with `f321ba8`.

## Due filters and sort, for information

The control has no `--due` or `--sort due`, so these run on the current binary
only, on the dated fixture. Cells are median / p95 in milliseconds. The last
column is the same binary's unfiltered `list`, or `list --json` for the JSON
case, from run 1.

| Records | Command | Run 1 | Run 2 | Unfiltered median |
| ---: | --- | ---: | ---: | ---: |
| 1,000 | `list --open --due overdue` | 2.97 / 3.22 | 2.98 / 3.13 | 3.48 |
| 10,000 | `list --open --due overdue` | 6.62 / 7.06 | 6.62 / 7.10 | 11.93 |
| 1,000 | `list --due 2026-10-01..2026-10-31` | 3.00 / 3.21 | 2.98 / 3.35 | 3.48 |
| 10,000 | `list --due 2026-10-01..2026-10-31` | 7.25 / 7.63 | 7.25 / 7.76 | 11.93 |
| 1,000 | `list --sort due` | 3.59 / 3.78 | 3.49 / 3.76 | 3.48 |
| 10,000 | `list --sort due` | 12.53 / 13.02 | 12.52 / 12.98 | 11.93 |
| 1,000 | `list --json --sort due` | 3.10 / 3.55 | 3.16 / 3.53 | 3.08 |
| 10,000 | `list --json --sort due` | 8.53 / 8.88 | 8.48 / 8.85 | 7.93 |

Filters print fewer rows, so they are faster than unfiltered text listing.
`--sort due` computes each row's due key once and stable-sorts the filtered
rows; at 10,000 records it adds about 0.6 ms over unfiltered listing.

## Method and provenance

Apple M4 Max, 48 GB RAM, macOS 27.0.1 arm64, AC power, local APFS storage.
Both binaries were built before measuring and copied to a scratch directory so
later builds could not change them. Other activity on the user's computer is
not controlled. These are warm-cache host measurements, not a cross-platform
claim or a throughput ceiling. No user task data was accessed.

| Binary | Source | SHA-256 |
| --- | --- | --- |
| Control | `a33b687`, extracted with `git archive` | `54c3fdd4d4864f21467d2828d27ce2b0825d6ab5e14a6b604459e565caf45670` |
| Current | `7283b8c` on `feat/due-dates`; uncommitted changes were docs and the harness only | `0c5a3412c1beab76eebdb1c796ee47a8bc09298b7207a70ede4616fcf2898990` |

The raw reports record `baseline_commit` as `1f2421c`. That field is the
local `origin/main` ref at run time, which was out of date; the control is
`a33b687`, identified by the hash above.

The [comparison harness](../../scripts/benchmark_concurrency_comparison.py)
ran with `--mode cli --sizes 0 100 1000 10000 --kinds plain mixed tagged dated`,
the list, add, and due operations, five warmups, and 50 measured samples per
case. Run 1 used the default seed 20261005 and run 2 used seed 20261021 for
the randomized order of cases each round. Each run took under a minute of wall
time, at 05:11 and 05:12 UTC on October 22 (October 21 local time). p95 is the
nearest-rank sample percentile. Fixture reset and correctness checks are
outside the timers. A preflight checks every case's output against a Python
model of the contract, including the JSON `due` values; the control is checked
without that field. Every timed read also checks that the snapshot bytes are
unchanged.

The harness pins `TZ=UTC`, so both binaries' "today" is the UTC date, and the
dated fixture is generated relative to that date.

Fixtures:

- Plain: the historical short open-task descriptions.
- Mixed: Unicode descriptions with 40 extra characters, alternating
  completion, and about one-third tombstones. Sizes count all stored records.
- Tagged: every other task is `(A) ` to `(D) ` followed by the plain text,
  `+projN`, and `@ctxN`; every fourth task is done.
- Dated: the tagged texts, where every tagged task also ends with a
  `due:YYYY-MM-DD` token from 30 days before to 30 days after the run date.
  Every third task is done, so about a sixth of the tasks are open and
  overdue.

The raw reports, [run 1](./2026-10-21-due-dates-cli-comparison.json) and
[run 2](./2026-10-21-due-dates-cli-comparison-run2.json), each retain 5,600
measured invocations across 112 cases, the randomized schedule, exact
commands, fixture byte sizes, binary hashes, and host metadata. Wall time
includes process launch and exit; stdout goes to the null device.
