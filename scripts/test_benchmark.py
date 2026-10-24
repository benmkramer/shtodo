"""Guard benchmark isolation and correctness using locally installed tools."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import benchmark


class BenchmarkTests(unittest.TestCase):
    def test_child_environment_does_not_inherit_storage_or_shell_overrides(self):
        with patch.dict(os.environ, {
            "TASKDATA": "/real/tasks", "TODO_FILE": "/real/todo.txt",
            "BASH_ENV": "/real/startup.sh", "XDG_CONFIG_HOME": "/real/config",
        }):
            env = benchmark.isolated_env(Path("/disposable"))
        self.assertFalse({"TASKDATA", "TODO_FILE", "BASH_ENV"} & env.keys())
        self.assertEqual(env["XDG_CONFIG_HOME"], "/disposable/.config")

    def test_failing_command_is_not_recorded_as_a_fast_success(self):
        with open(os.devnull, "r+b") as null, tempfile.TemporaryFile() as errors:
            with self.assertRaisesRegex(RuntimeError, "exited 7"):
                benchmark.timed([sys.executable, "-c", "raise SystemExit(7)"],
                                None, None, null, errors)

    def test_invalid_sample_count_is_rejected_before_running(self):
        result = subprocess.run(
            [sys.executable, str(Path(benchmark.__file__)), "--runs", "0"],
            capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 2)

    def test_reset_repeats_real_mutations_without_growing_or_reusing_deleted_tasks(self):
        binary = benchmark.executable(str(benchmark.ROOT / "target/release/shtodo"))
        with tempfile.TemporaryDirectory() as root:
            app = benchmark.Adapter("shtodo", binary, Path(root))
            app.prepare(3)
            for operation in ("add", "delete", "add", "delete"):
                app.reset(3)
                app.verify("list", 3)
                output = benchmark.capture(app.command(operation), app.env, app.home)
                self.assertNotIn("Already deleted", output)
                app.verify(operation, 3)

    def test_correctness_check_rejects_truncation_and_duplicates(self):
        binary = benchmark.executable(str(benchmark.ROOT / "target/release/shtodo"))
        with tempfile.TemporaryDirectory() as root:
            app = benchmark.Adapter("shtodo", binary, Path(root))
            app.prepare(2)
            for output in (benchmark.task_text(1),
                           benchmark.task_text(1) + "\n" + benchmark.task_text(1)):
                with self.assertRaisesRegex(RuntimeError, "content mismatch"):
                    app.verify("list", 2, output)

    def test_dated_fixture_mixes_dates_around_the_run_date(self):
        import benchmark_concurrency_comparison as comparison

        value = comparison.dated_fixture(600)
        tasks = value["tasks"]
        dates = [comparison.due(task["text"]) for task in tasks]
        self.assertEqual(sum(day is not None for day in dates), 300)
        self.assertTrue(all(day is None for day in dates[::2]), "odd IDs have no date")
        offsets = {(day - comparison.RUN_DATE).days for day in dates if day}
        self.assertEqual(offsets, set(range(-30, 31)))
        overdue = comparison.expected_rows(value, "list_overdue").count("\n")
        self.assertTrue(0.14 < overdue / len(tasks) < 0.19, overdue)
        self.assertTrue(all(comparison.due(task["text"]) for task in tasks[1::2]))

    def test_due_model_matches_the_token_grammar_and_new_operations(self):
        import benchmark_concurrency_comparison as comparison

        day = comparison.RUN_DATE
        self.assertEqual(comparison.due("(A) x due:2026-11-01. +p"), comparison.date(2026, 11, 1))
        for text in ("x due:2026-02-30", "x Due:2026-11-01", "x overdue:2026-11-01",
                     "x due:2026-1-05", "x due:today", "x due:", "x due:２０２６-11-01"):
            self.assertIsNone(comparison.due(text), text)
        self.assertEqual(comparison.due("x due:bad due:2026-11-02 due:2026-11-03"),
                         comparison.date(2026, 11, 2))
        value = dict(tasks=[
            dict(id=1, text="(B) late due:2000-01-01", completed=False, deletion_sequence=None),
            dict(id=2, text=f"(A) now due:{day}", completed=False, deletion_sequence=None),
            dict(id=3, text="plain", completed=False, deletion_sequence=None),
            dict(id=4, text="(A) done late due:2000-01-01", completed=True, deletion_sequence=None),
            dict(id=5, text=f"gone due:{day}", completed=False, deletion_sequence=1),
        ])
        ids = lambda output: [int(line.split()[0]) for line in output.splitlines()]
        self.assertEqual(ids(comparison.expected_rows(value, "list_due_today")), [2])
        self.assertEqual(ids(comparison.expected_rows(value, "list_due_date")), [2])
        self.assertEqual(ids(comparison.expected_rows(value, "list_overdue")), [1])
        self.assertEqual(ids(comparison.expected_rows(value, "list_sort_due")), [4, 1, 2, 3])
        rows = comparison.expected_rows(value, "list_json_sort_due")
        self.assertEqual([row["due"] for row in rows], ["2000-01-01", "2000-01-01", str(day), None])
        command, expected = comparison.cli_case(value | dict(next_task_id=6), "mixed", "add_due_today")
        self.assertEqual(command, ["add", f"{benchmark.ADDED_TASK} due:today"])
        self.assertEqual(expected["tasks"][-1]["text"], f"{benchmark.ADDED_TASK} due:{day}")

    def test_dated_tui_keystrokes_end_on_the_rows_due_on_the_run_date(self):
        import benchmark_concurrency_comparison as comparison

        opening, key, status, mode = comparison.KEYSTROKES[("dated", "search_keystroke")]
        self.assertEqual((opening + key).decode(), status)
        self.assertEqual((status, mode), (f"/due:{comparison.RUN_DATE}", "SEARCH"))
        opening, key, status, mode = comparison.KEYSTROKES[("dated", "filter_keystroke")]
        self.assertEqual((opening, key, status, mode), (b"fdue:toda", b"y", "f:due:today", "FILTER"))
        value = comparison.dated_fixture(600)
        searched = [task["id"] for task in value["tasks"]
                    if comparison.DATED_SEARCH[1:] in task["text"].lower()]
        due_today = [int(line.split()[0])
                     for line in comparison.expected_rows(value, "list_due_today").splitlines()]
        self.assertEqual(searched, due_today)
        self.assertTrue(searched)
        cases = {label: setup for label, _, setup, _ in comparison.TUI_CASES["dated"]}
        self.assertEqual(comparison.SETUP_STATUS[cases["current_due"]], "by due")
        self.assertEqual(comparison.SETUP_STATUS[cases["current_priority"]], "by priority")

    def test_nested_fixture_regroups_the_mixed_records_under_the_invariants(self):
        import benchmark_concurrency_comparison as comparison

        for size in (0, 100, 1003, 10000):
            mixed = comparison.mixed_fixture(size)
            nested = comparison.nested_fixture(size)
            flat = comparison.nested_flat_fixture(size)
            tasks = nested["tasks"]
            self.assertEqual(nested["schema_version"], 2 if size else 1)
            self.assertEqual(flat["schema_version"], 1)
            self.assertEqual([(task["id"], task["text"], task["completed"]) for task in tasks],
                             [(task["id"], task["text"], task["completed"]) for task in mixed["tasks"]])
            self.assertEqual(sum(task["deletion_sequence"] is not None for task in tasks),
                             sum(task["deletion_sequence"] is not None for task in mixed["tasks"]))
            # The flat twin drops the links and numbers its tombstones uniquely, as schema 1 needs.
            self.assertEqual([(task["id"], task["text"], task["completed"], task["deletion_sequence"] is None)
                              for task in flat["tasks"]],
                             [(task["id"], task["text"], task["completed"], task["deletion_sequence"] is None)
                              for task in tasks])
            self.assertFalse(any("parent_id" in task for task in flat["tasks"]))
            flat_sequences = [task["deletion_sequence"] for task in flat["tasks"] if task["deletion_sequence"]]
            self.assertEqual(flat_sequences, list(range(1, len(flat_sequences) + 1)))
            self.assertEqual(flat["next_deletion_sequence"], len(flat_sequences) + 1)
            by_id = {task["id"]: task for task in tasks}
            sequences = [task["deletion_sequence"] for task in tasks if task["deletion_sequence"]]
            self.assertTrue(all(sequence < nested["next_deletion_sequence"] for sequence in sequences))
            head = None
            whole = singles = 0
            for index, task in enumerate(tasks):
                # Groups of one top-level task and four subtasks, in storage order.
                self.assertEqual(task.get("parent_id"), None if index % 5 == 0 else tasks[index - index % 5]["id"])
                if "parent_id" not in task:
                    head = task
                    continue
                self.assertEqual(task["parent_id"], head["id"])
                if head["deletion_sequence"] is not None:
                    self.assertEqual(task["deletion_sequence"], head["deletion_sequence"])
                elif task["deletion_sequence"] is not None:
                    singles += 1
            whole = sum(1 for task in tasks if "parent_id" not in task and task["deletion_sequence"])
            # A subtask may share only its parent's sequence; every other sequence is unique.
            shared = [task["deletion_sequence"] for task in tasks
                      if task.get("parent_id") and by_id[task["parent_id"]]["deletion_sequence"]]
            self.assertEqual(len(sequences) - len(shared), len(set(sequences)))
            if size >= 100:
                self.assertGreater(whole, 0)
                self.assertGreater(singles, 0)
        binary = benchmark.executable(str(benchmark.ROOT / "target/release/shtodo"))
        with tempfile.TemporaryDirectory() as root:
            for value in (comparison.nested_fixture(1003), comparison.nested_flat_fixture(1003)):
                output = self.run_on_fixture(binary, Path(root), value, ["list"])
                self.assertEqual(output, comparison.expected_rows(value, "list"))

    def run_on_fixture(self, binary, root, value, command):
        import json

        home = root / "home"
        directory = home / ".shtodo/global"
        directory.mkdir(parents=True, exist_ok=True)
        (directory / "tasks.json").write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n")
        (directory / "tmp").mkdir(exist_ok=True)
        return benchmark.capture([binary, *command], benchmark.isolated_env(home), home)

    def test_tree_model_matches_the_binary_for_filters_sorts_and_json(self):
        import json
        import benchmark_concurrency_comparison as comparison

        def task(id, text, parent=None, deleted=None, completed=False):
            value = dict(id=id, text=text, completed=completed, deletion_sequence=deleted)
            return value | (dict(parent_id=parent) if parent else {})

        value = dict(schema_version=2, scope=dict(kind="global"), next_task_id=9,
                     next_deletion_sequence=3, tasks=[
                         task(1, "(C) one"), task(2, "(A) one a", 1), task(3, "(B) one b", 1, completed=True),
                         task(4, "(B) four due:2000-01-01"), task(5, "four a", 4, deleted=1),
                         task(6, "(A) six", deleted=2), task(7, "six a", 6, deleted=2), task(8, "(Z) eight")])
        rows = lambda operation: comparison.expected_rows(value, operation)
        self.assertEqual(rows("list"), "1  open  (C) one\n2  open    (A) one a\n3  done    (B) one b\n"
                         "4  open  (B) four due:2000-01-01\n8  open  (Z) eight\n")
        ids = lambda output: [int(line.split()[0]) for line in output.splitlines()]
        self.assertEqual(ids(rows("list_sort")), [4, 1, 2, 3, 8])
        self.assertEqual(ids(rows("list_sort_due")), [4, 1, 2, 3, 8])
        self.assertEqual(ids(rows("list_roots")), [1, 4, 8])
        self.assertEqual(rows("list_parent"), "")
        self.assertEqual([(row["id"], row["parent_id"]) for row in rows("list_json_sort_due")],
                         [(4, None), (1, None), (2, 1), (3, 1), (8, None)])
        binary = benchmark.executable(str(benchmark.ROOT / "target/release/shtodo"))
        with tempfile.TemporaryDirectory() as root:
            for operation in ("list", "list_sort", "list_sort_due", "list_roots", "list_json",
                              "list_json_sort_due"):
                output = self.run_on_fixture(binary, Path(root), value, comparison.READ_OPERATIONS[operation])
                if operation.startswith("list_json"):
                    output = json.loads(output)["tasks"]
                self.assertEqual(output, rows(operation), operation)
            nested = comparison.nested_fixture(1003)
            self.assertEqual(ids(comparison.expected_rows(nested, "list_parent")), [7, 8, 9, 10])
            output = self.run_on_fixture(binary, Path(root), nested, comparison.READ_OPERATIONS["list_parent"])
            self.assertEqual(output, comparison.expected_rows(nested, "list_parent"))

    def test_nested_mutation_models_match_the_binary(self):
        import json
        import benchmark_concurrency_comparison as comparison

        binary = benchmark.executable(str(benchmark.ROOT / "target/release/shtodo"))
        nested = comparison.nested_fixture(1003)
        with tempfile.TemporaryDirectory() as root:
            for operation in ("add_parent", "parent_none"):
                command, expected = comparison.cli_case(nested, "nested", operation)
                self.run_on_fixture(binary, Path(root), nested, command)
                actual = json.loads((Path(root) / "home/.shtodo/global/tasks.json").read_text())
                self.assertEqual(actual, expected, operation)
                self.assertEqual(actual["schema_version"], 2)
        ids = [task["id"] for task in comparison.cli_case(nested, "nested", "add_parent")[1]["tasks"]]
        self.assertEqual(ids[5:12], [6, 7, 8, 9, 10, 1004, 11])
        ids = [task["id"] for task in comparison.cli_case(nested, "nested", "parent_none")[1]["tasks"]]
        self.assertEqual(ids[5:11], [6, 7, 9, 10, 8, 11])

    @unittest.skipUnless((benchmark.ROOT / "target/benchmark-tools/taskbook/node_modules/.bin/tb").exists(),
                         "optional Taskbook installation unavailable")
    def test_taskbook_delete_can_be_restored_and_reset_clears_archive(self):
        binary = benchmark.executable(str(
            benchmark.ROOT / "target/benchmark-tools/taskbook/node_modules/.bin/tb"))
        with tempfile.TemporaryDirectory() as root:
            app = benchmark.Adapter("taskbook", binary, Path(root))
            app.prepare(3)
            for _ in range(2):
                app.reset(3)
                benchmark.capture(app.command("delete"), app.env, app.home)
                app.verify("delete", 3)
                benchmark.capture([app.runtime, binary, "--restore", "1"], app.env, app.home)
                app.verify("list", 3)

    @unittest.skipUnless((benchmark.ROOT / "target/benchmark-tools/topydo-venv/bin/topydo").exists(),
                         "optional topydo installation unavailable")
    def test_topydo_mutations_can_be_reverted_with_default_backups(self):
        binary = benchmark.executable(str(
            benchmark.ROOT / "target/benchmark-tools/topydo-venv/bin/topydo"))
        with tempfile.TemporaryDirectory() as root:
            app = benchmark.Adapter("topydo", binary, Path(root))
            app.prepare(3)
            for operation in ("add", "delete"):
                app.reset(3)
                benchmark.capture(app.command(operation), app.env, app.home)
                app.verify(operation, 3)
                benchmark.capture(app.command("list")[:-1] + ["revert"], app.env, app.home)
                app.verify("list", 3)


if __name__ == "__main__":
    unittest.main()
