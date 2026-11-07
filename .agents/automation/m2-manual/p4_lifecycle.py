"""M2 manual matrix, part 4: close-to-tray, single instance, tray menu and quit."""

from __future__ import annotations

import subprocess
import time
from pathlib import Path

import psutil
import win32con
import win32gui
from pywinauto import Desktop

import common as c

RESULT: dict = {"phase": "lifecycle", "steps": []}


def step(name: str, fn):
    print(f"\n=== {name} ===")
    value = fn()
    entry = {"step": name, "ok": True}
    if value is not None:
        entry["detail"] = value
    RESULT["steps"].append(entry)
    print(f"--- {name}: OK")
    return value


def post_close(hwnd: int) -> None:
    win32gui.PostMessage(hwnd, win32con.WM_CLOSE, 0, 0)


def wait_hidden(getter, timeout: float = 5.0) -> bool:
    return bool(
        c.wait_for(
            lambda: (lambda w: w if w and not w.visible else None)(getter()),
            timeout=timeout,
            interval=0.1,
        )
    )


def wait_visible(getter, timeout: float = 5.0) -> bool:
    return bool(
        c.wait_for(
            lambda: (lambda w: w if w and w.visible else None)(getter()),
            timeout=timeout,
            interval=0.1,
        )
    )


def app_processes() -> list[psutil.Process]:
    return [
        proc
        for proc in psutil.process_iter(["pid", "name"])
        if (proc.info["name"] or "").lower() == "lexigrab.exe"
    ]


def taskbar():
    for w in Desktop(backend="uia").windows():
        try:
            if w.class_name() == "Shell_TrayWnd":
                return w
        except Exception:
            continue
    raise RuntimeError("taskbar not found")


def overflow_window():
    def find():
        found = []
        win32gui.EnumWindows(
            lambda h, _: found.append(h)
            if win32gui.IsWindowVisible(h)
            and win32gui.GetClassName(h) == "TopLevelWindowForOverflowXamlIsland"
            else None,
            None,
        )
        return found[0] if found else None

    return c.wait_for(find, timeout=5, interval=0.1)


def find_menu():
    def find():
        found = []
        win32gui.EnumWindows(
            lambda h, _: found.append(h)
            if win32gui.IsWindowVisible(h) and win32gui.GetClassName(h) == "#32768"
            else None,
            None,
        )
        return found[0] if found else None

    handle = c.wait_for(find, timeout=5, interval=0.05)
    return Desktop(backend="uia").window(handle=handle) if handle else None


def menu_items(menu) -> list[str]:
    return [item.window_text() for item in menu.descendants(control_type="MenuItem")]


def open_lexigrab_tray_menu():
    """Open the overflow flyout and right-click the LexiGrab tray icon."""
    from pywinauto.keyboard import send_keys

    show = next(
        (b for b in taskbar().descendants(control_type="Button") if b.window_text() == "显示隐藏的图标"),
        None,
    )
    if show is None:
        raise RuntimeError("overflow button not found")

    def ensure_flyout():
        handle = overflow_window()
        if not handle:
            show.click_input()
            handle = overflow_window()
        if not handle:
            raise RuntimeError("overflow flyout did not open")
        return Desktop(backend="uia").window(handle=handle)

    flyout = ensure_flyout()
    for _round in range(3):
        buttons = [
            b
            for b in flyout.descendants(control_type="Button")
            if (b.element_info.automation_id or "") == "NotifyItemIcon"
        ]
        # The LexiGrab tray icon has an empty accessible name.
        buttons.sort(key=lambda b: 0 if not b.window_text() else 1)
        for button in buttons:
            try:
                button.click_input(button="right")
            except Exception:
                continue
            menu = find_menu()
            if menu is None:
                continue
            items = menu_items(menu)
            if any("打开管理器" in item for item in items):
                return menu, items
            send_keys("{ESC}")
            time.sleep(0.4)
            flyout = ensure_flyout()
        send_keys("{ESC}")
        time.sleep(0.3)
        flyout = ensure_flyout()
    raise RuntimeError("LexiGrab tray icon / menu not found")


def click_menu_item(menu, label: str) -> None:
    item = next(i for i in menu.descendants(control_type="MenuItem") if label in i.window_text())
    item.click_input()


def main() -> int:
    # Deterministic start: restart the app so the manager is visible.
    info = c.restart_app()
    pid = c.app_pid()
    assert pid, "app not running"
    app = c.connect(pid)
    RESULT["pid"] = pid
    RESULT["launch"] = info
    assert wait_visible(lambda: c.manager_window(pid)), "manager not visible after restart"

    def close_to_tray():
        assert wait_visible(lambda: c.manager_window(pid)), "manager not visible"
        post_close(c.manager_window(pid).hwnd)
        assert wait_hidden(lambda: c.manager_window(pid)), "manager did not hide on close"
        assert c.manager_window(pid).visible is False
        assert c.settings_window(pid) and not c.settings_window(pid).visible
        procs = app_processes()
        assert len(procs) == 1 and psutil.pid_exists(pid), [p.pid for p in procs]
        return {"manager_visible": False, "processes": [p.pid for p in procs], "alive": True}

    step("closing the manager only hides it (close to tray)", close_to_tray)

    def single_instance():
        before = [p.pid for p in app_processes()]
        process = subprocess.Popen(
            [str(c.EXE)],
            cwd=str(c.EXE.parent),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        exited = c.wait_for(lambda: process.poll() is not None, timeout=15, interval=0.1)
        assert exited, "second instance did not exit"
        after = [p.pid for p in app_processes()]
        assert after == before, (before, after)
        assert wait_visible(lambda: c.manager_window(pid)), "first instance did not show manager"
        return {
            "second_exit_code": process.returncode,
            "processes": after,
            "manager_visible": True,
        }

    step("second launch exits and focuses the first instance", single_instance)

    def tray_settings():
        post_close(c.manager_window(pid).hwnd)
        assert wait_hidden(lambda: c.manager_window(pid))
        menu, items = open_lexigrab_tray_menu()
        RESULT.setdefault("tray_menu_items", items)
        click_menu_item(menu, "设置")
        assert wait_visible(lambda: c.settings_window(pid)), "settings did not open"
        time.sleep(0.8)
        c.screenshot_window(c.settings_window(pid), "40-settings-from-tray.png")
        post_close(c.settings_window(pid).hwnd)
        assert wait_hidden(lambda: c.settings_window(pid)), "settings did not hide"
        return {"menu_items": items, "settings_opened": True}

    step("tray menu: 设置 opens the settings window", tray_settings)

    def tray_manager_and_quit():
        menu, items = open_lexigrab_tray_menu()
        click_menu_item(menu, "打开管理器")
        assert wait_visible(lambda: c.manager_window(pid)), "manager did not open"
        time.sleep(0.5)
        post_close(c.manager_window(pid).hwnd)
        assert wait_hidden(lambda: c.manager_window(pid))
        menu, items = open_lexigrab_tray_menu()
        click_menu_item(menu, "退出")
        gone = c.wait_for(lambda: not app_processes(), timeout=20, interval=0.2)
        assert gone, "app did not exit from tray"
        return {"quit_from_tray": True, "menu_items": items}

    step("tray menu: 打开管理器 and 退出", tray_manager_and_quit)

    c.save_json("p4-lifecycle.json", RESULT)
    print("\nP4 lifecycle: all steps passed; app exited via tray")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
