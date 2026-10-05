"""Unix PTY and process regressions using only isolated task storage."""
import codecs
import re
import unicodedata
import concurrent.futures
import json
import os
from pathlib import Path
import select
import signal
import struct
import subprocess
import tempfile
import threading
import time
import unittest

if os.name == "posix":
    import fcntl
    import pty
    import termios

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("SHTODO_BINARY", ROOT / "target/release/shtodo")).resolve()


def cli(home, *args, cwd=None):
    env = dict(os.environ, HOME=str(home), USERPROFILE=str(home), TERM="xterm-256color")
    return subprocess.run([str(BINARY), *args], env=env, cwd=cwd, capture_output=True, timeout=5)


class Tui:
    def __init__(self, home, cwd=None, local=False):
        self.master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 0, 0))
        self.output = bytearray()
        self.screen = [[" "] * 100 for _ in range(24)]
        self.row = self.col = 0
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.pending = ""
        self.cpu_seconds = None
        self.started = time.monotonic()

        def attach():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

        try:
            self.process = subprocess.Popen(
                [str(BINARY), *( ["--local"] if local else [])],
                stdin=slave, stdout=slave, stderr=slave, cwd=cwd,
                env=dict(os.environ, HOME=str(home), USERPROFILE=str(home), TERM="xterm-256color"),
                preexec_fn=attach,
            )
        except BaseException:
            os.close(self.master)
            raise
        finally:
            os.close(slave)
        try:
            self.wait_for(b"shtodo")
        except BaseException:
            self.close()
            raise

    def send(self, data):
        os.write(self.master, data)

    def drain(self, seconds):
        deadline = time.monotonic() + seconds
        start = len(self.output)
        while time.monotonic() < deadline:
            if select.select([self.master], [], [], max(0, min(0.1, deadline - time.monotonic())))[0]:
                try:
                    data = os.read(self.master, 65536)
                except OSError:
                    break
                if not data:
                    break
                self.output.extend(data)
                self.feed(data)
        return bytes(self.output[start:])

    def feed(self, data):
        # Ratatui sends cell diffs, so assertions reconstruct screen coordinates.
        self.pending += self.decoder.decode(data)
        while self.pending:
            if self.pending[0] == "\x1b":
                if len(self.pending) == 1:
                    return
                if self.pending[1] == "[":
                    match = re.match(r"\x1b\[([0-?]*)([ -/]*)([@-~])", self.pending)
                    if not match:
                        return
                    raw, _, command = match.groups()
                    self.pending = self.pending[match.end():]
                    if raw.startswith("?"):
                        continue
                    values = [int(item) if item else 0 for item in raw.split(";")]
                    amount = values[0] or 1
                    if command in ("H", "f"):
                        self.row = max(0, (values[0] or 1) - 1)
                        self.col = max(0, (values[1] if len(values) > 1 and values[1] else 1) - 1)
                    elif command == "G":
                        self.col = amount - 1
                    elif command == "A":
                        self.row = max(0, self.row - amount)
                    elif command == "B":
                        self.row += amount
                    elif command == "C":
                        self.col += amount
                    elif command == "D":
                        self.col = max(0, self.col - amount)
                    elif command == "J" and values[0] == 2:
                        self.screen = [[" "] * 100 for _ in range(24)]
                    elif command == "K" and self.row < 24:
                        start, end = (0, 100) if values[0] == 2 else ((0, self.col + 1) if values[0] == 1 else (self.col, 100))
                        self.screen[self.row][start:end] = [" "] * (end - start)
                    continue
                self.pending = self.pending[2:]
                continue
            character, self.pending = self.pending[0], self.pending[1:]
            if character == "\r":
                self.col = 0
            elif character == "\n":
                self.row += 1
            elif character >= " " and self.row < 24:
                if self.col >= 100:
                    self.col = 0
                    self.row += 1
                if self.row < 24:
                    width = 0 if unicodedata.combining(character) else (2 if unicodedata.east_asian_width(character) in "WF" else 1)
                    if width == 0 and self.col:
                        self.screen[self.row][self.col - 1] += character
                    else:
                        self.screen[self.row][self.col] = character
                        if width == 2 and self.col + 1 < 100:
                            self.screen[self.row][self.col + 1] = ""
                        self.col += width

    def text(self):
        return "\n".join("".join(row) for row in self.screen)

    def wait_for(self, text, timeout=4):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if text.decode("utf-8") in self.text():
                return
            self.drain(min(0.05, deadline - time.monotonic()))
        raise AssertionError(f"TUI did not show {text!r}: {bytes(self.output[-2500:])!r}")

    def wait_absent(self, text, timeout=4):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if text.decode("utf-8") not in self.text():
                return
            self.drain(min(0.05, deadline - time.monotonic()))
        raise AssertionError(f"TUI still shows {text!r}: {self.text()}")

    def close(self):
        if getattr(self, "process", None) is None:
            return
        try:
            self.send(b"\x03")
        except OSError:
            pass
        deadline = time.monotonic() + 3
        while True:
            pid, status, usage = os.wait4(self.process.pid, os.WNOHANG)
            if pid:
                self.process.returncode = os.waitstatus_to_exitcode(status)
                self.cpu_seconds = usage.ru_utime + usage.ru_stime
                break
            if time.monotonic() >= deadline:
                os.killpg(self.process.pid, signal.SIGKILL)
                pid, status, usage = os.wait4(self.process.pid, 0)
                self.process.returncode = os.waitstatus_to_exitcode(status)
                self.cpu_seconds = usage.ru_utime + usage.ru_stime
                break
            self.drain(0.02)
        os.close(self.master)
        self.process = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()


def wait_tasks(home, expected, cwd=None, local=False):
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline:
        result = cli(home, *( ["--local"] if local else []), "list", cwd=cwd)
        if result.returncode == 0 and all(text.encode() in result.stdout for text in expected):
            return result.stdout
        time.sleep(0.01)
    raise AssertionError(f"tasks did not converge: {result.stdout!r} {result.stderr!r}")


@unittest.skipUnless(os.name == "posix", "PTY regression harness requires Unix")
class ConcurrentUsage(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="shtodo-concurrent-")
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name) / "home"
        self.home.mkdir()
        self.cwd = Path(self.tmp.name) / "project"
        self.cwd.mkdir()

    def seed(self):
        self.assertEqual(cli(self.home, "add", "base").returncode, 0)
        self.assertEqual(cli(self.home, "add", "second").returncode, 0)

    def test_shell_writes_while_idle_tui_lives_and_refresh_needs_no_keypress(self):
        self.seed()
        with Tui(self.home) as tui:
            self.assertEqual(cli(self.home, "add", "shell arrival").returncode, 0)
            tui.wait_for(b"shell arrival")
            self.assertEqual(cli(self.home, "delete", "1").returncode, 0)
            tui.wait_absent(b"base")
            rows = cli(self.home, "list").stdout
            self.assertNotIn(b"base", rows)
            self.assertIn(b"shell arrival", rows)

    def test_new_shell_lifecycle_and_print_id_work_while_two_tuis_share_the_scope(self):
        self.seed()
        with Tui(self.home) as first, Tui(self.home) as second:
            reply = cli(self.home, "add", "--print-id", "third")
            self.assertEqual(reply.returncode, 0)
            self.assertEqual(reply.stdout, b"3\n")
            for args, expected in [
                (("done", "1"), b"Completed 1: base\n"),
                (("done", "1"), b"Already done 1: base\n"),
                (("reopen", "1"), b"Reopened 1: base\n"),
                (("edit", "1", "edited from shell"), b"Edited 1: edited from shell\n"),
                (("delete", "1"), b"Deleted 1: edited from shell\n"),
                (("restore", "1"), b"Restored 1: edited from shell\n"),
                (("restore", "1"), b"Already live 1: edited from shell\n"),
            ]:
                reply = cli(self.home, *args)
                self.assertEqual(reply.returncode, 0, reply.stderr)
                self.assertEqual(reply.stdout, expected)
            first.wait_for(b"edited from shell")
            second.wait_for(b"third")
            self.assertEqual(len(cli(self.home, "list").stdout.splitlines()), 3)

    def test_search_draft_survives_idle_refresh_and_filtered_reorder_keeps_hidden_slots(self):
        for text in ["needle first", "hidden row", "needle second"]:
            self.assertEqual(cli(self.home, "add", text).returncode, 0)
        with Tui(self.home) as tui:
            tui.send(b"/needle")
            tui.wait_for(b"SEARCH")
            self.assertEqual(cli(self.home, "add", "needle third").returncode, 0)
            tui.wait_for(b"needle third")
            tui.wait_for(b"SEARCH")
            tui.send(b"\rJ")
            deadline = time.monotonic() + 4
            while time.monotonic() < deadline:
                rows = cli(self.home, "list").stdout.splitlines()
                if [row.split(b"  ", 1)[0] for row in rows] == [b"3", b"2", b"1", b"4"]:
                    break
                time.sleep(0.01)
            else:
                self.fail(f"filtered reorder did not preserve hidden slots: {rows!r}")
            tui.wait_absent(b"hidden row")

    def test_trash_stays_open_and_refreshes_external_restoration_and_new_deletion(self):
        self.seed()
        self.assertEqual(cli(self.home, "delete", "1").returncode, 0)
        self.assertEqual(cli(self.home, "delete", "2").returncode, 0)
        with Tui(self.home) as tui:
            tui.send(b"t")
            tui.wait_for(b"TRASH")
            tui.wait_for(b"second")
            self.assertEqual(cli(self.home, "restore", "2").returncode, 0)
            tui.wait_absent(b"second")
            tui.wait_for(b"TRASH")
            tui.send(b"r")
            tui.wait_for(b"Trash is empty")
            self.assertEqual(cli(self.home, "delete", "2").returncode, 0)
            tui.wait_for(b"second")
            tui.wait_for(b"TRASH")

    def test_open_view_keeps_a_detached_draft_when_shell_completion_hides_the_target(self):
        self.seed()
        with Tui(self.home) as tui:
            tui.send(b"\te_draft")
            tui.wait_for(b"INSERT")
            tui.wait_for(b"base_draft")
            self.assertEqual(cli(self.home, "done", "1").returncode, 0)
            # The detached draft and remaining row must both survive the refresh.
            tui.drain(1.2)
            tui.wait_for(b"base_draft")
            tui.wait_for(b"second")
            tui.send(b"\r")
            wait_tasks(self.home, ["base_draft"])
            self.assertIn(b"1  done  base_draft", cli(self.home, "list").stdout)
            tui.wait_absent(b"base_draft")

    def test_two_tuis_add_from_cached_empty_state_and_both_converge(self):
        with Tui(self.home) as first, Tui(self.home) as second:
            self.assertFalse((self.home / ".shtodo").exists())
            first.send(b"ione\r")
            wait_tasks(self.home, ["one"])
            second.send(b"itwo\r")
            rows = wait_tasks(self.home, ["one", "two"])
            self.assertEqual(len(rows.splitlines()), 2)
            first.wait_for(b"two")
            second.wait_for(b"one")

    def test_conflicting_edit_preserves_draft_and_requires_review(self):
        self.seed()
        with Tui(self.home) as first, Tui(self.home) as second:
            first.send(b"e_one")
            second.send(b"e_two")
            first.wait_for(b"_one")
            second.wait_for(b"_two")
            first.send(b"\r")
            wait_tasks(self.home, ["base_one"])
            second.send(b"\r")
            second.wait_for(b"Draft conflict")
            self.assertNotIn(b"base_two", cli(self.home, "list").stdout)
            second.send(b"\x1b")
            second.wait_for(b"NORMAL")
            second.send(b"e_retry\r")
            wait_tasks(self.home, ["base_one_retry"])

    def test_deleted_edit_target_stays_detached_and_never_resurrects(self):
        self.seed()
        with Tui(self.home) as tui:
            tui.send(b"e_draft")
            tui.wait_for(b"_draft")
            self.assertEqual(cli(self.home, "delete", "1").returncode, 0)
            tui.wait_for(b"Draft conflict")
            tui.send(b"\r")
            tui.drain(0.1)
            self.assertNotIn(b"base", cli(self.home, "list").stdout)

    def test_local_scope_is_exact_directory_and_other_scopes_are_independent(self):
        child = self.cwd / "child"
        child.mkdir()
        with Tui(self.home, self.cwd, local=True) as tui:
            self.assertEqual(cli(self.home, "--local", "add", "parent arrival", cwd=self.cwd).returncode, 0)
            tui.wait_for(b"parent arrival")
            self.assertEqual(cli(self.home, "--local", "add", "child only", cwd=child).returncode, 0)
            self.assertEqual(cli(self.home, "add", "global only").returncode, 0)
            parent = cli(self.home, "--local", "list", cwd=self.cwd).stdout
            self.assertIn(b"parent arrival", parent)
            self.assertNotIn(b"child only", parent)
            self.assertNotIn(b"global only", parent)

    def test_missing_lock_is_recreated_by_tui_writes_without_a_shell_mutation(self):
        self.seed()
        path = self.home / ".shtodo/global/tasks.json"
        lock = path.with_suffix(".lock")
        expected = json.loads(path.read_text())
        lock.unlink()
        with Tui(self.home) as tui:
            tui.wait_for(b"2 open")
            self.assertFalse(lock.exists())
            tui.send(b" ")
            tui.wait_for(b"1 open")
            expected["tasks"][0]["completed"] = True
            self.assertEqual(json.loads(path.read_text()), expected)
            self.assertTrue(lock.exists())

            lock.unlink()
            tui.send(b" ")
            tui.wait_for(b"2 open")
            expected["tasks"][0]["completed"] = False
            self.assertEqual(json.loads(path.read_text()), expected)
            self.assertTrue(lock.exists())

    def test_missing_initialized_storage_keeps_tui_alive_and_does_not_reset_ids(self):
        self.seed()
        with Tui(self.home) as tui:
            path = self.home / ".shtodo/global/tasks.json"
            path.unlink()
            tui.wait_for(b"Storage unavailable")
            tui.send(b"ishould not save\r")
            tui.drain(0.1)
            self.assertFalse(path.exists())

    def test_shell_burst_preserves_each_success_and_unique_ids(self):
        barrier = threading.Barrier(8)
        def add(index):
            barrier.wait(timeout=5)
            return cli(self.home, "add", f"burst {index}")
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            results = list(pool.map(add, range(8)))
        rows = cli(self.home, "list").stdout
        for index, result in enumerate(results):
            if result.returncode == 0:
                self.assertIn(f"burst {index}".encode(), rows)
            else:
                self.assertEqual(result.stdout, b"")
                self.assertIn(b"lock wait timed out", result.stderr)
        self.assertTrue(all(result.returncode == 0 for result in results))
        tasks = json.loads((self.home / ".shtodo/global/tasks.json").read_text())["tasks"]
        self.assertEqual(len({task["id"] for task in tasks}), 8)

    def test_mixed_legacy_lock_allows_reads_but_returns_bounded_busy(self):
        self.seed()
        path = self.home / ".shtodo/global/tasks.lock"
        with path.open("r+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            self.assertEqual(cli(self.home, "list").returncode, 0)
            with Tui(self.home):
                result = cli(self.home, "add", "blocked")
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(result.stdout, b"")
                self.assertIn(b"lock wait timed out", result.stderr)
        self.assertEqual(cli(self.home, "add", "after release").returncode, 0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
