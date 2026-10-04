#!/usr/bin/env python3
"""Benchmark real CLI processes against disposable, identical task fixtures.

Python 3.10+, macOS/Linux. No Python dependencies. See docs/benchmarks.md.
"""

import argparse
from collections import Counter
from dataclasses import dataclass, field
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
import uuid
import zlib


ROOT = Path(__file__).resolve().parents[1]
TASK_PATTERN = re.compile(r"Benchmark task [0-9]{8}")
ADDED_TASK = "Benchmark task 99999999"


def task_text(index):
    return f"Benchmark task {index:08d}"


def isolated_env(home):
    # Start from an allowlist: inherited TASK*, TODO*, BASH_ENV, and XDG
    # settings can silently redirect a benchmark to someone's real task store.
    return {
        "PATH": os.environ.get("PATH", os.defpath),
        "HOME": str(home),
        "USERPROFILE": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_DATA_HOME": str(home / ".local/share"),
        "XDG_CACHE_HOME": str(home / ".cache"),
        "TMPDIR": str(home / "tmp"),
        "LC_ALL": "C",
        "LANG": "C",
        "TZ": "UTC",
        "TERM": "dumb",
        "NO_COLOR": "1",
    }


def capture(command, env=None, cwd=None, accepted=(0,), timeout=120):
    result = subprocess.run(
        command, env=env, cwd=cwd, stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, timeout=timeout,
    )
    if result.returncode not in accepted:
        raise RuntimeError(
            f"Command exited {result.returncode}: {shlex.join(command)}\n"
            f"{result.stdout}{result.stderr}"
        )
    return result.stdout


def executable(value):
    found = shutil.which(value)
    path = Path(found or value).expanduser().resolve()
    if not path.is_file() or not os.access(path, os.X_OK):
        raise ValueError(f"Executable unavailable: {value}")
    return str(path)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def summary(samples):
    ordered = sorted(samples)
    return {
        "median_ms": statistics.median(ordered),
        "p95_ms": ordered[math.ceil(0.95 * len(ordered)) - 1],
        "min_ms": ordered[0],
        "max_ms": ordered[-1],
        "mean_ms": statistics.mean(ordered),
        "stdev_ms": statistics.stdev(ordered) if len(ordered) > 1 else 0.0,
    }


class Adapter:
    def __init__(self, name, binary, root, node="node"):
        self.name = name
        self.binary = binary
        self.home = root / name / "active"
        self.snapshots = root / name / "snapshots"
        self.env = isolated_env(self.home)
        self.runtime = None
        if name == "taskbook":
            self.env.update(NO_UPDATE_NOTIFIER="1", FORCE_COLOR="0")
            # Resolve runtime-manager shims once, outside the timer. Launch the
            # actual Node executable so shim discovery is not charged to tb.
            self.runtime = executable(capture([node, "-p", "process.execPath"]).strip())
        self.home.mkdir(parents=True)
        (self.home / "tmp").mkdir()
        self.snapshots.mkdir()
        self.version = capture(self.command("version"), self.env, self.home).strip()
        self.fixture_bytes = {}

    def command(self, operation):
        if self.name == "shtodo":
            return [self.binary] + {
                "version": ["--version"], "list": ["list"],
                "add": ["add", ADDED_TASK], "delete": ["delete", "1"],
            }[operation]
        if self.name == "taskwarrior":
            return [self.binary] + {
                "version": ["--version"], "list": ["bench", "limit:0"],
                "add": ["add", ADDED_TASK], "delete": ["1", "delete"],
            }[operation]
        if self.name == "taskbook":
            return [self.runtime, self.binary] + {
                "version": ["--version"], "list": [],
                "add": ["--task", ADDED_TASK], "delete": ["--delete", "1"],
            }[operation]
        if self.name == "topydo":
            return [self.binary, "-c", str(self.home / ".topydo"), "-C", "0"] + {
                "version": ["-v"], "list": ["ls"],
                "add": ["add", ADDED_TASK], "delete": ["del", "-f", "1"],
            }[operation]
        return [self.binary, "-d", str(self.home / "todo.cfg"), "-p", "-f"] + {
            "version": ["-V"], "list": ["list"],
            "add": ["add", ADDED_TASK], "delete": ["del", "1"],
        }[operation]

    def prepare(self, size):
        shutil.rmtree(self.home)
        (self.home / "tmp").mkdir(parents=True)
        if self.name == "shtodo":
            path = self.home / ".shtodo/global/tasks.json"
            path.parent.mkdir(parents=True)
            path.write_text(json.dumps({
                "schema_version": 1, "scope": {"kind": "global"},
                "next_task_id": size + 1, "next_deletion_sequence": 1,
                "tasks": [{"id": i, "text": task_text(i), "completed": False,
                           "deletion_sequence": None} for i in range(1, size + 1)],
            }, indent=2) + "\n")
        elif self.name == "taskwarrior":
            # Relocated release packages can retain a compiled-in theme path.
            # Taskwarrior also searches beside taskrc; use the bundled theme.
            theme = Path(self.binary).parent.parent / "share/doc/task/rc/default.theme"
            if theme.exists():
                shutil.copyfile(theme, self.home / "default.theme")
            self.env.update(TASKRC=str(self.home / ".taskrc"),
                            TASKDATA=str(self.home / "data"))
            (self.home / ".taskrc").write_text(
                f"data.location={self.home / 'data'}\n"
                "confirmation=off\ncolor=off\nverbose=nothing\n"
                "detection=off\ndefaultwidth=160\n"
                "report.bench.description=Benchmark list\n"
                "report.bench.columns=id,status,description\n"
                "report.bench.labels=ID,Status,Description\n"
                "report.bench.sort=id+\nreport.bench.filter=status:pending\n"
            )
            data = [{"uuid": str(uuid.UUID(int=i)), "status": "pending",
                     "entry": "20260101T000000Z", "description": task_text(i)}
                    for i in range(1, size + 1)]
            source = self.home / "import.json"
            source.write_text(json.dumps(data))
            if size:
                capture([self.binary, "import", str(source)], self.env, self.home, timeout=900)
            source.unlink()
        elif self.name == "taskbook":
            (self.home / ".taskbook.json").write_text(json.dumps({
                "taskbookDirectory": str(self.home), "displayCompleteTasks": True,
                "displayProgressOverview": True,
            }))
            storage = self.home / ".taskbook/storage/storage.json"
            storage.parent.mkdir(parents=True)
            storage.write_text(json.dumps({str(i): {
                "_id": i, "_date": "Thu Jan 01 2026", "_timestamp": 1767225600000,
                "description": task_text(i), "isStarred": False, "boards": ["My Board"],
                "_isTask": True, "isComplete": False, "inProgress": False, "priority": 1,
            } for i in range(1, size + 1)}, indent=4))
        elif self.name == "topydo":
            (self.home / "data").mkdir()
            (self.home / "data/todo.txt").write_text(
                "".join(task_text(i) + "\n" for i in range(1, size + 1))
            )
            (self.home / "data/done.txt").touch()
            (self.home / ".topydo").write_text(
                f"[topydo]\nfilename = {self.home / 'data/todo.txt'}\n"
                f"archive_filename = {self.home / 'data/done.txt'}\n"
                "colors = 0\n[ls]\nlist_limit = -1\n"
            )
        else:
            (self.home / "todo.cfg").write_text(
                f"export TODO_DIR={shlex.quote(str(self.home / 'data'))}\n"
                'export TODO_FILE="$TODO_DIR/todo.txt"\n'
                'export DONE_FILE="$TODO_DIR/done.txt"\n'
                'export REPORT_FILE="$TODO_DIR/report.txt"\n'
                'export TODO_ACTIONS_DIR="$TODO_DIR/actions"\n'
            )
            (self.home / "data").mkdir()
            (self.home / "data/todo.txt").write_text(
                "".join(task_text(i) + "\n" for i in range(1, size + 1))
            )
            (self.home / "data/done.txt").touch()
        # Initialize normal app caches and validate the full list before taking
        # a baseline. Setup/import/cache creation is deliberately not timed.
        self.verify("list", size)
        snapshot = self.snapshots / str(size)
        shutil.copytree(self.home, snapshot)
        self.fixture_bytes[str(size)] = sum(
            p.stat().st_size for p in snapshot.rglob("*") if p.is_file()
        )

    def reset(self, size):
        shutil.rmtree(self.home)
        shutil.copytree(self.snapshots / str(size), self.home)

    def accepted(self, operation, size):
        # Taskwarrior returns 1 for an empty report, including after deleting
        # the sole task. Its output must still pass the separate content check.
        if self.name == "taskwarrior" and operation == "list" and size == 0:
            return (0, 1)
        return (0,)

    def verify(self, operation, size, output=None):
        if operation == "version":
            return
        expected = Counter(task_text(i) for i in range(1, size + 1))
        if operation == "add":
            expected[ADDED_TASK] += 1
        elif operation == "delete":
            del expected[task_text(1)]
        if output is None:
            output = capture(self.command("list"), self.env, self.home,
                             self.accepted("list", sum(expected.values())))
        actual = Counter(TASK_PATTERN.findall(output))
        if actual != expected:
            raise RuntimeError(
                f"{self.name}/{operation}/{size}: task content mismatch; "
                f"missing={list((expected - actual).items())[:5]}, "
                f"unexpected={list((actual - expected).items())[:5]}"
            )
        # Check our recoverable-delete contract in addition to display output.
        if self.name == "shtodo" and operation == "delete":
            data = json.loads((self.home / ".shtodo/global/tasks.json").read_text())
            if data["tasks"][0]["deletion_sequence"] != 1 or len(data["tasks"]) != size:
                raise RuntimeError("shtodo delete did not preserve its tombstone")
        if self.name == "taskbook" and operation == "delete":
            archive = json.loads((self.home / ".taskbook/archive/archive.json").read_text())
            if Counter(t["description"] for t in archive.values()) != Counter([task_text(1)]):
                raise RuntimeError("Taskbook delete did not archive the expected task")
        if self.name == "topydo" and operation in ("add", "delete"):
            backup = json.loads(zlib.decompress((self.home / "data/.todo.bak").read_bytes()))
            entries = [record for key, record in backup.items() if key != "index"]
            initial = Counter(task_text(i) for i in range(1, size + 1))
            if len(entries) != 1 or Counter(TASK_PATTERN.findall("".join(entries[0][0]))) != initial:
                raise RuntimeError("topydo mutation did not back up the initial task set")


@dataclass
class Case:
    adapter: Adapter
    operation: str
    size: int
    samples: list = field(default_factory=list)


def timed(command, env, cwd, null, errors, accepted=(0,)):
    errors.seek(0)
    errors.truncate()
    started = time.perf_counter_ns()
    process = subprocess.Popen(command, env=env, cwd=cwd, stdin=null,
                               stdout=null, stderr=errors)
    # Blocking wait avoids the millisecond polling/rounding in wait(timeout=).
    code = process.wait()
    elapsed = (time.perf_counter_ns() - started) / 1_000_000
    if code not in accepted:
        errors.seek(0)
        raise RuntimeError(
            f"Timed command exited {code}: {shlex.join(command)}\n"
            + errors.read().decode(errors="replace")
        )
    return elapsed


def optional_command(command):
    try:
        return capture(command, cwd=ROOT).strip()
    except (OSError, RuntimeError, subprocess.TimeoutExpired):
        return "unavailable"


def machine_metadata():
    data = {"platform": platform.platform(), "architecture": platform.machine(),
            "python": platform.python_version(), "logical_cpus": os.cpu_count()}
    if sys.platform == "darwin":
        # Select only hardware fields, excluding serial numbers and device IDs.
        try:
            hardware = json.loads(capture([
                "system_profiler", "SPHardwareDataType", "-json",
            ]))["SPHardwareDataType"][0]
            data["hardware"] = {k: hardware[k] for k in (
                "machine_model", "chip_type", "physical_memory", "number_processors",
            ) if k in hardware}
        except (OSError, RuntimeError, KeyError, ValueError):
            data["hardware"] = "unavailable"
        data["power"] = optional_command(["pmset", "-g", "batt"]).splitlines()[0]
    return data


def markdown_report(report):
    meta = report["metadata"]
    lines = ["# CLI benchmark results", "", f"Run: {meta['started_at']}", "",
             f"Platform: {meta['machine']['platform']}", "",
             f"Repetitions: {meta['runs']}; warmups: {meta['warmups']}; "
             f"random seed: {meta['seed']}.", "",
             "Wall time includes process launch and exit. Warm filesystem caches; "
             "stdout goes to the null device. Fixtures are restored before every "
             "invocation outside the timer. Mutation results are checked outside "
             "the timer. p95 uses the nearest-rank sample percentile.", "",
             "Version is a startup proxy, not TUI first-frame latency. "
             "Delete and durability semantics differ.", "",
             "Scope: shtodo is deliberately a small local checklist. The other "
             "tools provide different capabilities, including organization, "
             "scheduling, automation, or interoperability. These selected CLI "
             "timings do not establish feature parity or overall product quality. "
             "See docs/benchmarks.md for the feature comparison and limitations.", "",
             "| App | Operation | Initial tasks | Median ms | p95 ms |",
             "| --- | --- | ---: | ---: | ---: |"]
    for row in report["results"]:
        stats = row["statistics"]
        lines.append(f"| {row['app']} | {row['operation']} | {row['size']} | "
                     f"{stats['median_ms']:.3f} | {stats['p95_ms']:.3f} |")
    lines += ["", "All individual samples, exact commands, executable hashes, "
              "configurations, and environment details are in the companion JSON.", ""]
    return "\n".join(lines)


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apps", nargs="+", choices=["shtodo", "taskwarrior", "todotxt", "taskbook", "topydo"],
                        default=["shtodo", "taskwarrior", "todotxt"])
    parser.add_argument("--sizes", nargs="+", type=int, default=[0, 10, 100, 1000, 10000])
    parser.add_argument("--runs", type=int, default=50)
    parser.add_argument("--warmups", type=int, default=5)
    parser.add_argument("--seed", type=int, default=20261004)
    parser.add_argument("--shtodo", default=str(ROOT / "target/release/shtodo"))
    parser.add_argument("--taskwarrior", default="task")
    parser.add_argument("--todotxt", default="todo.sh")
    parser.add_argument("--taskbook", default="tb")
    parser.add_argument("--node", default="node", help="Node runtime for Taskbook")
    parser.add_argument("--topydo", default="topydo")
    parser.add_argument("--output", type=Path, default=ROOT / "target/benchmarks/latest.json")
    parser.add_argument("--work-dir", type=Path, default=ROOT / "target/benchmarks",
                        help="Parent for a disposable directory; selects benchmark filesystem")
    args = parser.parse_args()
    if args.runs < 1 or args.warmups < 0 or any(n < 0 or n >= 99999999 for n in args.sizes):
        parser.error("runs must be positive, warmups nonnegative, and sizes in 0..99999998")
    if len(set(args.apps)) != len(args.apps) or len(set(args.sizes)) != len(args.sizes):
        parser.error("apps and sizes must not contain duplicates")
    return args


def main():
    args = parse_args()
    paths = {name: executable(getattr(args, name)) for name in args.apps}
    true = executable("true")
    args.work_dir.mkdir(parents=True, exist_ok=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    metadata = {
        "started_at": datetime.now(timezone.utc).isoformat(),
        "machine": machine_metadata(), "runs": args.runs, "warmups": args.warmups,
        "seed": args.seed, "sizes": args.sizes, "git_commit": optional_command(
            ["git", "rev-parse", "HEAD"]), "git_status": optional_command(
            ["git", "status", "--short"]), "rustc": optional_command(["rustc", "--version"]),
        "harness_sha256": digest(__file__), "cargo_lock_sha256": digest(ROOT / "Cargo.lock"),
        "cargo_toml": (ROOT / "Cargo.toml").read_text(),
        "argv": sys.argv, "work_dir": str(args.work_dir.resolve()),
        "method": "perf_counter_ns + Popen/wait, warm cache, reset every sample, null stdout",
        "bash_version": optional_command(["bash", "--version"]).splitlines()[0],
        "dependency_locks": {str(p.relative_to(ROOT)): digest(p)
                             for p in sorted((ROOT / "scripts/benchmark-deps").rglob("*"))
                             if p.is_file()},
    }
    rng = random.Random(args.seed)
    with tempfile.TemporaryDirectory(prefix="run-", dir=args.work_dir.resolve()) as work:
        adapters = [Adapter(name, path, Path(work), args.node) for name, path in paths.items()]
        cases = []
        for adapter in adapters:
            print(f"Preparing {adapter.name}: {adapter.version.splitlines()[0]}", flush=True)
            for size in sorted(set(args.sizes) | {0}):
                print(f"  {size:,} tasks", flush=True)
                adapter.prepare(size)
                if size in args.sizes:
                    cases.extend(Case(adapter, op, size) for op in ("list", "add", "delete")
                                 if op != "delete" or size > 0)
            cases.append(Case(adapter, "version", 0))

        # Preflight the exact commands with captured output. A fast failure,
        # truncated list, no-op mutation, or wrong deletion invalidates the run.
        for case in cases:
            a = case.adapter
            a.reset(case.size)
            output = capture(a.command(case.operation), a.env, a.home,
                             a.accepted(case.operation, case.size))
            a.verify(case.operation, case.size, output if case.operation == "list" else None)
        print(f"Validated {len(cases)} workloads; starting randomized rounds.", flush=True)
        baseline = []
        schedule = []
        with open(os.devnull, "r+b") as null, tempfile.TemporaryFile() as errors:
            for round_index in range(-args.warmups, args.runs):
                order = cases.copy() + [None]
                rng.shuffle(order)
                for case in order:
                    if case is None:
                        a = adapters[0]
                        elapsed = timed([true], a.env, a.home, null, errors)
                        if round_index >= 0:
                            baseline.append(elapsed)
                            schedule.append([round_index, "process-baseline", "true", 0])
                        continue
                    a = case.adapter
                    a.reset(case.size)
                    elapsed = timed(a.command(case.operation), a.env, a.home, null, errors,
                                    a.accepted(case.operation, case.size))
                    if case.operation in ("add", "delete"):
                        a.verify(case.operation, case.size)
                    if round_index >= 0:
                        case.samples.append(elapsed)
                        schedule.append([round_index, a.name, case.operation, case.size])
                if round_index == -1 or (round_index + 1) % 10 == 0:
                    print(f"Completed {'warmups' if round_index < 0 else str(round_index + 1) + '/' + str(args.runs) + ' rounds'}", flush=True)
        tools = {}
        for a in adapters:
            a.reset(0)
            tools[a.name] = {
                "version": a.version, "path": a.binary, "sha256": digest(a.binary),
                "env": a.env, "fixture_bytes": a.fixture_bytes,
                "config": {p.name: p.read_text() for p in (
                    a.home / ".taskrc", a.home / "todo.cfg", a.home / "default.theme",
                    a.home / ".taskbook.json", a.home / ".topydo")
                           if p.exists()},
            }
            if a.runtime:
                tools[a.name]["runtime"] = {
                    "path": a.runtime, "sha256": digest(a.runtime),
                    "version": capture([a.runtime, "--version"]).strip(),
                }
                lock = Path(a.binary).parent.parent.parent / "package-lock.json"
                if lock.exists():
                    tools[a.name]["installed_package_lock_sha256"] = digest(lock)
            if a.name == "topydo":
                shebang = Path(a.binary).read_text().splitlines()[0]
                if shebang.startswith("#!"):
                    runtime = json.loads(capture(shlex.split(shebang[2:]) + ["-c",
                        "import importlib.metadata as m, json, sys; "
                        "print(json.dumps({'path': sys.executable, 'version': sys.version, "
                        "'packages': {d.metadata['Name']: d.version for d in m.distributions()}}))"],
                        a.env, a.home))
                    runtime["sha256"] = digest(runtime["path"])
                    tools[a.name]["runtime"] = runtime
        results = [{"app": c.adapter.name, "operation": c.operation, "size": c.size,
                    "command": c.adapter.command(c.operation), "samples_ms": c.samples,
                    "statistics": summary(c.samples)} for c in cases]
        results.append({"app": "process-baseline", "operation": "true", "size": 0,
                        "command": [true], "samples_ms": baseline, "statistics": summary(baseline)})
        metadata["completed_at"] = datetime.now(timezone.utc).isoformat()
        report = {"schema_version": 1, "metadata": metadata, "tools": tools,
                  "results": results, "schedule": schedule}
        args.output.write_text(json.dumps(report, indent=2) + "\n")
        args.output.with_suffix(".md").write_text(markdown_report(report))
    print(f"Wrote {args.output} and {args.output.with_suffix('.md')}", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.TimeoutExpired) as error:
        sys.exit(str(error))
