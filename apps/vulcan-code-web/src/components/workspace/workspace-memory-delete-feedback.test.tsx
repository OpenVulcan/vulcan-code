// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { WorkspaceMemoryPanel } from "./WorkspaceMemoryPanel";

// Verify failed deletion remains actionable with its error inside the active confirmation.
// 验证删除失败后仍可操作，且错误位于当前确认框内部。
it("shows delete failure in the dialog and clears it when a fresh confirmation opens", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  // run rejects without changing any real memory or session suppression.
  // run 直接拒绝，不改变任何真实记忆或会话抑制状态。
  const run = vi.fn().mockRejectedValue(new Error("isolated delete failure"));
  // container and root mount the real card and deletion confirmation.
  // container 和 root 挂载真实卡片与删除确认框。
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  // clickButton locates visible localized actions and fails when the expected control is absent.
  // clickButton 定位可见本地化动作，并在预期控件不存在时失败。
  const clickButton = async (scope: ParentNode, text: string) => {
    const button = Array.from(scope.querySelectorAll("button")).find((entry) => (entry.textContent ?? "").includes(text) || (entry.getAttribute("aria-label") ?? "").includes(text));
    expect(button).toBeDefined();
    await act(async () => button!.click());
  };
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><WorkspaceMemoryPanel
      selectedConversationId="isolated-session" onRunVmmAction={run}
      snapshot={{ dynamicCards: [{ tag: "memory", description: "隔离来源", items: [{ content: "隔离正文", detail: "", ttlLabel: "本会话", memoryId: 42, dedupeKey: "isolated-memory" }] }], staticCards: [] }}
    /></I18nProvider>));
    await clickButton(container, "42");
    await clickButton(document.querySelector('[role="dialog"]')!, "永久删除");
    expect(document.querySelector('[role="dialog"] [role="alert"]')?.textContent).toContain("isolated delete failure");
    expect(document.querySelector('[role="dialog"]')?.textContent).toContain("隔离正文");
    await clickButton(document.querySelector('[role="dialog"]')!, "取消");
    await clickButton(container, "42");
    expect(document.querySelector('[role="dialog"] [role="alert"]')).toBeNull();
    expect(run).toHaveBeenCalledOnce();
  } finally { await act(async () => root.unmount()); container.remove(); }
});
