"""Compare two shtodo release binaries with paired, isolated CLI/TUI workloads."""

import argparse
import contextlib
import copy
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import random
import select
import subprocess
import tempfile
import time
from unittest.mock import patch

import benchmark
from benchmark_concurrency import drain_all, fixture
import test_concurrency


ROOT = Path(__file__).resolve().parents[1]


def plain_fixture(size):
    return dict(schema_version=1, scope=dict(kind="global"), next_task_id=size + 1,
                next_deletion_sequence=1, tasks=[dict(
                    id=index, text=benchmark.task_text(index), completed=False,
                    deletion_sequence=None) for index in range(1, size + 1)])


def cli_case(value, kind, operation):
    expected = copy.deepcopy(value)
    tasks = expected["tasks"]
    active_id = 1 if kind == "plain" else 2
    commands = {
        "list": ["list"], "add": ["add", benchmark.ADDED_TASK],
        "delete": ["delete", str(active_id)], "done": ["done", "2"],
        "reopen": ["reopen", "3"], "edit": ["edit", "2", "Changed café 東京"],
        "restore": ["restore", "1"], "noop_done": ["done", "3"],
        "noop_delete": ["delete", "1"], "version": ["--version"],
    }
    if operation == "add":
        tasks.append(dict(id=expected["next_task_id"], text=benchmark.ADDED_TASK,
                          completed=False, deletion_sequence=None))
        expected["next_task_id"] += 1
    elif operation == "delete":
        tasks[active_id - 1]["deletion_sequence"] = expected["next_deletion_sequence"]
        expected["next_deletion_sequence"] += 1
    elif operation == "done":
        tasks[1]["completed"] = True
    elif operation == "reopen":
        tasks[2]["completed"] = False
    elif operation == "edit":
        tasks[1]["text"] = "Changed café 東京"
    elif operation == "restore":
        tasks[0]["deletion_sequence"] = None
    return commands[operation], expected


def measure_cli(args, binaries, rng):
    cases = []
    schedule = []
    with tempfile.TemporaryDirectory(prefix="shtodo-paired-cli-") as directory:
        root = Path(directory)
        fixtures = {}
        for size in args.sizes:
            seed = root / "fixtures" / str(size)
            fixture(seed, size)
            fixtures[size] = json.loads((seed / ".shtodo/global/tasks.json").read_text())
        for label, binary in binaries.items():
            home = root / label
            home.mkdir()
            (home / "tmp").mkdir()
            env = benchmark.isolated_env(home)
            path = home / ".shtodo/global/tasks.json"
            path.parent.mkdir(parents=True)
            for size in args.sizes:
                for kind in ("plain", "mixed"):
                    if kind == "plain":
                        value = plain_fixture(size)
                    else:
                        value = fixtures[size]
                    data = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()
                    operations = ["list", "add"]
                    if size and (kind == "plain" or size >= 2):
                        operations.append("delete")
                    if kind == "mixed" and size >= 3:
                        operations += ["done", "reopen", "edit", "restore", "noop_done", "noop_delete"]
                    for operation in operations:
                        command, expected = cli_case(value, kind, operation)
                        cases.append(dict(label=label, binary=binary, home=home, env=env,
                                          path=path, fixture=data, expected=expected,
                                          operation=operation, kind=kind, records=size,
                                          command=[binary, *command], samples=[]))
            value = plain_fixture(0)
            cases.append(dict(label=label, binary=binary, home=home, env=env, path=path,
                              fixture=(json.dumps(value) + "\n").encode(), expected=value,
                              operation="version", kind="plain", records=0,
                              command=[binary, "--version"], samples=[]))

        def reset(case):
            case["path"].write_bytes(case["fixture"])
            case["path"].with_name("tasks.lock").unlink(missing_ok=True)
            return case["path"].stat().st_mtime_ns

        def verify(case, before):
            operation = case["operation"]
            if operation in ("list", "version"):
                return
            actual = json.loads(case["path"].read_text())
            assert actual == case["expected"], (case["label"], case["kind"], operation, case["records"])
            if operation.startswith("noop_"):
                assert case["path"].read_bytes() == case["fixture"]
                assert case["path"].stat().st_mtime_ns == before

        for case in cases:
            before = reset(case)
            output = benchmark.capture(case["command"], case["env"], case["home"])
            if case["operation"] == "list":
                expected = "".join(f"{task['id']}  {'done' if task['completed'] else 'open'}  {task['text']}\n"
                                   for task in case["expected"]["tasks"] if task["deletion_sequence"] is None)
                assert output == expected, (case["label"], case["kind"], case["records"])
            verify(case, before)
        print(f"Preflight passed for {len(cases)} paired CLI cases", flush=True)
        with open(os.devnull, "r+b") as null, tempfile.TemporaryFile() as errors:
            for round_index in range(-args.warmups, args.runs):
                order = cases.copy()
                rng.shuffle(order)
                for case in order:
                    before = reset(case)
                    elapsed = benchmark.timed(case["command"], case["env"], case["home"], null, errors)
                    verify(case, before)
                    if round_index >= 0:
                        case["samples"].append(elapsed)
                        schedule.append([round_index, case["label"], case["kind"], case["operation"], case["records"]])
                if round_index == -1 or (round_index + 1) % 10 == 0:
                    print(f"CLI round {round_index + 1}/{args.runs} complete", flush=True)
    return dict(schedule=schedule, cases=[dict(
        binary=case["label"], fixture=case["kind"], records=case["records"],
        snapshot_bytes=len(case["fixture"]), operation=case["operation"],
        command=case["command"], samples_ms=case["samples"],
        statistics=benchmark.summary(case["samples"])) for case in cases])


def wait_screen(tui, predicate):
    deadline = time.monotonic() + 5
    while not predicate(tui):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise AssertionError(f"TUI acknowledgement timed out: {tui.text()}")
        if select.select([tui.master], [], [], remaining)[0]:
            data = os.read(tui.master, 65536)
            if not data:
                raise AssertionError("TUI exited during timing")
            tui.output.extend(data)
            tui.feed(data)


def measure_tui(args, binaries, rng):
    results = []
    with tempfile.TemporaryDirectory(prefix="shtodo-paired-tui-") as directory:
        root = Path(directory)
        for size in [size for size in args.sizes if size >= 3]:
            with contextlib.ExitStack() as stack:
                cases = []
                labels = list(binaries)
                rng.shuffle(labels)
                for label in labels:
                    home = root / f"{size}-{label}"
                    size_bytes = fixture(home, size)
                    with patch.object(test_concurrency, "BINARY", Path(binaries[label])):
                        tui = stack.enter_context(test_concurrency.Tui(home))
                    wait_screen(tui, lambda value: "".join(value.screen[-1]).startswith("NORMAL"))
                    cases.append(dict(binary=label, home=home, tui=tui, records=size,
                                      snapshot_bytes=size_bytes, completed=False, help=False,
                                      samples={"toggle_to_frame": [], "help_to_frame": []}))
                for round_index in range(-args.warmups, args.runs):
                    order = [(case, operation) for case in cases for operation in case["samples"]]
                    rng.shuffle(order)
                    for case, operation in order:
                        tui = case["tui"]
                        if operation == "toggle_to_frame":
                            case["completed"] = not case["completed"]
                            marker = "✓" if case["completed"] else "○"
                            predicate = lambda value: any(
                                marker in "".join(row) and "Task 2 café" in "".join(row)
                                for row in value.screen)
                            key = b" "
                        else:
                            predicate = lambda value: "".join(value.screen[-1]).startswith("HELP")
                            key = b"?"
                        started = time.perf_counter_ns()
                        tui.send(key)
                        wait_screen(tui, predicate)
                        elapsed = (time.perf_counter_ns() - started) / 1_000_000
                        if operation == "help_to_frame":
                            tui.send(b"?")
                            wait_screen(tui, lambda value: "".join(value.screen[-1]).startswith("NORMAL"))
                        else:
                            value = json.loads((case["home"] / ".shtodo/global/tasks.json").read_text())
                            assert value["tasks"][1]["completed"] == case["completed"]
                            assert len(value["tasks"]) == size
                        if round_index >= 0:
                            case["samples"][operation].append(elapsed)
                for case in cases:
                    for operation, samples in case["samples"].items():
                        results.append(dict(binary=case["binary"], records=size,
                                            snapshot_bytes=case["snapshot_bytes"], operation=operation,
                                            samples_ms=samples, statistics=benchmark.summary(samples)))
            print(f"TUI {size} records complete", flush=True)
    return results


def measure_idle(args, binaries):
    groups = []
    with tempfile.TemporaryDirectory(prefix="shtodo-paired-idle-") as directory, contextlib.ExitStack() as stack:
        tuis = []
        for size in (0, 1000, 10000):
            for label, binary in binaries.items():
                home = Path(directory) / f"{size}-{label}"
                size_bytes = fixture(home, size)
                with patch.object(test_concurrency, "BINARY", Path(binary)):
                    tui = stack.enter_context(test_concurrency.Tui(home))
                tuis.append(tui)
                groups.append(dict(binary=label, records=size, clients=1, snapshot_bytes=size_bytes,
                                   idle_output_bytes=[]))
        drain_all(tuis, args.idle_warmup)
        for sample in range(args.idle_samples):
            counts = drain_all(tuis, args.idle_seconds)
            assert not any(counts), counts
            for group, count in zip(groups, counts):
                group["idle_output_bytes"].append(count)
            print(f"Paired idle sample {sample + 1}/{args.idle_samples} complete", flush=True)
        for group, tui in zip(groups, tuis):
            tui.close()
            elapsed = time.monotonic() - tui.started
            group.update(cpu_seconds=tui.cpu_seconds, elapsed_seconds=elapsed,
                         percent_of_one_core=100 * tui.cpu_seconds / elapsed)
    return groups


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--current", default=str(ROOT / "target/release/shtodo"))
    parser.add_argument("--mode", choices=("cli", "tui", "idle"), required=True)
    parser.add_argument("--sizes", type=int, nargs="+", default=[0, 100, 1000, 10000])
    parser.add_argument("--runs", type=int, default=50)
    parser.add_argument("--warmups", type=int, default=5)
    parser.add_argument("--seed", type=int, default=20261005)
    parser.add_argument("--idle-seconds", type=float, default=60)
    parser.add_argument("--idle-samples", type=int, default=3)
    parser.add_argument("--idle-warmup", type=float, default=5)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.runs < 1 or args.warmups < 0 or any(size < 0 for size in args.sizes):
        parser.error("runs must be positive, warmups and sizes nonnegative")
    if args.idle_seconds <= 0 or args.idle_samples < 1 or args.idle_warmup < 0:
        parser.error("idle duration and samples must be positive; warmup nonnegative")
    args.sizes = sorted(set(args.sizes))
    binaries = {label: benchmark.executable(value) for label, value in (
        ("baseline", args.baseline), ("current", args.current))}
    started = datetime.now(timezone.utc).isoformat()
    provenance = {label: dict(path=binary, sha256=benchmark.digest(binary),
                             version=benchmark.capture([binary, "--version"]).strip())
                  for label, binary in binaries.items()}
    rng = random.Random(args.seed)
    data = {"cli": measure_cli, "tui": measure_tui, "idle": measure_idle}[args.mode](
        args, binaries, *([rng] if args.mode != "idle" else []))
    report = dict(schema_version=1, started_at=started,
                  completed_at=datetime.now(timezone.utc).isoformat(), mode=args.mode,
                  machine=benchmark.machine_metadata(), binaries=provenance,
                  settings=vars(args) | {"output": str(args.output)},
                  harness_sha256=benchmark.digest(__file__),
                  cargo_lock_sha256=benchmark.digest(ROOT / "Cargo.lock"),
                  baseline_commit=benchmark.optional_command(["git", "rev-parse", "origin/main"]),
                  current_git_status=benchmark.optional_command(["git", "status", "--short"]),
                  rustc=benchmark.optional_command(["rustc", "--version"]),
                  method="Paired release binaries on independent temporary global scopes. CLI cases shuffled each round; snapshot reset and correctness checks outside timers; null stdout; process launch/exit included. Plain fixtures match the historical short open-task workload; mixed fixtures have Unicode and one-third tombstones. TUI cases shuffled each round; input ends at an acknowledged marker/mode frame, including rendering and PTY/harness work; completion persisted before frame. Idle runs six independent scopes together, one TUI each, with wait4 CPU including initialization/warmup and no terminal output after warmup. Warm caches, local storage; not a throughput ceiling or a cross-platform result.",
                  results=data)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(f"Wrote {args.output}", flush=True)


if __name__ == "__main__":
    main()
