# CLI benchmark results

Run: 2026-10-05T16:52:28.489013+00:00

Platform: macOS-27.0.1-arm64-arm-64bit-Mach-O

Repetitions: 50; warmups: 5; random seed: 20261004.

Wall time includes process launch and exit. Warm filesystem caches; stdout goes to the null device. Fixtures are restored before every invocation outside the timer. Mutation results are checked outside the timer. p95 uses the nearest-rank sample percentile.

Version is a startup proxy, not TUI first-frame latency. Delete and durability semantics differ.

Scope: shtodo is deliberately a small local checklist. The other tools provide different capabilities, including organization, scheduling, automation, or interoperability. These selected CLI timings do not establish feature parity or overall product quality. See docs/benchmarks.md for the feature comparison and limitations.

| App | Operation | Initial tasks | Median ms | p95 ms |
| --- | --- | ---: | ---: | ---: |
| shtodo | list | 0 | 2.797 | 3.340 |
| shtodo | add | 0 | 12.164 | 13.573 |
| shtodo | list | 10 | 2.840 | 3.281 |
| shtodo | add | 10 | 11.966 | 13.685 |
| shtodo | delete | 10 | 12.252 | 14.176 |
| shtodo | list | 100 | 2.988 | 3.294 |
| shtodo | add | 100 | 12.121 | 14.503 |
| shtodo | delete | 100 | 12.495 | 14.174 |
| shtodo | list | 1000 | 3.728 | 4.086 |
| shtodo | add | 1000 | 13.043 | 14.423 |
| shtodo | delete | 1000 | 13.127 | 14.846 |
| shtodo | version | 0 | 2.821 | 3.199 |
| taskwarrior | list | 0 | 8.868 | 9.361 |
| taskwarrior | add | 0 | 9.066 | 9.620 |
| taskwarrior | list | 10 | 8.844 | 9.857 |
| taskwarrior | add | 10 | 9.177 | 10.075 |
| taskwarrior | delete | 10 | 9.288 | 10.014 |
| taskwarrior | list | 100 | 10.061 | 11.140 |
| taskwarrior | add | 100 | 10.864 | 11.845 |
| taskwarrior | delete | 100 | 11.354 | 12.259 |
| taskwarrior | list | 1000 | 20.365 | 21.347 |
| taskwarrior | add | 1000 | 24.960 | 26.170 |
| taskwarrior | delete | 1000 | 28.191 | 29.635 |
| taskwarrior | version | 0 | 6.513 | 7.111 |
| todotxt | list | 0 | 34.170 | 35.894 |
| todotxt | add | 0 | 24.587 | 25.655 |
| todotxt | list | 10 | 34.454 | 36.448 |
| todotxt | add | 10 | 24.450 | 25.692 |
| todotxt | delete | 10 | 19.085 | 20.391 |
| todotxt | list | 100 | 35.239 | 36.789 |
| todotxt | add | 100 | 24.479 | 25.351 |
| todotxt | delete | 100 | 19.283 | 20.379 |
| todotxt | list | 1000 | 46.151 | 47.385 |
| todotxt | add | 1000 | 24.344 | 25.562 |
| todotxt | delete | 1000 | 19.335 | 20.375 |
| todotxt | version | 0 | 9.069 | 9.803 |
| taskbook | list | 0 | 57.130 | 59.344 |
| taskbook | add | 0 | 58.111 | 61.797 |
| taskbook | list | 10 | 57.806 | 59.764 |
| taskbook | add | 10 | 57.730 | 59.200 |
| taskbook | delete | 10 | 57.975 | 60.181 |
| taskbook | list | 100 | 58.625 | 60.028 |
| taskbook | add | 100 | 57.820 | 60.309 |
| taskbook | delete | 100 | 58.376 | 61.015 |
| taskbook | list | 1000 | 63.856 | 65.598 |
| taskbook | add | 1000 | 59.807 | 61.582 |
| taskbook | delete | 1000 | 59.354 | 61.212 |
| taskbook | version | 0 | 56.526 | 57.947 |
| topydo | list | 0 | 34.720 | 36.035 |
| topydo | add | 0 | 35.748 | 36.716 |
| topydo | list | 10 | 36.817 | 37.853 |
| topydo | add | 10 | 36.735 | 38.375 |
| topydo | delete | 10 | 37.114 | 38.506 |
| topydo | list | 100 | 41.306 | 43.013 |
| topydo | add | 100 | 37.695 | 38.800 |
| topydo | delete | 100 | 38.296 | 39.511 |
| topydo | list | 1000 | 88.262 | 91.578 |
| topydo | add | 1000 | 46.784 | 48.340 |
| topydo | delete | 1000 | 50.831 | 52.488 |
| topydo | version | 0 | 33.783 | 35.063 |
| process-baseline | true | 0 | 2.091 | 2.519 |

All individual samples, exact commands, executable hashes, configurations, and environment details are in the companion JSON.
