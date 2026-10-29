import { describe, expect, it } from "vitest";
import {
  detectPlatform,
  isLocalResourceUri,
  LOCAL_RESOURCE_SCHEME,
  toRenderableUri,
  WINDOWS_RESOURCE_ORIGIN,
} from "./resource";

function withUserAgent<T>(value: string, run: () => T): T {
  const original = Object.getOwnPropertyDescriptor(window.navigator, "userAgent");
  Object.defineProperty(window.navigator, "userAgent", {
    configurable: true,
    get: () => value,
  });
  try {
    return run();
  } finally {
    if (original) {
      Object.defineProperty(window.navigator, "userAgent", original);
    } else {
      delete (window.navigator as unknown as Record<string, unknown>).userAgent;
    }
  }
}

describe("toRenderableUri", () => {
  it("Windows 上把 mdx-res:// 转成 mdx-res.localhost", () => {
    expect(toRenderableUri("mdx-res://local-1/img/a.png", "windows")).toBe(
      "http://mdx-res.localhost/local-1/img/a.png",
    );
  });

  it("非 Windows 平台保持逻辑 URI 不变", () => {
    expect(toRenderableUri("mdx-res://local-1/img/a.png", "macos")).toBe(
      "mdx-res://local-1/img/a.png",
    );
    expect(toRenderableUri("mdx-res://local-1/img/a.png", "linux")).toBe(
      "mdx-res://local-1/img/a.png",
    );
    expect(toRenderableUri("mdx-res://local-1/img/a.png", "other")).toBe(
      "mdx-res://local-1/img/a.png",
    );
  });

  it("保留已编码路径，不会二次编码", () => {
    expect(toRenderableUri("mdx-res://local-1/img/a%20b%2Fc.png", "windows")).toBe(
      "http://mdx-res.localhost/local-1/img/a%20b%2Fc.png",
    );
  });

  it("保留 query 与 fragment", () => {
    expect(toRenderableUri("mdx-res://local-1/audio/a.mp3?v=2#t=10", "windows")).toBe(
      "http://mdx-res.localhost/local-1/audio/a.mp3?v=2#t=10",
    );
  });

  it("非 mdx-res 的 URI 原样返回", () => {
    for (const uri of [
      "https://example.com/a.png",
      "http://example.com/a.png",
      "data:image/png;base64,AAAA",
      "blob:https://example.com/uuid",
      "/relative/a.png",
      "",
    ]) {
      expect(toRenderableUri(uri, "windows")).toBe(uri);
    }
  });

  it("默认平台由 detectPlatform 决定", () => {
    withUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", () => {
      expect(toRenderableUri("mdx-res://local-1/a.png")).toBe(
        "http://mdx-res.localhost/local-1/a.png",
      );
    });
    withUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)", () => {
      expect(toRenderableUri("mdx-res://local-1/a.png")).toBe("mdx-res://local-1/a.png");
    });
  });
});

describe("detectPlatform", () => {
  it("识别 Windows / macOS / Linux / 其他", () => {
    withUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", () => {
      expect(detectPlatform()).toBe("windows");
    });
    withUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0)", () => {
      expect(detectPlatform()).toBe("macos");
    });
    withUserAgent("Mozilla/5.0 (X11; Linux x86_64)", () => {
      expect(detectPlatform()).toBe("linux");
    });
    withUserAgent("", () => {
      expect(detectPlatform()).toBe("other");
    });
  });
});

describe("resource constants", () => {
  it("导出常量与 isLocalResourceUri", () => {
    expect(LOCAL_RESOURCE_SCHEME).toBe("mdx-res://");
    expect(WINDOWS_RESOURCE_ORIGIN).toBe("http://mdx-res.localhost/");
    expect(isLocalResourceUri("mdx-res://local-1/a.png")).toBe(true);
    expect(isLocalResourceUri("http://mdx-res.localhost/local-1/a.png")).toBe(false);
    expect(isLocalResourceUri("https://example.com/a.png")).toBe(false);
  });
});
