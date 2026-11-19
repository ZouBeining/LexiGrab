"""启动/关闭独立 profile 的 Firefox Developer Edition，并连接 Marionette。"""

from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import time
from pathlib import Path
from typing import Any

from marionette import Marionette

FIREFOX_EXE = Path(r"D:\Firefox Developer Edition\firefox.exe")

USER_JS = """\
// LexiGrab M3 自动化 profile
user_pref("marionette.port", 2828);
user_pref("marionette.enabled", true);
user_pref("xpinstall.signatures.required", false);
user_pref("extensions.autoDisableScopes", 0);
user_pref("extensions.enabledScopes", 15);
user_pref("extensions.install.import.legacy", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.startup.page", 0);
user_pref("browser.startup.homepage", "about:blank");
user_pref("browser.aboutwelcome.enabled", false);
user_pref("browser.sessionstore.resume_from_crash", false);
user_pref("browser.sessionstore.restore_on_demand", false);
user_pref("toolkit.telemetry.enabled", false);
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("datareporting.healthreport.uploadEnabled", false);
user_pref("app.update.auto", false);
user_pref("app.update.enabled", false);
user_pref("signon.rememberSignons", false);
user_pref("devtools.debugger.remote-enabled", true);
user_pref("devtools.chrome.enabled", true);
user_pref("extensions.webextensions.remote", false);
user_pref("network.proxy.type", 5);
user_pref("network.trr.mode", 5);
user_pref("network.dns.disableIPv6", true);
user_pref("network.http.speculative-parallel-limit", 0);
user_pref("network.prefetch-next", false);
user_pref("media.peerconnection.enabled", false);
user_pref("security.enterprise_roots.enabled", true);
"""


def wait_for_port(port: int, timeout: float = 60.0) -> None:
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.5):
                return
        except OSError:
            time.sleep(0.2)
    raise TimeoutError(f"Marionette 端口 {port} 在 {timeout}s 内未就绪")


def make_profile(directory: Path) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    (directory / "user.js").write_text(USER_JS, encoding="utf-8")
    return directory


def prefs_file(profile: Path) -> Path:
    return profile / "prefs.js"


class FirefoxSession:
    """一个 Firefox 进程 + 一个 Marionette 会话。"""

    def __init__(self, profile: Path, log_path: Path, headless: bool = False, port: int = 2828) -> None:
        self.profile = profile
        self.log_path = log_path
        self.headless = headless
        self.port = port
        self.process: subprocess.Popen[bytes] | None = None
        self.client: Marionette | None = None

    def start(self, timeout: float = 60.0) -> Marionette:
        if not FIREFOX_EXE.is_file():
            raise FileNotFoundError(f"未找到 Firefox：{FIREFOX_EXE}")
        make_profile(self.profile)
        args = [
            str(FIREFOX_EXE),
            "-profile",
            str(self.profile),
            "-no-remote",
            "-marionette",
        ]
        if self.headless:
            args.append("-headless")
        env = dict(os.environ)
        env["MOZ_CRASHREPORTER_DISABLE"] = "1"
        if self.headless:
            env["MOZ_HEADLESS"] = "1"
        else:
            env.pop("MOZ_HEADLESS", None)
        # Firefox 138+ 的 Marionette 在 chrome 上下文需要显式授权。
        env["MOZ_REMOTE_ALLOW_SYSTEM_ACCESS"] = "1"
        self.log_path.parent.mkdir(parents=True, exist_ok=True)
        log = self.log_path.open("wb")
        self._log_handle = log
        self.process = subprocess.Popen(args, stdout=log, stderr=subprocess.STDOUT, env=env)
        wait_for_port(self.port, timeout)
        self.client = Marionette(port=self.port)
        self.client.start_session()
        return self.client

    def stop(self) -> None:
        if self.client is not None:
            self.client.close()
            self.client = None
        if self.process is not None and self.process.poll() is None:
            try:
                self.process.terminate()
                self.process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=10)
            self.process = None
        try:
            self._log_handle.close()
        except Exception:
            pass

    # ---------------------------------------------------------------- 扩展

    # ---------------------------------------------------------------- 扩展

    def install_addon(self, path: str, temporary: bool = False) -> str:
        assert self.client is not None
        return self.client.install_addon(path, temporary)

    def uninstall_addon(self, addon_id: str) -> None:
        assert self.client is not None
        self.client.uninstall_addon(addon_id)

    def get_extension_host(self, addon_id: str = "lexigrab@localhost") -> str:
        assert self.client is not None
        self.client.set_context("chrome")
        try:
            host = self.client.execute_script(
                """
                const { WebExtensionPolicy } = globalThis;
                const policy = WebExtensionPolicy.getByID(arguments[0]);
                return policy ? policy.mozExtensionHostname : null;
                """,
                args=[addon_id],
                sandbox=None,
            )
        finally:
            self.client.set_context("content")
        if not host:
            raise RuntimeError(f"扩展未安装：{addon_id}")
        return str(host)

    def list_addons(self) -> list[dict[str, Any]]:
        """读取 profile 中扩展状态（不含敏感字段）。"""
        assert self.client is not None
        self.client.set_context("chrome")
        try:
            data = self.client.execute_async(
                """
                const done = arguments[arguments.length - 1];
                AddonManager.getAllAddons().then((addons) => {
                  done(addons.map((addon) => ({
                    id: addon.id,
                    type: addon.type,
                    name: addon.name,
                    version: addon.version,
                    isActive: addon.isActive,
                    isSystem: addon.isSystem,
                    signedState: addon.signedState,
                  })));
                }, (error) => done({ error: String(error) }));
                """,
                sandbox=None,
            )
        finally:
            self.client.set_context("content")
        return list(data)
