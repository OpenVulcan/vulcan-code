// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { TerminalViewportRuntime } from "./terminal-viewport-runtime";

// emulator records presentation options while never connecting to a terminal process.
// emulator 记录展示选项，不连接终端进程。
const emulator = vi.hoisted(() => ({ instances: [] as Array<{ options: Record<string, unknown>; dispose: ReturnType<typeof vi.fn> }> }));
vi.mock("@xterm/xterm", () => ({ Terminal: class {
  options: Record<string, unknown>;
  dispose = vi.fn();
  constructor(options: Record<string, unknown>) { this.options = options; emulator.instances.push(this); }
  open() {} refresh() {} blur() {} write() {}
  onData() { return { dispose: vi.fn() }; }
  onBinary() { return { dispose: vi.fn() }; }
} }));

describe("terminal theme", () => {
  it("updates the existing read-only emulator from application tokens", async () => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    // computed styles represent the same root-class palette switch used by the shell.
    // 计算样式模拟外壳使用的同一根类名色板切换。
    const styles = vi.spyOn(globalThis, "getComputedStyle").mockImplementation(() => ({
      getPropertyValue: (key: string) => `${document.documentElement.classList.contains("dark") ? "dark" : "light"}:${key}`,
    } as CSSStyleDeclaration));
    const host = document.createElement("div"); document.body.append(host);
    const root = createRoot(host);
    try {
      await act(async () => root.render(<TerminalViewportRuntime chunks={[]} readOnly ariaLabel="Synthetic terminal" fixedRows={24} fixedCols={80} />));
      const instance = emulator.instances.at(-1)!;
      expect(instance.options.theme).toMatchObject({ background: "light:--background", foreground: "light:--foreground" });
      const count = emulator.instances.length;
      await act(async () => { document.documentElement.classList.add("dark"); });
      expect(instance.options.theme).toMatchObject({ background: "dark:--background", foreground: "dark:--foreground" });
      expect(emulator.instances).toHaveLength(count);
      await act(async () => root.unmount());
      expect(instance.dispose).toHaveBeenCalledOnce();
    } finally {
      document.documentElement.classList.remove("dark"); styles.mockRestore(); host.remove();
    }
  });
});
