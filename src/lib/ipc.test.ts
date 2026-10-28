import { describe, expect, it } from "vitest";
import { toIpcError } from "./ipc";

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
