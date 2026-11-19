"""真实桌面端（lexigrab.exe debug 产物）的启动/停止与桥接探活。"""

from __future__ import annotations

import hashlib
import http.client
import json
import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[3]
EXE = ROOT / "src-tauri" / "target" / "debug" / "lexigrab.exe"
MINI_MDX = ROOT / "src-tauri" / "tests" / "fixtures" / "mini.mdx"
MINI_MDD = ROOT / "src-tauri" / "tests" / "fixtures" / "mini.mdd"
TEMP_ROOT = Path(tempfile.gettempdir()) / "lexigrab-m3"

BRIDGE_PORT = 12787
# 自动化用确定性令牌：由用途派生 64 位十六进制，不写入任何真实配对信息。
TOKEN = hashlib.sha256(b"lexigrab-m3-token").hexdigest()
ROTATED_TOKEN = hashlib.sha256(b"lexigrab-m3-token-rotated").hexdigest()


def build_config(
    *,
    port: int = BRIDGE_PORT,
    token: str = TOKEN,
    bridge_enabled: bool = True,
    start_minimized: bool = True,
    anki_endpoint: str = "http://127.0.0.1:8765",
    deck: str = "LexiGrab E2E",
    model: str = "LexiGrab Basic",
    tags: list[str] | None = None,
) -> str:
    tags = tags or ["global-dict", "lexigrab-m3"]
    tag_text = ", ".join(json.dumps(tag) for tag in tags)
    return f"""\
[meta]
schema_version = 2
device_id = "m3-automation-device"

[general]
hotkey = "CmdOrCtrl+Shift+D"
theme = "figma"
query_strategy = "local_first"
max_entries_per_source = 3
max_entries_total = 20
start_minimized = {str(start_minimized).lower()}

[anki]
endpoint = {json.dumps(anki_endpoint)}
deck = {json.dumps(deck)}
model = {json.dumps(model)}
tags = [{tag_text}]
duplicate_policy = "warn"
field_map = {{ word = "Word", definition = "Definition", sentence = "Sentence", source = "Source" }}

[bridge]
enabled = {str(bridge_enabled).lower()}
port = {port}
token = {json.dumps(token)}

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
paths = [{json.dumps(str(MINI_MDX))}]
"""


def http_json(
    method: str,
    path: str,
    body: dict[str, Any] | None = None,
    *,
    port: int = BRIDGE_PORT,
    token: str | None = TOKEN,
    origin: str | None = None,
    timeout: float = 15.0,
) -> tuple[int, dict[str, str], Any]:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    headers: dict[str, str] = {}
    if token is not None:
        headers["X-Auth-Token"] = token
    payload = None
    if body is not None:
        payload = json.dumps(body, ensure_ascii=False).encode("utf-8")
        headers["Content-Type"] = "application/json"
    if origin is not None:
        headers["Origin"] = origin
    connection.request(method, path, body=payload, headers=headers)
    response = connection.getresponse()
    raw = response.read()
    response_headers = {key.lower(): value for key, value in response.getheaders()}
    connection.close()
    try:
        data = json.loads(raw.decode("utf-8")) if raw else None
    except json.JSONDecodeError:
        data = raw.decode("utf-8", errors="replace")
    return response.status, response_headers, data


class DesktopApp:
    """在 %TEMP% 下运行 exe（仓库目录带 Low 完整性标签，见 M2 报告 §3.3）。"""

    def __init__(self, name: str = "desktop") -> None:
        self.root = TEMP_ROOT / name
        self.process: subprocess.Popen[bytes] | None = None
        self._log_handle = None

    def prepare(self, config_text: str) -> None:
        if self.root.exists():
            shutil.rmtree(self.root, ignore_errors=True)
        self.root.mkdir(parents=True, exist_ok=True)
        shutil.copy2(EXE, self.root / "lexigrab.exe")
        (self.root / "config.toml").write_text(config_text, encoding="utf-8")

    def start(self) -> None:
        env = dict(os.environ)
        env["LEXIGRAB_CONFIG_DIR"] = str(self.root)
        env.pop("LEXIGRAB_HOME", None)
        self._log_handle = (self.root / "stdout.log").open("wb")
        self.process = subprocess.Popen(
            [str(self.root / "lexigrab.exe")],
            cwd=str(self.root),
            env=env,
            stdout=self._log_handle,
            stderr=subprocess.STDOUT,
        )

    def wait_healthy(self, timeout: float = 45.0, token: str = TOKEN, port: int = BRIDGE_PORT) -> dict[str, Any]:
        deadline = time.time() + timeout
        last: Exception | None = None
        while time.time() < deadline:
            if self.process is not None and self.process.poll() is not None:
                raise RuntimeError(f"桌面端提前退出，exit={self.process.returncode}")
            try:
                status, _, data = http_json("GET", "/v1/health", token=token, port=port)
                if status == 200:
                    return data
            except Exception as error:  # noqa: BLE001 - 启动期连接失败属预期
                last = error
            time.sleep(0.3)
        raise TimeoutError(f"桥接 {port} 未就绪：{last}")

    def stop(self) -> None:
        if self.process is not None and self.process.poll() is None:
            try:
                self.process.terminate()
                self.process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=10)
        self.process = None
        if self._log_handle is not None:
            self._log_handle.close()
            self._log_handle = None

    def log_tail(self, lines: int = 40) -> str:
        log_dir = self.root / "logs"
        candidates = sorted(log_dir.glob("*.log")) if log_dir.is_dir() else []
        if not candidates:
            stdout = self.root / "stdout.log"
            if stdout.is_file():
                return stdout.read_text(encoding="utf-8", errors="replace")[-4000:]
            return ""
        return candidates[-1].read_text(encoding="utf-8", errors="replace")[-4000:][-4000:]
