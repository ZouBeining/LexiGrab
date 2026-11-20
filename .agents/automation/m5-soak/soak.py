"""LexiGrab M5 eight-hour soak harness.

The desktop app is launched against the M1 mini dictionary fixture with the
loopback bridge enabled. Every interval the harness performs one real
`POST /v1/selection` request and samples the process RSS, handle count, thread
count and CPU time. Results are appended to `artifacts/soak.jsonl` and a summary
is printed at the end.

The app is copied to `%TEMP%` before launch because this repository may carry a
Low mandatory integrity label: an executable started inside it cannot inject
input or write to `%APPDATA%` on this machine (see M2 acceptance notes).

Usage (from the repository root):

    python -u .agents/automation/m5-soak/soak.py --minutes 480
    python -u .agents/automation/m5-soak/soak.py --minutes 20 --interval-min 1

Exit codes: 0 success, 2 the app or bridge never came up, 3 a sample failed in a
way that invalidates the run (process died).
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

MANUAL = Path(__file__).resolve().parent
ROOT = MANUAL.parents[2]
ARTIFACTS = MANUAL / "artifacts"
FIXTURES = ROOT / "src-tauri" / "tests" / "fixtures"
DEFAULT_EXE = ROOT / "src-tauri" / "target" / "debug" / "lexigrab.exe"

try:
    import psutil
except ImportError:  # pragma: no cover - documented in README
    print("psutil is required: pip install psutil", file=sys.stderr)
    raise SystemExit(2)

WORDS = ["hello", "bank", "lexigrab-soak-missing"]

CONFIG_TEMPLATE = """\
[meta]
schema_version = 2
device_id = "m5-soak"

[general]
hotkey = "{hotkey}"
theme = "figma"
query_strategy = "local_first"
max_entries_per_source = 3
max_entries_total = 20
start_minimized = false

[anki]
endpoint = "http://127.0.0.1:8765"
deck = "Default"
model = "Basic"
tags = ["global-dict"]
duplicate_policy = "warn"
field_map = {{ word = "Word", definition = "Definition", sentence = "Sentence", source = "Source" }}

[bridge]
enabled = true
port = {port}
token = "{token}"

[cache]
ttl_hours = 24
negative_ttl_minutes = 5

[sense]
enabled = false
selector_endpoint = "http://127.0.0.1:8787/v1/select"
selector_timeout_ms = 800
selector_allow_remote = false
low_confidence = 0.55
high_confidence = 0.72
max_senses = 20

[llm]
enabled = false
endpoint = "http://127.0.0.1:11434/v1/chat/completions"
model = ""
api_key_env = "LEXIGRAB_LLM_API_KEY"
timeout_ms = 15000
max_tokens = 320
allow_remote = false
cache_ttl_hours = 168

[[sources]]
id = "local-mini"
name = "Mini"
type = "local_file"
enabled = true
priority = 10
paths = ["mini.mdx"]
"""


def now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def free_token() -> str:
    # 64 lowercase hex characters, matching generate_bridge_token().
    return os.urandom(32).hex()


def prepare_home(home: Path, port: int, token: str) -> Path:
    if home.exists():
        shutil.rmtree(home, ignore_errors=True)
    # `LEXIGRAB_HOME` 的配置目录是 `<home>/config`，日志与缓存也相对它布局。
    config_dir = home / "config"
    config_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(FIXTURES / "mini.mdx", config_dir / "mini.mdx")
    shutil.copy2(FIXTURES / "mini.mdd", config_dir / "mini.mdd")
    config = CONFIG_TEMPLATE.format(
        hotkey="Ctrl+Alt+Shift+F9",
        port=port,
        token=token,
    )
    (config_dir / "config.toml").write_text(config, encoding="utf-8")
    return config_dir / "config.toml"


def executable_dir() -> Path:
    """Pick a writable, Medium-integrity directory for the copied executable.

    `%TEMP%` is not always the current user's directory on shared machines
    (this one points at another profile and is not writable), so fall back to
    the local app data temp directory and finally to the harness directory.
    """
    candidates = [
        Path(os.environ.get("LOCALAPPDATA", "")) / "Temp" / "lexigrab-soak",
        Path(os.environ.get("TEMP", "")) / "lexigrab-soak",
        MANUAL / "exe",
    ]
    for candidate in candidates:
        try:
            candidate.mkdir(parents=True, exist_ok=True)
            probe = candidate / ".write-probe"
            probe.write_text("ok", encoding="utf-8")
            probe.unlink()
            return candidate
        except OSError:
            continue
    raise SystemExit("no writable directory for the soak executable")


def copy_exe(exe: Path) -> Path:
    if not exe.is_file():
        print(f"missing executable: {exe}", file=sys.stderr)
        print("build it with: pnpm tauri build --debug --no-bundle", file=sys.stderr)
        raise SystemExit(2)
    target_dir = executable_dir()
    target = target_dir / "lexigrab.exe"
    shutil.copy2(exe, target)
    return target


def launch(exe: Path, home: Path) -> subprocess.Popen[bytes]:
    env = dict(os.environ)
    env["LEXIGRAB_HOME"] = str(home)
    env["RUST_BACKTRACE"] = "1"
    creation = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    return subprocess.Popen(
        [str(exe)],
        cwd=str(exe.parent),
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=creation,
    )


def request(url: str, token: str, method: str = "GET", body: dict | None = None) -> tuple[int, dict | str]:
    data = None
    headers = {
        "X-Auth-Token": token,
        "Origin": "moz-extension://lexigrab-soak",
    }
    if body is not None:
        data = json.dumps(body).encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            payload = response.read()
            try:
                return response.status, json.loads(payload)
            except json.JSONDecodeError:
                return response.status, payload.decode("utf-8", "replace")
    except urllib.error.HTTPError as error:
        return error.code, error.read().decode("utf-8", "replace")
    except OSError as error:
        return 0, str(error)


def wait_for_health(port: int, token: str, process: subprocess.Popen[bytes], timeout: float = 30.0) -> bool:
    url = f"http://127.0.0.1:{port}/v1/health"
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if process.poll() is not None:
            return False
        status, _ = request(url, token)
        if status == 200:
            return True
        time.sleep(0.5)
    return False


def sample(process: subprocess.Popen[bytes]) -> dict:
    proc = psutil.Process(process.pid)
    memory = proc.memory_info()
    return {
        "timestamp": now(),
        "rss_bytes": memory.rss,
        "private_bytes": memory.private,
        "handles": proc.num_handles() if hasattr(proc, "num_handles") else None,
        "threads": proc.num_threads(),
        "cpu_seconds": sum(proc.cpu_times()[:2]),
    }


def percentile(values: list[float], ratio: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, int(round(len(ordered) * ratio)) - 1))
    return ordered[index]


def main() -> int:
    parser = argparse.ArgumentParser(description="LexiGrab M5 soak harness")
    parser.add_argument("--minutes", type=float, default=480.0, help="total run length (default 480)")
    parser.add_argument("--interval-min", type=float, default=10.0, help="sampling interval (default 10)")
    parser.add_argument("--port", type=int, default=12789)
    parser.add_argument("--exe", type=Path, default=DEFAULT_EXE)
    parser.add_argument("--home", type=Path, default=MANUAL / "home")
    parser.add_argument("--keep", action="store_true", help="keep the application running after the run")
    args = parser.parse_args()

    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    token = free_token()
    config = prepare_home(args.home, args.port, token)
    exe = copy_exe(args.exe)
    print(f"[{now()}] launching {exe} (home={args.home}, port={args.port})")
    process = launch(exe, args.home)
    if not wait_for_health(args.port, token, process):
        print(f"[{now()}] bridge did not become healthy", file=sys.stderr)
        if process.poll() is None:
            process.terminate()
        return 2
    print(f"[{now()}] bridge healthy; config={config}")

    log_path = ARTIFACTS / "soak.jsonl"
    started = time.monotonic()
    deadline = started + args.minutes * 60.0
    latencies: list[float] = []
    samples: list[dict] = []
    failures = 0

    try:
        while True:
            word = WORDS[len(samples) % len(WORDS)]
            request_started = time.monotonic()
            status, payload = request(
                f"http://127.0.0.1:{args.port}/v1/selection",
                token,
                method="POST",
                body={
                    "word": word,
                    "sentence": f"Soak sample for {word}.",
                    "url": "https://example.com/soak",
                    "title": "Soak",
                },
            )
            elapsed_ms = (time.monotonic() - request_started) * 1000.0
            if status == 200:
                latencies.append(elapsed_ms)
            else:
                failures += 1
            if process.poll() is not None:
                print(f"[{now()}] process exited during sample", file=sys.stderr)
                return 3
            record = sample(process)
            record.update(
                {
                    "word": word,
                    "status": status,
                    "latency_ms": round(elapsed_ms, 1),
                    "entries": len(payload.get("entries", [])) if isinstance(payload, dict) else None,
                    "failures": failures,
                }
            )
            samples.append(record)
            with log_path.open("a", encoding="utf-8") as handle:
                handle.write(json.dumps(record, ensure_ascii=False) + "\n")
            print(
                f"[{record['timestamp']}] {word:<26} http={status} {elapsed_ms:7.1f}ms "
                f"rss={record['rss_bytes'] / 1024 / 1024:6.1f}MB "
                f"handles={record['handles']} threads={record['threads']}"
            )
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            time.sleep(min(args.interval_min * 60.0, remaining))
    finally:
        if not args.keep and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()

    if not samples:
        return 3
    warmup = samples[3:] if len(samples) > 3 else samples
    first_rss = warmup[0]["rss_bytes"]
    last_rss = warmup[-1]["rss_bytes"]
    growth = (last_rss - first_rss) / first_rss * 100.0 if first_rss else 0.0
    handles = [s["handles"] for s in warmup if s["handles"] is not None]
    summary = {
        "started": now(),
        "minutes": round((time.monotonic() - started) / 60.0, 2),
        "samples": len(samples),
        "failures": failures,
        "selection_p50_ms": round(statistics.median(latencies), 1) if latencies else None,
        "selection_p95_ms": round(percentile(latencies, 0.95), 1) if latencies else None,
        "rss_first_mb": round(first_rss / 1024 / 1024, 1),
        "rss_last_mb": round(last_rss / 1024 / 1024, 1),
        "rss_growth_percent": round(growth, 2),
        "handles_first": handles[0] if handles else None,
        "handles_last": handles[-1] if handles else None,
        "handles_growth": (handles[-1] - handles[0]) if handles else None,
    }
    with (ARTIFACTS / "summary.json").open("w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=2)
    print(json.dumps(summary, indent=2))
    return 0 if failures == 0 else 3


if __name__ == "__main__":
    raise SystemExit(main())
