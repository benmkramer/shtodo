# Performance of priority styling, sort, and the filter editor in the TUI

October 21, 2026. This compares the PR 1 merge at `527127c` (the control) with
PR 2 of the [tags and priority plan](../plans/2026-10-21-tags-and-priority.md):
styled priority and tag spans, `s` priority order, and the `f` filter editor
with its tag hint line. Both are `0.1.0-beta.4` release builds with the same
Cargo manifest, lockfile, Rust 1.98.0, and optimization profile. Exact binary
hashes distinguish them.

Result: every TUI gate passes. The Filter-mode keystroke missed the plan's
original gate, within 5% of the `/` search keystroke, after the viewport
mitigation, and the work stopped for the owner. On October 21, 2026 the owner
restated that gate as within 5% of `/` or within 0.5 ms absolute (plan
question 13), matching the absolute bounds the shell gates carry. The final
run passes the restated gate at +0.28 ms.

## Gates

The plan's TUI gates, at 10,000 records, final run:

| Gate | Fixture | Reference | Current | Change | Limit | Result |
| --- | --- | ---: | ---: | ---: | --- | --- |
| Completion-to-frame, Manual order | Mixed | Control 15.71 ms | 15.37 ms | -2.2% (-0.34 ms) | 5% | Pass |
| Completion-to-frame, Priority order | Mixed | Control 15.71 ms | 15.38 ms | -2.1% (-0.33 ms) | Report only | |
| Filter keystroke-to-frame | Tagged | Current `/` keystroke 0.574 ms | 0.853 ms | +48.7% (+0.28 ms) | Restated: 5% or 0.5 ms of `/` (original: 5% of `/`) | Pass restated (missed original) |

Information only, final run at 10,000 records:

| Case | Fixture | Control | Current | Change |
| --- | --- | ---: | ---: | ---: |
| Completion-to-frame, Manual order | Tagged | 13.70 ms | 13.21 ms | -3.6% |
| Completion-to-frame, Priority order | Tagged | 13.70 ms | 13.45 ms | -1.8% |
| `/` keystroke-to-frame | Mixed | 1.49 ms | 1.24 ms | -16.9% |
| `/` keystroke-to-frame | Tagged | 0.71 ms | 0.57 ms | -19.3% |
| Help-to-frame | Mixed | 3.36 ms | 3.29 ms | -1.8% |

The keystroke cases open the editor untimed (`/+` or `f+`), then time one
`p`, ending at the frame that shows `/+p` or `f:+p`. Both preview the same
rows: the half of the tagged fixture that carries `+proj0` to `+proj4`.

## Runs and mitigations

Three full runs, each 50 interleaved samples per case after 5 warmups, seed
`20261005`. The JSON for every run is kept:

| Run | Binary SHA-256 | Change | Filter vs `/` at 10,000 | Manual completion vs control |
| --- | --- | --- | ---: | ---: |
| [1](./2026-10-21-tags-and-priority-tui-comparison-run1.json) | `d8fde118…` | Stage 3c as first written (`daae639`) | 2.36 vs 1.60 ms, +47.6% | +2.1% |
| [2](./2026-10-21-tags-and-priority-tui-comparison-run2.json) | `cebead30…` | Hint tag counts cached per completed tokens (`af89c20`) | 1.93 vs 1.67 ms, +15.1% | +2.6% |
| [3, final](./2026-10-21-tags-and-priority-tui-comparison.json) | `24c70e1a…` | Styled items built only for the visible window (`5cae464`) | 0.853 vs 0.574 ms, +48.7% | -2.2% |

- Run 1: every Filter keystroke recounted tags over all 10,000 rows, although
  the counts depend only on the completed tokens. Caching them in the App-side
  filter session, recomputed when the completed tokens change or a refresh
  installs new tasks, removed 0.43 ms.
- Run 2 to run 3: the plan's viewport mitigation. Building styled items only
  for the rows the list shows cut the shared rendering cost of both keystroke
  cases by about 1.1 ms. That fixes the regression in `/` search against the
  control (run 1: +125% on the tagged fixture; now -19%). It does not shrink
  the Filter-only cost, which stays at 0.25 to 0.28 ms, so the ratio gets worse as
  the shared cost falls.
- The remaining 0.28 ms has not been attributed. Filter-only work per
  keystroke is: the buffer split and token parse, the preview predicate (a tag
  scan per row in place of `/`'s lowercase copy and substring search), the
  prefix filter over the cached counts, and the hint row. No further change was
  tried, per the STOP rule, before the owner restated the gate.

## Idle refresh

The [paired idle run](./2026-10-21-tags-and-priority-tui-idle-comparison.json)
was taken from the same control and the run 3 binary
(`24c70e1a…`). It runs control and current together on six independent
scopes, one TUI per binary for each of 0, 1,000, and 10,000 mixed records (Unicode text, one-third tombstones), as in
the October 5 idle report.
After a 5-second warmup there are three 60-second idle windows. Every window
has zero terminal output from every process. CPU is measured with `wait4` over
each process's lifetime (about 185 s), including startup and warmup.

| Records | Snapshot bytes | Control CPU (% of one core) | Current CPU (% of one core) |
| ---: | ---: | ---: | ---: |
| 0 | 132 | 0.0258% | 0.0229% |
| 1,000 | 166,983 | 0.0346% | 0.0366% |
| 10,000 | 1,691,822 | 0.1261% | 0.1219% |

The differences are within run-to-run noise in both directions. Idle refresh
compares canonical bytes before parsing, and nothing in this change runs on
that path. The ignored release test
`session::measurements::idle_refresh_measurements` (`cargo test --release
--locked --lib idle_refresh_measurements -- --ignored`) also passes: over three
60-second samples at 0, 1,000, and 10,000 records with one and two sessions,
unchanged bytes cause zero parses. With filter and sort active, the session
unit test
`refresh_should_keep_filter_editor_buffer_cursor_filter_and_sort_and_reconcile_tags`
and the sorted refresh test cover no parse or redraw on equal bytes.

## Method and provenance

- Control: `target/control-527127c/target/release/shtodo`, built from
  `git archive 527127c`, SHA-256
  `a1911bfed685201069c3d197c1bebac29fa21745d08b4c20e592d081c05a6c74`. The
  JSON's `baseline_commit` field is the harness default (`1f2421c`) and does
  not name this control. The hash does.
- Current: this branch's release build at each run's commit, with the
  benchmark harness changes not yet committed (`current_git_status`).
- Command: `python3 scripts/benchmark_concurrency_comparison.py --baseline
  target/control-527127c/target/release/shtodo --mode tui --kinds mixed tagged
  --sizes 100 1000 10000 --output <file>`.
- The harness adds a tagged TUI fixture (every other task has a priority and
  two tags), a `current_priority` case that presses `s` before timing, and a
  Filter keystroke case. The priority and Filter cases need this branch's
  binary, so they run for it only.
- Machine: Apple M4 Max (Mac16,5), 48 GB, macOS 27.0.1 arm64, on AC power,
  Python 3.14.6. Warm caches, local storage.
- Idle command: `python3 scripts/benchmark_concurrency_comparison.py
  --baseline target/control-527127c/target/release/shtodo --mode idle
  --output docs/benchmarks/2026-10-21-tags-and-priority-tui-idle-comparison.json`,
  with the default 5-second warmup, three 60-second samples, and mixed
  fixtures.
