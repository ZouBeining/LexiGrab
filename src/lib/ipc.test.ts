// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import {
  openCacheDir,
  openConfigDir,
  openLogDir,
  openWindow,
  toIpcError,
} from "./ipc";

describe("toIpcError", () => {
  it("保留 Rust 返回的错误码与详情", () => {
    const error = toIpcError({
      code: "HOTKEY_CONFLICT",
      message: "快捷键被占用",
      detail: "register failed",
    });
    expect(error).toEqual({
      code: "HOTKEY_CONFLICT",
      message: "快捷键被占用",
      detail: "register failed",
    });
  });

  it("把字符串错误归一化为 INTERNAL", () => {
    const error = toIpcError("窗口不存在");
    expect(error.code).toBe("INTERNAL");
    expect(error.message).toBe("窗口不存在");
    expect(error.detail).toBeNull();
  });

  it("未知对象不会丢失可用信息", () => {
    const error = toIpcError({ code: 42, message: 1 });
    expect(error.code).toBe("INTERNAL");
    expect(error.message).toBe("发生未知错误");
  });
});

describe("目录命令封装", () => {
  beforeEach(() => {
    mocks.invoke.mockReset().mockResolvedValue(undefined);
  });

  it("openConfigDir 调用 open_config_dir", async () => {
    await openConfigDir();
    expect(mocks.invoke).toHaveBeenCalledWith("open_config_dir", undefined);
  });

  it("openLogDir 调用 open_log_dir", async () => {
    await openLogDir();
    expect(mocks.invoke).toHaveBeenCalledWith("open_log_dir", undefined);
  });

  it("openCacheDir 调用 open_cache_dir", async () => {
    await openCacheDir();
    expect(mocks.invoke).toHaveBeenCalledWith("open_cache_dir", undefined);
  });

  it("openWindow 传递白名单目标", async () => {
    await openWindow("settings");
    expect(mocks.invoke).toHaveBeenCalledWith("open_window", { target: "settings" });
  });
});
