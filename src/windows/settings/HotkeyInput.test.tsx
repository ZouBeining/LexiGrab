import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HotkeyInput } from "./HotkeyInput";

afterEach(cleanup);

function renderRecorder(value = "CmdOrCtrl+Shift+D") {
  const onChange = vi.fn();
  render(<HotkeyInput value={value} onChange={onChange} />);
  const button = screen.getByRole("button", { name: "录制全局热键" });
  fireEvent.click(button);
  return { onChange, button };
}

describe("HotkeyInput", () => {
  it("records a modifier combination as a Tauri accelerator", () => {
    const { onChange, button } = renderRecorder();
    fireEvent.keyDown(button, { key: "k", ctrlKey: true, shiftKey: true });
    expect(onChange).toHaveBeenCalledWith("CmdOrCtrl+Shift+K");
  });

  it("keeps recording until at least one modifier is pressed", () => {
    const { onChange, button } = renderRecorder();
    fireEvent.keyDown(button, { key: "k" });
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByText(/必须包含 Ctrl、Alt 或 Shift/)).toBeTruthy();
  });

  it("cancels recording on Escape without changing the value", () => {
    const { onChange, button } = renderRecorder();
    fireEvent.keyDown(button, { key: "Escape" });
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "录制全局热键" })).toBeTruthy();
  });

  it("shows the current accelerator when idle", () => {
    render(<HotkeyInput value="CmdOrCtrl+Alt+J" onChange={vi.fn()} />);
    expect(screen.getByText("CmdOrCtrl+Alt+J")).toBeTruthy();
  });
});
