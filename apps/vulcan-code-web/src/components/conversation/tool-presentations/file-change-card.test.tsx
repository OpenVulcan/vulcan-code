// @vitest-environment jsdom
// 使用浏览器文档环境验证文件调用语义和差异计数。
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "@/i18n";
import type { ToolExecutionConversationBlock } from "../conversation-tool-execution-types";
import { resolveFileChangePresentation } from "./file-change-presentation";
import { FileChangePresentationCard } from "./file-change-card";

describe("file operation labels", () => {
  // Equal filesystem changes must retain the actual tool's distinct user-facing verb.
  // 相同文件系统变更必须保留实际工具各自的用户动作文案。
  it.each([["file_write", "写入"], ["file_edit", "编辑"], ["apply_patch", "应用补丁"], ["file", "写入"]])(
    "renders %s with its invocation label and actual terminal counts", (toolName, label) => {
      // This is the normalized event projection consumed by the production resolver.
      // 此处使用生产解析器消费的规范事件投影。
      const blocks: ToolExecutionConversationBlock[] = [{
        id: "file-result", body: "", kind: "tool-result", title: toolName,
        summary: "", toolName, toolCallId: "file-call", rawMetadata: {
          arguments: { path: "example.html", ...(toolName === "file" ? { action: "write" } : {}) },
          v1_event: true, v1_tool_event: {
            kind: "tool_call", api_call_id: 1, tool_call_id: 1, tool_name: toolName,
            input: { path: "example.html" }, request_stream_id: "file-stream", status: "completed",
            changeset: { mode: "applied", files: [{ change: "modify", path: "example.html", hunks: [{
              before: "", after: "", deleted: [{ line: 1, content: "old" }],
              inserted: [{ line: 1, content: "new" }, { line: 2, content: "extra" }],
            }] }] },
          },
        },
      }];
      // Resolve real presentation facts before rendering the shared production card.
      // 渲染共享生产卡片之前先解析真实展示事实。
      const presentation = resolveFileChangePresentation(blocks, "completed", {})!;
      const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
        <FileChangePresentationCard presentation={presentation} expanded={false}
          expandedToolPresentationIds={new Set()} requestProgress={null} onExpandedChange={() => undefined} />
      </I18nProvider>);
      const surface = document.createElement("div");
      surface.innerHTML = markup;
      expect(surface.querySelector(".file-operation-label")?.textContent).toBe(label);
      expect(surface.textContent).toContain("example.html");
      expect(surface.textContent).toContain("+2");
      expect(surface.textContent).toContain("-1");
      expect(surface.textContent).not.toContain("已生成");
    },
  );
});
