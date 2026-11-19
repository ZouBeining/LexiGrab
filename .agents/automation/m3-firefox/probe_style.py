"""样式探针：验证 Shadow DOM 内 border/shadow 工具类是否生效。"""

from __future__ import annotations

import json
import shutil
import time
from pathlib import Path

from desktop import BRIDGE_PORT, ROTATED_TOKEN, DesktopApp, build_config
from ffx import FirefoxSession
from run import Browser
from testpage import TestPageServer

HERE = Path(__file__).resolve().parent
ARTIFACTS = HERE / "artifacts"
PROFILE = ARTIFACTS / "style-profile"
XPI = HERE.parents[2] / "extension" / "dist" / "lexigrab-firefox-0.1.0.xpi"


def main() -> int:
    if PROFILE.exists():
        shutil.rmtree(PROFILE, ignore_errors=True)
    server = TestPageServer()
    server.start()
    app = DesktopApp("style-probe")
    app.prepare(build_config(token=ROTATED_TOKEN))
    app.start()
    app.wait_healthy(token=ROTATED_TOKEN)

    ff = FirefoxSession(profile=PROFILE, log_path=ARTIFACTS / "style-firefox.log")
    client = ff.start()
    browser = Browser(client)
    try:
        ff.install_addon(str(XPI), temporary=False)
        host = ff.get_extension_host()
        client.navigate(f"moz-extension://{host}/options.html")
        time.sleep(1.2)
        browser.pair(BRIDGE_PORT, ROTATED_TOKEN)

        browser.open(server.url("/en"))
        result = browser.select_and_open("hello", 0)
        print("dialog:", bool(result.get("ok")))
        style = browser.js(
            """
            const host = document.getElementById("lexigrab-overlay-host");
            const root = host.shadowRoot;
            const dialog = root.querySelector('[role="dialog"]');
            const header = root.querySelector("header");
            const button = [...root.querySelectorAll("button")].find((item) => item.textContent.includes("保存所选义项"));
            const read = (element) => {
              const style = getComputedStyle(element);
              return {
                background: style.backgroundColor,
                borderRadius: style.borderRadius,
                border: style.border,
                boxShadow: style.boxShadow,
                width: style.width,
                padding: style.padding,
                color: style.color,
              };
            };
            return {
              dialog: read(dialog),
              header: read(header),
              button: read(button),
              twBorderStyle: getComputedStyle(dialog).getPropertyValue("--tw-border-style"),
              twInsetShadow: getComputedStyle(dialog).getPropertyValue("--tw-inset-shadow"),
            };
            """
        )
        print(json.dumps(style, ensure_ascii=False, indent=2))
        import base64

        (ARTIFACTS / "60-style-probe.png").write_bytes(base64.b64decode(client.screenshot()))
        return 0
    finally:
        ff.stop()
        server.stop()
        app.stop()


if __name__ == "__main__":
    raise SystemExit(main())
