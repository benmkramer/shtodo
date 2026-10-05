"""Measure actual idle TUIs on isolated Unicode/tombstone fixtures (Unix)."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import platform
import select
import tempfile
import time

from test_concurrency import BINARY, Tui


def fixture(home, size):
    directory = home / ".shtodo/global"
    directory.mkdir(parents=True)
    deleted = 0
    tasks = []
    for index in range(size):
        sequence = None
        if index % 3 == 0:
            deleted += 1
            sequence = deleted
        tasks.append(dict(id=index + 1, text=f"Task {index + 1} café 東京 " + "x" * 40,
                          completed=index % 2 == 0, deletion_sequence=sequence))
    value = dict(schema_version=1, scope=dict(kind="global"), next_task_id=size + 1,
                 next_deletion_sequence=deleted + 1, tasks=tasks)
    path = directory / "tasks.json"
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")
    # All fixture mutations create the same stable lock inode used by shtodo.
    (directory / "tasks.lock").touch()
    return path.stat().st_size


def drain_all(tuis, duration):
    deadline = time.monotonic() + duration
    before = [len(tui.output) for tui in tuis]
    while time.monotonic() < deadline:
        ready = select.select([tui.master for tui in tuis], [], [], max(0, min(0.2, deadline - time.monotonic())))[0]
        for tui in tuis:
            if tui.master in ready:
                data = os.read(tui.master, 65536)
                if not data:
                    raise RuntimeError("TUI exited during measurement")
                tui.output.extend(data)
                tui.feed(data)
    return [len(tui.output) - start for tui, start in zip(tuis, before)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seconds", type=float, default=60)
    parser.add_argument("--samples", type=int, default=3)
    parser.add_argument("--warmup", type=float, default=5)
    parser.add_argument("--clients", type=int, nargs="+", choices=(1, 2), default=[1, 2])
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.seconds <= 0 or args.samples < 1 or args.warmup < 0:
        parser.error("sample duration and count must be positive; warmup nonnegative")
    result = dict(platform=platform.platform(), machine=platform.machine(),
                  binary_sha256=hashlib.sha256(BINARY.read_bytes()).hexdigest(),
                  sample_seconds=args.seconds, samples=args.samples, warmup_seconds=args.warmup,
                  client_counts=args.clients,
                  method="Three independent fixture sizes run concurrently within each requested client-count phase. CPU uses per-process wait4 usage including startup and warmup; idle output bytes exclude warmup. This is not isolated throughput or a power-loss test.", groups=[])
    for clients in args.clients:
        with tempfile.TemporaryDirectory(prefix="shtodo-idle-") as root, contextlib.ExitStack() as stack:
            groups = []
            all_tuis = []
            for size in (0, 1000, 10000):
                home = Path(root) / str(size)
                byte_size = fixture(home, size)
                tuis = [stack.enter_context(Tui(home)) for _ in range(clients)]
                all_tuis.extend(tuis)
                groups.append(dict(records=size, clients=clients, snapshot_bytes=byte_size,
                                   idle_output_bytes=[], tuis=tuis))
            drain_all(all_tuis, args.warmup)
            for sample in range(args.samples):
                output = drain_all(all_tuis, args.seconds)
                offset = 0
                for group in groups:
                    sample_output = output[offset:offset + clients]
                    assert sample_output == [0] * clients, sample_output
                    group["idle_output_bytes"].append(sample_output)
                    offset += clients
                print(f"{clients} clients: sample {sample + 1}/{args.samples} complete", flush=True)
            for group in groups:
                cpu = []
                for tui in group.pop("tuis"):
                    tui.close()
                    elapsed = time.monotonic() - tui.started
                    cpu.append(dict(cpu_seconds=tui.cpu_seconds, elapsed_seconds=elapsed,
                                    percent_of_one_core=100 * tui.cpu_seconds / elapsed))
                group["process_usage"] = cpu
                result["groups"].append(group)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
