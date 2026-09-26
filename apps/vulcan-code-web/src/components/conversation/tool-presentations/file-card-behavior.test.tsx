// @vitest-environment jsdom
import React from "react";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { ToolPresentationCard } from "../conversation-tool-presentation-card";
import type { ToolPresentation } from "./types";
import { resolveFileAccessPresentation, resolveFileChangePresentation } from "./file-change-presentation";
import { resolveMarkdownToolPresentation } from "./markdown-output-presentation";
import { fileTargetLabel } from "./tool-card-common";

/** Builds blocks from one canonical event; returns the exact metadata consumed by production adapters.
 * 从规范事件构建块；返回生产适配器消费的精确元数据。 */
function blocks(event: ToolCallEvent) {
  return [{ id: "file-card", kind: "tool-result" as const, toolName: event.tool_name,
    title: event.tool_name, summary: "", body: typeof event.output?.model_output === "string" ? event.output.model_output : "",
    rawMetadata: { v1_event: true, v1_tool_event: event, arguments: event.input } }];
}

/** Renders a resolved card with explicit expansion; returns markup without fetching hidden state.
 * 使用明确展开状态渲染卡片；返回标记，不读取隐藏状态。 */
function card(presentation: ToolPresentation, event: ToolCallEvent, expanded: boolean) {
  return <I18nProvider initialLocale="zh-CN"><ToolPresentationCard presentation={presentation}
    v1ToolEvent={event} expanded={expanded} expandedToolPresentationIds={new Set()} requestProgress={null}
    detailLoadEnabled={false} onExpandedChange={() => undefined} onLoadToolDetail={async () => null} /></I18nProvider>;
}

describe("file inspection and repository cards", () => {
  it.each(["read", "list", "stat"] as const)("shows failed %s in one collapsed row without payload tabs", action => {
    // Failures contain the same Markdown and host error envelope as canonical tool completion.
    // 失败包含与规范工具终态相同的 Markdown 与宿主错误信封。
    const event: ToolCallEvent = { kind: "tool_call", api_call_id: 1, tool_call_id: 1,
      tool_name: "file", request_stream_id: "missing", input: { action, path: "missing.txt" },
      status: "failed", output: { model_output: "# Tool failed: file\n\n[path_not_found] missing.txt",
        host_result: { kind: "tool_error", payload: { status: "failed", error: "[path_not_found] missing.txt" } } } };
    const presentation = resolveFileAccessPresentation(blocks(event), "failed", {})!;
    const collapsed = renderToStaticMarkup(card(presentation, event, false));
    expect(collapsed).toContain(">失败</span>");
    expect(collapsed).toContain('aria-expanded="false"');
    expect(collapsed).not.toContain("path_not_found");
    const expanded = renderToStaticMarkup(card(presentation, event, true));
    expect(expanded).toContain("path_not_found");
    expect(expanded).not.toContain('role="tab"');
    expect(expanded).not.toContain("host_result");
  });

  it("keeps empty reads informative and expandable", () => {
    // Zero source lines remain a successful, explicitly explained read.
    // 零源文件行保持为成功且明确解释的读取。
    const body = "# File: empty.txt\n\n[empty_file] The file exists but contains no text.";
    const event: ToolCallEvent = { kind: "tool_call", api_call_id: 1, tool_call_id: 1,
      tool_name: "file", request_stream_id: "empty", input: { action: "read", path: "empty.txt" },
      status: "completed", output: { model_output: body, host_result: { kind: "file_access", payload: {
        schema: "vulcan.file_read.v2", action: "read", total_line_count: 0,
        range: { source_start_line: 1, output_start_line: 1, line_count: 0 },
      } } } };
    const presentation = resolveFileAccessPresentation(blocks(event), "completed", {})!;
    const collapsed = renderToStaticMarkup(card(presentation, event, false));
    expect(collapsed).toContain("空文件");
    expect(collapsed).toContain('aria-disabled="false"');
    expect(collapsed).not.toContain(">失败</span>");
    expect(renderToStaticMarkup(card(presentation, event, true))).toContain("empty_file");
  });

  it.each([".", "./", ".\\"])("hides the implicit workspace root %s but not its parent", path => {
    expect(fileTargetLabel(path, [], key => key)).toBe("");
    expect(fileTargetLabel("..", [], key => key)).toBe("..");
  });

  it("renders repository overview as one Markdown source card with nested fences intact", () => {
    const event: ToolCallEvent = { kind: "tool_call", api_call_id: 1, tool_call_id: 1,
      tool_name: "vulcan_codekit_repo_map", request_stream_id: "repo", input: {},
      status: "completed", output: { model_output: "# Repository\n\n```rust\nfn main() {}\n```\nEND" } };
    const presentation = resolveMarkdownToolPresentation(blocks(event), "completed")!;
    const markup = renderToStaticMarkup(card(presentation, event, true));
    expect(markup).toContain("查看仓库概览");
    expect(markup).toContain("markdown-code-block");
    expect(markup).toContain("END");
    expect(markup).not.toContain('role="tab"');
  });

  it("opens failed mutation details only after clicking the summary", async () => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const event: ToolCallEvent = { kind: "tool_call", api_call_id: 1, tool_call_id: 1,
      tool_name: "file", request_stream_id: "rename", input: { action: "move", path: "old.txt", destination: "new.txt" },
      status: "failed", output: { model_output: "[destination_exists] target exists",
        host_result: { kind: "tool_error", payload: { error: "[destination_exists] target exists" } } } };
    const presentation = resolveFileChangePresentation(blocks(event), "failed", {})!;
    /** Owns only user expansion for the interaction test; returns the production card.
     * 在交互测试中仅持有用户展开状态；返回生产卡片。 */
    function Harness() {
      const [expanded, setExpanded] = React.useState(false);
      return <I18nProvider initialLocale="zh-CN"><ToolPresentationCard presentation={presentation} v1ToolEvent={event}
        expanded={expanded} expandedToolPresentationIds={new Set()} requestProgress={null} detailLoadEnabled={false}
        onExpandedChange={(_, next) => setExpanded(next)} onLoadToolDetail={async () => null} /></I18nProvider>;
    }
    const host = document.createElement("div");
    const root = createRoot(host);
    try {
      await React.act(async () => root.render(<Harness />));
      expect(host.querySelector(".file-change-error-text")).toBeNull();
      await React.act(async () => host.querySelector<HTMLButtonElement>(".tool-presentation-summary")!.click());
      expect(host.querySelector(".file-change-error-text")?.textContent).toContain("destination_exists");
    } finally {
      await React.act(async () => root.unmount());
    }
  });
});
