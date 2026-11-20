"""M4 远程源端到端验收：启动本地 mock + 远程 JSONPath/脚本源 + 失败源。

运行：`python run.py`（在仓库根目录或本目录均可）。
需要已构建的 `lexigrab-dev`；脚本会自动 `cargo build --bin lexigrab-dev`。
产物写入本目录 `artifacts/results.json`。
"""

import json
import socket
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent
CARGO_DIR = ROOT / "src-tauri"
ARTIFACTS = HERE / "artifacts"
TOKEN = "0" * 64


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def wait_ready(process, port, timeout=15.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        if process.poll() is not None:
            raise RuntimeError("mock server exited early")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.5):
                return
        except OSError:
            time.sleep(0.1)
    raise RuntimeError("mock server did not start")


def config_text(port: int, mini_mdx: Path, script_path: Path) -> str:
    return f"""
[meta]
schema_version = 2
device_id = "m4-mock"

[general]
hotkey = "CmdOrCtrl+Shift+D"
theme = "figma"
query_strategy = "aggregate"
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
enabled = false
port = 12787
token = "{TOKEN}"

[cache]
ttl_hours = 24
negative_ttl_minutes = 5

[sense]
enabled = true
selector_endpoint = "http://127.0.0.1:{port}/v1/select"
selector_timeout_ms = 800
selector_allow_remote = false
low_confidence = 0.55
high_confidence = 0.72
max_senses = 20

[llm]
enabled = true
endpoint = "http://127.0.0.1:{port}/v1/chat/completions"
model = "mock-model"
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
paths = ["{mini_mdx.as_posix()}"]

[[sources]]
id = "remote-jsonpath"
name = "Remote JSONPath"
type = "remote_http"
enabled = true
priority = 20
url_template = "http://127.0.0.1:{port}/dict?q={{word}}"
response_parser = {{ mode = "jsonpath", expression = "$.entries[*]" }}
timeout_ms = 3000

[[sources]]
id = "remote-script"
name = "Remote Script"
type = "remote_http"
enabled = true
priority = 30
url_template = "http://127.0.0.1:{port}/script?q={{word}}"
response_parser = {{ mode = "script", path = "{script_path.as_posix()}", timeout_ms = 200 }}
timeout_ms = 3000

[[sources]]
id = "remote-fail"
name = "Remote Fail"
type = "remote_http"
enabled = true
priority = 40
url_template = "http://127.0.0.1:{port}/fail?q={{word}}"
response_parser = {{ mode = "jsonpath", expression = "$.entries[*]" }}
timeout_ms = 1000
"""


SCRIPT = """
function parse(ctx) {
  const data = JSON.parse(ctx.responseBody);
  return data.items.map((item) => ({
    headword: item.word,
    definition_html: item.html,
    phonetic: item.phonetic || null,
    resources: [],
  }));
}
"""


def run_lookup(config_path: Path, word: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--bin",
            "lexigrab-dev",
            "--",
            "lookup",
            "--config",
            str(config_path),
            "--word",
            word,
            "--json",
        ],
        cwd=CARGO_DIR,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        timeout=180,
    )


def main() -> int:
    ARTIFACTS.mkdir(exist_ok=True)
    port = free_port()
    server = subprocess.Popen(
        [sys.executable, "-u", str(HERE / "mock_server.py"), str(port)],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    results = {"port": port, "checks": [], "passed": False}
    try:
        wait_ready(server, port)
        work = ARTIFACTS / "work"
        work.mkdir(exist_ok=True)
        script_path = work / "parser.js"
        script_path.write_text(SCRIPT, encoding="utf-8")
        config_path = work / "config.toml"
        config_path.write_text(
            config_text(port, (CARGO_DIR / "tests/fixtures/mini.mdx").resolve(), script_path),
            encoding="utf-8",
        )

        proc = run_lookup(config_path, "hello")
        output = proc.stdout.strip()
        stderr = proc.stderr.strip()
        results["exit_code"] = proc.returncode
        results["stderr_tail"] = stderr.splitlines()[-8:]
        try:
            entries = json.loads(output) if output else []
        except json.JSONDecodeError:
            entries = []
        sources = sorted({entry.get("source_name", "") for entry in entries})
        results["source_names"] = sources
        results["entry_count"] = len(entries)
        results["checks"] = [
            {"name": "exit_code_ok", "passed": proc.returncode == 0},
            {"name": "local_source", "passed": "Mini" in sources},
            {"name": "jsonpath_source", "passed": "Remote JSONPath" in sources},
            {"name": "script_source", "passed": "Remote Script" in sources},
            {
                "name": "sanitized_no_script",
                "passed": all("<script" not in entry.get("definition_html", "") for entry in entries),
            },
            {
                "name": "relative_resource_dropped",
                "passed": all(
                    resource.get("kind") == "local"
                    or resource.get("url", "").startswith("https://example.com")
                    for entry in entries
                    for resource in entry.get("resources", [])
                ),
            },
            {
                "name": "failed_source_isolated",
                "passed": "Remote Fail" not in sources and len(entries) >= 3,
            },
        ]
        results["passed"] = all(check["passed"] for check in results["checks"])

        # 空结果：远程空数组不应报错，CLI 返回 2（未找到）。
        empty_proc = run_lookup(config_path, "no-such-word-xyz")
        results["empty_exit_code"] = empty_proc.returncode
        results["checks"].append(
            {"name": "empty_result_exit_4", "passed": empty_proc.returncode == 4}
        )
        results["passed"] = all(check["passed"] for check in results["checks"])
    finally:
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()
        (ARTIFACTS / "results.json").write_text(
            json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8"
        )
        print(json.dumps(results, ensure_ascii=False, indent=2))
    return 0 if results.get("passed") else 1


if __name__ == "__main__":
    sys.exit(main())
