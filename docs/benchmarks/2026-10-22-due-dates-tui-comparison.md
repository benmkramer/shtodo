# Performance of due-date styling, Due order, and due filter tokens in the TUI

October 22, 2026. This compares the PR 1 merge at `00343c6` (the control) with
PR 2 of the [due dates plan](../plans/2026-10-21-due-dates.md): styled due
tokens and status markers, `s` cycling Manual, Priority, and Due order, and
`due:WHEN` tokens in the `f` filter editor. Both are `0.1.0-beta.4` release
builds with the same Cargo manifest, lockfile, Rust 1.98.0, and optimization
profile. Exact binary hashes distinguish them.

Result: every PR 2 gate passes in both runs, each on its relative bound.

## Gates

The plan's PR 2 gates at 10,000 records. Each passes if it is within the
relative bound or within the absolute bound. Medians of 50 samples:

| Gate | Fixture | Reference | Run 1 | Run 2 | Bound | Result |
| --- | --- | --- | ---: | ---: | --- | --- |
| Completion-to-frame, Manual order | Mixed | Control 15.60 / 15.61 ms | 15.48 ms, -0.7% (-0.12 ms) | 15.38 ms, -1.4% (-0.22 ms) | 5% or 0.5 ms | Pass |
| Completion-to-frame, Manual order | Dated | Control 15.02 / 14.97 ms | 14.82 ms, -1.4% (-0.21 ms) | 14.65 ms, -2.1% (-0.32 ms) | 5% or 0.5 ms | Pass |
| Completion-to-frame, Due order | Dated | Priority order, same build: 14.98 / 15.13 ms | 15.67 ms, +4.6% (+0.69 ms) | 15.75 ms, +4.1% (+0.63 ms) | 5% or 0.5 ms | Pass on 5% |
| Filter keystroke-to-frame, final `y` of `due:today` | Dated | `/` keystroke, same build and fixture: 0.914 / 0.900 ms | 0.807 ms, -11.7% (-0.11 ms) | 0.811 ms, -9.9% (-0.09 ms) | 5% or 0.5 ms | Pass |
| Idle CPU at 10,000 records | Mixed | Control 0.1066% of one core | 0.1071%, +0.5% (+0.0005 points) | | 10% or 0.05 percentage points | Pass |
| Idle refresh, unchanged bytes and date | Mixed | n/a | Zero output, parses, and redraws | | Exact | Pass |

The Due-order case costs about 0.65 ms more than Priority order at 10,000
records in both runs, inside the 5% bound and over the 0.5 ms one. It is the
order rebuild that every reconcile performs while the list is sorted: Due
order collects a `(bool, Option<Date>, priority key)` per live task with
`sort_by_cached_key`, which parses each task's text for a due token (half of
the dated fixture has one), while Priority order compares a key that reads two
bytes. The planning micro-measurement estimated 0.5 ms for that sort; this is
the release profile (`opt-level = "s"`) plus the key allocation. At 1,000
records the difference is +1.0% and +0.4% (under 0.1 ms).

The two keystroke cases open the editor untimed (`/due:2026-10-2` or
`fdue:toda`), then time one character that ends on the same rows: the tasks
due on the run date. The search finds the literal `due:2026-10-22`; the filter
completes `due:today`. The Filter keystroke is faster because a due term
compares one parsed date per row, where search lowercases each row's text and
scans it for the substring.

Information only, at 10,000 records:

| Case | Fixture | Control (run 1 / run 2) | Current (run 1 / run 2) | Change |
| --- | --- | ---: | ---: | ---: |
| Completion-to-frame, Priority order | Mixed | 15.60 / 15.61 ms | 15.60 / 15.56 ms | +0.0% / -0.3% |
| `/` keystroke-to-frame | Mixed | 1.290 / 1.284 ms | 1.285 / 1.281 ms | -0.3% / -0.3% |
| `/` keystroke-to-frame | Dated | 0.912 / 0.911 ms | 0.914 / 0.900 ms | +0.2% / -1.2% |
| Help-to-frame | Mixed | 3.45 / 3.55 ms | 3.43 / 3.48 ms | -0.7% / -2.0% |

At 1,000 dated records the `/` keystroke is +13.9% and +14.4% (+0.07 ms)
against the control, which renders the same visible rows without due
styling; the cost is the per-row due parse and extra spans for the at most
one screen of styled rows.

The frame after a simulated day change, from the ignored release test
`session::measurements::day_change_frame_measurements` (`cargo test --release
--locked --lib day_change_frame_measurements -- --ignored --nocapture`), at
10,000 dated records: each sample calls `App::set_today` with a new date and
draws one 100x24 frame to ratatui's `TestBackend`, which is the single redraw
a session makes on a new day. It excludes the PTY and terminal output.

| Case | Median | p95 |
| --- | ---: | ---: |
| Manual order, no filter | 0.186 ms | 0.197 ms |
| Due order | 0.222 ms | 0.239 ms |
| Accepted `due:today` filter | 0.786 ms | 0.875 ms |

Due order is not rebuilt on a new day, so it costs about the same as Manual
order (an earlier sample of the same test measured 0.230 ms for Manual and
0.224 ms for Due). A relative due filter re-resolves its terms and reconciles
the selection against every row once.

## Runs

Two full runs, each 50 interleaved samples per case after 5 warmups:

| Run | Seed | JSON |
| --- | ---: | --- |
| 1 | `20261005` | [run 1](./2026-10-22-due-dates-tui-comparison-run1.json) |
| 2 | `20261022` | [run 2](./2026-10-22-due-dates-tui-comparison-run2.json) |

No mitigation was needed, so both runs use the same binaries. Every gate has
the same sign and result in both runs.

## Idle refresh

The [paired idle run](./2026-10-22-due-dates-tui-idle-comparison.json) runs
control and current together on six independent scopes, one TUI per binary
for each of 0, 1,000, and 10,000 mixed records (Unicode text, one-third
tombstones), as in the earlier idle reports. After a 5-second warmup there are
three 60-second idle windows. CPU is measured with `wait4` over each process's
lifetime, including startup and warmup.

| Records | Snapshot bytes | Idle output (three windows) | Control CPU (% of one core) | Current CPU (% of one core) |
| ---: | ---: | --- | ---: | ---: |
| 0 | 132 | 0, 0, 0 bytes from each | 0.0258% | 0.0258% |
| 1,000 | 166,983 | 0, 0, 0 bytes from each | 0.0340% | 0.0347% |
| 10,000 | 1,691,822 | 0, 0, 0 bytes from each | 0.1066% | 0.1071% |

Each process ran about 185 s. The differences are within run-to-run noise.

The current binary reads the local date at each one-second refresh deadline.
On the same date it changes nothing, so idle output stays zero. The ignored
release test `session::measurements::idle_refresh_measurements` (`cargo test
--release --locked --lib idle_refresh_measurements -- --ignored`) passes: over three
60-second samples at 0, 1,000, and 10,000 records with one and two sessions,
starting on the real date that `tick` reads, every sample has 59 to 60 reads
per session, zero parses, and zero redraw requests.
The session unit tests
`the_date_should_be_read_once_per_second_without_parsing_or_redrawing` and
`refresh_should_rebuild_due_order_and_a_new_day_should_only_restyle` cover
the date rule exactly: the same date and bytes parse nothing and request no
redraw; a new date requests one redraw, rebuilds no order, and then idles.

## Method and provenance

- Control: `target/control-00343c6/target/release/shtodo`, built from
  `git archive 00343c6`, SHA-256
  `8a9b8cdfe6f895ab916cc0f461e108413bbc88e8966c239af74cc040c10268c4`. The
  JSON's `baseline_commit` field (`origin/main`) is `00343c6` as well.
- Current: the release build of `3141433`, copied to
  `target/candidate-3141433/shtodo` before measuring so no later build could
  replace it, SHA-256
  `5b78327c0599c000d4950e70d31003947e9703cee0662b72cf673117a1a46847`.
  Later commits on the branch change only documentation and tests; the
  release build of the final commit has the same SHA-256.
- Command: `python3 scripts/benchmark_concurrency_comparison.py --baseline
  target/control-00343c6/target/release/shtodo --current
  target/candidate-3141433/shtodo --mode tui --kinds mixed dated --sizes 100
  1000 10000 --seed <seed> --output <file>`.
- The harness adds a dated TUI fixture (every other task has a priority, two
  tags, and a due date from 30 days before to 30 days after the run date;
  every third task is done), a `current_due` case that presses `s` twice and
  waits for `by due` before timing, and a pair of keystroke cases that end on
  the same rows. The sorted and Filter cases need this branch's binary, so
  they run for it only. The isolated environment pins `TZ=UTC`, and both runs
  finished before midnight UTC, so the binaries and the fixture agree on the
  run date (October 22).
- Machine: Apple M4 Max (Mac16,5), 48 GB, macOS 27.0.1 arm64, on AC power,
  Python 3.14.6. Warm caches, local storage.
- Idle command: `python3 scripts/benchmark_concurrency_comparison.py
  --baseline target/control-00343c6/target/release/shtodo --current
  target/candidate-3141433/shtodo --mode idle --output
  docs/benchmarks/2026-10-22-due-dates-tui-idle-comparison.json`, with the
  default 5-second warmup, three 60-second samples, and mixed fixtures.
