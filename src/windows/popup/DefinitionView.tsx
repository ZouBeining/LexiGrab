// SPDX-License-Identifier: AGPL-3.0-only
import { useMemo, type MouseEvent } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { toRenderableUri, type Platform } from "../../lib/resource";

/** Matches logical local-resource URIs inside sanitized dictionary HTML. */
const RESOURCE_URI_PATTERN = /mdx-res:\/\/[^"'\s>]+/g;

/**
 * Rewrite every logical `mdx-res://` URI for the current platform so the WebView
 * can load MDD resources (docs/BACKEND_STRUCTURE.md §2.1, M2 T2.2).
 */
export function rewriteResourceUris(html: string, platform?: Platform): string {
  return html.replace(RESOURCE_URI_PATTERN, (uri) => toRenderableUri(uri, platform));
}

export interface DefinitionViewProps {
  /** Already sanitized dictionary HTML coming from the backend. */
  html: string;
  className?: string;
}

/**
 * The only place where dictionary HTML is rendered. Media sizing is expressed
 * with Tailwind child variants so shared CSS stays untouched.
 */
export function DefinitionView({ html, className }: DefinitionViewProps) {
  const rendered = useMemo(() => rewriteResourceUris(html), [html]);

  const handleClick = (event: MouseEvent<HTMLDivElement>) => {
    const target = event.target;
    const anchor = target instanceof Element ? target.closest("a") : null;
    // Dictionary links always go to the system browser; the WebView never navigates.
    if (anchor) {
      event.preventDefault();
      const href = anchor.getAttribute("href");
      if (href) {
        void openUrl(href).catch(() => {
          // 无默认浏览器等系统错误不打断释义展示。
        });
      }
    }
  };

  return (
    <div
      className={[
        "entry-html break-words text-sm leading-relaxed text-ink",
        "[&_a]:text-accent [&_a]:underline",
        "[&_img]:h-auto [&_img]:max-w-full",
        "[&_video]:h-auto [&_video]:max-w-full",
        "[&_audio]:w-full",
        "[&_table]:w-full [&_table]:border-collapse",
        "[&_ul]:list-disc [&_ul]:pl-5 [&_ol]:list-decimal [&_ol]:pl-5",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
      onClick={handleClick}
      dangerouslySetInnerHTML={{ __html: rendered }}
    />
  );
}
