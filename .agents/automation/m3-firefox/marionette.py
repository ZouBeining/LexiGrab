"""Firefox Marionette 最小客户端（仅标准库）。

Marionette 是 Firefox 内置的远程控制协议，监听 TCP，消息为
``<字节长度>:<UTF-8 JSON>``。这里只实现 M3 验收需要的命令，
不依赖 geckodriver / selenium。
"""

from __future__ import annotations

import json
import socket
import time
from typing import Any


class MarionetteError(RuntimeError):
    """Marionette 返回的协议错误。"""

    def __init__(self, command: str, error: str, message: str, stacktrace: str = "") -> None:
        super().__init__(f"{command}: {error}: {message}")
        self.command = command
        self.error = error
        self.message = message
        self.stacktrace = stacktrace


class Marionette:
    """同步 Marionette 连接。"""

    def __init__(self, host: str = "127.0.0.1", port: int = 2828, timeout: float = 60.0) -> None:
        self.sock = socket.create_connection((host, port), timeout=5.0)
        self.sock.settimeout(timeout)
        self._buffer = b""
        self._next_id = 0
        self.session_id: str | None = None
        self._read_packet()  # 握手包，暂不校验内容

    # ------------------------------------------------------------------ 底层

    def _read_exact(self, size: int) -> bytes:
        chunks: list[bytes] = []
        remaining = size
        while remaining > 0:
            chunk = self.sock.recv(remaining)
            if not chunk:
                raise ConnectionError("Marionette 连接被关闭")
            chunks.append(chunk)
            remaining -= len(chunk)
        return b"".join(chunks)

    def _read_packet(self) -> Any:
        while b":" not in self._buffer:
            self._buffer += self._read_exact(1)
        length_text, _, rest = self._buffer.partition(b":")
        length = int(length_text)
        self._buffer = rest
        while len(self._buffer) < length:
            self._buffer += self.sock.recv(length - len(self._buffer))
        payload, self._buffer = self._buffer[:length], self._buffer[length:]
        return json.loads(payload.decode("utf-8"))

    def command(self, name: str, params: dict[str, Any] | None = None) -> Any:
        """发送命令并返响应值；协议错误抛出 MarionetteError。"""
        message_id = self._next_id
        self._next_id += 1
        payload = json.dumps([0, message_id, name, params or {}], ensure_ascii=False).encode("utf-8")
        self.sock.sendall(str(len(payload)).encode("ascii") + b":" + payload)
        while True:
            packet = self._read_packet()
            if not isinstance(packet, list) or len(packet) < 4:
                continue
            if packet[1] != message_id:
                continue
            error = packet[2]
            if error:
                raise MarionetteError(name, str(error.get("error")), str(error.get("message")), str(error.get("stacktrace", "")))
            result = packet[3]
            # Marionette 自有命令（Addon:* 等）把返回值包在 {"value": ...} 里，
            # WebDriver 命令返回原始值。
            if isinstance(result, dict) and set(result) == {"value"}:
                return result["value"]
            return result

    # ---------------------------------------------------------------- 会话

    def start_session(self) -> dict[str, Any]:
        result = self.command(
            "WebDriver:NewSession",
            {"capabilities": {"alwaysMatch": {"pageLoadStrategy": "eager"}}},
        )
        self.session_id = result.get("sessionId")
        return result

    def close(self) -> None:
        try:
            if self.session_id:
                self.command("WebDriver:DeleteSession")
        except Exception:
            pass
        finally:
            try:
                self.sock.close()
            except OSError:
                pass

    # ---------------------------------------------------------------- 页面

    def set_context(self, value: str) -> None:
        """value: content | chrome"""
        self.command("Marionette:SetContext", {"value": value})

    def navigate(self, url: str, timeout: float = 45.0, wait: bool = False) -> None:
        self.command("WebDriver:Navigate", {"url": url})
        if wait:
            self.wait_for_document(timeout)

    def wait_for_document(self, timeout: float = 45.0) -> None:
        deadline = time.time() + timeout
        while time.time() < deadline:
            state = self.execute_script("return document.readyState", sandbox="default")
            if state == "complete":
                return
            time.sleep(0.2)
        raise TimeoutError("document.readyState 未到 complete")

    def execute_script(self, script: str, args: list[Any] | None = None, sandbox: str | None = None) -> Any:
        params: dict[str, Any] = {"script": script, "args": args or []}
        if sandbox is not None:
            params["sandbox"] = sandbox
        return self.command("WebDriver:ExecuteScript", params)

    def execute_async(self, script: str, args: list[Any] | None = None, sandbox: str | None = None, timeout_ms: int = 20000) -> Any:
        params: dict[str, Any] = {"script": script, "args": args or [], "scriptTimeout": timeout_ms}
        if sandbox is not None:
            params["sandbox"] = sandbox
        return self.command("WebDriver:ExecuteAsyncScript", params)

    def screenshot(self) -> str:
        """返回 base64 PNG。"""
        return self.command("WebDriver:TakeScreenshot", {"full": False, "hash": False})

    def set_window_rect(self, width: int, height: int, x: int = 40, y: int = 40) -> None:
        self.command("WebDriver:SetWindowRect", {"x": x, "y": y, "width": width, "height": height})

    def get_window_handles(self) -> list[str]:
        return self.command("WebDriver:GetWindowHandles")

    def switch_to_window(self, handle: str) -> None:
        self.command("WebDriver:SwitchToWindow", {"handle": handle, "focus": True})

    # ---------------------------------------------------------------- 扩展

    def install_addon(self, path: str, temporary: bool = False) -> str:
        return str(self.command("Addon:Install", {"path": path, "temporary": temporary}))

    def uninstall_addon(self, addon_id: str) -> None:
        self.command("Addon:Uninstall", {"id": addon_id})
