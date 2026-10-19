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
  it("preserves the terminal palette and emulator when the application theme changes", async () => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    // computed styles represent the same root scheme switch used by the shell.
    // 计算样式模拟外壳使用的同一根配色方案切换。
    const styles = vi.spyOn(globalThis, "getComputedStyle").mockImplementation(() => ({
      getPropertyValue: (key: string) => `${document.documentElement.dataset.vkScheme === "dark" ? "dark" : "light"}:${key}`,
    } as CSSStyleDeclaration));
    const host = document.createElement("div"); document.body.append(host);
    const root = createRoot(host);
    try {
      await act(async () => root.render(<TerminalViewportRuntime chunks={[]} readOnly ariaLabel="Synthetic terminal" fixedRows={24} fixedCols={80} />));
      const instance = emulator.instances.at(-1)!;
      expect(instance.options.theme).toMatchObject({ background: "#111318", foreground: "#d9dee8" });
      const count = emulator.instances.length;
      await act(async () => { document.documentElement.dataset.vkScheme = "dark"; });
      expect(instance.options.theme).toMatchObject({ background: "#111318", foreground: "#d9dee8" });
      expect(styles).not.toHaveBeenCalled();
      expect(emulator.instances).toHaveLength(count);
      await act(async () => root.unmount());
      expect(instance.dispose).toHaveBeenCalledOnce();
    } finally {
      delete document.documentElement.dataset.vkScheme; styles.mockRestore(); host.remove();
    }
  });
});
