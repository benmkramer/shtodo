#!/usr/bin/env python3
"""Fetch the pinned macOS 27 ARM64 comparison tools into target/benchmark-tools.

Requires Python 3.12+. No global installation or task configuration changes.
Other platforms can use installed executables with scripts/benchmark.py instead.
"""

import hashlib
import json
from pathlib import Path
import platform
import sys
import tarfile
import urllib.request


ROOT = Path(__file__).resolve().parents[1]


def main():
    if sys.version_info < (3, 12):
        sys.exit("The pinned-tool downloader requires Python 3.12 or newer.")
    if (sys.platform != "darwin" or platform.machine() != "arm64"
            or int(platform.mac_ver()[0].split(".")[0]) < 27):
        sys.exit("These pinned binaries require macOS 27+ on Apple Silicon. "
                 "Supply native tool paths to benchmark.py on other platforms.")
    manifest = json.loads((ROOT / "docs/benchmarks/2026-10-04-tools.json").read_text())
    destination = ROOT / "target/benchmark-tools"
    destination.mkdir(parents=True, exist_ok=True)
    for name, artifact in manifest.items():
        print(f"Downloading {name} {artifact['version']}", flush=True)
        headers = {}
        if name == "taskwarrior":
            token_url = ("https://ghcr.io/token?service=ghcr.io&"
                         "scope=repository:homebrew/core/task:pull")
            with urllib.request.urlopen(token_url, timeout=60) as response:
                headers["Authorization"] = "Bearer " + json.load(response)["token"]
        request = urllib.request.Request(artifact["source"], headers=headers)
        with urllib.request.urlopen(request, timeout=60) as response:
            data = response.read()
        if hashlib.sha256(data).hexdigest() != artifact["sha256"]:
            sys.exit(f"Checksum mismatch for {name}; archive was not extracted.")
        archive = destination / f"{name}.tar.gz"
        archive.write_bytes(data)
        with tarfile.open(archive) as package:
            package.extractall(destination, filter="data")
    print(f"Verified and unpacked tools in {destination}")


if __name__ == "__main__":
    main()
