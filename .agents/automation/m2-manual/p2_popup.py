"""M2 manual matrix, part 2: selection capture, popup quick/edit modes, focus, latency."""

from __future__ import annotations

import time

from pywinauto import Application, Desktop
from pywinauto.keyboard import send_keys

import common as c

RESULT: dict = {"phase": "popup", "steps": [], "latency": {}}
NOACTIVATE = 0x08000000
TOPMOST = 0x00000008
TOOLWINDOW = 0x00000080


def step(name: str, fn):
    print(f"\n=== {name} ===")
    value = fn()
    entry = {"step": name, "ok": True}
    if value is not None:
        entry["detail"] = value
    RESULT["steps"].append(entry)
    print(f"--- {name}: OK")
    return value


def notepad(reset: bool = False) -> tuple[int, object]:
    win = c.launch_notepad()
    app = Application(backend="uia").connect(handle=win.hwnd, timeout=10)
    np = app.window(handle=win.hwnd)
    np.set_focus()
    time.sleep(0.3)
    if reset:
        send_keys("^a", pause=0.02)
        send_keys("{BACKSPACE}", pause=0.02)
    return win.pid, np


def notepad_set(np, text: str) -> None:
    np.set_focus()
    time.sleep(0.2)
    send_keys("^a", pause=0.02)
    send_keys("{BACKSPACE}", pause=0.02)
    send_keys(text, with_spaces=True, pause=0.02)
    time.sleep(0.2)


def select_all(np) -> None:
    np.set_focus()
    send_keys("^a", pause=0.02)


def select_first_word(np) -> None:
    np.set_focus()
    send_keys("{VK_CONTROL down}{HOME}{VK_CONTROL up}", pause=0.02)
    send_keys(
        "{VK_CONTROL down}{VK_SHIFT down}{RIGHT}{VK_SHIFT up}{VK_CONTROL up}", pause=0.03
    )


def popup_hidden(pid: int, attempts: int = 4) -> None:
    for _ in range(attempts):
        win = c.popup_window(pid)
        if win and win.visible:
            c.send_hotkey()
            time.sleep(0.45)
        else:
            return
    if c.popup_window(pid).visible:
        raise RuntimeError("could not hide popup")


def trigger_and_wait(pid: int, timeout: float = 5.0) -> tuple[object, float]:
    start = time.perf_counter()
    c.send_hotkey()
    shown = c.wait_for(
        lambda: (lambda w: w if w and w.visible else None)(c.popup_window(pid)),
        timeout=timeout,
        interval=0.001,
    )
    latency = (time.perf_counter() - start) * 1000
    if not shown:
        raise RuntimeError("popup did not become visible")
    return shown, latency


def popup_spec(app, win):
    return app.window(handle=win.hwnd)


def wait_popup_text(app, pid: int, text: str, timeout: float = 5.0) -> bool:
    end = time.perf_counter() + timeout
    while time.perf_counter() < end:
        win = c.popup_window(pid)
        if win and win.visible:
            try:
                if any(text in t for t in c.uia_texts(app.window(handle=win.hwnd))):
                    return True
            except Exception:
                pass
        time.sleep(0.1)
    return False


def close_popup(app, pid: int) -> None:
    win = c.popup_window(pid)
    if not win or not win.visible:
        return
    spec = app.window(handle=win.hwnd)
    spec.child_window(title="关闭弹窗", control_type="Button").click_input()
    c.wait_for(lambda: (lambda w: w and not w.visible)(c.popup_window(pid)), timeout=5)


def main() -> int:
    pid = c.app_pid()
    app = c.connect(pid)
    popup_hidden(pid)
    np_win = c.launch_notepad()
    np_hwnd = np_win.hwnd
    np_pid, np = notepad(reset=True)
    fg_np = c.foreground()
    print("notepad hwnd:", hex(fg_np.hwnd), fg_np.title)

    def quick_mode():
        notepad_set(np, "hello")
        select_all(np)
        shown, latency = trigger_and_wait(pid)
        time.sleep(0.8)
        spec = popup_spec(app, shown)
        texts = c.uia_texts(spec)
        fg = c.foreground()
        assert fg.hwnd == np_hwnd, f"focus stolen by {fg.title}"
        assert "hello" in texts, texts[:30]
        assert any("hello · mini" in t for t in texts), texts[:40]
        assert any(" · " in t and "mini" in t for t in texts)
        assert any("原句" in t for t in texts)
        assert any("保存所选义项" in t for t in texts)
        ex = shown.exstyle
        width_ok = 800 <= shown.width <= 900  # 420 logical * 2 DPI (+ borders)
        assert width_ok, shown.rect()
        c.screenshot_window(shown, "20-popup-quick-hello.png")
        close_popup(app, pid)
        return {
            "latency_ms": round(latency, 1),
            "rect": shown.rect(),
            "exstyle_noactivate": bool(ex & NOACTIVATE),
            "exstyle_topmost": bool(ex & TOPMOST),
            "exstyle_toolwindow": bool(ex & TOOLWINDOW),
            "foreground_after": fg.title,
            "texts": [t for t in texts if len(t) < 60][:25],
        }

    step("quick mode: selection -> popup, focus stays in Notepad", quick_mode)

    def edit_and_restore():
        notepad_set(np, "hello")
        select_all(np)
        shown, latency = trigger_and_wait(pid)
        time.sleep(0.8)
        spec = popup_spec(app, shown)
        spec.child_window(title="编辑", control_type="Button").click_input()
        # Edit mode: activating show() resizes to 560x640 and takes focus.
        edited = c.wait_for(
            lambda: (
                lambda w: w if w and w.visible and w.width > 1000 else None
            )(c.popup_window(pid)),
            timeout=5,
        )
        assert edited, c.popup_window(pid).rect()
        time.sleep(0.5)
        fg = c.foreground()
        assert fg.hwnd == edited.hwnd, f"edit popup not focused: {fg.title}"
        assert not (edited.exstyle & NOACTIVATE), hex(edited.exstyle)
        edited_spec = popup_spec(app, edited)
        edit_texts = c.uia_texts(edited_spec)
        assert any("编辑模式" in t for t in edit_texts)
        assert any("保存卡片" in t for t in edit_texts)
        c.screenshot_window(edited, "21-popup-edit-mode.png")

        # Esc cancels editing but keeps the popup open in the results view.
        send_keys("{ESC}", pause=0.02)
        time.sleep(0.6)
        assert c.popup_window(pid).visible, "popup closed on Esc"
        c.screenshot_window(c.popup_window(pid), "22-popup-after-esc.png")
        close_ms_start = time.perf_counter()
        close_popup(app, pid)
        close_ms = (time.perf_counter() - close_ms_start) * 1000
        fg2 = c.foreground()
        assert fg2.hwnd == np_hwnd, f"focus not restored: {fg2.title}"
        return {
            "quick_latency_ms": round(latency, 1),
            "edit_rect": edited.rect(),
            "edit_foreground": fg.title,
            "close_ms": round(close_ms, 1),
            "foreground_after_close": fg2.title,
        }

    step("edit mode takes focus, X restores Notepad focus", edit_and_restore)

    def hotkey_toggle():
        notepad_set(np, "hello")
        select_all(np)
        trigger_and_wait(pid)
        time.sleep(0.5)
        assert c.popup_window(pid).visible
        c.send_hotkey()
        hidden = c.wait_for(lambda: (lambda w: w and not w.visible)(c.popup_window(pid)), timeout=3)
        assert hidden, "second hotkey did not hide popup"
        assert c.foreground().hwnd == np_hwnd
        return "second hotkey hides the popup, focus stays in Notepad"

    step("hotkey toggles the visible popup", hotkey_toggle)

    def sentence_capture():
        notepad_set(np, "hello is in this test sentence.")
        select_first_word(np)
        shown, _ = trigger_and_wait(pid)
        time.sleep(1.0)
        spec = popup_spec(app, shown)
        texts = c.uia_texts(spec)
        assert any("hello" == t for t in texts) or any(t.startswith("hello") for t in texts), texts[:20]
        # The sentence row must contain the whole line, not just the selected word.
        sentence_line = next((t for t in texts if "test sentence" in t), None)
        assert sentence_line, texts
        c.screenshot_window(shown, "23-popup-sentence.png")
        close_popup(app, pid)
        return {"sentence": sentence_line}

    step("UIA sentence capture fills 原句", sentence_capture)

    def empty_result():
        notepad_set(np, "zzzznotaword")
        select_all(np)
        shown, _ = trigger_and_wait(pid)
        assert wait_popup_text(app, pid, "未找到释义", 5)
        c.screenshot_window(shown, "24-popup-empty.png")
        close_popup(app, pid)
        return "empty state rendered"

    step("empty result state", empty_result)

    def capture_error():
        # Focus the manager (no text selection) and press the hotkey.
        manager_win = c.manager_window(pid)
        import win32gui

        win32gui.SetForegroundWindow(manager_win.hwnd)
        time.sleep(0.4)
        shown, _ = trigger_and_wait(pid)
        assert wait_popup_text(app, pid, "未捕获到文本", 5) or wait_popup_text(
            app, pid, "未能读取选区", 3
        ), c.uia_texts(popup_spec(app, shown))
        c.screenshot_window(shown, "25-popup-capture-error.png")
        close_popup(app, pid)
        return "capture error state rendered"

    step("capture error state", capture_error)

    def edge_capture():
        import subprocess

        edge_exe = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"
        profile = c.ARTIFACTS / "edge-profile"
        subprocess.Popen(
            [
                edge_exe,
                "--new-window",
                "--no-first-run",
                "--no-default-browser-check",
                f"--user-data-dir={profile}",
                "data:text/html,<html><body><h1>hello</h1></body></html>",
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

        def edge_window():
            for win in c._all_windows():
                if win.cls == "Chrome_WidgetWin_1" and win.visible and "hello" in win.title:
                    return win
            return None

        win = c.wait_for(edge_window, timeout=20, interval=0.2)
        assert win, "Edge window did not appear"
        import win32gui

        win32gui.SetForegroundWindow(win.hwnd)
        time.sleep(1.0)
        send_keys("^a", pause=0.02)
        time.sleep(0.3)
        shown, latency = trigger_and_wait(pid)
        time.sleep(0.8)
        texts = c.uia_texts(popup_spec(app, shown))
        assert any(t == "hello" for t in texts), texts[:25]
        c.screenshot_window(shown, "26-popup-edge.png")
        close_popup(app, pid)
        return {"edge_title": win.title, "latency_ms": round(latency, 1)}

    step("Edge (Chromium) selection capture", edge_capture)

    def latency_trials():
        notepad_set(np, "hello")
        select_all(np)
        popup_hidden(pid)
        visible_samples: list[float] = []
        first_result_samples: list[float] = []
        for index in range(12):
            popup_hidden(pid)
            select_all(np)
            start = time.perf_counter()
            c.send_hotkey()
            win = c.wait_for(
                lambda: (lambda w: w if w and w.visible else None)(c.popup_window(pid)),
                timeout=2,
                interval=0.001,
            )
            visible_ms = (time.perf_counter() - start) * 1000
            if not win:
                raise RuntimeError("popup missing during latency trial")
            visible_samples.append(visible_ms)
            spec = app.window(handle=win.hwnd)
            ready = False
            while (time.perf_counter() - start) < 2:
                try:
                    if spec.child_window(
                        title="hello · mini", control_type="RadioButton"
                    ).exists(timeout=0.01):
                        ready = True
                        break
                except Exception:
                    pass
                time.sleep(0.005)
            assert ready, "result did not appear"
            first_result_samples.append((time.perf_counter() - start) * 1000)
            close_popup(app, pid)
            print(
                f"  trial {index + 1}: visible {visible_ms:.1f} ms, first result {first_result_samples[-1]:.1f} ms"
            )
        stats = {
            "visible_ms": [round(v, 1) for v in visible_samples],
            "first_result_ms": [round(v, 1) for v in first_result_samples],
            "visible_p50": round(sorted(visible_samples)[len(visible_samples) // 2], 1),
            "visible_p95": round(c.p95(visible_samples), 1),
            "first_result_p50": round(sorted(first_result_samples)[len(first_result_samples) // 2], 1),
            "first_result_p95": round(c.p95(first_result_samples), 1),
        }
        c.screenshot_window(c.popup_window(pid), "27-popup-latency-final.png")
        return stats

    RESULT["latency"] = step("hotkey latency over 12 trials", latency_trials)

    c.save_json("p2-popup.json", RESULT)
    print("\nP2 popup: all steps passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
