# Performance of the concurrency change

October 5, 2026. This compares unchanged `origin/main` at `707b0ec` with the
uncommitted concurrency implementation before the missing-lock repair, including
the integrated shell lifecycle, trash, and search/status tabs. Both are
`0.1.0-beta.3` release builds with the same Cargo manifest, lockfile, Rust 1.98.0,
and optimization profile.
The binary hashes distinguish them despite the identical version strings.
The subsequent repair permits TUI writes and sync recovery to create an absent
lock file. These samples have not been rerun after that repair; "Current" below
refers to the measured binary identified here.

The measured shell commands show no large regression. The cost is clearer in
TUI saves: completing a task at 10,000 stored records takes about 3.3 ms longer,
or 26%, while Help input remains similar. The fresh transaction reads and
validates the latest snapshot for every write, whereas the old TUI mutates its
in-memory list. That explains the expected direction of the change; this
end-to-end measurement does not isolate the contribution of individual steps.

## Method and provenance

Apple M4 Max, 48 GB RAM, macOS 27.0.1 arm64, AC power, local APFS storage.
Both binaries were built before measuring. Timed benchmark batches ran
sequentially. Other activity on the user's computer is not controlled. These
are warm-cache host measurements, not a cross-platform claim or throughput
ceiling. No user task data was accessed.

| Binary | Source | SHA-256 |
| --- | --- | --- |
| Baseline | `707b0eca2d7d29d5ab3adeb04d5e1dc35c632bdc`, extracted with `git archive` | `c2332442eb8089bb2417577495f832865c9603e82ea93a8bbfbccb78ee67bfe7` |
| Current | Same commit plus the worktree's concurrency implementation | `f43b695f1670322e7a17bb6bc10ab1ae56059740eaff26f39230ba9ffe72f131` |

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

## Shell commands

The [raw paired CLI report](./2026-10-05-concurrency-cli-comparison.json)
retains 4,100 measured invocations across 82 cases, their randomized schedule,
exact commands, fixture byte sizes, binary hashes, and host metadata.
Wall time includes process launch/exit and the command's ordinary persistence;
stdout goes to the null device. No TUI is open in this batch.

Median milliseconds for the historical plain fixture:

| Records | Operation | Baseline | Current | Change |
| ---: | --- | ---: | ---: | ---: |
| 0 | List | 2.98 | 3.01 | +1.0% |
| 0 | Add | 11.44 | 11.69 | +2.2% |
| 100 | List | 3.30 | 3.12 | -5.4% |
| 100 | Add | 11.57 | 11.57 | 0.0% |
| 100 | Delete | 11.62 | 11.59 | -0.2% |
| 1,000 | List | 4.02 | 4.07 | +1.4% |
| 1,000 | Add | 12.25 | 12.15 | -0.8% |
| 1,000 | Delete | 12.21 | 12.41 | +1.6% |
| 10,000 | List | 13.08 | 13.23 | +1.1% |
| 10,000 | Add | 17.32 | 17.42 | +0.6% |
| 10,000 | Delete | 17.54 | 17.61 | +0.4% |

At 10,000 mixed records, the stored snapshot is 1,691,822 bytes. This batch also
times the newer lifecycle commands and no-write paths:

| Operation | Baseline median ms | Current median ms | Baseline p95 ms | Current p95 ms |
| --- | ---: | ---: | ---: | ---: |
| List | 11.70 | 11.71 | 12.47 | 12.74 |
| Add | 18.83 | 18.70 | 20.77 | 20.77 |
| Delete | 19.05 | 19.12 | 20.64 | 20.63 |
| Done | 18.70 | 18.92 | 20.63 | 20.31 |
| Reopen | 18.90 | 18.98 | 20.38 | 20.67 |
| Edit | 18.89 | 19.09 | 20.45 | 20.73 |
| Restore | 18.75 | 18.86 | 20.18 | 20.63 |
| Already-done done | 6.73 | 6.75 | 7.34 | 7.57 |
| Already-deleted delete | 6.75 | 6.85 | 7.42 | 7.47 |

All sizes are in the raw report. The observed
10,000-record shell mutation medians remain within about 2% of the control.
Both implementations already reload and save the whole snapshot for shell
commands; changing lock lifetime introduces little visible additional cost
in these uncontended samples. Small differences between host measurements do
not establish a universal speedup or slowdown.

## TUI actions

The [raw paired TUI report](./2026-10-05-concurrency-tui-comparison.json)
retains 600 measured actions. Two TUIs run on independent baseline/current
scopes at each size, using the mixed fixtures and 100-column by 24-row PTYs.
Each input ends when the reconstructed screen acknowledges the new completion
marker or Help footer. The timing includes rendering, PTY transport, and Python
screen reconstruction. TUI startup is outside the timer. Each completion is
also checked in the persisted snapshot before the next action.

| Records | Action | Baseline median ms | Current median ms | Baseline p95 ms | Current p95 ms |
| ---: | --- | ---: | ---: | ---: | ---: |
| 100 | Toggle completion to frame | 8.88 | 8.84 | 10.15 | 9.90 |
| 1,000 | Toggle completion to frame | 8.60 | 9.41 | 9.63 | 10.26 |
| 10,000 | Toggle completion to frame | 12.90 | 16.21 | 14.51 | 17.85 |
| 100 | Help to frame | 4.24 | 4.20 | 4.86 | 4.76 |
| 1,000 | Help to frame | 4.20 | 4.26 | 4.82 | 4.61 |
| 10,000 | Help to frame | 3.91 | 3.80 | 4.36 | 4.29 |

The 10,000-record completion median is 25.6% higher, with a 3.31 ms absolute
increase. At 1,000 records the increase is 0.80 ms, or 9.3%. At 100 records it
is effectively unchanged in this batch. Snapshot size, including tombstones,
is therefore a relevant scaling dimension even when few tasks are visible.

## Sustained idle cost

The [paired idle report](./2026-10-05-concurrency-idle-comparison.json) runs
baseline and current together on six independent scopes: one TUI per binary
for each of 0/1,000/10,000 mixed records. After five seconds of warmup there
are three 60-second idle windows. Every window has zero terminal output from
every process. Per-process CPU is measured with `wait4` across the whole
lifetime, including startup and warmup; the output windows alone do not give
independent CPU measurements.

| Records | Baseline CPU (% of one core) | Current CPU (% of one core) | Snapshot bytes read per second per current TUI |
| ---: | ---: | ---: | ---: |
| 0 | 0.0015% | 0.0270% | 132 |
| 1,000 | 0.0023% | 0.0363% | 166,983 |
| 10,000 | 0.0055% | 0.1167% | 1,691,822 |

The relative idle CPU increase is about 21 times at 10,000 records, over an
almost-zero baseline. The absolute increase is about 0.111 percentage points
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

The [two-TUI idle report](./2026-10-05-concurrency-idle-two-tuis.json) separately
runs two current TUIs sharing each scope, again for five seconds of warmup and
three 60-second windows. Every TUI emits zero bytes in every window:

| Records | CPU per TUI (% of one core) | Combined CPU (% of one core) |
| ---: | ---: | ---: |
| 0 | 0.0262% to 0.0278% | 0.0540% |
| 1,000 | 0.0369% to 0.0374% | 0.0743% |
| 10,000 | 0.1125% to 0.1193% | 0.2318% |

At 10,000 records, the two readers together access about 3.4 MB of snapshot
bytes per second. CPU cost is approximately additive for these two clients.
The old binary cannot open two TUIs sharing one scope because of its lifetime
writer lock, so there is no equivalent successful baseline for this case.

## Concurrent writer load

The [fresh load report](./2026-10-05-concurrency-load.json) retains three
sequential repetitions on independent 10,000-record mixed fixtures with a
live current TUI. Each repetition measures 30 shell additions paced at ten
per second, 30 Help open/close inputs, an eight-writer barrier burst, and one
deliberate held-lock Busy. Combined median and nearest-rank p95:

| Workload | Samples | Median ms | p95 ms | Max ms |
| --- | ---: | ---: | ---: | ---: |
| Paced shell additions | 90 | 27.95 | 29.47 | 33.01 |
| Input to mode acknowledgement | 90 | 4.00 | 7.58 | 11.28 |
| Eight simultaneous shell additions | 24 | 136.56 | 334.39 | 341.76 |

All 114 successful additions survived, with the complete expected text set,
consecutive unique IDs, and correct next-ID counter in each fixture. None of
those additions returned Busy. The three deliberate held-lock attempts each
returned Busy with no addition, after 1,005.70 to 1,007.54 ms. The acquisition
budget does not bound filesystem work after obtaining the lock.

An actual baseline TUI rejected the control shell addition in 3.88 ms, leaving
the snapshot unchanged. This is a failed operation, not successful mutation
latency; the old binary provides no equivalent live-TUI writer benchmark.
The new load timer uses capture pipes and subprocess communication, whereas
the paired CLI timer uses null output and a blocking wait. Subtracting their
latencies would not isolate the overhead from having a live TUI.

The load harness now uses the shared median/nearest-rank percentile helper.
The original October 4 load report used lower-rank selections for p50/p95;
its raw samples and reported values remain intact. These fresh results have
not been pooled with those earlier samples.

## Interpretation

The performance change is concentrated in large-list TUI writes and sustained
idle polling. At 10,000 mixed records, completing a task adds about 3.3 ms to
the median; idle CPU rises by about 0.111 percentage points of one core. The
continuous snapshot-byte reads are a real additional cost despite skipped
parsing/drawing. Shell command latency remains close to the unchanged control
at the measured sizes. Concurrent writes while a TUI is open now succeed,
with contention reflected in the burst latency and bounded Busy behavior.

If optimizing further, measure changes to unchanged-snapshot polling and
large-list TUI transactions against these same workloads. Preserve latest
snapshot reloads under the writer lock and the conflict/durability contracts.
This comparison does not profile individual storage steps, measure TUI startup,
establish battery life, or exercise slow/network storage and other platforms.

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
