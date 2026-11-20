"""Sample peak RSS while running `lexigrab-dev bench` against a large dictionary.

Usage (repository root, after `cargo build --release --bin lexigrab-dev`):

    python .agents/automation/m5-soak/bench_rss.py \
        --exe src-tauri/target/release/lexigrab-dev.exe \
        --config src-tauri/target/perf/config.toml \
        --word word00042 --iterations 200 --runs 3

The bench process opens the dictionary, performs the iterations and exits, so
peak RSS approximates the in-memory cost of the loaded source. Each run prints
one JSON line; the summary prints median p95 and peak RSS.
"""

from __future__ import annotations

import argparse
import json
import re
import statistics
import subprocess
import sys
import time

import psutil

P95 = re.compile(r"p95_ms=([0-9.]+)")
OPEN = re.compile(r"open_ms=([0-9.]+)")


def run_once(exe: str, args: list[str], word: str, iterations: int) -> dict:
    started = time.monotonic()
    process = subprocess.Popen(
        [exe, "bench", *args, "--word", word, "--iterations", str(iterations)],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    peak_rss = 0
    proc = psutil.Process(process.pid)
    while process.poll() is None:
        try:
            peak_rss = max(peak_rss, proc.memory_info().rss)
        except psutil.Error:
            break
        time.sleep(0.005)
    output = process.communicate()[0] or ""
    elapsed = time.monotonic() - started
    p95 = P95.search(output)
    open_ms = OPEN.search(output)
    return {
        "exit": process.returncode,
        "wall_ms": round(elapsed * 1000, 1),
        "open_ms": float(open_ms.group(1)) if open_ms else None,
        "p95_ms": float(p95.group(1)) if p95 else None,
        "peak_rss_mb": round(peak_rss / 1024 / 1024, 1),
        "output": output.strip(),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", required=True)
    parser.add_argument("--config", required=True)
    parser.add_argument("--word", default="word00042")
    parser.add_argument("--iterations", type=int, default=200)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()

    results = []
    for index in range(args.runs):
        result = run_once(args.exe, ["--config", args.config], args.word, args.iterations)
        print(json.dumps({"run": index + 1, **result}, ensure_ascii=False))
        if result["exit"] != 0:
            print(f"bench failed: {result['output']}", file=sys.stderr)
            return 2
        results.append(result)

    summary = {
        "runs": args.runs,
        "iterations": args.iterations,
        "open_ms": round(statistics.median(r["open_ms"] for r in results), 1),
        "p95_ms": round(statistics.median(r["p95_ms"] for r in results), 2),
        "peak_rss_mb": round(statistics.median(r["peak_rss_mb"] for r in results), 1),
    }
    print(json.dumps(summary, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
