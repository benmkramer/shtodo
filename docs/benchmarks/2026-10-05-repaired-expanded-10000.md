# CLI benchmark results

Run: 2026-10-05T16:55:29.316749+00:00

Platform: macOS-27.0.1-arm64-arm-64bit-Mach-O

Repetitions: 50; warmups: 5; random seed: 20261004.

Wall time includes process launch and exit. Warm filesystem caches; stdout goes to the null device. Fixtures are restored before every invocation outside the timer. Mutation results are checked outside the timer. p95 uses the nearest-rank sample percentile.

Version is a startup proxy, not TUI first-frame latency. Delete and durability semantics differ.

Scope: shtodo is deliberately a small local checklist. The other tools provide different capabilities, including organization, scheduling, automation, or interoperability. These selected CLI timings do not establish feature parity or overall product quality. See docs/benchmarks.md for the feature comparison and limitations.

| App | Operation | Initial tasks | Median ms | p95 ms |
| --- | --- | ---: | ---: | ---: |
| shtodo | list | 10000 | 12.772 | 13.349 |
| shtodo | add | 10000 | 17.466 | 20.029 |
| shtodo | delete | 10000 | 17.427 | 20.384 |
| shtodo | version | 0 | 3.016 | 3.423 |
| topydo | list | 10000 | 991.253 | 1001.433 |
| topydo | add | 10000 | 154.074 | 159.270 |
| topydo | delete | 10000 | 400.173 | 404.292 |
| topydo | version | 0 | 34.017 | 34.850 |
| process-baseline | true | 0 | 2.334 | 2.681 |

All individual samples, exact commands, executable hashes, configurations, and environment details are in the companion JSON.
