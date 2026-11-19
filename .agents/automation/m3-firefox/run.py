"""M3 真机验收编排：Firefox Developer Edition + 真实桌面端 + 真实 Anki。

场景顺序：
1. 安装 .xpi、选项页配对；
2. 本地中英文页划词→查询→制卡、同词同句/不同句；
3. Anki 关闭→错误提示→重启重试；
4. 令牌轮换后的错误与重新配对；
5. 重启 Firefox 后扩展与 storage.local 保留；
6. 真实站点矩阵（Wikipedia 英文/中文、新闻页）；
7. 端口冲突；
8. 桥接鉴权/CORS 实机复核。

产物：artifacts/results.json、截图与 firefox.log。
"""

from __future__ import annotations

import base64
import json
import socket
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

import anki_api
from desktop import (
    BRIDGE_PORT,
    ROTATED_TOKEN,
    TEMP_ROOT,
    TOKEN,
    DesktopApp,
    build_config,
    http_json,
)
from ffx import FirefoxSession
from testpage import TestPageServer

HERE = Path(__file__).resolve().parent
ARTIFACTS = HERE / "artifacts"
XPI = HERE.parents[2] / "extension" / "dist" / "lexigrab-firefox-0.1.0.xpi"
PROFILE = ARTIFACTS / "profile"
ANKI_EXE = Path(r"D:\Anki\Anki.exe")

RESULTS: list[dict[str, Any]] = []


def record(name: str, ok: bool, detail: Any = None) -> None:
    entry = {"scenario": name, "ok": ok, "detail": detail}
    RESULTS.append(entry)
    print(("PASS " if ok else "FAIL ") + name + ("" if detail is None else ": " + json.dumps(detail, ensure_ascii=False)[:800]))


class Browser:
    """Marionette 之上的一组扩展交互。"""

    def __init__(self, client) -> None:  # noqa: ANN001 - marionette.Marionette
        self.client = client
        self.shots = 0

    # ---------------------------------------------------------------- 基础

    def js(self, script: str, args: list[Any] | None = None) -> Any:
        return self.client.execute_script(script, args=args)

    def wait_js(self, script: str, args: list[Any] | None = None, timeout: float = 15.0, interval: float = 0.2) -> Any:
        deadline = time.time() + timeout
        latest: Any = None
        while time.time() < deadline:
            latest = self.js(script, args)
            if latest:
                return latest
            time.sleep(interval)
        return latest

    def screenshot(self, name: str) -> str:
        path = ARTIFACTS / f"{name}.png"
        path.write_bytes(base64.b64decode(self.client.screenshot()))
        return str(path)

    # ---------------------------------------------------------------- 页面

    def open(self, url: str, settle: float = 1.5, timeout: float = 40.0) -> str:
        self.client.navigate(url, timeout=timeout)
        deadline = time.time() + 25
        while time.time() < deadline:
            try:
                if self.js("return document.readyState") == "complete":
                    break
            except Exception:  # noqa: BLE001 - 导航中上下文可能被销毁
                pass
            time.sleep(0.3)
        # content script 以 document_idle 注入；等宿主挂载再操作。
        self.wait_js("return !!document.getElementById('lexigrab-overlay-host')", timeout=20)
        time.sleep(settle)
        return self.js("return document.title")

    def select(self, word: str, occurrence: int = 0) -> dict[str, Any] | None:
        return self.js(
            """
            const word = arguments[0].toLowerCase();
            const wanted = arguments[1];
            const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
            let node;
            let seen = 0;
            while ((node = walker.nextNode())) {
              const parent = node.parentElement;
              if (!parent || parent.closest("script,style,noscript,#lexigrab-overlay-host")) continue;
              const text = (node.nodeValue || "").toLowerCase();
              let index = text.indexOf(word);
              while (index >= 0) {
                if (seen === wanted) {
                  const range = document.createRange();
                  range.setStart(node, index);
                  range.setEnd(node, index + word.length);
                  const selection = window.getSelection();
                  selection.removeAllRanges();
                  selection.addRange(range);
                  document.dispatchEvent(new MouseEvent("mouseup", { bubbles: true, composed: true, view: window }));
                  const block = parent.closest("p,li,h1,h2,h3,h4,td,article,div");
                  return {
                    selected: node.nodeValue.slice(index, index + word.length),
                    block: (block ? block.innerText : node.nodeValue).trim().slice(0, 240),
                  };
                }
                seen += 1;
                index = text.indexOf(word, index + word.length);
              }
            }
            return null;
            """,
            [word, occurrence],
        )

    def query_button(self) -> bool:
        return bool(
            self.js(
                """
                const host = document.getElementById("lexigrab-overlay-host");
                if (!host || !host.shadowRoot) return false;
                return !!host.shadowRoot.querySelector('button[aria-label="用 LexiGrab 查询所选单词"]');
                """
            )
        )

    def click_query(self) -> bool:
        return bool(
            self.js(
                """
                const host = document.getElementById("lexigrab-overlay-host");
                const button = host && host.shadowRoot
                  ? host.shadowRoot.querySelector('button[aria-label="用 LexiGrab 查询所选单词"]')
                  : null;
                if (!button) return false;
                button.click();
                return true;
                """
            )
        )

    def dialog(self) -> str | None:
        return self.js(
            """
            const host = document.getElementById("lexigrab-overlay-host");
            if (!host || !host.shadowRoot) return null;
            const dialog = host.shadowRoot.querySelector('[role="dialog"]');
            return dialog ? dialog.innerText : null;
            """
        )

    def wait_dialog(self, timeout: float = 20.0) -> str | None:
        deadline = time.time() + timeout
        latest: str | None = None
        while time.time() < deadline:
            latest = self.dialog()
            if latest and "正在查询" not in latest and "正在获取语境建议" not in latest:
                return latest
            time.sleep(0.25)
        return latest

    def click_text(self, needle: str) -> bool:
        return bool(
            self.js(
                """
                const host = document.getElementById("lexigrab-overlay-host");
                if (!host || !host.shadowRoot) return false;
                const button = [...host.shadowRoot.querySelectorAll("button")]
                  .find((item) => item.textContent.includes(arguments[0]));
                if (!button) return false;
                button.click();
                return true;
                """,
                [needle],
            )
        )

    def close_bubble(self) -> bool:
        return bool(
            self.js(
                """
                const host = document.getElementById("lexigrab-overlay-host");
                const button = host && host.shadowRoot
                  ? host.shadowRoot.querySelector('button[aria-label="关闭"]')
                  : null;
                if (!button) return false;
                button.click();
                return true;
                """
            )
        )

    def overlay_gone(self) -> bool:
        return not self.js(
            """
            const host = document.getElementById("lexigrab-overlay-host");
            return !!(host && host.shadowRoot && host.shadowRoot.querySelector('[role="dialog"]'));
            """
        )

    def select_and_open(self, word: str, occurrence: int = 0, timeout: float = 20.0) -> dict[str, Any]:
        picked = self.select(word, occurrence)
        if not picked:
            return {"ok": False, "reason": "selection-not-found", "dialog": None}
        if not self.wait_js(
            """
            const host = document.getElementById("lexigrab-overlay-host");
            if (!host || !host.shadowRoot) return false;
            return !!host.shadowRoot.querySelector('button[aria-label="用 LexiGrab 查询所选单词"]');
            """,
            timeout=6,
        ):
            return {"ok": False, "reason": "query-button-missing", "picked": picked, "dialog": None}
        self.click_query()
        dialog = self.wait_dialog(timeout)
        return {"ok": bool(dialog), "picked": picked, "dialog": dialog}

    # ---------------------------------------------------------------- 选项页

    def pair(self, port: int, token: str) -> dict[str, Any]:
        inputs = self.wait_js("return document.querySelectorAll('input').length >= 2", timeout=10)
        if not inputs:
            return {"ok": False, "reason": "options-inputs-missing"}
        self.js(
            """
            const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
            const inputs = document.querySelectorAll("input");
            setter.call(inputs[0], arguments[0]);
            inputs[0].dispatchEvent(new Event("input", { bubbles: true }));
            setter.call(inputs[1], arguments[1]);
            inputs[1].dispatchEvent(new Event("input", { bubbles: true }));
            return true;
            """,
            [str(port), token],
        )
        self.js(
            """
            const button = [...document.querySelectorAll("button")].find((item) => item.textContent.trim() === "测试连接");
            button.click();
            return true;
            """
        )
        notice = self.wait_js("return document.querySelector('[role=status]')?.innerText ?? null", timeout=15)
        alert = self.js("return document.querySelector('[role=alert]')?.innerText ?? null")
        self.js(
            """
            const button = [...document.querySelectorAll("button")].find((item) => item.textContent.trim() === "保存");
            button.click();
            return true;
            """
        )
        save_notice = self.wait_js(
            "return ([...document.querySelectorAll('[role=status]')].map((item) => item.innerText).find((text) => text.includes('已保存'))) ?? null",
            timeout=10,
        )
        return {"ok": bool(notice and save_notice), "notice": notice, "alert": alert, "save": save_notice}

    def read_settings(self) -> Any:
        return self.js(
            """
            return browser.storage.local.get("lexigrab:settings").then((result) => result["lexigrab:settings"] ?? null);
            """
        )


# ---------------------------------------------------------------------- 场景


def scenario_local_matrix(browser: Browser, server: TestPageServer) -> None:
    anki_api.delete_notes(f'deck:"{anki_api.DECK}"')
    page = browser.open(server.url("/en"))
    result = browser.select_and_open("hello")
    dialog = result.get("dialog") or ""
    ok = bool(result.get("ok") and "used as a greeting" in dialog)
    record("本地英文页：划词→查询", ok, {"page": page, "picked": result.get("picked"), "dialog": dialog[:300]})
    browser.screenshot("10-local-en-results")

    saved = browser.click_text("保存所选义项")
    time.sleep(2)
    dialog = browser.dialog() or ""
    note = anki_api.latest_note()
    record(
        "本地英文页：保存到 Anki",
        bool(saved and "已保存" in dialog and note),
        {"dialog": dialog[-160:], "note": {k: v["value"] for k, v in note["fields"].items()} if note else None},
    )
    browser.screenshot("11-local-en-saved")
    browser.close_bubble()

    # 同词同句再存一张：应提示同语境但仍写入。
    browser.select("hello")
    browser.wait_js(
        """
        const host = document.getElementById("lexigrab-overlay-host");
        return !!(host && host.shadowRoot && host.shadowRoot.querySelector('button[aria-label="用 LexiGrab 查询所选单词"]'));
        """,
        timeout=6,
    )
    browser.click_query()
    browser.wait_dialog()
    browser.click_text("保存所选义项")
    time.sleep(1.8)
    browser.click_text("再存一张")
    time.sleep(0.6)
    browser.click_text("保存所选义项")
    time.sleep(2)
    dialog = browser.dialog() or ""
    record(
        "同词同句同释义：提示已存过但仍写入",
        "此前已保存过" in dialog,
        {"dialog": dialog[-260:]},
    )
    browser.screenshot("12-local-same-context")
    browser.close_bubble()

    # 同词不同句：两张卡，且不再提示。
    page = browser.open(server.url("/multi"))
    first = browser.select_and_open("hello", 0)
    browser.click_text("保存所选义项")
    time.sleep(1.8)
    browser.close_bubble()
    second = browser.select_and_open("hello", 1)
    browser.click_text("保存所选义项")
    time.sleep(1.8)
    dialog = browser.dialog() or ""
    notes = anki_api.notes(f'deck:"{anki_api.DECK}"')
    record(
        "同词不同句：连续两张卡且不触发同语境提示",
        bool(first.get("ok") and second.get("ok") and "已保存" in dialog and "此前已保存过" not in dialog and len(notes) >= 2),
        {
            "page": page,
            "sentences": [first.get("picked", {}).get("block"), second.get("picked", {}).get("block")],
            "notes": [note["fields"]["Sentence"]["value"] for note in notes][-3:],
        },
    )
    browser.screenshot("13-local-different-sentence")

    # 中文页：中文标点切句 + 划词。
    page = browser.open(server.url("/zh"))
    result = browser.select_and_open("hello")
    dialog = result.get("dialog") or ""
    sentence = result.get("picked", {}).get("block", "")
    record(
        "中文页：划词与中文标点句",
        bool(result.get("ok") and "used as a greeting" in dialog),
        {"page": page, "block": sentence, "dialog": dialog[:200]},
    )
    browser.screenshot("14-local-zh-results")
    browser.close_bubble()

    # 新闻页：world。
    page = browser.open(server.url("/news"))
    result = browser.select_and_open("world")
    dialog = result.get("dialog") or ""
    record(
        "新闻页：world 查询",
        bool(result.get("ok") and "earth" in dialog.lower()),
        {"page": page, "picked": result.get("picked"), "dialog": dialog[:200]},
    )
    browser.screenshot("15-local-news-results")
    browser.close_bubble()


def scenario_anki_closed(browser: Browser, server: TestPageServer) -> None:
    subprocess.run(["taskkill", "/IM", "Anki.exe", "/F"], capture_output=True, check=False)
    deadline = time.time() + 20
    while time.time() < deadline:
        with socket.socket() as probe:
            if probe.connect_ex(("127.0.0.1", 8765)) != 0:
                break
        time.sleep(0.5)

    browser.open(server.url("/en"))
    result = browser.select_and_open("hello")
    browser.click_text("保存所选义项")
    time.sleep(2)
    dialog = browser.dialog() or ""
    record(
        "Anki 关闭：保存失败并给出可操作提示",
        bool("Anki 未连接" in dialog and "ANKI_UNREACHABLE" in dialog and "启动 Anki" in dialog),
        {"dialog": dialog[-300:]},
    )
    browser.screenshot("20-anki-closed-error")

    subprocess.Popen([str(ANKI_EXE)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    deadline = time.time() + 90
    ready = False
    while time.time() < deadline:
        try:
            anki_api.call("version")
            ready = True
            break
        except Exception:
            time.sleep(2)
    record("Anki 重启并恢复 AnkiConnect", ready, None)

    browser.click_text("保存所选义项")
    time.sleep(2)
    dialog = browser.dialog() or ""
    record("Anki 恢复后重试保存成功", "已保存" in dialog, {"dialog": dialog[-260:]})
    browser.screenshot("21-anki-retry-saved")
    browser.close_bubble()


def scenario_token_rotation(browser: Browser, app: DesktopApp, host: str, server: TestPageServer) -> None:
    app.stop()
    app.prepare(build_config(token=ROTATED_TOKEN))
    app.start()
    app.wait_healthy(token=ROTATED_TOKEN)

    browser.open(server.url("/en"))
    result = browser.select_and_open("hello")
    dialog = result.get("dialog") or ""
    record(
        "令牌轮换后：旧令牌被拒绝并提示重新配对",
        bool("令牌错误" in dialog and "AUTH_FAILED" in dialog),
        {"dialog": dialog[-300:]},
    )
    browser.screenshot("30-token-rotated-error")
    browser.close_bubble()

    browser.open(f"moz-extension://{host}/options.html")
    pair = browser.pair(BRIDGE_PORT, ROTATED_TOKEN)
    record("令牌轮换后：选项页重新配对成功", bool(pair.get("ok")), pair)

    browser.open(server.url("/en"))
    result = browser.select_and_open("hello")
    dialog = result.get("dialog") or ""
    record(
        "令牌轮换后：新令牌查询成功",
        bool(result.get("ok") and "used as a greeting" in dialog),
        {"dialog": dialog[:200]},
    )
    browser.close_bubble()


def scenario_persistence(browser: Browser, ff: FirefoxSession, host: str, server: TestPageServer) -> None:
    ff.stop()
    time.sleep(2)
    client = ff.start()
    browser.client = client

    addons = [item for item in ff.list_addons() if item.get("id") == "lexigrab@localhost"]
    record(
        "重启 Firefox：扩展仍在且启用",
        bool(addons and addons[0].get("isActive")),
        addons[0] if addons else None,
    )

    client.navigate(f"moz-extension://{host}/options.html")
    time.sleep(1.5)
    stored = browser.read_settings()
    record(
        "重启 Firefox：storage.local 保留端口与令牌",
        bool(stored and stored.get("port") == BRIDGE_PORT and stored.get("token") == ROTATED_TOKEN),
        {"port": stored.get("port"), "token_length": len(stored.get("token", ""))} if stored else None,
    )

    browser.open(server.url("/en"))
    result = browser.select_and_open("hello")
    record(
        "重启 Firefox：再次划词→查询成功",
        bool(result.get("ok") and "used as a greeting" in (result.get("dialog") or "")),
        {"dialog": (result.get("dialog") or "")[:200]},
    )
    browser.screenshot("40-restart-query")
    browser.close_bubble()


def scenario_real_sites(browser: Browser) -> None:
    sites = [
        ("Wikipedia 英文", "https://en.wikipedia.org/wiki/Hello", ["hello"]),
        ("Wikipedia 中文", "https://zh.wikipedia.org/wiki/Hello", ["hello"]),
        ("新闻页 BBC", "https://www.bbc.com/news/world", ["world", "news"]),
    ]
    for label, url, words in sites:
        try:
            title = browser.open(url, settle=3.0)
        except Exception as error:  # noqa: BLE001 - 站点加载失败要记录而不是中断
            record(f"真实站点 {label}：加载", False, {"url": url, "error": str(error)})
            continue
        mounted = browser.js("return !!document.getElementById('lexigrab-overlay-host')")
        selections: list[dict[str, Any]] = []
        attempts = 0
        # 页面里可能有隐藏/无正文的同词节点，用户实际选不到；跳过它们继续找。
        # 同词不足 3 个可见处时用备用词（如新闻页的 news）补足交互次数。
        for word in words:
            occurrence = 0
            while len(selections) < 3 and occurrence < 8 and attempts < 16:
                result = browser.select_and_open(word, occurrence)
                occurrence += 1
                attempts += 1
                if not result.get("ok"):
                    continue
                block = (result.get("picked") or {}).get("block", "").strip()
                if not block:
                    continue
                dialog = result.get("dialog") or ""
                has_results = "来源：" in dialog and "未找到释义" not in dialog
                selections.append(
                    {
                        "word": word,
                        "occurrence": occurrence - 1,
                        "picked": result.get("picked"),
                        "dialog_has_results": has_results,
                        "empty": "未找到释义" in dialog,
                        "error": "令牌错误" in dialog or "未连接" in dialog,
                    }
                )
                browser.close_bubble()
                time.sleep(0.4)
            if len(selections) >= 3:
                break
        hit = len(selections)
        results = sum(1 for item in selections if item["dialog_has_results"])
        record(
            f"真实站点 {label}：3 次划词",
            bool(mounted and hit >= 3 and results >= 2),
            {"title": title, "url": url, "mounted": mounted, "selected": hit, "attempts": attempts, "with_results": results, "selections": selections},
        )
        browser.screenshot(f"50-{label.replace(' ', '-')}")


def scenario_port_conflict(app: DesktopApp) -> None:
    app.stop()
    occupant = socket.socket()
    occupant.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    occupant.bind(("127.0.0.1", BRIDGE_PORT))
    occupant.listen(5)
    try:
        app.prepare(build_config(token=ROTATED_TOKEN))
        app.start()
        time.sleep(6)
        log = app.log_tail()
        reachable = True
        try:
            http_json("GET", "/v1/health", token=ROTATED_TOKEN, timeout=3)
        except Exception:
            reachable = False
        record(
            "端口被占用：桥接不崩溃、不换端口，健康检查不可达",
            (not reachable) and ("BRIDGE_PORT_IN_USE" in log or "桥接服务启动失败" in log),
            {"reachable": reachable, "log": log[-500:]},
        )
        app.stop()
    finally:
        occupant.close()

    app.prepare(build_config(token=ROTATED_TOKEN))
    app.start()
    health = app.wait_healthy(token=ROTATED_TOKEN)
    record("端口释放后：同一端口重启桥接成功", bool(health and health.get("ok")), health)


def scenario_bridge_http(app: DesktopApp) -> None:
    status, headers, body = http_json("GET", "/v1/health", token=None, origin="moz-extension://m3-test", timeout=5)
    record(
        "桥接：无令牌 401 且回显扩展 Origin",
        status == 401 and headers.get("access-control-allow-origin") == "moz-extension://m3-test",
        {"status": status, "origin": headers.get("access-control-allow-origin"), "body": body},
    )
    status, _, body = http_json("GET", "/v1/health", token=TOKEN, timeout=5)
    record("桥接：轮换后旧令牌 401", status == 401, {"status": status, "body": body})
    status, _, body = http_json("GET", "/v1/health", token=ROTATED_TOKEN, timeout=5)
    record("桥接：当前令牌 200", status == 200 and body.get("ok") is True, body)
    status, _, body = http_json("GET", "/v1/health", token=ROTATED_TOKEN, origin="https://evil.example", timeout=5)
    record("桥接：非扩展 Origin 403", status == 403, {"status": status, "body": body})
    status, headers, _ = http_json("OPTIONS", "/v1/selection", token=None, origin="moz-extension://test", timeout=5)
    record(
        "桥接：预检 204 与允许头",
        status == 204 and "x-auth-token" in headers.get("access-control-allow-headers", ""),
        {"status": status, "allow_headers": headers.get("access-control-allow-headers")},
    )


# ---------------------------------------------------------------------- 主流程


def main() -> int:
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    if PROFILE.exists():
        import shutil

        shutil.rmtree(PROFILE, ignore_errors=True)
    anki_api.ensure_fixtures()
    anki_api.delete_notes(f'deck:"{anki_api.DECK}"')

    server = TestPageServer()
    server.start()
    app = DesktopApp("e2e")
    app.prepare(build_config())
    app.start()
    health = app.wait_healthy()
    record("桌面端桥接 health", bool(health and health.get("ok")), health)

    ff = FirefoxSession(profile=PROFILE, log_path=ARTIFACTS / "firefox.log")
    client = ff.start()
    browser = Browser(client)
    try:
        addon = ff.install_addon(str(XPI), temporary=False)
        host = ff.get_extension_host()
        record("安装未签名 .xpi", addon == "lexigrab@localhost", {"addon": addon, "host": host})

        client.navigate(f"moz-extension://{host}/options.html")
        time.sleep(1.5)
        pair = browser.pair(BRIDGE_PORT, TOKEN)
        record("选项页配对（测试连接 + 保存）", bool(pair.get("ok")), pair)
        stored = browser.read_settings()
        record(
            "storage.local 写入配对信息",
            bool(stored and stored.get("port") == BRIDGE_PORT and stored.get("token") == TOKEN),
            {"port": stored.get("port"), "token_length": len(stored.get("token", ""))} if stored else None,
        )

        scenario_local_matrix(browser, server)
        scenario_anki_closed(browser, server)
        scenario_token_rotation(browser, app, host, server)
        scenario_persistence(browser, ff, host, server)
        scenario_real_sites(browser)
        scenario_port_conflict(app)
        scenario_bridge_http(app)
    except Exception as error:  # noqa: BLE001 - 记录后仍写出结果
        record("未捕获异常", False, {"error": str(error), "type": type(error).__name__})
        raise
    finally:
        try:
            ff.stop()
        finally:
            server.stop()
            app.stop()
            summary = {
                "date": time.strftime("%Y-%m-%dT%H:%M:%S"),
                "firefox": str(Path(r"D:\Firefox Developer Edition\firefox.exe")),
                "xpi": str(XPI),
                "passed": sum(1 for item in RESULTS if item["ok"]),
                "failed": sum(1 for item in RESULTS if not item["ok"]),
                "scenarios": RESULTS,
            }
            (ARTIFACTS / "results.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8")
            print(json.dumps({"passed": summary["passed"], "failed": summary["failed"]}, ensure_ascii=False))

    return 0 if all(item["ok"] for item in RESULTS) else 2


if __name__ == "__main__":
    sys.exit(main())
