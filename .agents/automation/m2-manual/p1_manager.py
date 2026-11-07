"""M2 manual matrix, part 1: manager source CRUD, sorting, persistence, big dictionary."""

from __future__ import annotations

import shutil
import time
from pathlib import Path

from pywinauto import Desktop

import common as c

RESULT: dict = {"phase": "manager", "steps": []}


def step(name: str, fn):
    started = time.perf_counter()
    print(f"\n=== {name} ===")
    value = fn()
    entry = {"step": name, "ok": True, "seconds": round(time.perf_counter() - started, 2)}
    if value is not None:
        entry["detail"] = value
    RESULT["steps"].append(entry)
    print(f"--- {name}: OK {entry['seconds']}s")
    return value


def source_names(manager) -> list[str]:
    rows = []
    for item in manager.descendants(control_type="ListItem"):
        try:
            text = item.window_text() or ""
        except Exception:
            continue
        if not text.strip():
            continue
        rows.append(text)
    return rows


def wait_status(manager, needle: str, timeout: float = 30.0) -> bool:
    return bool(
        c.wait_for(lambda: any(needle in t for t in c.uia_texts(manager)), timeout=timeout, interval=0.3)
    )


def main() -> int:
    # Fresh profile: the manager must be able to bootstrap the config itself.
    c.kill_app()
    shutil.rmtree(c.HOME, ignore_errors=True)
    fixture_src = c.ROOT / "src-tauri" / "tests" / "fixtures"
    for name in ("mini.mdx", "mini.mdd"):
        shutil.copy2(fixture_src / name, c.FIXTURES / name)
    shutil.copy2(c.FIXTURES / "mini.mdx", c.FIXTURES / "mini-copy.mdx")
    shutil.copy2(c.FIXTURES / "mini.mdd", c.FIXTURES / "mini-copy.mdd")
    launch = c.launch_app()
    RESULT["launch"] = launch
    print("app:", launch)

    pid = c.app_pid()
    app = c.connect(pid)
    manager = c.manager_spec(app, pid)
    assert c.wait_text(manager, "词典源"), "manager did not render"
    c.screenshot_window(c.manager_window(pid), "01-manager-empty.png")
    assert "还没有词典源" in c.uia_texts(manager), "expected empty state"

    def add_mini():
        c.add_source(manager, [c.FIXTURES / "mini.mdx"])
        assert wait_status(manager, "3 条目", 10), c.texts_containing(manager, "条目")
        texts = [t for t in c.uia_texts(manager) if "mini" in t]
        c.screenshot_window(c.manager_window(pid), "02-manager-one-source.png")
        return texts

    step("add mini.mdx via file picker", add_mini)

    def query_hello():
        click_ms = c.click_nav(manager, "查询测试")
        assert c.wait_text(manager, "要查询的词", 5), "query tester did not render"
        query = manager.child_window(auto_id="tester-query", control_type="Edit")
        query.click_input()
        c.type_text("hello")
        c.click(manager, "查询")
        assert c.wait_for(
            lambda: any("条结果" in t for t in c.uia_texts(manager)), timeout=10, interval=0.2
        ), "query produced no summary"
        time.sleep(0.5)
        texts = [t for t in c.uia_texts(manager) if "条结果" in t or "最长耗时" in t or "Mini" in t]
        c.screenshot_window(c.manager_window(pid), "03-manager-query-hello.png")
        c.click_nav(manager, "词典源")
        return {"nav_switch_ms": round(click_ms, 1), "texts": texts}

    step("query tester: hello", query_hello)

    def add_second():
        c.add_source(manager, [c.FIXTURES / "mini-copy.mdx"])
        assert wait_status(manager, "3 条目", 10)
        time.sleep(0.5)
        config = c.config_data()
        names = [s["name"] for s in config["sources"]]
        assert names == ["mini", "mini-copy"], names
        assert [s["priority"] for s in config["sources"]] == [10, 20]
        c.screenshot_window(c.manager_window(pid), "04-manager-two-sources.png")
        return {"sources": [{"name": s["name"], "priority": s["priority"]} for s in config["sources"]]}

    step("add second source (mini-copy)", add_second)

    def reorder():
        c.row_button(manager, "mini-copy", "上移 mini-copy").click_input()
        def ordered():
            return sorted(c.config_data()["sources"], key=lambda item: item["priority"])

        assert c.wait_for(
            lambda: [s["name"] for s in ordered()] == ["mini-copy", "mini"],
            timeout=5,
            interval=0.2,
        ), ordered()
        assert [s["priority"] for s in ordered()] == [10, 20]
        # UI order follows priority: first ListItem must now be mini-copy.
        order = source_names(manager)
        c.screenshot_window(c.manager_window(pid), "05-manager-reordered.png")
        return {"ui_rows": order[:2]}

    step("move source up (reorder + priority rewrite)", reorder)

    def by_name(name: str) -> dict:
        return next(s for s in c.config_data()["sources"] if s["name"] == name)

    def toggle():
        c.row_button(manager, "mini-copy", "停用 mini-copy").click_input()
        assert c.wait_for(lambda: by_name("mini-copy")["enabled"] is False, timeout=5, interval=0.2), by_name("mini-copy")
        assert wait_status(manager, "已停用", 5)
        c.screenshot_window(c.manager_window(pid), "06-manager-source-disabled.png")
        c.row_button(manager, "mini-copy", "启用 mini-copy").click_input()
        assert c.wait_for(lambda: by_name("mini-copy")["enabled"] is True, timeout=5, interval=0.2), by_name("mini-copy")
        return "disable/enable persisted"

    step("disable + enable source", toggle)

    def edit_name():
        c.row_button(manager, "mini-copy", "编辑").click_input()
        assert c.wait_text(manager, "编辑词典源", 5)
        name_edit = manager.child_window(auto_id="source-name", control_type="Edit")
        name_edit.wait("enabled visible", timeout=5)
        name_edit.click_input()
        time.sleep(0.4)
        from pywinauto.keyboard import send_keys

        send_keys("^a", pause=0.02)
        send_keys("mini copy", with_spaces=True, pause=0.03)
        time.sleep(0.3)
        if "mini copy" not in name_edit.window_text():
            name_edit.set_edit_text("mini copy")
            time.sleep(0.3)
        assert "mini copy" in name_edit.window_text(), name_edit.window_text()
        c.click(manager, "保存")
        assert c.wait_for(
            lambda: any(s["name"] == "mini copy" for s in c.config_data()["sources"]),
            timeout=5,
            interval=0.2,
        ), c.config_data()["sources"]
        assert wait_status(manager, "mini copy", 3)
        c.screenshot_window(c.manager_window(pid), "07-manager-edited-name.png")
        return "renamed to 'mini copy'"

    step("edit source name", edit_name)

    def duplicate_path():
        c.click(manager, "添加源")
        assert c.wait_text(manager, "添加本地词典", 5)
        c.choose_file(manager, c.FIXTURES / "mini.mdx")
        c.click(manager, "保存")
        assert c.wait_for(
            lambda: any("该文件已被「mini」使用" in t for t in c.uia_texts(manager)),
            timeout=5,
            interval=0.2,
        ), c.texts_containing(manager, "已被")
        c.screenshot_window(c.manager_window(pid), "08-manager-duplicate-path.png")
        c.click(manager, "关闭对话框")
        return "duplicate path rejected with inline error"

    step("duplicate path is rejected", duplicate_path)

    oxford = c.DICT_DIR / "Oxford ALD_9th_En-En.mdx"

    def add_large():
        c.add_source(manager, [oxford])
        # Immediately try to navigate while the 41 MB dictionary opens in background.
        switch_ms = c.click_nav(manager, "运行状态")
        assert c.wait_text(manager, "热键、词典加载数与本地目录。", 5), "status view did not render"
        c.screenshot_window(c.manager_window(pid), "09-manager-status-during-load.png")
        c.click_nav(manager, "词典源")
        started = time.perf_counter()
        ready = c.wait_for(
            lambda: any("Oxford" in t and "条目" in t for t in c.uia_texts(manager)),
            timeout=60,
            interval=0.3,
        )
        load_s = time.perf_counter() - started
        assert ready, "Oxford source never became ready"
        c.screenshot_window(c.manager_window(pid), "10-manager-large-dict-ready.png")
        return {"nav_switch_ms": round(switch_ms, 1), "load_seconds": round(load_s, 2)}

    step("add 41 MB Oxford dictionary, keep UI responsive", add_large)

    def query_apple():
        c.click_nav(manager, "查询测试")
        assert c.wait_text(manager, "要查询的词", 5)
        query = manager.child_window(auto_id="tester-query", control_type="Edit")
        query.click_input()
        c.type_text("apple")
        c.click(manager, "查询")
        assert c.wait_for(
            lambda: any("条结果" in t for t in c.uia_texts(manager)), timeout=20, interval=0.2
        ), "query produced no summary"
        time.sleep(0.6)
        summary = [t for t in c.uia_texts(manager) if "条结果" in t]
        c.screenshot_window(c.manager_window(pid), "11-manager-query-apple.png")
        c.click_nav(manager, "词典源")
        return summary

    step("query tester: apple (large dictionary)", query_apple)

    def delete_source():
        assert c.wait_for(
            lambda: any("mini copy" in t for t in c.uia_texts(manager)), timeout=10, interval=0.2
        ), "source list did not render"
        c.row_button(manager, "mini copy", "删除").click_input()
        assert c.wait_text(manager, "删除词典源？", 5)
        c.click(manager, "删除配置文件")
        assert c.wait_for(
            lambda: sorted(s["name"] for s in c.config_data()["sources"])
            == ["Oxford ALD_9th_En-En", "mini"],
            timeout=10,
            interval=0.3,
        ), c.config_data()["sources"]
        time.sleep(0.5)
        c.screenshot_window(c.manager_window(pid), "12-manager-after-delete.png")
        return [s["name"] for s in c.config_data()["sources"]]

    step("delete a source", delete_source)

    def restart_persist():
        before = sorted(
            (
                {"name": s["name"], "enabled": s["enabled"], "priority": s["priority"]}
                for s in c.config_data()["sources"]
            ),
            key=lambda item: item["name"],
        )
        info = c.restart_app()
        pid2 = c.app_pid()
        app2 = c.connect(pid2)
        manager2 = c.manager_spec(app2, pid2)
        assert c.wait_text(manager2, "词典源", 10)
        ok = c.wait_for(
            lambda: all(
                any(s["name"] in t for t in c.uia_texts(manager2)) for s in before
            ),
            timeout=15,
            interval=0.3,
        )
        assert ok, c.uia_texts(manager2)
        after = sorted(
            (
                {"name": s["name"], "enabled": s["enabled"], "priority": s["priority"]}
                for s in c.config_data()["sources"]
            ),
            key=lambda item: item["name"],
        )
        assert after == before, (after, before)
        c.screenshot_window(c.manager_window(pid2), "13-manager-after-restart.png")
        return {"startup": info, "sources": before}

    step("restart keeps sources and order", restart_persist)

    c.save_json("p1-manager.json", RESULT)
    print("\nP1 manager: all steps passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

