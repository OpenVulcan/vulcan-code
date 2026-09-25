// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { VmmMemoryActionRequest, VmmMemoryActionResult } from "@/types";
import { WorkspaceMemoryPanel } from "./workspace-memory-panel";

// Verifies opening static memory restores the selected session's durable profile without a prompt.
// 验证打开静态记忆即可恢复选中会话的长期画像，无需提交提示词。
it("loads static profile for the exact selected conversation", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  // run returns a distinct read-only profile for each exact conversation request.
  // run 为每个准确会话请求返回不同的只读画像。
  const run = vi.fn(async (request: VmmMemoryActionRequest): Promise<VmmMemoryActionResult> => request.action === "load_dynamic_memory_for_panel" ? ({
    action: request.action,
    summary: "loaded",
    kind: "dynamic_memory_cards",
    request: {},
    response: {},
    data: { kind: "dynamic_memory_cards", value: [] },
  }) : ({
    action: request.action,
    summary: "loaded",
    kind: "static_profile",
    request: {},
    response: {},
    data: {
      kind: "static_profile",
      value: {
        sections: [{
          target: "user",
          label: "USER",
          dates: [{
            date: "2026-09-16",
            nodes: [{
              id: 1,
              priority: "P2",
              level: "L2",
              refreshWeight: "W0",
              content: `profile-${request.conversationId}`,
              sourceKind: "turn_extract",
              sourceId: 1,
              bindId: 1,
            }],
          }],
        }],
      },
    },
  }));
  // container and root mount the real panel through a restart-like empty snapshot.
  // container 与 root 使用模拟重启后空快照挂载真实侧栏。
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><WorkspaceMemoryPanel
      selectedConversationId="session-a" onRunVmmAction={run}
      snapshot={{ dynamicCards: [], staticCards: [] }}
    /></I18nProvider>));
    expect(run).toHaveBeenCalledWith(expect.objectContaining({
      action: "load_dynamic_memory_for_panel",
      conversationId: "session-a",
    }));
    const staticTab = Array.from(container.querySelectorAll('[role="tab"]')).find((tab) => tab.textContent?.includes("静态"));
    expect(staticTab).toBeDefined();
    await act(async () => {
      staticTab!.dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 }));
    });
    expect(staticTab?.getAttribute("data-state")).toBe("active");
    expect(run).toHaveBeenCalledWith(expect.objectContaining({
      action: "load_static_profile_for_panel",
      conversationId: "session-a",
    }));
    expect(container.textContent).toContain("profile-session-a");

    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><WorkspaceMemoryPanel
      selectedConversationId="session-b" onRunVmmAction={run}
      snapshot={{ dynamicCards: [], staticCards: [] }}
    /></I18nProvider>));
    expect(run).toHaveBeenCalledWith(expect.objectContaining({
      action: "load_static_profile_for_panel",
      conversationId: "session-b",
    }));
    expect(container.textContent).toContain("profile-session-b");
    expect(container.textContent).not.toContain("profile-session-a");
  } finally {
    await act(async () => root.unmount());
    container.remove();
  }
});

// Verifies persisted dynamic leases appear on restart without relabeling them as live prompt injection.
// 验证重启后持久动态租约可直接展示，且不会被误当成实时提示词注入。
it("loads persisted dynamic memory for the selected conversation", async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  const run = vi.fn(async (request: VmmMemoryActionRequest): Promise<VmmMemoryActionResult> => ({
    action: request.action,
    summary: "loaded",
    kind: "dynamic_memory_cards",
    request: {},
    response: {},
    data: {
      kind: "dynamic_memory_cards",
      value: [{
        tag: "VMM_MEMORY",
        description: "resident",
        items: [{
          content: `resident-${request.conversationId}`,
          detail: `resident-${request.conversationId}`,
          ttlLabel: "剩余 2 回合",
          memoryId: 901,
        }],
      }],
    },
  }));
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  try {
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><WorkspaceMemoryPanel
      selectedConversationId="session-a" onRunVmmAction={run}
      snapshot={{ dynamicCards: [], staticCards: [] }}
    /></I18nProvider>));
    expect(container.textContent).toContain("resident-session-a");
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><WorkspaceMemoryPanel
      selectedConversationId="session-b" onRunVmmAction={run}
      snapshot={{ dynamicCards: [], staticCards: [] }}
    /></I18nProvider>));
    expect(container.textContent).toContain("resident-session-b");
    expect(container.textContent).not.toContain("resident-session-a");
  } finally {
    await act(async () => root.unmount());
    container.remove();
  }
});
