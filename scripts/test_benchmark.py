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
