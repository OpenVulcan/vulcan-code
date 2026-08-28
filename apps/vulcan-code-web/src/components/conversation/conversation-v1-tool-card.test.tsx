import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { ConversationV1ToolCard } from "./conversation-v1-tool-card";
import { I18nProvider } from "@/i18n";
import type { ToolCallDisplayItem } from "@/service/conversation-event-display-projection";
import {
  resourceId,
  type PersistentChangeSet,
  type ToolPayload,
} from "@/service/conversation-event-window-store";
import type { ConversationSessionResourceCatalogV1 } from "@/state/conversation-session-resources-v1";

/** Builds one event-native card fixture.
 * 构建一个事件原生卡片夹具。 */
function cardItem(options: {
  seq?: number;
  toolName: string;
  input: ToolPayload;
  output?: ToolPayload;
  changeset?: PersistentChangeSet;
  status?: ToolCallDisplayItem["tool_call"]["status"];
  externalRefs?: ToolCallDisplayItem["tool_call"]["external_refs"];
  generatedResources?: ToolCallDisplayItem["tool_call"]["generated_resources"];
}): ToolCallDisplayItem {
  // seq keeps state-matrix fixtures independent while preserving exact event identity.
  // seq 让状态矩阵夹具彼此独立，同时保留精确事件身份。
  const seq = options.seq ?? 40;
  return {
    kind: "tool_call",
    seq,
    event_revision: 1,
    turn_id: 4,
    created_at_ms: 1,
    updated_at_ms: 2,
    visibility: "provider",
    group: { turn_id: 4, api_call_id: 8 },
    tool_call: {
      kind: "tool_call",
      api_call_id: 8,
      tool_call_id: seq + 9,
      tool_name: options.toolName,
      input: options.input,
      output: options.output,
      changeset: options.changeset,
      status: options.status ?? "completed",
      external_refs: options.externalRefs,
      generated_resources: options.generatedResources,
    },
  };
}

/** Builds the deleted-file card that previously showed -208 only in its header.
 * 构建曾经只在头部显示 -208 的删除文件卡片。 */
function deletedFileItem(): ToolCallDisplayItem {
  return cardItem({
    toolName: "file_edit",
    input: { path: "README.md" },
    changeset: {
      mode: "applied",
      files: [{
        change: "delete",
        path: "README.md",
        hunks: [],
        lifecycle_content: {
          mode: "truncated",
          head: "deleted 1\ndeleted 2",
          tail: "deleted 207\ndeleted 208",
          total_line_count: 208,
        },
        before_text_format: { encoding: "utf-8", line_ending: "LF" },
      }],
    },
  });
}

describe("ConversationV1ToolCard", () => {
  it("renders no placeholder icon node for directory listing", () => {
    const markup = renderToStaticMarkup(
      <ConversationV1ToolCard
        item={cardItem({
          toolName: "file",
          input: { action: "list", path: "D:/project" },
          output: { markdown: "- src" },
        })}
      />,
    );

    expect(markup).not.toContain("data-tool-icon=");
    expect(markup).toContain("查询文件列表");
    expect(markup).toContain("D:/project");
  });

  it("renders -208 accounting, retained deleted lines, and stable utf-8/LF metadata", () => {
    const collapsedMarkup = renderToStaticMarkup(<ConversationV1ToolCard item={deletedFileItem()} />);
    const expandedMarkup = renderToStaticMarkup(
      <ConversationV1ToolCard item={deletedFileItem()} defaultExpanded />,
    );

    expect(collapsedMarkup).toContain("-208");
    expect(collapsedMarkup).toContain("utf-8 · LF");
    expect(expandedMarkup).toContain("-208");
    expect(expandedMarkup).toContain("utf-8 · LF");
    expect(expandedMarkup).toContain("deleted 1");
    expect(expandedMarkup).toContain("省略 204 行");
    expect(expandedMarkup).toContain("deleted 208");
    expect(expandedMarkup).toContain('data-change-row="deleted"');
  });

  it("keeps rename paths, hunk counts, deleted rows, and format transition stable", () => {
    const item = cardItem({
      toolName: "apply_patch",
      input: { patch: "rename and edit" },
      changeset: {
        mode: "applied",
        files: [{
          change: "rename",
          old_path: "docs/old.md",
          new_path: "docs/new.md",
          hunks: [{
            before: "before context",
            after: "after context",
            deleted: [{ line: 7, content: "old text" }],
            inserted: [{ line: 7, content: "new text" }],
          }],
          lifecycle_content: {
            mode: "full",
            content: "old text\n",
            total_line_count: 1,
          },
          before_text_format: { encoding: "utf-8", line_ending: "CRLF" },
          after_text_format: { encoding: "utf-8", line_ending: "LF" },
        }],
      },
    });
    const collapsedMarkup = renderToStaticMarkup(<ConversationV1ToolCard item={item} />);
    const expandedMarkup = renderToStaticMarkup(<ConversationV1ToolCard item={item} defaultExpanded />);

    for (const markup of [collapsedMarkup, expandedMarkup]) {
      expect(markup).toContain("docs/old.md → docs/new.md");
      expect(markup).toContain("+1");
      expect(markup).toContain("-1");
      expect(markup).toContain("utf-8 · CRLF → utf-8 · LF");
    }
    expect(expandedMarkup).toContain("old text");
    expect(expandedMarkup).toContain("new text");
    expect(expandedMarkup).toContain('data-change-row="deleted"');
    expect(expandedMarkup).toContain('data-change-row="added"');
  });

  it("keeps all persisted active and terminal states expandable", () => {
    // states covers the five user-visible states required by the V1 card contract.
    // states 覆盖 V1 卡片契约要求的五种用户可见状态。
    const states = [
      ["completed", "已执行"],
      ["failed", "执行失败"],
      ["interrupted", "已中断"],
      ["waiting_user", "等待用户"],
      ["running", "正在调用"],
    ] as const;

    states.forEach(([status, label], index) => {
      const markup = renderToStaticMarkup(
        <ConversationV1ToolCard
          item={cardItem({
            seq: 100 + index,
            toolName: "shell_command",
            input: { command: `state-${status}` },
            output: { text: `output-${status}` },
            status,
          })}
          defaultExpanded
        />,
      );

      expect(markup).toContain(`data-tool-state="${status}"`);
      expect(markup).toContain('aria-expanded="true"');
      expect(markup).toContain(label);
      expect(markup).toContain(`output-${status}`);
    });
  });

  it("uses the established card shell for Markdown, native, shell, and unknown tools", () => {
    // cases exercise each migrated semantic body without creating a parallel visual root.
    // cases 覆盖每种迁移后的语义正文，且不创建平行视觉根节点。
    const cases = [
      cardItem({
        seq: 201,
        toolName: "vulcan_codekit_rg",
        input: { rg_pattern: "ToolCall" },
        output: { markdown: "# Markdown result" },
      }),
      cardItem({
        seq: 202,
        toolName: "create_goal",
        input: { objective: "Finish stage 9" },
        output: { text: "goal created" },
      }),
      cardItem({
        seq: 203,
        toolName: "shell_command",
        input: { command: "echo ok" },
        output: { text: "ok" },
      }),
      cardItem({
        seq: 204,
        toolName: "future_unknown_tool",
        input: { value: "input" },
        output: { text: "unknown output" },
      }),
    ];

    cases.forEach((item) => {
      const markup = renderToStaticMarkup(
        <I18nProvider initialLocale="zh-CN">
          <ConversationV1ToolCard item={item} />
        </I18nProvider>,
      );
      expect(markup).toContain('class="tool-presentation-card v1-tool-card');
      expect(markup).toContain('class="tool-presentation-summary v1-tool-card-summary');
    });
  });

  it("renders failed shell output on the same event-native card", () => {
    const markup = renderToStaticMarkup(
      <ConversationV1ToolCard
        item={cardItem({
          toolName: "shell_command",
          input: { command: "exit 1" },
          output: { text: "failed output" },
          status: "failed",
        })}
        defaultExpanded
      />,
    );

    expect(markup).toContain('data-tool-state="failed"');
    expect(markup).toContain("执行失败");
    expect(markup).toContain("exit 1");
    expect(markup).toContain("failed output");
  });

  it("renders image, generic, and missing generated resources from the project catalog", () => {
    // catalog contains the exact primary and preview resources referenced by the tool event.
    // catalog 包含工具事件引用的精确主资源与预览资源。
    const catalog: ConversationSessionResourceCatalogV1 = {
      sessionId: "session-generated",
      status: "ready",
      resources: new Map([
        [resourceId(201), {
          availability: "available",
          resource_id: resourceId(201),
          original_name: "render.png",
          mime: "image/png",
          size: 4096,
        }],
        [resourceId(202), {
          availability: "available",
          resource_id: resourceId(202),
          original_name: "archive.zip",
          mime: "application/zip",
          size: 8192,
        }],
        [resourceId(204), {
          availability: "available",
          resource_id: resourceId(204),
          original_name: "render-preview.png",
          mime: "image/png",
          size: 1024,
        }],
        [resourceId(203), {
          availability: "missing",
          resource_id: resourceId(203),
          placeholder: "[MISSING PROJECT RESOURCE]",
        }],
      ]),
      error: null,
      contentUrl: (currentResourceId) => `http://127.0.0.1/resource/${currentResourceId}`,
    };
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1ToolCard
          defaultExpanded
          item={cardItem({
            toolName: "shell_command",
            input: { command: "render-assets" },
            generatedResources: [
              {
                resource_id: resourceId(201),
                kind: "image",
                preview_resource_id: resourceId(204),
              },
              { resource_id: resourceId(202), kind: "file" },
              { resource_id: resourceId(203), kind: "file" },
            ],
          })}
          resourceCatalog={catalog}
        />
      </I18nProvider>,
    );

    expect(markup).toContain('data-generated-resource-count="3"');
    expect(markup).toContain('data-generated-resource-kind="image"');
    expect(markup).toContain('src="http://127.0.0.1/resource/204"');
    expect(markup).toContain("render.png");
    expect(markup).toContain('data-generated-resource-kind="file"');
    expect(markup).toContain("archive.zip");
    expect(markup).toContain("[MISSING PROJECT RESOURCE]");
  });

  it("disables only missing navigation targets while keeping the historical card visible", () => {
    const item = cardItem({
      toolName: "shell_session",
      input: { command: "pwsh" },
      externalRefs: [{ kind: "terminal", id: "terminal-1" }],
    });
    const missingMarkup = renderToStaticMarkup(
      <ConversationV1ToolCard item={item} defaultExpanded />,
    );
    const availableMarkup = renderToStaticMarkup(
      <ConversationV1ToolCard
        item={item}
        defaultExpanded
        availableExternalRefKeys={new Set(["terminal:terminal-1"])}
        onOpenExternalRef={() => {}}
      />,
    );

    expect(missingMarkup).toContain("terminal: terminal-1");
    expect(missingMarkup).toContain("disabled");
    expect(availableMarkup).toContain("terminal: terminal-1");
    expect(availableMarkup).not.toContain("disabled");
  });

  it("renders ask, dynamic subagent, and terminal events through the same persisted card shell", () => {
    const askMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1ToolCard
          item={cardItem({
            seq: 301,
            toolName: "ask",
            input: { questions: [{ header: "范围", question: "继续执行吗？", options: [] }] },
            output: { host_result: { kind: "ask_question", payload: { answers: [["继续"]] } } },
          })}
        />
      </I18nProvider>,
    );
    const subagentMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1ToolCard
          item={cardItem({
            seq: 302,
            toolName: "review-agent",
            input: { prompt: "审核持久化契约" },
            output: { model_output: "审核完成" },
            externalRefs: [{ kind: "subagent", id: "child-session-1" }],
          })}
        />
      </I18nProvider>,
    );
    const terminalMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1ToolCard
          item={cardItem({
            seq: 303,
            toolName: "terminal_session",
            input: { chars: "Get-Process" },
            output: { text: "process list" },
            externalRefs: [{ kind: "terminal", id: "terminal-2" }],
          })}
          defaultExpanded
        />
      </I18nProvider>,
    );

    for (const markup of [askMarkup, subagentMarkup, terminalMarkup]) {
      expect(markup).toContain("tool-presentation-card v1-tool-card");
    }
    expect(askMarkup).toContain("ask");
    expect(subagentMarkup).toContain("review-agent");
    expect(terminalMarkup).toContain('data-tool-detail="true"');
    expect(terminalMarkup).toContain("Get-Process");
    expect(terminalMarkup).toContain("terminal: terminal-2");
  });
});
