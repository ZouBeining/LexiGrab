// SPDX-License-Identifier: AGPL-3.0-only
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RuntimeStatus } from "../../lib/types";

const mocks = vi.hoisted(() => ({
  getRuntimeStatus: vi.fn(),
  openConfigDir: vi.fn(),
  openLogDir: vi.fn(),
  openCacheDir: vi.fn(),
  openWindow: vi.fn(),
  getVersion: vi.fn(),
  getTauriVersion: vi.fn(),
  openUrl: vi.fn(),
}));

vi.mock("../../lib/ipc", () => ({
  getRuntimeStatus: mocks.getRuntimeStatus,
  openConfigDir: mocks.openConfigDir,
  openLogDir: mocks.openLogDir,
  openCacheDir: mocks.openCacheDir,
  openWindow: mocks.openWindow,
  toIpcError: (cause: unknown) => cause,
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: mocks.getVersion,
  getTauriVersion: mocks.getTauriVersion,
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: mocks.openUrl }));

import { AboutSection, parentDirectory, webviewVersionFrom } from "./AboutSection";

const STATUS: RuntimeStatus = {
  version: "0.1.0",
  hotkey: {
    accelerator: "CmdOrCtrl+Shift+D",
    registered: true,
    message: null,
  },
  config_dir: "C:\\Users\\tester\\AppData\\Roaming\\LexiGrab",
  log_dir: "C:\\Users\\tester\\AppData\\Local\\LexiGrab\\logs",
  cache_path: "C:\\Users\\tester\\AppData\\Local\\LexiGrab\\cache.db",
  popup_visible: false,
  sources_ready: 1,
};

const DEFAULT_USER_AGENT =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 " +
  "(KHTML, like Gecko) Chrome/120.0.6099.130 Safari/537.36 Edg/120.0.2210.91";

function setUserAgent(value: string) {
  Object.defineProperty(navigator, "userAgent", { configurable: true, value });
}

function setClipboard(writeText: (value: string) => Promise<void>) {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
}

afterEach(cleanup);

beforeEach(() => {
  mocks.getRuntimeStatus.mockReset().mockResolvedValue(STATUS);
  mocks.openConfigDir.mockReset().mockResolvedValue(undefined);
  mocks.openLogDir.mockReset().mockResolvedValue(undefined);
  mocks.openCacheDir.mockReset().mockResolvedValue(undefined);
  mocks.openWindow.mockReset().mockResolvedValue(undefined);
  mocks.getVersion.mockReset().mockResolvedValue("0.1.0");
  mocks.getTauriVersion.mockReset().mockResolvedValue("2.9.0");
  mocks.openUrl.mockReset().mockResolvedValue(undefined);
  setUserAgent(DEFAULT_USER_AGENT);
  setClipboard(vi.fn().mockResolvedValue(undefined));
  window.localStorage.clear();
});

describe("AboutSection", () => {
  it("显示构建信息与 Tauri/WebView 运行时版本", async () => {
    render(<AboutSection />);

    expect(await screen.findByText("构建信息")).toBeTruthy();
    await waitFor(() => expect(mocks.getVersion).toHaveBeenCalledTimes(1));
    expect(screen.getByText("0.1.0")).toBeTruthy();
    expect(await screen.findByText("2.9.0")).toBeTruthy();
    expect(screen.getByText("120.0.2210.91")).toBeTruthy();
  });

  it("版本接口失败时回退 RuntimeStatus.version 并显示未知 Tauri 版本", async () => {
    mocks.getRuntimeStatus.mockResolvedValue({ ...STATUS, version: "0.1.0-fallback" });
    mocks.getVersion.mockRejectedValue(new Error("not in tauri"));
    mocks.getTauriVersion.mockRejectedValue(new Error("not in tauri"));

    render(<AboutSection />);

    expect(await screen.findByText("0.1.0-fallback")).toBeTruthy();
    expect(screen.getByText("未知")).toBeTruthy();
  });

  it("三个目录按钮分别调用 open_config_dir / open_log_dir / open_cache_dir", async () => {
    render(<AboutSection />);
    await screen.findByText("构建信息");

    fireEvent.click(screen.getByRole("button", { name: "打开配置目录" }));
    fireEvent.click(screen.getByRole("button", { name: "打开日志目录" }));
    fireEvent.click(screen.getByRole("button", { name: "打开缓存目录" }));

    await waitFor(() => expect(mocks.openConfigDir).toHaveBeenCalledTimes(1));
    expect(mocks.openLogDir).toHaveBeenCalledTimes(1);
    expect(mocks.openCacheDir).toHaveBeenCalledTimes(1);
  });

  it("复制路径写入剪贴板并通过 status 提示", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    setClipboard(writeText);

    render(<AboutSection />);
    await screen.findByText("构建信息");
    fireEvent.click(screen.getByRole("button", { name: "复制配置目录路径" }));

    await waitFor(() => expect(writeText).toHaveBeenCalledWith(STATUS.config_dir));
    const notice = await screen.findByRole("status");
    expect(notice.textContent).toContain("已复制配置目录路径");
    expect(notice.getAttribute("aria-live")).toBe("polite");
  });

  it("缓存行展示并复制 cache.db 的父目录", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    setClipboard(writeText);

    render(<AboutSection />);
    await screen.findByText("构建信息");
    expect(screen.getByText("C:\\Users\\tester\\AppData\\Local\\LexiGrab")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "复制缓存目录路径" }));
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith("C:\\Users\\tester\\AppData\\Local\\LexiGrab"),
    );
  });

  it("复制失败时给出打开目录后手动复制的回退提示", async () => {
    setClipboard(vi.fn().mockRejectedValue(new Error("clipboard blocked")));

    render(<AboutSection />);
    await screen.findByText("构建信息");
    fireEvent.click(screen.getByRole("button", { name: "复制日志目录路径" }));

    const notice = await screen.findByRole("status");
    expect(notice.textContent).toContain("复制日志目录路径失败");
    expect(notice.textContent).toContain("请点击“打开”后在文件管理器里复制");
  });

  it("许可证与仓库入口通过 plugin-opener 打开", async () => {
    render(<AboutSection />);
    await screen.findByText("构建信息");

    fireEvent.click(screen.getByRole("button", { name: "打开许可证" }));
    fireEvent.click(screen.getByRole("button", { name: "打开仓库" }));

    await waitFor(() =>
      expect(mocks.openUrl).toHaveBeenCalledWith(
        "https://github.com/ZouBeining/LexiGrab/blob/main/LICENSE",
      ),
    );
    expect(mocks.openUrl).toHaveBeenCalledWith("https://github.com/ZouBeining/LexiGrab");
  });

  it("读取运行时状态失败时显示 role=alert", async () => {
    mocks.getRuntimeStatus.mockRejectedValue({
      code: "INTERNAL",
      message: "运行时状态不可用",
      detail: null,
    });

    render(<AboutSection />);

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("读取版本信息失败");
    expect(alert.textContent).toContain("运行时状态不可用");
  });

  it("重新打开首次引导写入标记并打开管理器", async () => {
    window.localStorage.setItem("lexigrab.guide.dismissed", "1");

    render(<AboutSection />);
    await screen.findByText("构建信息");
    fireEvent.click(screen.getByRole("button", { name: "重新打开首次引导" }));

    expect(window.localStorage.getItem("lexigrab.guide.force")).toBe("1");
    expect(window.localStorage.getItem("lexigrab.guide.dismissed")).toBeNull();
    await waitFor(() => expect(mocks.openWindow).toHaveBeenCalledWith("manager"));
  });
});

describe("webviewVersionFrom", () => {
  it("优先读取 Edge WebView2 的 Edg 版本", () => {
    expect(webviewVersionFrom(DEFAULT_USER_AGENT)).toBe("120.0.2210.91");
  });

  it("回退到 Chrome 版本", () => {
    expect(
      webviewVersionFrom(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/118.0 Safari/537.36",
      ),
    ).toBe("118.0");
  });

  it("无法识别时返回 null", () => {
    expect(webviewVersionFrom("Mozilla/5.0")).toBeNull();
  });
});

describe("parentDirectory", () => {
  it("取 Windows 与 POSIX 路径的父目录", () => {
    expect(parentDirectory("C:\\Users\\tester\\AppData\\Local\\LexiGrab\\cache.db")).toBe(
      "C:\\Users\\tester\\AppData\\Local\\LexiGrab",
    );
    expect(parentDirectory("/home/user/.local/share/LexiGrab/cache.db")).toBe(
      "/home/user/.local/share/LexiGrab",
    );
  });

  it("没有分隔符时原样返回", () => {
    expect(parentDirectory("cache.db")).toBe("cache.db");
  });
});
