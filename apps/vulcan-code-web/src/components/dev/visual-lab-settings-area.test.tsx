// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { VisualLabSettingsArea } from "./visual-lab-settings-area";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { DEMO_LATENCY_MS } from "./visual-lab/visual-lab-plugin-pages";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: (key: string) => key }) }));
// button resolves an exact visible action in the real component tree, including portals.
// button 在真实组件树（含浮层）中解析可见文字完全匹配的操作。
function button(name: string): HTMLButtonElement {
  const found = Array.from(document.querySelectorAll("button")).find((item) => item.textContent === name && !item.closest('[hidden]'));
  if (!found) throw new Error(`Missing button: ${name}`);
  return found;
}

// settle waits past one synthetic plugin action so its result renders.
// settle 等待一次合成插件 Action 完成，使其结果完成渲染。
async function settle() {
  await act(async () => new Promise((resolve) => setTimeout(resolve, DEMO_LATENCY_MS + 50)));
}

// region resolves one page section by its exact heading text.
// region 按精确标题文字解析一个页面分区。
function region(title: string): HTMLElement {
  const found = Array.from(document.querySelectorAll<HTMLElement>('[role="region"]')).find((item) => item.querySelector(":is(h2, h3, h4)")?.textContent === title && !item.closest("[hidden]"));
  if (!found) throw new Error(`Missing page section: ${title}; the heading may have changed`);
  return found;
}

describe("visual lab local interactions", () => {
  let host: HTMLDivElement;
  let root: Root;
  beforeEach(async () => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
    host = document.createElement("div"); document.body.append(host); root = createRoot(host);
    await act(async () => root.render(<TooltipProvider><VisualLabSettingsArea /></TooltipProvider>));
  });
  afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });
  it("labels fixtures and responds to actions, navigation and keyboard card activation", async () => {
    expect(host.textContent).toContain("synthetic samples");
    await act(async () => button("Actions and status").click());
    await act(async () => button("Primary").click());
    expect(host.querySelector("[data-visual-lab-status]")?.textContent).toBe("Demo action: Primary");
    await act(async () => button("Navigation and feedback").click());
    await act(async () => button("Other page").click());
    expect(button("Other page").getAttribute("aria-current")).toBe("page");
    await act(async () => button("Nested folder").click());
    expect(button("Nested folder").getAttribute("aria-pressed")).toBe("true");
    await act(async () => button("Content surfaces").click());
    const card = host.querySelector<HTMLElement>('[data-slot="card"][role="button"]')!;
    await act(async () => card.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
    expect(host.querySelector("[data-visual-lab-status]")?.textContent).toContain("Interactive card");
    expect(host.querySelector('[role="progressbar"]')?.getAttribute("aria-label")).toBe("Demo progress");
  });
  it("persists checkbox and radio changes through real menu close and reopen", async () => {
    await act(async () => button("Menus and overlays").click());
    await act(async () => button("Open dropdown").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
    const checkbox = document.querySelector<HTMLElement>('[role="menuitemcheckbox"]')!;
    expect(checkbox.getAttribute("aria-checked")).toBe("true");
    await act(async () => checkbox.click());
    await act(async () => button("Open dropdown").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
    expect(document.querySelector('[role="menuitemcheckbox"]')?.getAttribute("aria-checked")).toBe("false");
    const compact = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitemradio"]')).find((item) => item.textContent === "Compact density")!;
    await act(async () => compact.click());
    await act(async () => button("Open dropdown").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
    expect(Array.from(document.querySelectorAll('[role="menuitemradio"]')).find((item) => item.textContent === "Compact density")?.getAttribute("aria-checked")).toBe("true");
  });
  it("returns focus to each actual modal launcher", async () => {
    await act(async () => button("Modal surfaces").click());
    for (const launcher of ["Dialog", "Confirmation", "Sheet"]) {
      const trigger = button(launcher);
      await act(async () => { trigger.focus(); trigger.click(); });
      const dialog = document.querySelector<HTMLElement>('[role="dialog"], [role="alertdialog"]')!;
      expect(dialog).not.toBeNull();
      await act(async () => dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
      await act(async () => new Promise((resolve) => setTimeout(resolve, 0)));
      expect(document.activeElement).toBe(trigger);
    }
  });
  it("opens the context menu from Shift F10 and preserves its checked state", async () => {
    await act(async () => button("Menus and overlays").click());
    const trigger = host.querySelector<HTMLElement>(".visual-lab-context-target")!;
    await act(async () => { trigger.focus(); trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "F10", shiftKey: true, bubbles: true })); });
    expect(document.querySelector('[role="menuitemcheckbox"]')?.getAttribute("aria-checked")).toBe("true");
    await act(async () => document.querySelector<HTMLElement>('[role="menuitemcheckbox"]')!.click());
    await act(async () => trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ContextMenu", bubbles: true })));
    expect(document.querySelector('[role="menuitemcheckbox"]')?.getAttribute("aria-checked")).toBe("false");
  });
  it("composes the page kit and reveals the save bar for an unsaved draft", async () => {
    await act(async () => button("Page kit").click());
    expect(region("Data table").querySelectorAll("tbody tr").length).toBeGreaterThan(0);
    expect(document.querySelector('[data-slot="page-action-bar"]')).toBeNull();
    await act(async () => region("Save bar").querySelector<HTMLElement>('[role="switch"]')!.click());
    expect(document.querySelector('[data-slot="page-action-bar"]')).not.toBeNull();
  });
  it("renders the declarative plugin settings page against the in-memory host", async () => {
    await act(async () => button("Plugin settings page").click());
    await settle();
    expect(region("Behavior").textContent).toContain("Enable plugin");
    await act(async () => button("Test connection").click());
    await settle();
    expect(host.querySelector("[data-visual-lab-status]")?.textContent).toBe("Demo action: invoke connection-test");
  });
});
