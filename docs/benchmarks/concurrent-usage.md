# Concurrent usage measurements

October 4, 2026, on macOS 27.0.1 arm64, Rust 1.98.0, with APFS for the workspace
and temporary fixture directories. These measurements cover the approved
implementation over baseline `74d049e`, using schema 1 and the release binary
identified in the raw reports. They are separate from the
[historical CLI comparisons](../benchmarks.md).

These idle and refresh samples were taken before integration with
`origin/main` at `0982c5e` (shell lifecycle, trash, and search/filter).
They remain historical evidence for the initial concurrency implementation;
the combined code is covered by the integration validation recorded in the
[plan](../plans/concurrent-usage.md#integration-with-originmain-october-4-2026).
The [October 5 paired comparison](./concurrent-usage-comparison.md) measures
the combined binary against unchanged beta.3, including shell commands, TUI
saves, and fresh sustained idle samples. The numbers below remain the original
pre-integration results.

All fixtures use temporary homes, Unicode descriptions, and about one-third
tombstones. The fixture sizes are total records, including tombstones. The
production TUI uses a 100-column by 24-row PTY. No user task data is accessed.
Other local checks and measurement groups ran concurrently, so this is a host
sample rather than an isolated hardware benchmark. It does not establish
Windows/Linux behavior, a throughput ceiling, or power-loss durability.

## Idle refresh and terminal CPU

Each fixture receives a five-second warmup and three 60-second samples.
The native harness first opens one TUI per scope, then two sharing each scope;
the three fixture sizes run concurrently within each phase. Per-process CPU
comes from `wait4`, including startup and warmup. Terminal output excludes
warmup. The [native raw report](./2026-10-04-concurrency-idle.json) retains all
sample output counts, CPU seconds, elapsed times, and the binary SHA-256.

The separate release test executable measures the actual `Session` refresh
path with private read/parse counters and redraw requests, for all six
size/client groups in one process. Its [raw refresh report](./2026-10-04-concurrency-refresh.json)
retains every read duration and per-sample counts. Each TUI made 59 or 60
scheduled reads per 60-second sample; every sample had zero parses and zero
redraw requests after initialization. Equal bytes are still read and compared.
Terminal output bytes are a separate check from counting redraw requests.

| Records | TUIs sharing scope | CPU per TUI (% of one core) | Idle output per sample (bytes) |
| ---: | ---: | ---: | ---: |
| 0 | 1 | 0.028 | 0 |
| 1,000 | 1 | 0.034 | 0 |
| 10,000 | 1 | 0.109 | 0 |
| 0 | 2 | 0.029 to 0.030 | 0 |
| 1,000 | 2 | 0.038 to 0.040 | 0 |
| 10,000 | 2 | 0.109 to 0.114 | 0 |

| Records | Sessions sharing scope | Refresh p50 (ms) | p95 (ms) | Max (ms) |
| ---: | ---: | ---: | ---: | ---: |
| 0 | 1 | 0.258 | 0.436 | 2.806 |
| 0 | 2 | 0.071 | 0.120 | 0.511 |
| 1,000 | 1 | 0.175 | 0.343 | 0.517 |
| 1,000 | 2 | 0.118 | 0.210 | 1.286 |
| 10,000 | 1 | 1.091 | 1.575 | 1.697 |
| 10,000 | 2 | 0.716 | 1.265 | 8.261 |

At 10,000 records the native fixture is 1,691,822 bytes and the Rust fixture
is 1,696,818 bytes; their text and completion patterns differ slightly.
Polling therefore reads about 1.7 MB per second per TUI at that size even
though it skips parsing and drawing. Costs still scale with total records and
tombstones. The larger observed refresh outliers are retained in the report.

## Concurrent writes and input

The [load report](./2026-10-04-concurrency-load.json) measures a 10,000-record
fixture with a live TUI, 30 shell additions paced at 10 per second, and 30 Help
open/close inputs. Input timing ends when the reconstructed footer acknowledges
the requested mode, including PTY transport and harness processing. Command
timing includes process startup, lock acquisition, full-snapshot work, and sync;
it does not isolate time spent inside the storage transaction.

| Workload | Samples | p50 (ms) | p95 (ms) | Max (ms) |
| --- | ---: | ---: | ---: | ---: |
| Paced shell additions | 30 | 32.25 | 36.36 | 41.12 |
| Input to mode acknowledgement | 30 | 4.61 | 11.36 | 11.68 |
| Eight simultaneous shell additions | 8 | 115.80 | 274.86 | 335.40 |

All 38 additions survived with unique IDs; no write in those two workloads
returned Busy. A separate acknowledged holder kept the legacy writer lock
through an attempted shell addition: one explicit Busy returned after
1,007.89 ms, with no success output or added record. This includes startup and
the one-second lock acquisition budget. The 100-millisecond interactive budget
is covered by the held-process storage regression. These acquisition budgets
do not bound subsequent filesystem work.

The input sample exercises navigation between Help and Normal while a shell
writer is active. It does not measure typing latency during every save or
performance on slow/network storage. The current results support retaining
the synchronous transaction and one-second refresh design for this workload.

## Reproducing

Unix regression tests require `python3` for the PTY harness. `cargo test` invokes
that harness against its built debug binary. Windows skips the PTY wrapper and
retains the Rust storage/process tests. The existing CI matrix is unchanged;
its Windows/Linux runs have not been triggered from this worktree.

Build and measure a release binary from the checkout:

```sh
cargo build --release --locked
python3 scripts/test_concurrency.py
python3 scripts/benchmark_concurrency.py \
  --output /tmp/shtodo-concurrency-idle.json
python3 scripts/benchmark_concurrency_load.py \
  --output /tmp/shtodo-concurrency-load.json
SHTODO_REFRESH_REPORT=/tmp/shtodo-concurrency-refresh.json \
  cargo test --release --locked --lib \
  session::measurements::idle_refresh_measurements -- --ignored --exact --nocapture
```

The scripts default to `target/release/shtodo`; set `SHTODO_BINARY` to choose
another binary. The ignored refresh test runs for about 185 seconds; the
native one/two-TUI sequence runs for about 370 seconds. An isolated writable
`CARGO_HOME` was used for this session because the shared cache is read-only.
The scripts and raw artifacts are checked in so later runs can use the same
workload without changing the historical benchmark data.
