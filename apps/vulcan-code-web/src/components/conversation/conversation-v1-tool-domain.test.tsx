// @vitest-environment jsdom
// 使用 jsdom 验证领域结果及原始详情的真实交互。
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConversationV1ToolDomainBody, ConversationV1ToolRawDetails } from "./conversation-v1-tool-domain";
import { ConversationV1ToolCard } from "./conversation-v1-tool-card";
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
  it("retains all list items while initially mounting only the first six", async () => {
    // rows deliberately exceed the first-level result bound.
    // rows 刻意超过首层结果数量上限。
    const rows = Array.from({ length: 9 }, (_, index) => ({ title: `document-${index}` }));
    await act(async () => root.render(<ConversationV1ToolDomainBody domain={{ summary: "", diagnostic: null, sections: [{ title: "文档", value: rows }] }} />));
    expect(host.textContent).toContain("document-5");
    expect(host.textContent).not.toContain("document-8");
    await act(async () => host.querySelector<HTMLButtonElement>("button")!.click());
    expect(host.textContent).toContain("document-8");
    expect(host.querySelectorAll(".v1-tool-domain-list-item")).toHaveLength(9);
  });

  it("keeps raw details unmounted until expanded and copies the complete original payload", async () => {
    // writeText captures the clipboard payload without touching the system clipboard.
    // writeText 捕获复制载荷，不接触系统剪贴板。
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    // input contains nontrivial whitespace to verify exact JSON copying.
    // input 包含非平凡空白，用于验证精确 JSON 复制。
    const input = { command: "  line 1\nline 2  " };
    await act(async () => root.render(<ConversationV1ToolRawDetails id="raw-copy-test" input={input} output={{ model_output: "retained output" }} />));
    expect(host.querySelector("pre")).toBeNull();
    await act(async () => host.querySelector<HTMLButtonElement>(".v1-tool-domain-raw-toggle")!.click());
    expect(host.querySelectorAll("pre")).toHaveLength(2);
    await act(async () => host.querySelector<HTMLButtonElement>(".v1-tool-domain-raw-heading button")!.click());
    expect(writeText).toHaveBeenCalledWith(JSON.stringify(input, null, 2));
    expect(host.textContent).toContain("已复制");
    delete (navigator as unknown as { clipboard?: unknown }).clipboard;
  });

  it("reports clipboard failure visibly and preserves selectable raw JSON", async () => {
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) } });
    await act(async () => root.render(<ConversationV1ToolRawDetails id="raw-failure-test" input={{ text: "retained" }} />));
    await act(async () => host.querySelector<HTMLButtonElement>(".v1-tool-domain-raw-toggle")!.click());
    await act(async () => host.querySelector<HTMLButtonElement>(".v1-tool-domain-raw-heading button")!.click());
    expect(host.textContent).toContain("复制失败，请选择文本复制");
    expect(host.querySelector("pre")?.textContent).toContain("retained");
    delete (navigator as unknown as { clipboard?: unknown }).clipboard;
  });

  it("distinguishes null, zero, false and an empty result list", () => {
    // markup is static because the unknown-value check needs no interaction.
    // markup 使用静态渲染，因为未知值检查不需要交互。
    const markup = renderToStaticMarkup(<ConversationV1ToolDomainBody domain={{ summary: "", diagnostic: null, sections: [{ title: "数据", value: { unknown: null, zero: 0, flag: false, rows: [] } }] }} />);
    expect(markup).toContain(">-</span>");
    expect(markup).toContain('title="0">0</span>');
    expect(markup).toContain(">否</span>");
    expect(markup).toContain("无条目");
  });

  it("renders canonical media text before any raw envelope and preserves exact tool state", () => {
    // item carries the actual V1 serialized model_output envelope for media understanding.
    // item 携带媒体理解实际 V1 序列化 model_output 信封。
    const item: ToolCallDisplayItem = {
      kind: "tool_call", seq: 803, event_revision: 1, turn_id: 1, created_at_ms: 1, updated_at_ms: 2, visibility: "provider", group: { turn_id: 1, api_call_id: 1 },
      tool_call: { kind: "tool_call", api_call_id: 1, tool_call_id: 2, tool_name: "image_understand", status: "completed", request_stream_id: "media-test", input: { model: "provider/model", args: {} }, output: { model_output: JSON.stringify({ schema_version: "vulcan.media-understanding.v1", media_kind: "image", answer: "可读图像说明", segments: [], warnings: [] }), is_error: false, host_result: null } },
    };
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationV1ToolCard item={item} defaultExpanded /></I18nProvider>);
    expect(markup).toContain("可读图像说明");
    expect(markup).toContain('data-tool-state="completed"');
    expect(markup).not.toContain("schema_version");
    expect(markup).not.toContain("model_output");
  });
});
