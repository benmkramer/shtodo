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
