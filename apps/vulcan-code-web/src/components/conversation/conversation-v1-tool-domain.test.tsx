// @vitest-environment jsdom
// 使用 jsdom 验证领域结果及原始详情的真实交互。
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConversationV1ToolDomainBody } from "./conversation-v1-tool-domain";
import ConversationBranchToolDomain from "./conversation-branch-tool-domain";
import { I18nProvider } from "@/i18n";
import type { ToolCallDisplayItem } from "@/service/conversation-event-display-projection";

// host isolates DOM and clipboard observations from other conversation tests.
// host 将 DOM 与剪贴板观察结果和其他会话测试隔离。
let host: HTMLDivElement;
// root owns the current mounted test components.
// root 持有当前挂载的测试组件。
let root: Root;

beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.restoreAllMocks();
  delete (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
});

describe("V1 tool domain readability and progressive details", () => {
  // The production path must reuse the known-tool code blocks and keep raw payloads in peer tabs.
  // 生产路径须复用已知工具代码块，并将原始载荷放入同级标签页。
  it("renders unknown tools as result/input/output tabs", async () => {
    // Nested fences and a long result must remain one complete copyable source block.
    // 嵌套围栏与长结果必须保持为一个完整可复制的源码块。
    const markdown = "# REPO MAP\n\n```json\n{}\n```\n" + "目录说明。".repeat(400) + "\n\n## 完整结果末尾";
    // item carries a canonical unknown-tool result, with diagnostics that belong only to raw output.
    // item 携带规范未知工具结果，其诊断字段仅属于原始输出。
    const item: ToolCallDisplayItem = {
      kind: "tool_call", seq: 91, event_revision: 1, turn_id: 1, created_at_ms: 1, updated_at_ms: 2,
      visibility: "provider", group: { turn_id: 1, api_call_id: 1 },
      tool_call: { kind: "tool_call", api_call_id: 1, tool_call_id: 91, tool_name: "unknown_repo_map",
        status: "completed", request_stream_id: "generic-tabs", input: { dir: "crates", max_depth: 3 },
        output: { model_output: markdown, host_result: { diagnostic_marker: "raw-only" }, is_error: false } },
    };
    // writeText records exact copied data without touching the system clipboard.
    // writeText 记录精确复制数据，不接触系统剪贴板。
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    try {
      await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><ConversationBranchToolDomain event={item.tool_call} id="generic-tabs" tabbed /></I18nProvider>));
      expect(Array.from(host.querySelectorAll('[role="tab"]'), (tab) => tab.textContent)).toEqual(["返回信息", "输入", "输出"]);
      expect(host.querySelector(".markdown-code-language > span:last-child")?.textContent).toBe("markdown");
      expect(host.querySelectorAll(".markdown-code-block")).toHaveLength(1);
      expect(host.querySelector("pre")?.textContent).toContain("## 完整结果末尾");
      expect(host.querySelector("h1")).toBeNull();
      expect(host.textContent).not.toContain("原始详情");
      expect(host.textContent).not.toContain("raw-only");
      await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="复制代码"]')!.click());
      expect(writeText).toHaveBeenLastCalledWith(markdown);
      await act(async () => host.querySelectorAll('[role="tab"]')[1].dispatchEvent(new MouseEvent("mousedown", { bubbles: true, button: 0 })));
      expect(host.querySelector(".markdown-code-language > span:last-child")?.textContent).toBe("json");
      expect(Array.from(host.querySelectorAll(".markdown-code-line"), line => line.textContent).join("\n")).toBe(JSON.stringify(item.tool_call.input, null, 2));
      expect(host.querySelector("h1")).toBeNull();
      await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="复制代码"]')!.click());
      expect(writeText).toHaveBeenLastCalledWith(JSON.stringify(item.tool_call.input, null, 2));
      await act(async () => host.querySelectorAll('[role="tab"]')[2].dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Enter" })));
      expect(host.querySelector(".markdown-code-language > span:last-child")?.textContent).toBe("json");
      await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="复制代码"]')!.click());
      expect(writeText).toHaveBeenLastCalledWith(JSON.stringify(item.tool_call.output, null, 2));
      expect(host.querySelectorAll('[role="tabpanel"]:not([hidden])')).toHaveLength(1);
    } finally {
      delete (navigator as unknown as { clipboard?: unknown }).clipboard;
    }
  });

  it("retains all list items while initially mounting only the first six", async () => {
    // rows deliberately exceed the first-level result bound.
    // rows 刻意超过首层结果数量上限。
    const rows = Array.from({ length: 9 }, (_, index) => ({ title: `document-${index}` }));
    await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><ConversationV1ToolDomainBody domain={{ summary: "", diagnostic: null, sections: [{ title: "文档", value: rows }] }} /></I18nProvider>));
    expect(host.textContent).toContain("document-5");
    expect(host.textContent).not.toContain("document-8");
    await act(async () => host.querySelector<HTMLButtonElement>("button")!.click());
    expect(host.textContent).toContain("document-8");
    expect(host.querySelectorAll(".v1-tool-domain-list-item")).toHaveLength(9);
  });

  it("distinguishes null, zero, false and an empty result list", () => {
    // markup is static because the unknown-value check needs no interaction.
    // markup 使用静态渲染，因为未知值检查不需要交互。
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationV1ToolDomainBody domain={{ summary: "", diagnostic: null, sections: [{ title: "数据", value: { unknown: null, zero: 0, flag: false, rows: [] } }] }} /></I18nProvider>);
    expect(markup).toContain(">-</span>");
    expect(markup).toContain('title="0">0</span>');
    expect(markup).toContain(">否</span>");
    expect(markup).toContain("无条目");
  });

  it("renders canonical media text before any raw envelope", () => {
    // item carries the actual V1 serialized model_output envelope for media understanding.
    // item 携带媒体理解实际 V1 序列化 model_output 信封。
    const item: ToolCallDisplayItem = {
      kind: "tool_call", seq: 803, event_revision: 1, turn_id: 1, created_at_ms: 1, updated_at_ms: 2, visibility: "provider", group: { turn_id: 1, api_call_id: 1 },
      tool_call: { kind: "tool_call", api_call_id: 1, tool_call_id: 2, tool_name: "image_understand", status: "completed", request_stream_id: "media-test", input: { model: "provider/model", args: {} }, output: { model_output: JSON.stringify({ schema_version: "vulcan.media-understanding.v1", media_kind: "image", answer: "可读图像说明", segments: [], warnings: [] }), is_error: false, host_result: null } },
    };
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationBranchToolDomain event={item.tool_call} id="media-test" /></I18nProvider>);
    expect(markup).toContain("可读图像说明");
    expect(markup).not.toContain("schema_version");
    expect(markup).not.toContain("model_output");
  });
});
