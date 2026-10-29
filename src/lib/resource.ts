/**
 * 本地资源 URI 转换。
 *
 * 契约：docs/BACKEND_STRUCTURE.md §2.1 规定逻辑 URI 为
 * `mdx-res://{dict_id}/{percent-encoded path}`；Windows 上 Tauri 的自定义协议
 * 通过 `http://mdx-res.localhost/` 访问，其他平台保持逻辑 URI 不变。
 */

export const LOCAL_RESOURCE_SCHEME = "mdx-res://";

/** Windows 自定义协议的渲染前缀（对应 mdx-res://）。 */
export const WINDOWS_RESOURCE_ORIGIN = "http://mdx-res.localhost/";

export type Platform = "windows" | "macos" | "linux" | "other";

/** 从 userAgent 判断平台；非桌面环境（测试/SSR）按 other 处理。 */
export function detectPlatform(): Platform {
  const userAgent = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (/windows/i.test(userAgent)) return "windows";
  if (/macintosh|mac os x/i.test(userAgent)) return "macos";
  if (/linux|x11/i.test(userAgent)) return "linux";
  return "other";
}

/**
 * 把逻辑资源 URI 转成当前平台可渲染的 URI。
 * 非 `mdx-res://` 的输入原样返回（http/https/data 等由调用方决定是否展示）。
 * 路径中的百分号编码与 query/fragment 保持不变。
 */
export function toRenderableUri(logical: string, platform: Platform = detectPlatform()): string {
  if (!logical.startsWith(LOCAL_RESOURCE_SCHEME)) {
    return logical;
  }
  if (platform !== "windows") {
    return logical;
  }
  return `${WINDOWS_RESOURCE_ORIGIN}${logical.slice(LOCAL_RESOURCE_SCHEME.length)}`;
}

/** 判断一个 URI 是否为本应用的本地词典资源。 */
export function isLocalResourceUri(uri: string): boolean {
  return uri.startsWith(LOCAL_RESOURCE_SCHEME);
}
