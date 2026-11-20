// SPDX-License-Identifier: AGPL-3.0-only
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Field } from "./Field";

afterEach(cleanup);

function control(id = "source-name") {
  return <input id={id} className="h-10 rounded-lg border border-hairline" />;
}

describe("Field", () => {
  it("label 与控件关联，hint 通过 aria-describedby 关联", () => {
    render(
      <Field label="源名称" hint="用于管理器展示">
        {control()}
      </Field>,
    );
    const input = screen.getByLabelText("源名称");
    expect(input.getAttribute("aria-describedby")).toBe("source-name-hint");
    expect(screen.getByText("用于管理器展示").id).toBe("source-name-hint");
  });

  it("错误时改为关联错误文案并标记 aria-invalid", () => {
    render(
      <Field label="路径" hint="选择 .mdx 文件" error="同一路径不能重复添加">
        {control("source-path")}
      </Field>,
    );
    const input = screen.getByLabelText("路径");
    expect(input.getAttribute("aria-describedby")).toBe("source-path-error");
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(screen.getByRole("alert").textContent).toContain("同一路径不能重复添加");
    expect(screen.queryByText("选择 .mdx 文件")).toBeNull();
  });

  it("未传 id 时自动生成控件 id", () => {
    render(<Field label="查询">{<input />}</Field>);
    const input = screen.getByLabelText("查询");
    expect(input.id).not.toBe("");
    expect(input.getAttribute("aria-required")).toBeNull();
  });

  it("required 时标记 aria-required 并显示星号占位", () => {
    render(
      <Field label="牌组" required>
        {control("deck")}
      </Field>,
    );
    expect(screen.getByLabelText(/牌组/).getAttribute("aria-required")).toBe("true");
  });
});
