// SPDX-License-Identifier: AGPL-3.0-only
/**
 * 释义 HTML 的二次清洗。服务端已经用 ammonia 清洗过一次，扩展仍按
 * APP_FLOW §8.2 再做一次，因为响应可能来自旧版桌面端或被杀软改写。
 *
 * DOMPurify 在部分非浏览器 DOM 实现中没有可用文档，此时 `sanitize` 会原样
 * 返回输入。为不把不安全的 HTML 注入页面，这里再做一次 DOM 兜底清洗。
 */
import DOMPurify from "dompurify";

const FORBID_TAGS = ["style", "form", "input", "button", "video", "audio", "iframe", "script"];

export function sanitizeDefinitionHtml(html: string): string {
  const purified = DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS,
    FORBID_ATTR: ["style"],
  });
  if (DOMPurify.isSupported && !/<script/i.test(purified)) {
    return purified;
  }
  return fallbackSanitize(html);
}

/** 用 DOMParser + 属性白名单清洗；不依赖 DOMPurify 的 DOM 支持。 */
function fallbackSanitize(html: string): string {
  const parsed = new DOMParser().parseFromString(html, "text/html");
  const doc = parsed;
  doc
    .querySelectorAll(FORBID_TAGS.join(","))
    .forEach((element) => element.remove());
  for (const element of Array.from(doc.body.querySelectorAll("*"))) {
    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLowerCase();
      const value = attribute.value.trim().toLowerCase();
      const isEventHandler = name.startsWith("on");
      const isScriptUrl =
        (name === "href" || name === "src" || name === "xlink:href") && value.startsWith("javascript:");
      if (isEventHandler || isScriptUrl || name === "style") {
        element.removeAttribute(attribute.name);
      }
    }
  }
  return doc.body.innerHTML;
}
