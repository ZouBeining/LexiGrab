// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import { rewriteResourceUris } from "./DefinitionView";

describe("rewriteResourceUris", () => {
  const html =
    '<p>释义</p><img src="mdx-res://dict-1/images/a%20b.png" alt="a">' +
    '<audio src="mdx-res://dict-1/audio/word.mp3"></audio>' +
    '<a href="mdx-res://dict-1/defs/page.html">see also</a>' +
    '<a href="https://example.com/x">external</a>';

  it("maps mdx-res URIs to the Windows protocol origin", () => {
    const rendered = rewriteResourceUris(html, "windows");

    expect(rendered).toContain('src="http://mdx-res.localhost/dict-1/images/a%20b.png"');
    expect(rendered).toContain('src="http://mdx-res.localhost/dict-1/audio/word.mp3"');
    expect(rendered).toContain('href="http://mdx-res.localhost/dict-1/defs/page.html"');
    expect(rendered).not.toContain("mdx-res://");
  });

  it("keeps logical URIs on non-Windows platforms", () => {
    const rendered = rewriteResourceUris(html, "macos");

    expect(rendered).toContain('src="mdx-res://dict-1/images/a%20b.png"');
    expect(rendered).toContain('href="https://example.com/x"');
  });

  it("leaves HTML without local resources untouched", () => {
    const plain = "<p>纯文本释义</p>";
    expect(rewriteResourceUris(plain, "windows")).toBe(plain);
  });
});
