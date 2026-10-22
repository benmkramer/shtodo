"""Compare two shtodo release binaries with paired, isolated CLI/TUI workloads."""

import argparse
import contextlib
import copy
from datetime import date, datetime, timedelta, timezone
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


def tagged_text(index):
    # Every other task carries todo.txt tokens: a priority marker and two tags.
    if index % 2:
        return benchmark.task_text(index)
    return f"({'ABCD'[index // 2 % 4]}) {benchmark.task_text(index)} +proj{index % 5} @ctx{index % 3}"


def tagged_fixture(size):
    return dict(schema_version=1, scope=dict(kind="global"), next_task_id=size + 1,
                next_deletion_sequence=1, tasks=[dict(
                    id=index, text=tagged_text(index), completed=index % 4 == 0,
                    deletion_sequence=None) for index in range(1, size + 1)])


# The isolated environment pins TZ=UTC, so the binaries' "today" is the UTC date.
RUN_DATE = datetime.now(timezone.utc).date()


def dated_text(index):
    # Every other task carries a priority, two tags, and a due date from 30 days before to 30
    # days after the run date; with every third task done, about a sixth are open and overdue.
    if index % 2:
        return benchmark.task_text(index)
    return f"{tagged_text(index)} due:{RUN_DATE + timedelta(days=index // 2 % 61 - 30)}"


def dated_fixture(size):
    return dict(schema_version=1, scope=dict(kind="global"), next_task_id=size + 1,
                next_deletion_sequence=1, tasks=[dict(
                    id=index, text=dated_text(index), completed=index % 3 == 0,
                    deletion_sequence=None) for index in range(1, size + 1)])


MONTH_START = RUN_DATE.replace(day=1)
MONTH_END = (MONTH_START + timedelta(days=31)).replace(day=1) - timedelta(days=1)

# Read-only operations; the info cases need filter flags, so they run for the current binary only.
READ_OPERATIONS = {
    "list": ["list"], "list_json": ["list", "--json"],
    "list_tag": ["list", "--tag", "+proj1"], "list_priority": ["list", "--priority", "A-C"],
    "list_search": ["list", "--search", "x"], "list_sort": ["list", "--sort", "priority"],
    "list_json_sort": ["list", "--json", "--sort", "priority"],
    "list_due_today": ["list", "--due", "today"],
    "list_due_date": ["list", "--due", str(RUN_DATE)],
    "list_overdue": ["list", "--open", "--due", "overdue"],
    "list_due_month": ["list", "--due", f"{MONTH_START}..{MONTH_END}"],
    "list_sort_due": ["list", "--sort", "due"],
    "list_json_sort_due": ["list", "--json", "--sort", "due"],
}
INFO_OPERATIONS = {"list_tag": (1000, 10000), "list_priority": (1000, 10000),
                   "list_search": (1000, 10000), "list_sort": (10000,),
                   "list_json_sort": (10000,)}
# Current-only due cases on the dated fixture: two gated first-clock-read pairs, then info.
DUE_OPERATIONS = {"list_due_today": (1000, 10000), "list_due_date": (1000, 10000),
                  "list_overdue": (1000, 10000), "list_due_month": (1000, 10000),
                  "list_sort_due": (1000, 10000), "list_json_sort_due": (1000, 10000)}
ADDED_DUE_TASK = f"{benchmark.ADDED_TASK} due:today"


def due(text):
    """The first valid `due:YYYY-MM-DD` token, as the shell contract defines it."""
    for token in text.split():
        if not token.startswith("due:"):
            continue
        value = token[4:].rstrip(".,;:!?")
        if len(value) == 10 and value[4] == value[7] == "-" and (value[:4] + value[5:7] + value[8:]).isascii() \
                and (value[:4] + value[5:7] + value[8:]).isdigit():
            try:
                return date.fromisoformat(value)
            except ValueError:
                continue
    return None


def expected_rows(value, operation):
    """Python model of the derived-token list contract for the benchmark fixtures."""
    tasks = [task for task in value["tasks"] if task["deletion_sequence"] is None]

    def priority(text):
        return text[1] if len(text) > 3 and text[0] == "(" and text[2:4] == ") " and "A" <= text[1] <= "Z" else None

    def tags(text):
        found = []
        for token in text.split():
            tag = token.rstrip(".,;:!?")
            if token[0] in "+@" and len(tag) > 1 and tag not in found:
                found.append(tag)
        return found

    def priority_key(task):
        return (priority(task["text"]) is None, priority(task["text"]) or "")

    if operation == "list_tag":
        tasks = [task for task in tasks if "+proj1" in tags(task["text"])]
    elif operation in ("list_due_today", "list_due_date"):
        tasks = [task for task in tasks if due(task["text"]) == RUN_DATE]
    elif operation == "list_overdue":
        tasks = [task for task in tasks
                 if not task["completed"] and (due(task["text"]) or RUN_DATE) < RUN_DATE]
    elif operation == "list_due_month":
        tasks = [task for task in tasks if MONTH_START <= (due(task["text"]) or date.min) <= MONTH_END]
    elif operation in ("list_sort_due", "list_json_sort_due"):
        tasks = sorted(tasks, key=lambda task: (due(task["text"]) is None, due(task["text"]) or date.min,
                                                priority_key(task)))
    elif operation == "list_priority":
        tasks = [task for task in tasks if priority(task["text"]) in ("A", "B", "C")]
    elif operation == "list_search":
        tasks = [task for task in tasks if "x" in task["text"].lower()]
    elif operation in ("list_sort", "list_json_sort"):
        tasks = sorted(tasks, key=priority_key)
    if operation.startswith("list_json"):
        return [dict(id=task["id"], text=task["text"], state="done" if task["completed"] else "open",
                     deleted=False, priority=priority(task["text"]), tags=tags(task["text"]),
                     due=str(due(task["text"])) if due(task["text"]) else None)
                for task in tasks]
    return "".join(f"{task['id']}  {'done' if task['completed'] else 'open'}  {task['text']}\n"
                   for task in tasks)


def cli_case(value, kind, operation):
    expected = copy.deepcopy(value)
    tasks = expected["tasks"]
    active_id = 1 if kind == "plain" else 2
    commands = READ_OPERATIONS | {
        "add": ["add", benchmark.ADDED_TASK],
        "add_due_today": ["add", ADDED_DUE_TASK],
        "delete": ["delete", str(active_id)], "done": ["done", "2"],
        "reopen": ["reopen", "3"], "edit": ["edit", "2", "Changed café 東京"],
        "restore": ["restore", "1"], "noop_done": ["done", "3"],
        "noop_delete": ["delete", "1"], "version": ["--version"],
    }
    if operation in ("add", "add_due_today"):
        text = benchmark.ADDED_TASK if operation == "add" else f"{benchmark.ADDED_TASK} due:{RUN_DATE}"
        tasks.append(dict(id=expected["next_task_id"], text=text,
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
                for kind in args.kinds:
                    if kind == "plain":
                        value = plain_fixture(size)
                    elif kind == "tagged":
                        value = tagged_fixture(size)
                    elif kind == "dated":
                        value = dated_fixture(size)
                    else:
                        value = fixtures[size]
                    data = (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode()
                    operations = ["list", "list_json", "add"]
                    if size and (kind == "plain" or size >= 2):
                        operations.append("delete")
                    if kind == "mixed" and size >= 3:
                        operations += ["done", "reopen", "edit", "restore", "noop_done", "noop_delete"]
                    if kind == "tagged" and label == "current":
                        operations += [operation for operation, sizes in INFO_OPERATIONS.items()
                                       if size in sizes]
                    if kind == "dated" and label == "current":
                        operations += [operation for operation, sizes in DUE_OPERATIONS.items()
                                       if size in sizes]
                    if kind == "mixed" and label == "current":
                        operations.append("add_due_today")
                    if args.operations:
                        operations = [operation for operation in operations if operation in args.operations]
                    for operation in operations:
                        command, expected = cli_case(value, kind, operation)
                        cases.append(dict(label=label, binary=binary, home=home, env=env,
                                          path=path, fixture=data, expected=expected,
                                          operation=operation, kind=kind, records=size,
                                          command=[binary, *command], samples=[]))
            value = plain_fixture(0)
            if args.operations and "version" not in args.operations:
                continue
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
            if operation in READ_OPERATIONS or operation == "version":
                assert case["path"].read_bytes() == case["fixture"]
                return
            actual = json.loads(case["path"].read_text())
            assert actual == case["expected"], (case["label"], case["kind"], operation, case["records"])
            if operation.startswith("noop_"):
                assert case["path"].read_bytes() == case["fixture"]
                assert case["path"].stat().st_mtime_ns == before

        for case in cases:
            before = reset(case)
            output = benchmark.capture(case["command"], case["env"], case["home"])
            if case["operation"] in READ_OPERATIONS:
                expected = expected_rows(case["expected"], case["operation"])
                if case["operation"].startswith("list_json"):
                    output = json.loads(output)["tasks"]
                    if case["label"] == "baseline":
                        # The control predates the derived due field.
                        expected = [{key: value for key, value in task.items()
                                     if key != "due"} for task in expected]
                assert output == expected, (case["label"], case["kind"], case["operation"], case["records"])
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


def token_tui_fixture(home, size, builder):
    directory = home / ".shtodo/global"
    directory.mkdir(parents=True)
    path = directory / "tasks.json"
    path.write_text(json.dumps(builder(size), indent=2, ensure_ascii=False) + "\n")
    (directory / "tasks.lock").touch()
    return path.stat().st_size


# TUI cases per fixture: (case label, binary label, keys sent before timing, operations).
# The sorted cases and Filter mode need this branch's binary, so they run for it only.
TUI_CASES = {
    "mixed": [("baseline", "baseline", b"", ("toggle_to_frame", "help_to_frame", "search_keystroke")),
              ("current", "current", b"", ("toggle_to_frame", "help_to_frame", "search_keystroke")),
              ("current_priority", "current", b"s", ("toggle_to_frame",))],
    "tagged": [("baseline", "baseline", b"", ("toggle_to_frame", "search_keystroke")),
               ("current", "current", b"", ("toggle_to_frame", "search_keystroke", "filter_keystroke")),
               ("current_priority", "current", b"s", ("toggle_to_frame",))],
    # Due order on `s s` is gated against Priority order on the same binary.
    "dated": [("baseline", "baseline", b"", ("toggle_to_frame", "search_keystroke")),
              ("current", "current", b"", ("toggle_to_frame", "search_keystroke", "filter_keystroke")),
              ("current_priority", "current", b"s", ("toggle_to_frame",)),
              ("current_due", "current", b"ss", ("toggle_to_frame",))],
}
# The status text each sort setup waits for before timing.
SETUP_STATUS = {b"s": "by priority", b"ss": "by due"}
TUI_FIXTURES = {"tagged": tagged_fixture, "dated": dated_fixture}
# On the dated fixture both keystrokes end on the same rows, the tasks due on the run date: the
# search finds the date's literal token, and the filter's final `y` completes `due:today`.
DATED_SEARCH = f"/due:{RUN_DATE}"
# Keystroke cases: untimed keys that open the editor, then one timed character, then Esc.
KEYSTROKES = {
    ("mixed", "search_keystroke"): (b"/Tas", b"k", "/Task", "SEARCH"),
    ("tagged", "search_keystroke"): (b"/+", b"p", "/+p", "SEARCH"),
    ("tagged", "filter_keystroke"): (b"f+", b"p", "f:+p", "FILTER"),
    ("dated", "search_keystroke"): (DATED_SEARCH[:-1].encode(), DATED_SEARCH[-1].encode(),
                                    DATED_SEARCH, "SEARCH"),
    ("dated", "filter_keystroke"): (b"fdue:toda", b"y", "f:due:today", "FILTER"),
}


def footer(value):
    return "".join(value.screen[-1])


def measure_tui(args, binaries, rng):
    results = []
    with tempfile.TemporaryDirectory(prefix="shtodo-paired-tui-") as directory:
        root = Path(directory)
        for kind in args.kinds:
            for size in [size for size in args.sizes if size >= 3]:
                with contextlib.ExitStack() as stack:
                    cases = []
                    specs = list(TUI_CASES[kind])
                    rng.shuffle(specs)
                    for label, binary, setup, operations in specs:
                        home = root / f"{kind}-{size}-{label}"
                        if kind in TUI_FIXTURES:
                            size_bytes = token_tui_fixture(home, size, TUI_FIXTURES[kind])
                            # Task 1 is selected first; sorting keeps it selected, now further down.
                            target, marker_text = 1, "task 00000001"
                        else:
                            size_bytes = fixture(home, size)
                            target, marker_text = 2, "Task 2 café"
                        with patch.object(test_concurrency, "BINARY", Path(binaries[binary])):
                            tui = stack.enter_context(test_concurrency.Tui(home))
                        wait_screen(tui, lambda value: footer(value).startswith("NORMAL"))
                        if setup:
                            tui.send(setup)
                            wait_screen(tui, lambda value, status=SETUP_STATUS[setup]: status in "".join(
                                value.screen[1]))
                        value = json.loads((home / ".shtodo/global/tasks.json").read_text())
                        completed = value["tasks"][target - 1]["completed"]
                        cases.append(dict(label=label, binary=binary, home=home, tui=tui, kind=kind,
                                          records=size, snapshot_bytes=size_bytes, target=target,
                                          marker_text=marker_text, completed=completed,
                                          samples={operation: [] for operation in operations}))
                    for round_index in range(-args.warmups, args.runs):
                        order = [(case, operation) for case in cases for operation in case["samples"]]
                        rng.shuffle(order)
                        for case, operation in order:
                            tui = case["tui"]
                            if operation == "toggle_to_frame":
                                case["completed"] = not case["completed"]
                                marker = "✓" if case["completed"] else "○"
                                predicate = lambda value, text=case["marker_text"], marker=marker: any(
                                    marker in "".join(row) and text in "".join(row)
                                    for row in value.screen)
                                key = b" "
                            elif operation == "help_to_frame":
                                predicate = lambda value: footer(value).startswith("HELP")
                                key = b"?"
                            else:
                                opening, key, status, mode = KEYSTROKES[(case["kind"], operation)]
                                tui.send(opening)
                                wait_screen(tui, lambda value, mode=mode, status=status[:-1]: footer(
                                    value).startswith(mode) and "".join(value.screen[1]).rstrip().endswith(status))
                                predicate = lambda value, status=status: status in "".join(value.screen[1])
                            started = time.perf_counter_ns()
                            tui.send(key)
                            wait_screen(tui, predicate)
                            elapsed = (time.perf_counter_ns() - started) / 1_000_000
                            if operation == "toggle_to_frame":
                                value = json.loads((case["home"] / ".shtodo/global/tasks.json").read_text())
                                assert value["tasks"][case["target"] - 1]["completed"] == case["completed"]
                                assert len(value["tasks"]) == size
                            else:
                                tui.send(b"?" if operation == "help_to_frame" else b"\x1b")
                                wait_screen(tui, lambda value: footer(value).startswith("NORMAL"))
                            if round_index >= 0:
                                case["samples"][operation].append(elapsed)
                    for case in cases:
                        for operation, samples in case["samples"].items():
                            results.append(dict(binary=case["label"], fixture=kind, records=size,
                                                snapshot_bytes=case["snapshot_bytes"], operation=operation,
                                                samples_ms=samples, statistics=benchmark.summary(samples)))
                print(f"TUI {kind} {size} records complete", flush=True)
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
    parser.add_argument("--kinds", nargs="+", choices=("plain", "mixed", "tagged", "dated"),
                        default=["plain", "mixed"])
    parser.add_argument("--operations", nargs="+",
                        help="CLI operations to time (default: all for each fixture)")
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
