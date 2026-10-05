# Performance of the concurrency change

October 5, 2026. This compares unchanged beta.3 at `707b0ec` with the
repaired concurrency implementation, including the integrated shell lifecycle,
trash, search/status tabs, and missing-lock repair. Both are `0.1.0-beta.3`
release builds with the same Cargo manifest, lockfile, Rust 1.98.0, and
optimization profile. Exact binary hashes distinguish them.

At 10,000 mixed records, the completion-to-frame median goes from
12.69 to 16.13 ms (+27.1%). Each new TUI write reads and validates
the latest snapshot under the lock, while the old TUI mutates its in-memory
list. That explains the expected direction of the cost; these timings do not
isolate individual storage steps or the missing-lock repair's contribution.

## Method and provenance

Apple M4 Max, 48 GB RAM, macOS 27.0.1 arm64, AC power, local APFS storage.
Both binaries were built before measuring. Timed benchmark batches ran
sequentially. Other activity on the user's computer is not controlled. These
are warm-cache host measurements, not a cross-platform claim or throughput
ceiling. No user task data was accessed.

| Binary | Source | SHA-256 |
| --- | --- | --- |
| Baseline | `707b0eca2d7d29d5ab3adeb04d5e1dc35c632bdc`, extracted with `git archive` | `c2332442eb8089bb2417577495f832865c9603e82ea93a8bbfbccb78ee67bfe7` |
| Current | Same commit plus concurrency changes and the missing-lock repair | `0210c031885d6990e1df4a119cd14f2b0851e9e37a4b161eca1bc2e492a47281` |

The [comparison harness](../../scripts/benchmark_concurrency_comparison.py)
uses independent temporary global scopes, five warmups, 50 measured samples
per case, and a seeded randomized order between baseline/current cases each
round. p95 is the nearest-rank sample percentile. Fixture reset, correctness
checks, and preparation are outside the timers. All correctness checks passed,
including exact list output, persisted mutation results, and byte/mtime
preservation for no-op commands.

There are two CLI fixture families:

- Plain: the original benchmark's short descriptions and all-open tasks.
- Mixed: Unicode descriptions with 40 extra characters, alternating completion
  states, and about one-third tombstones. Sizes count all stored records.

TUI fixtures have an existing stable lock file. Correctness regressions
separately cover absent-lock writes and sync recovery.

## Shell commands

The [raw paired CLI report](./2026-10-05-repaired-concurrency-cli-comparison.json)
retains 4,100 measured invocations across 82 cases, their randomized schedule,
exact commands, fixture byte sizes, binary hashes, and host metadata.
Wall time includes process launch/exit and the command's ordinary persistence;
stdout goes to the null device. No TUI is open in this batch.

Median milliseconds for the historical plain fixture:

| Records | Operation | Baseline | Current | Change |
| ---: | --- | ---: | ---: | ---: |
| 0 | List | 3.24 | 3.20 | -1.3% |
| 0 | Add | 11.47 | 11.50 | +0.3% |
| 100 | List | 3.37 | 3.34 | -1.0% |
| 100 | Add | 11.91 | 11.64 | -2.2% |
| 100 | Delete | 11.88 | 11.62 | -2.2% |
| 1,000 | List | 4.21 | 4.34 | +2.9% |
| 1,000 | Add | 12.24 | 12.37 | +1.1% |
| 1,000 | Delete | 12.31 | 12.28 | -0.2% |
| 10,000 | List | 13.28 | 13.53 | +1.9% |
| 10,000 | Add | 17.66 | 17.58 | -0.5% |
| 10,000 | Delete | 17.65 | 17.61 | -0.2% |

At 10,000 mixed records, the stored snapshot is 1,691,822 bytes. This batch also
times the newer lifecycle commands and no-write paths:

| Operation | Baseline median ms | Current median ms | Baseline p95 ms | Current p95 ms |
| --- | ---: | ---: | ---: | ---: |
| List | 11.85 | 11.99 | 12.67 | 12.89 |
| Add | 18.95 | 19.21 | 20.33 | 20.57 |
| Delete | 18.94 | 19.24 | 20.26 | 20.69 |
| Done | 18.97 | 19.44 | 20.48 | 20.81 |
| Reopen | 18.97 | 19.29 | 21.75 | 20.66 |
| Edit | 19.16 | 19.01 | 21.27 | 20.80 |
| Restore | 18.98 | 19.11 | 20.53 | 20.55 |
| Already-done done | 6.92 | 6.95 | 7.63 | 7.59 |
| Already-deleted delete | 7.01 | 7.07 | 7.85 | 7.73 |

All sizes are in the raw report. For the listed mixed workloads, median changes
range from -0.8% to +2.5% in this batch.
Both implementations already reload and save the whole snapshot for shell
commands. Small differences between host measurements do not establish a
universal speedup or slowdown.

## TUI actions

The [raw paired TUI report](./2026-10-05-repaired-concurrency-tui-comparison.json)
retains 600 measured actions. Two TUIs run on independent baseline/current
scopes at each size, using the mixed fixtures and 100-column by 24-row PTYs.
Each input ends when the reconstructed screen acknowledges the new completion
marker or Help footer. Timing includes rendering, PTY transport, and Python
screen reconstruction. TUI startup is outside the timer. Each completion is
checked in the persisted snapshot before the next action.

| Records | Action | Baseline median ms | Current median ms | Baseline p95 ms | Current p95 ms |
| ---: | --- | ---: | ---: | ---: | ---: |
| 100 | Toggle completion to frame | 8.80 | 8.75 | 9.97 | 9.85 |
| 1,000 | Toggle completion to frame | 8.60 | 9.31 | 9.31 | 9.85 |
| 10,000 | Toggle completion to frame | 12.69 | 16.13 | 13.90 | 17.30 |
| 100 | Help to frame | 4.47 | 4.42 | 5.35 | 5.35 |
| 1,000 | Help to frame | 4.29 | 4.24 | 4.90 | 5.08 |
| 10,000 | Help to frame | 4.00 | 3.81 | 4.52 | 4.25 |

The 10,000-record completion median differs by 3.44 ms (+27.1%).
Snapshot size, including tombstones, is a relevant scaling dimension even
when few tasks are visible.

## Sustained idle cost

The [paired idle report](./2026-10-05-repaired-concurrency-idle-comparison.json) runs
baseline and current together on six independent scopes: one TUI per binary
for each of 0/1,000/10,000 mixed records. After five seconds of warmup there
are three 60-second idle windows. Every window has zero terminal output from
every process. Per-process CPU is measured with `wait4` across the whole
lifetime, including startup and warmup; the output windows alone do not give
independent CPU measurements.

| Records | Baseline CPU (% of one core) | Current CPU (% of one core) | Snapshot bytes read per second per current TUI |
| ---: | ---: | ---: | ---: |
| 0 | 0.0016% | 0.0243% | 132 |
| 1,000 | 0.0027% | 0.0361% | 166,983 |
| 10,000 | 0.0062% | 0.1246% | 1,691,822 |

At 10,000 records, idle CPU is about 20.2 times the almost-zero
baseline, with an absolute difference of 0.118 percentage points
of one core. The old TUI blocks waiting for input; the new one wakes once per
second to read and compare canonical bytes, enabling external changes to appear
without a keypress. Unchanged bytes skip parsing and drawing. Reading the
snapshot still scales with its full stored size, including tombstones.

The read rates follow the implemented one-second schedule and fixture sizes;
they are logical file reads, not measured physical disk traffic. Warm page
caches can serve them. These samples do not establish battery impact or the
cost on slow/network storage. The earlier instrumented refresh counts remain
in the [initial report](./concurrent-usage.md); that test executable was not
rerun for this comparison.

The [two-TUI idle report](./2026-10-05-repaired-concurrency-idle-two-tuis.json) separately
runs two current TUIs sharing each scope, again for five seconds of warmup and
three 60-second windows. Every TUI emits zero bytes in every window:

| Records | CPU per TUI (% of one core) | Combined CPU (% of one core) |
| ---: | ---: | ---: |
| 0 | 0.0202% to 0.0223% | 0.0425% |
| 1,000 | 0.0322% to 0.0331% | 0.0653% |
| 10,000 | 0.1249% to 0.1310% | 0.2559% |

At 10,000 records, the two readers together access about 3.4 MB of snapshot
bytes per second. The old binary cannot open two TUIs sharing one scope because
of its lifetime writer lock, so there is no equivalent successful baseline.

## Concurrent writer load

The [fresh load report](./2026-10-05-repaired-concurrency-load.json) retains three sequential
repetitions on independent 10,000-record mixed fixtures with a live current TUI.
Each repetition measures 30 shell additions paced at ten per second, 30 Help
open/close inputs, an eight-writer barrier burst, and one deliberate held-lock
Busy. Combined median and nearest-rank p95:

| Workload | Samples | Median ms | p95 ms | Max ms |
| --- | ---: | ---: | ---: | ---: |
| Paced shell additions | 90 | 28.40 | 31.71 | 36.27 |
| Input to mode acknowledgement | 90 | 4.60 | 8.61 | 9.37 |
| Eight simultaneous shell additions | 24 | 150.31 | 355.94 | 360.14 |

All 114 successful additions survived, with the complete expected text set,
consecutive unique IDs, and correct next-ID counter in each fixture. None
returned Busy. The three deliberate held-lock attempts returned Busy with no
addition after 1,007.09 to 1,015.96 ms.
The acquisition budget does not bound filesystem work after obtaining the lock.

An actual baseline TUI rejected the control shell addition in
4.87 ms, leaving the snapshot unchanged.
This is a failed operation, not successful mutation latency; the old binary
provides no equivalent live-TUI writer benchmark. The load timer uses capture
pipes and subprocess communication, whereas the paired CLI timer uses null
output and a blocking wait. Subtracting their latencies would not isolate
overhead from having a live TUI.

The load harness uses the shared median/nearest-rank percentile helper.
The original October 4 load report used lower-rank selections for p50/p95;
its raw samples and reported values remain intact.

## Interpretation

The concurrency change adds latest-snapshot reads and validation to TUI writes,
and continuous snapshot-byte reads while idle. At 10,000 mixed records, the
completion-to-frame median changes by 3.44 ms, and idle CPU by
0.118 percentage points of one core. Concurrent writes while a
TUI is open succeed, with contention reflected in burst latency and bounded
Busy behavior.

Future optimizations should measure unchanged-snapshot polling and large-list
transactions against these same workloads while preserving latest-snapshot
reloads under the lock and the conflict/durability contracts. This comparison
does not profile individual storage steps, measure TUI startup, establish
battery life, or exercise slow/network storage and other platforms.

## Earlier measurements

The [earlier October 5 report](./2026-10-05-pre-repair-concurrency-comparison.md)
and all its raw samples remain unchanged. It used binary SHA-256
`f43b695f1670322e7a17bb6bc10ab1ae56059740eaff26f39230ba9ffe72f131`
before the missing-lock repair. The fresh samples above are not pooled with
that batch or the [October 4 measurements](./concurrent-usage.md).

## Reproducing the paired measurements

Create an unchanged control in an ignored directory rather than changing the
working tree being measured:

```sh
mkdir -p target/benchmark-main-source
git archive 707b0ec | tar -x -C target/benchmark-main-source
cargo build --release --locked --manifest-path target/benchmark-main-source/Cargo.toml \
  --target-dir target/benchmark-main-build
cargo build --release --locked
python3 scripts/benchmark_concurrency_comparison.py \
  --baseline target/benchmark-main-build/release/shtodo --mode cli \
  --output /tmp/shtodo-paired-cli.json
python3 scripts/benchmark_concurrency_comparison.py \
  --baseline target/benchmark-main-build/release/shtodo --mode tui \
  --output /tmp/shtodo-paired-tui.json
python3 scripts/benchmark_concurrency_comparison.py \
  --baseline target/benchmark-main-build/release/shtodo --mode idle \
  --output /tmp/shtodo-paired-idle.json
python3 scripts/benchmark_concurrency.py --clients 2 \
  --output /tmp/shtodo-two-tuis-idle.json
for run in 1 2 3; do
  python3 scripts/benchmark_concurrency_load.py \
    --output "/tmp/shtodo-load-$run.json"
done
```

An isolated writable `CARGO_HOME` was used here because the shared cache is
read-only. The comparison scripts use only Python's standard library. The
Unix PTY portions require macOS or Linux. Historical October 4 samples are
retained in the [initial measurement report](./concurrent-usage.md).
