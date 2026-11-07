"""Shared helpers for the LexiGrab M2 manual-matrix automation (pywinauto + Win32).

All coordinates/rectangles from pywinauto/win32 are physical pixels (the test
machine runs at 200% DPI), so use them directly.
"""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import time
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Callable, Iterable

import psutil
import win32api
import win32con
import win32gui
import win32process
from PIL import ImageGrab
from pywinauto import Application, Desktop
from pywinauto.keyboard import send_keys

ROOT = Path(__file__).resolve().parents[3]
MANUAL = Path(__file__).resolve().parent
HOME = MANUAL / "home"
ARTIFACTS = MANUAL / "artifacts"
FIXTURES = MANUAL / "fixtures"
DICT_DIR = ROOT / "references" / "dict"

# The repository root carries a Low mandatory integrity label on this machine
# (icacls: "Mandatory Label\Low Mandatory Level"). A process started from an
# executable inside it therefore runs at Low integrity, and Low processes cannot
# SendInput into normal Medium-integrity windows, which breaks selection capture.
# The harness therefore always runs a copy of the freshly built debug binary
# from a Medium-labeled directory (%TEMP%).
BUILT_EXE = ROOT / "src-tauri" / "target" / "debug" / "lexigrab.exe"
EXE_DIR = Path(os.environ.get("TEMP", str(MANUAL / "artifacts"))) / "lexigrab-e2e"
EXE = EXE_DIR / "lexigrab.exe"


def prepare_exe() -> Path:
    """Copy the built debug binary to a Medium-integrity directory and return it."""
    if not BUILT_EXE.is_file():
        raise RuntimeError(f"built binary missing: {BUILT_EXE}; run `pnpm tauri build --debug --no-bundle`")
    EXE_DIR.mkdir(parents=True, exist_ok=True)
    try:
        shutil.copy2(BUILT_EXE, EXE)
    except PermissionError:
        # The previous app instance still holds its image; keep the existing copy.
        if not EXE.is_file():
            raise
    return EXE


prepare_exe()

ARTIFACTS.mkdir(exist_ok=True)
FIXTURES.mkdir(exist_ok=True)


# ---------------------------------------------------------------- process/windows


def app_pid() -> int | None:
    for proc in psutil.process_iter(["pid", "name"]):
        if proc.info["name"] and proc.info["name"].lower() == "lexigrab.exe":
            return int(proc.info["pid"])
    return None


@dataclass
class WindowInfo:
    hwnd: int
    pid: int
    title: str
    cls: str
    left: int
    top: int
    right: int
    bottom: int
    visible: bool
    style: int
    exstyle: int

    @property
    def width(self) -> int:
        return self.right - self.left

    @property
    def height(self) -> int:
        return self.bottom - self.top

    def rect(self) -> tuple[int, int, int, int]:
        return self.left, self.top, self.right, self.bottom


def windows_of(pid: int) -> list[WindowInfo]:
    found: list[WindowInfo] = []

    def callback(hwnd: int, _extra: object) -> None:
        try:
            _, win_pid = win32process.GetWindowThreadProcessId(hwnd)
            if win_pid != pid:
                return
            rect = win32gui.GetWindowRect(hwnd)
            title = win32gui.GetWindowText(hwnd)
            cls = win32gui.GetClassName(hwnd)
            # Skip 0x0 helper windows created by tao/webview2.
            if rect == (0, 0, 0, 0) and not title:
                return
            found.append(
                WindowInfo(
                    hwnd=hwnd,
                    pid=win_pid,
                    title=title,
                    cls=cls,
                    left=rect[0],
                    top=rect[1],
                    right=rect[2],
                    bottom=rect[3],
                    visible=bool(win32gui.IsWindowVisible(hwnd)),
                    style=win32gui.GetWindowLong(hwnd, win32con.GWL_STYLE),
                    exstyle=win32gui.GetWindowLong(hwnd, win32con.GWL_EXSTYLE),
                )
            )
        except Exception:
            return

    win32gui.EnumWindows(callback, None)
    return found


def manager_window(pid: int) -> WindowInfo | None:
    for win in windows_of(pid):
        if win.title == "LexiGrab" and win.cls == "Tauri Window" and win.width > 1200:
            return win
    return None


def popup_window(pid: int) -> WindowInfo | None:
    for win in windows_of(pid):
        if win.title == "LexiGrab" and win.cls == "Tauri Window" and win.width <= 1200:
            return win
    return None


def settings_window(pid: int) -> WindowInfo | None:
    for win in windows_of(pid):
        if win.title == "LexiGrab 设置":
            return win
    return None


def wait_for(predicate: Callable[[], object | None], timeout: float = 10.0, interval: float = 0.02):
    end = time.perf_counter() + timeout
    while time.perf_counter() < end:
        value = predicate()
        if value:
            return value
        time.sleep(interval)
    return None


def foreground() -> WindowInfo:
    hwnd = win32gui.GetForegroundWindow()
    rect = win32gui.GetWindowRect(hwnd)
    _, pid = win32process.GetWindowThreadProcessId(hwnd)
    return WindowInfo(
        hwnd=hwnd,
        pid=pid,
        title=win32gui.GetWindowText(hwnd),
        cls=win32gui.GetClassName(hwnd),
        left=rect[0],
        top=rect[1],
        right=rect[2],
        bottom=rect[3],
        visible=bool(win32gui.IsWindowVisible(hwnd)),
        style=win32gui.GetWindowLong(hwnd, win32con.GWL_STYLE),
        exstyle=win32gui.GetWindowLong(hwnd, win32con.GWL_EXSTYLE),
    )


# ---------------------------------------------------------------- app lifecycle


def launch_app(extra_env: dict[str, str] | None = None, wait: float = 15.0) -> dict:
    """Start lexigrab.exe detached (cmd start) with LEXIGRAB_HOME and wait for manager."""
    env = {
        "LEXIGRAB_HOME": str(HOME),
        "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS": "--force-renderer-accessibility",
    }
    if extra_env:
        env.update(extra_env)
    HOME.mkdir(parents=True, exist_ok=True)
    child_env = os.environ.copy()
    child_env.update(env)
    subprocess.Popen(
        [str(EXE)],
        env=child_env,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP,
        close_fds=True,
    )
    started = time.perf_counter()
    pid = wait_for(app_pid, timeout=wait, interval=0.05)
    if not pid:
        raise RuntimeError("lexigrab.exe did not start")
    manager = wait_for(lambda: (manager_window(pid) or None), timeout=wait, interval=0.05)
    if not manager:
        raise RuntimeError("manager window did not appear")
    # Let the webview finish its first paint.
    wait_for(lambda: manager_window(pid).visible if manager_window(pid) else False, timeout=wait)
    time.sleep(1.0)
    return {"pid": pid, "startup_ms": round((time.perf_counter() - started) * 1000, 1)}


def kill_app(pid: int | None = None, timeout: float = 8.0) -> None:
    if pid is None:
        pid = app_pid()
    if not pid:
        return
    try:
        proc = psutil.Process(pid)
        proc.terminate()
        try:
            proc.wait(timeout=timeout)
        except psutil.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=timeout)
    except psutil.NoSuchProcess:
        pass


def restart_app(extra_env: dict[str, str] | None = None) -> dict:
    kill_app()
    time.sleep(0.5)
    return launch_app(extra_env)


# ---------------------------------------------------------------- notepad


def notepad_window() -> WindowInfo | None:
    for win in _all_windows():
        if win.cls == "Notepad" and win.visible and win.title:
            return win
    return None


def launch_notepad() -> WindowInfo:
    """Always start a fresh untitled Notepad window.

    Existing Notepad windows may belong to the user (open documents); reusing one
    would type into their file, so only windows that appear after the launch are
    considered.
    """
    before = {win.hwnd for win in _all_windows() if win.cls == "Notepad" and win.visible}
    subprocess.Popen(
        ["notepad.exe"],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        creationflags=subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP,
    )

    def fresh() -> WindowInfo | None:
        for win in _all_windows():
            if win.cls == "Notepad" and win.visible and win.hwnd not in before:
                return win
        return None

    win = wait_for(fresh, timeout=15, interval=0.1)
    if not win:
        raise RuntimeError("Notepad window did not appear")
    return win


def _all_windows() -> list[WindowInfo]:
    found: list[WindowInfo] = []

    def callback(hwnd: int, _extra: object) -> None:
        rect = win32gui.GetWindowRect(hwnd)
        title = win32gui.GetWindowText(hwnd)
        if rect == (0, 0, 0, 0) and not title:
            return
        _, pid = win32process.GetWindowThreadProcessId(hwnd)
        found.append(
            WindowInfo(
                hwnd=hwnd,
                pid=pid,
                title=title,
                cls=win32gui.GetClassName(hwnd),
                left=rect[0],
                top=rect[1],
                right=rect[2],
                bottom=rect[3],
                visible=bool(win32gui.IsWindowVisible(hwnd)),
                style=win32gui.GetWindowLong(hwnd, win32con.GWL_STYLE),
                exstyle=win32gui.GetWindowLong(hwnd, win32con.GWL_EXSTYLE),
            )
        )

    win32gui.EnumWindows(callback, None)
    return found


# ---------------------------------------------------------------- UIA access


def connect(pid: int | None = None) -> "Application":
    pid = pid or app_pid()
    if not pid:
        raise RuntimeError("app is not running")
    return Application(backend="uia").connect(process=pid, timeout=10)


def desktop() -> "Desktop":
    return Desktop(backend="uia")


def window_spec(app: "Application", win: WindowInfo):
    return app.window(handle=win.hwnd)


def manager_spec(app: "Application", pid: int | None = None):
    pid = pid or app_pid()
    win = wait_for(lambda: manager_window(pid or 0), timeout=5)
    if not win:
        raise RuntimeError("manager window missing")
    return app.window(handle=win.hwnd)


def popup_spec(app: "Application", pid: int | None = None):
    pid = pid or app_pid()
    win = wait_for(lambda: popup_window(pid or 0), timeout=5)
    if not win:
        raise RuntimeError("popup window missing")
    return app.window(handle=win.hwnd)


def wait_visible(win_getter: Callable[[int], WindowInfo | None], pid: int, timeout: float = 5.0) -> WindowInfo | None:
    return wait_for(lambda: (lambda w: w if w and w.visible else None)(win_getter(pid)), timeout=timeout)


def find_button(spec, title: str | None = None, title_re: str | None = None, timeout: float = 5.0):
    kwargs = {"control_type": "Button"}
    if title is not None:
        kwargs["title"] = title
    if title_re is not None:
        kwargs["title_re"] = title_re
    return spec.child_window(**kwargs)


def click(spec, title: str | None = None, title_re: str | None = None, timeout: float = 5.0) -> None:
    button = spec.child_window(
        **({"title": title} if title is not None else {"title_re": title_re}),
        control_type="Button",
    )
    button.wait("enabled visible", timeout=timeout)
    button.click_input()


def has_text(spec, text: str, timeout: float = 3.0) -> bool:
    end = time.perf_counter() + timeout
    while time.perf_counter() < end:
        try:
            if spec.child_window(title=text).exists(timeout=0.2):
                return True
        except Exception:
            pass
        try:
            if text in (spec.window_text() or ""):
                return True
        except Exception:
            pass
        time.sleep(0.1)
    return False


def uia_texts(spec) -> list[str]:
    texts: list[str] = []
    try:
        for item in spec.descendants():
            try:
                name = item.window_text()
            except Exception:
                continue
            if name and name.strip():
                texts.append(name.strip())
    except Exception:
        pass
    return texts


def find_element(spec, title: str | None = None, title_re: str | None = None, control_type: str | None = None, timeout: float = 5.0):
    kwargs: dict = {}
    if title is not None:
        kwargs["title"] = title
    if title_re is not None:
        kwargs["title_re"] = title_re
    if control_type is not None:
        kwargs["control_type"] = control_type
    element = spec.child_window(**kwargs)
    element.wait("exists", timeout=timeout)
    return element


# ---------------------------------------------------------------- input, timing


def send_hotkey(keys: str = "^+d") -> None:
    send_keys(keys, pause=0.01)


def focus(hwnd: int) -> None:
    try:
        win32gui.SetForegroundWindow(hwnd)
    except Exception:
        # Fallback: click the title bar area via pywinauto callers.
        pass


def select_all() -> None:
    send_keys("^a", pause=0.01)


def type_text(text: str) -> None:
    send_keys(text, with_spaces=True, with_tabs=True, pause=0.01)


def measure(fn: Callable[[], object], trials: int) -> list[float]:
    samples: list[float] = []
    for _ in range(trials):
        start = time.perf_counter()
        fn()
        samples.append((time.perf_counter() - start) * 1000)
    return samples


def p95(values: Iterable[float]) -> float:
    ordered = sorted(values)
    if not ordered:
        return 0.0
    index = max(0, min(len(ordered) - 1, int(round(0.95 * (len(ordered) - 1)))))
    return ordered[index]


# ---------------------------------------------------------------- artifacts


def screenshot_window(win: WindowInfo, name: str) -> Path:
    image = ImageGrab.grab(bbox=win.rect(), all_screens=True)
    path = ARTIFACTS / name
    image.save(path)
    return path


def screenshot_screen(name: str) -> Path:
    image = ImageGrab.grab(all_screens=True)
    path = ARTIFACTS / name
    image.save(path)
    return path


def save_json(name: str, data: object) -> Path:
    path = ARTIFACTS / name
    path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"[artifact] {path}")
    return path


# ---------------------------------------------------------------- config


def config_path() -> Path:
    return HOME / "config" / "config.toml"


def read_config() -> str:
    return config_path().read_text(encoding="utf-8")


def write_config(text: str) -> None:
    config_path().parent.mkdir(parents=True, exist_ok=True)
    config_path().write_text(text, encoding="utf-8")


def set_config_value(key: str, value: str, section: str | None = None) -> None:
    """Replace `key = ...` in the given TOML section (single-line values only)."""
    lines = read_config().splitlines()
    current = None
    output: list[str] = []
    replaced = False
    for line in lines:
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            current = stripped[1:-1]
        if current == section and re.match(rf"\s*{re.escape(key)}\s*=", line):
            indent = line[: len(line) - len(line.lstrip())]
            output.append(f"{indent}{key} = {value}")
            replaced = True
        else:
            output.append(line)
    if not replaced:
        raise RuntimeError(f"config key {section}.{key} not found")
    write_config("\n".join(output) + "\n")


def log_tail(n: int = 20) -> str:
    logs = sorted((HOME / "logs").glob("lexigrab.log.*"))
    if not logs:
        return ""
    lines = logs[-1].read_text(encoding="utf-8", errors="replace").splitlines()
    return "\n".join(lines[-n:])


def config_data() -> dict:
    import tomllib

    return tomllib.loads(read_config())


def _native_dialog(pid: int) -> WindowInfo | None:
    return wait_for(
        lambda: next(
            (w for w in windows_of(pid) if w.cls == "#32770" and w.visible and w.title in ("打开", "Open")),
            None,
        ),
        timeout=10,
    )


def choose_file(manager, path: Path) -> None:
    """Open the Tauri file picker and select a single file."""
    pid = app_pid()
    click(manager, "选择 .mdx 文件")
    dialog = _native_dialog(pid or 0)
    if not dialog:
        raise RuntimeError("native file dialog did not open")
    spec = Desktop(backend="uia").window(handle=dialog.hwnd)
    combo = spec.child_window(auto_id="1148", control_type="ComboBox")
    edit = combo.child_window(control_type="Edit")
    edit.wait("enabled visible", timeout=5)
    edit.set_edit_text(str(path.resolve()))
    time.sleep(0.2)
    edit.type_keys("{ENTER}")
    if not wait_for(
        lambda: not [w for w in windows_of(pid or 0) if w.cls == "#32770" and w.visible],
        timeout=10,
    ):
        raise RuntimeError("native file dialog did not close")


def add_source(manager, files: list[Path], name: str | None = None) -> None:
    pid = app_pid()
    click(manager, "添加源")
    find_element(manager, title="添加本地词典", control_type="Text", timeout=5)
    for file in files:
        choose_file(manager, file)
    if name is not None:
        name_edit = manager.child_window(auto_id="source-name", control_type="Edit")
        name_edit.set_edit_text(name)
    click(manager, "保存")
    wait_for(lambda: not [w for w in windows_of(pid or 0) if w.cls == "#32770" and w.visible], timeout=5)
    time.sleep(0.3)


def open_source_row(manager, name: str):
    """Return the ListItem wrapper for a source row by its exact name prefix."""
    for item in manager.descendants(control_type="ListItem"):
        try:
            text = item.window_text() or ""
        except Exception:
            continue
        if text.startswith(name) or f"\n{name}\n" in text or name in text.split("\n")[0]:
            return item
    raise AssertionError(f"source row not found: {name}")


def row_button(manager, name: str, label: str):
    row = open_source_row(manager, name)
    for button in row.descendants(control_type="Button"):
        try:
            if button.window_text() == label:
                return button
        except Exception:
            continue
    raise AssertionError(f"button {label!r} not found in row {name!r}")


def click_nav(manager, label: str) -> float:
    start = time.perf_counter()
    button = manager.child_window(title=label, control_type="Button")
    button.wait("enabled visible", timeout=5)
    button.click()
    return (time.perf_counter() - start) * 1000


def wait_text(spec, text: str, timeout: float = 10.0) -> bool:
    """Wait until any UIA element name contains `text` (avoids ambiguous lookups)."""
    return bool(
        wait_for(lambda: any(text in name for name in uia_texts(spec)), timeout=timeout, interval=0.15)
    )


def texts_containing(spec, needle: str) -> list[str]:
    return [text for text in uia_texts(spec) if needle in text]


# ---------------------------------------------------------------- misc


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def summarize_rect(win: WindowInfo) -> dict:
    return {"hwnd": win.hwnd, "title": win.title, "rect": win.rect(), "visible": win.visible}
