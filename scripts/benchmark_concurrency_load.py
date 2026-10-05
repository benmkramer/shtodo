"""Measure CLI command and PTY input latency under local concurrent writes."""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import platform
import select
import tempfile
import threading
import time

from benchmark_concurrency import fixture
from benchmark import summary
from test_concurrency import BINARY, Tui, cli


def summarize(values):
    statistics = summary(values)
    return dict(samples=len(values), raw_ms=values,
                p50_ms=statistics["median_ms"],
                p95_ms=statistics["p95_ms"], max_ms=statistics["max_ms"])


def wait_mode(tui, mode):
    deadline = time.monotonic() + 4
    while not "".join(tui.screen[-1]).startswith(mode):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise AssertionError(f"no {mode} frame: {tui.text()}")
        if select.select([tui.master], [], [], remaining)[0]:
            data = os.read(tui.master, 65536)
            if not data:
                raise AssertionError("TUI exited during latency measurement")
            tui.output.extend(data)
            tui.feed(data)


def command(home, text):
    started = time.perf_counter()
    reply = cli(home, "add", text)
    elapsed = (time.perf_counter() - started) * 1000
    if reply.returncode != 0:
        raise AssertionError(reply.stderr)
    return elapsed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="shtodo-load-") as root:
        home = Path(root)
        size = fixture(home, 10000)
        with Tui(home) as tui, concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            wait_mode(tui, "NORMAL")
            started = threading.Event()

            def moderate_writer():
                started.wait(timeout=4)
                samples = []
                next_at = time.monotonic()
                for index in range(30):
                    samples.append(command(home, f"moderate {index}"))
                    next_at += 0.1
                    time.sleep(max(0, next_at - time.monotonic()))
                return samples

            writer = pool.submit(moderate_writer)
            input_samples = []
            started.set()
            for index in range(30):
                before = time.perf_counter()
                tui.send(b"?")
                wait_mode(tui, "HELP" if index % 2 == 0 else "NORMAL")
                input_samples.append((time.perf_counter() - before) * 1000)
                time.sleep(0.1)
            writes = writer.result(timeout=5)

            barrier = threading.Barrier(8)

            def burst(index):
                barrier.wait(timeout=4)
                return command(home, f"burst {index}")

            futures = [pool.submit(burst, index) for index in range(8)]
            burst_samples = [future.result(timeout=5) for future in futures]

            # An acknowledged holder exercises the actual shell acquisition budget.
            import fcntl
            with (home / ".shtodo/global/tasks.lock").open("r+") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                before = time.perf_counter()
                busy = cli(home, "add", "must not commit")
                busy_ms = (time.perf_counter() - before) * 1000
                assert busy.returncode != 0 and b"timed out" in busy.stderr
                assert not busy.stdout
            values = json.loads((home / ".shtodo/global/tasks.json").read_text())
            assert len(values["tasks"]) == 10038
            assert {task["id"] for task in values["tasks"]} == set(range(1, 10039))
            assert values["next_task_id"] == 10039
            added = {task["text"] for task in values["tasks"] if task["id"] > 10000}
            assert added == {f"moderate {index}" for index in range(30)} | {
                f"burst {index}" for index in range(8)}

        result = dict(binary_sha256=hashlib.sha256(BINARY.read_bytes()).hexdigest(),
                      platform=platform.platform(), machine=platform.machine(),
                      records=10000, snapshot_bytes=size,
                      method="Actual release binary, 100x24 PTY, 30 shell adds paced at 10/second, and 30 Help open/close inputs. Input latency ends when the reconstructed footer acknowledges the requested mode and includes PTY transport/harness work. CLI latency includes process startup, lock wait, parsing, full serialization, replacement and sync; it does not isolate transaction time. An eight-writer barrier burst follows; a separately held legacy lock measures Busy.",
                      moderate_cli=summarize(writes), input_to_mode_frame=summarize(input_samples),
                      burst_cli=summarize(burst_samples), busy=dict(count=1, latency_ms=busy_ms))
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n")
        print(json.dumps({name: {key: value for key, value in data.items() if key != "raw_ms"}
                          for name, data in result.items() if isinstance(data, dict)}, indent=2))


if __name__ == "__main__":
    main()
