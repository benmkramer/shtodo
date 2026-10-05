#!/usr/bin/env python3
"""Render a benchmark JSON file as PNG and SVG; requires matplotlib."""

import argparse
import json
import os
from pathlib import Path

# Keep matplotlib's writable cache with the other local benchmark artifacts.
os.environ.setdefault("MPLCONFIGDIR", str(Path(__file__).resolve().parents[1]
                                        / "target/benchmarks/matplotlib"))
os.environ.setdefault("XDG_CACHE_HOME", str(Path(__file__).resolve().parents[1]
                                          / "target/benchmarks/cache"))
import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.ticker import FuncFormatter, LogLocator, NullFormatter


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    args = parser.parse_args()
    report = json.loads(args.results.read_text())
    metadata = report["metadata"]
    styles = {"shtodo": ("#007F73", "o"), "taskwarrior": ("#8052AB", "s"),
              "todotxt": ("#CA661B", "^"), "taskbook": ("#2874B5", "D"),
              "topydo": ("#A84C69", "v")}
    labels = {"shtodo": "shtodo", "taskwarrior": "Taskwarrior", "todotxt": "todo.txt CLI",
              "taskbook": "Taskbook", "topydo": "topydo"}
    operations = [("list", "List all tasks"), ("add", "Add one task"),
                  ("delete", "Delete one task")]
    fig, axes = plt.subplots(1, 3, figsize=(12, 5.0), sharey=True)
    for axis, (operation, title) in zip(axes, operations):
        for app, (color, marker) in styles.items():
            rows = sorted((r for r in report["results"] if r["app"] == app
                           and r["operation"] == operation and r["size"] > 0),
                          key=lambda r: r["size"])
            if not rows:
                continue
            sizes = [r["size"] for r in rows]
            medians = [r["statistics"]["median_ms"] for r in rows]
            tails = [r["statistics"]["p95_ms"] for r in rows]
            axis.plot(sizes, medians, color=color, marker=marker, label=labels[app], lw=2)
            axis.fill_between(sizes, medians, tails, color=color, alpha=0.13)
        axis.set(xscale="log", yscale="log", title=title, xlabel="Initial open tasks (log scale)")
        axis.set_xticks(sorted(n for n in metadata["sizes"] if n > 0))
        axis.xaxis.set_major_formatter(FuncFormatter(lambda n, _: f"{n:,.0f}"))
        axis.yaxis.set_major_locator(LogLocator(base=10, subs=(1, 2, 5)))
        axis.yaxis.set_major_formatter(FuncFormatter(lambda n, _: f"{n:g}"))
        axis.yaxis.set_minor_formatter(NullFormatter())
        axis.grid(axis="y", alpha=0.25, which="major")
        axis.spines[["top", "right"]].set_visible(False)
    axes[0].set_ylabel("Wall time (ms, log scale)\nLower is faster")
    handles, names = axes[0].get_legend_handles_labels()
    fig.legend(handles, names, loc="upper center", bbox_to_anchor=(0.5, 0.93), ncol=len(names), frameon=False)
    fig.suptitle("Selected CLI operations on synthetic task lists", fontsize=15, y=0.99)
    hardware = metadata["machine"].get("hardware", {})
    chip = hardware.get("chip_type", metadata["machine"]["architecture"]) if isinstance(hardware, dict) else hardware
    shtodo = report["tools"].get("shtodo")
    provenance = f"Measured {metadata['started_at'][:10]}"
    if shtodo:
        provenance += f" | {shtodo['version'].splitlines()[0]} | binary SHA-256: {shtodo['sha256'][:12]}"
    fig.text(0.06, 0.165, provenance, fontsize=9, color="#555555")
    fig.text(0.06, 0.125,
             f"{chip} | {metadata['runs']} samples per case | warm caches | "
             "lines: median; shading: median to p95 (not a confidence interval)", fontsize=9)
    fig.text(0.06, 0.075,
             "Feature sets differ; shtodo is a deliberately small checklist. "
             "These timings do not measure overall product quality.", fontsize=9, color="#555555")
    fig.text(0.06, 0.025,
             "CLI only; stdout discarded. Recovery/durability differ. "
             "Scope and methodology: docs/benchmarks.md; samples: companion JSON.", fontsize=9, color="#555555")
    fig.subplots_adjust(left=0.075, right=0.98, top=0.78, bottom=0.28, wspace=0.16)
    for extension in (".svg", ".png"):
        output = args.results.with_suffix(extension)
        fig.savefig(output, dpi=170)
        if extension == ".svg":
            # Matplotlib adds trailing spaces to multiline SVG path data.
            # Keep checked-in figures clean without changing the coordinates.
            output.write_text("\n".join(line.rstrip() for line in
                                        output.read_text().splitlines()) + "\n")
        print(output)
    plt.close(fig)


if __name__ == "__main__":
    main()
