import { describe, expect, it } from "vitest";
import { basename, normalisePath, stripMdxExtension, summarisePaths } from "./paths";

describe("manager path helpers", () => {
  it("reads the last segment for Windows and POSIX paths", () => {
    expect(basename("C:\\dicts\\Mini.mdx")).toBe("Mini.mdx");
    expect(basename("C:/dicts/Mini.mdx")).toBe("Mini.mdx");
  });

  it("normalises separators and case for duplicate detection", () => {
    expect(normalisePath("C:\\Dicts\\Mini.MDX")).toBe("c:\\dicts\\mini.mdx");
    expect(normalisePath("C:/Dicts/Mini.MDX")).toBe("c:\\dicts\\mini.mdx");
  });

  it("strips only the .mdx extension", () => {
    expect(stripMdxExtension("Mini.mdx")).toBe("Mini");
    expect(stripMdxExtension("Mini.mdd")).toBe("Mini.mdd");
  });

  it("summarises one or two files fully and truncates longer lists", () => {
    expect(summarisePaths(["C:\\a\\One.mdx"])).toBe("One.mdx");
    expect(summarisePaths(["C:\\a\\One.mdx", "C:\\a\\Two.mdx"])).toBe("One.mdx、Two.mdx");
    expect(summarisePaths(["C:\\a\\One.mdx", "C:\\a\\Two.mdx", "C:\\a\\Three.mdx"])).toBe(
      "3 个文件 · One.mdx、Two.mdx …",
    );
  });
});
