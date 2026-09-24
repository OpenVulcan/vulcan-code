// @vitest-environment jsdom
// 使用 jsdom 验证 V1 工具卡 Markdown 代码块渲染。

import React from "react";
import { renderToStaticMarkup as renderMarkup } from "react-dom/server";
import { TooltipProvider } from "@/components/ui/tooltip";
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

/** Matches the production tooltip boundary when verifying dedicated subagent controls and result cards.
 * 验证专属子代理控制与结果卡时保持生产提示框边界一致。
 * Parameter tree is the card fixture; returns its provider-complete static markup.
 * 参数 tree 为卡片夹具；返回含完整 Provider 的静态标记。 */
function renderToStaticMarkup(tree: React.ReactNode): string {
  return renderMarkup(<TooltipProvider>{tree}</TooltipProvider>);
}

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
      request_stream_id: `v1-card-tool-${seq + 9}`,
      output: options.output,
      changeset: options.changeset,
      status: options.status ?? "completed",
      external_refs: options.externalRefs,
      generated_resources: options.generatedResources,
    },
  };
}

/** Builds the exact persisted V1 output envelope produced by persistent_tool_output_from_result.
 * 构建 persistent_tool_output_from_result 产出的精确持久化 V1 输出信封。 */
function subagentToolOutput(
  // Public tool id stored beside the output.
  // 与输出一同存储的公开工具标识。
  toolName: string,
  // Exact host-result semantic kind.
  // 精确宿主结果语义类型。
  hostKind: string,
  // Exact structured host payload.
  // 精确结构化宿主载荷。
  hostPayload: ToolPayload,
): ToolPayload {
  return {
    tool_call_id: `provider-${toolName}`,
    tool_name: toolName,
    model_output: hostPayload,
    host_result: {
      kind: hostKind,
      payload: hostPayload,
    },
    is_error: false,
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
  it("localizes model-extension cards and renders bounded JSON payload sections", () => {
    const item = cardItem({
      toolName: "video_generate",
      input: { prompt: "A blue flame", duration_seconds: 6 },
      output: { task_id: "video-task-1", status: "submitted" },
    });
    const collapsedMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="en-US">
        <ConversationV1ToolCard item={item} />
      </I18nProvider>,
    );
    const expandedMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="en-US">
        <ConversationV1ToolCard
          defaultExpanded
          item={item}
        />
      </I18nProvider>,
    );

    expect(collapsedMarkup).toContain("Video generation");
    expect(collapsedMarkup).not.toContain("video_generate");
    expect(collapsedMarkup).toContain('data-provider-tool-icon-family="video"');
    expect(collapsedMarkup).toContain("lucide-video");
    expect(collapsedMarkup).not.toContain("Input");
    expect(collapsedMarkup).not.toContain("Output");
    expect(collapsedMarkup.match(/aria-expanded="false"/g)).toHaveLength(1);
    expect(collapsedMarkup).not.toContain("A blue flame");
    expect(collapsedMarkup).not.toContain("video-task-1");
    expect(expandedMarkup).toContain('role="tablist"');
    expect(expandedMarkup).toContain('data-payload-tab="input"');
    expect(expandedMarkup).not.toContain('data-conversation-measured-collapse-id="model-extension:v1-tool:40:input"');
    expect(expandedMarkup).toContain('data-payload-tab="output"');
    expect(expandedMarkup).toContain("model-extension-payload-body");
    expect(expandedMarkup).toContain("data-media-generation-result");
    expect(expandedMarkup).not.toContain("原始详情");
    expect(expandedMarkup).not.toContain("A blue flame");
    expect(expandedMarkup).toContain("video-task-1");
  });

  it("localizes every media-understanding card and reuses its capability icon", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1ToolCard
          item={cardItem({
            toolName: "video_understand",
            input: { resource: "attachment://video-1" },
            output: { text: "视频摘要" },
          })}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("视频理解");
    expect(markup).not.toContain("video_understand");
    expect(markup).toContain('data-provider-tool-icon-family="video"');
    expect(markup).toContain("lucide-video");
  });

  it("retains result/input/output tabs while a model extension is running", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="en-US">
        <ConversationV1ToolCard
          defaultExpanded
          item={cardItem({
            toolName: "video_generate",
            input: { prompt: "A pending sunrise" },
            status: "running",
          })}
        />
      </I18nProvider>,
    );

    expect(markup).toContain('data-payload-tab="input"');
    expect(markup).toContain('data-payload-tab="output"');
    expect(markup).not.toContain("A pending sunrise");
  });

  it("renders directory listing directly with a folder-search icon and no payload tabs", () => {
    const markup = renderToStaticMarkup(
      <ConversationV1ToolCard
        defaultExpanded
        item={cardItem({
          toolName: "file",
          input: { action: "list", path: "D:/project" },
          output: { markdown: "- src" },
        })}
      />,
    );

    expect(markup).toContain('data-tool-icon="folder-search"');
    expect(markup).toContain("查询文件列表");
    expect(markup).toContain("D:/project");
    expect(markup).toContain("src");
    expect(markup).not.toContain('role="tab"');
    expect(markup).not.toContain("原始详情");
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
    // states covers every user-visible state required by the V1 card contract.
    // states 覆盖 V1 卡片契约要求的全部用户可见状态。
    const states = [
      ["prepared", "准备调用"],
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
    expect(missingMarkup).toContain('class="v1-tool-card-navigation-button" disabled=""');
    expect(availableMarkup).toContain("terminal: terminal-1");
    expect(availableMarkup).not.toContain('class="v1-tool-card-navigation-button" disabled=""');
  });

  it.each(["ask_question", "reply_question", "wait_question"])("hides protocol details in the expanded %s card", (toolName) => {
    // Real receipt fields must not leak through the generic technical or raw-output panel.
    // 真实回执字段不得通过通用技术详情或原始输出面板泄漏。
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
      <ConversationV1ToolCard defaultExpanded item={cardItem({ toolName,
        input: toolName === "ask_question" ? { target_session_id: "private-session-id", question: "请确认结果" }
          : { question_id: "private-question-id", answer: "已确认结果" },
        output: { model_output: { operation_id: "private-operation-id", question_id: "private-question-id",
          answer: { body: "已确认结果" }, delivery: { state: "projected" } } },
      })} />
    </I18nProvider>);
    expect(markup).toContain("已确认结果");
    expect(markup).not.toContain("技术详情");
    expect(markup).not.toContain("工具回执");
    expect(markup).not.toContain("private-operation-id");
    expect(markup).not.toContain("private-session-id");
  });

  it("renders ask, dynamic subagent, and terminal events through the same persisted card shell", () => {
    const askMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1ToolCard
          item={cardItem({
            seq: 301,
            toolName: "ask",
            input: { questions: [{ header: "范围", question: "继续执行吗？", options: [] }] },
            output: { model_output: "## User Answers", host_result: { kind: "ask", payload: { tool_name: "ask", headers: ["范围"], answers: [["继续"]], display: { kind: "ask_question", questions: [{ header: "范围", question: "继续执行吗？", options: [], multiple: false, answers: ["继续"], answered: true }] } } } },
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

    for (const markup of [askMarkup, terminalMarkup]) {
      expect(markup).toContain("tool-presentation-card v1-tool-card");
    }
    expect(askMarkup).toContain("向用户提问");
    expect(subagentMarkup).toContain("tool-presentation-card subagent-orchestration");
    expect(subagentMarkup).toContain("review-agent");
    expect(terminalMarkup).toContain('data-tool-detail="true"');
    expect(terminalMarkup).toContain("Get-Process");
    expect(terminalMarkup).toContain("terminal: terminal-2");
  });

  it("routes every built-in subagent orchestration tool through the complete rich card", () => {
    // cases cover the complete public orchestration surface that previously fell into the generic V1 card.
    // cases 覆盖此前落入通用 V1 卡片的完整公开调度工具面。
    const cases: Array<{
      toolName: string;
      input: ToolPayload;
      hostKind: string;
      hostPayload: ToolPayload;
      expectedText: string;
    }> = [
      {
        toolName: "spawn_agent",
        input: { agent: "reviewer", prompt: "审核持久化链路" },
        hostKind: "subagent_task",
        hostPayload: {
          task_name: "reviewer",
          display_title: "reviewer",
          execution_status: "running",
          prompt: "审核持久化链路",
        },
        expectedText: "reviewer",
      },
      {
        toolName: "send_message",
        input: { target: "queue-agent", message: "补充检查队列", delivery_mode: "queue" },
        hostKind: "subagent_message",
        hostPayload: {
          target: "queue-agent",
          message: "补充检查队列",
          queued: true,
          delivery_mode: "queue",
          delivery_status: "queued",
        },
        expectedText: "queue-agent",
      },
      {
        toolName: "wait_agent",
        input: { timeout_ms: 10000 },
        hostKind: "subagent_wait",
        hostPayload: {
          message: "Received terminal child feedback.",
          timed_out: false,
          no_active_agents: false,
          terminal_event_count: 1,
          agents: [{
            task_name: "wait-agent",
            status: "completed",
            result: "等待完成",
            usage_segments: [{
              provider_call_count: 2,
              tool_call_count: 1,
              total_tokens: 420,
              cached_input_tokens: 120,
            }],
          }],
        },
        expectedText: "wait-agent",
      },
      {
        toolName: "agent_orchestration_info",
        input: {},
        hostKind: "subagent_status",
        hostPayload: {
          tool: "agent_orchestration_info",
          agents: [{ task_name: "info-agent", status: "running" }],
        },
        expectedText: "info-agent",
      },
      {
        toolName: "list_agents",
        input: {},
        hostKind: "subagent_status",
        hostPayload: {
          tool: "list_agents",
          agents: [{ task_name: "listed-agent", status: "completed", result: "已完成" }],
        },
        expectedText: "listed-agent",
      },
      {
        toolName: "close_agent",
        input: { target: "closed-agent" },
        hostKind: "subagent_control",
        hostPayload: {
          operation: "close",
          target: "closed-agent",
          closed_count: 1,
          closed_agents: ["closed-agent"],
          cleared_message_count: 1,
        },
        expectedText: "closed-agent",
      },
      {
        toolName: "interrupt_agent",
        input: { target: "interrupted-agent" },
        hostKind: "subagent_control",
        hostPayload: {
          operation: "interrupt",
          target: "interrupted-agent",
          stopped_count: 1,
          stopped_subagent_task_ids: ["child-interrupted"],
        },
        expectedText: "interrupted-agent",
      },
    ];

    cases.forEach((testCase, index) => {
      const markup = renderToStaticMarkup(
        <I18nProvider initialLocale="zh-CN">
          <ConversationV1ToolCard
            defaultExpanded
            item={cardItem({
              seq: 400 + index,
              toolName: testCase.toolName,
              input: testCase.input,
              output: subagentToolOutput(
                testCase.toolName,
                testCase.hostKind,
                testCase.hostPayload,
              ),
              externalRefs: testCase.toolName === "spawn_agent"
                ? [{ kind: "subagent", id: "child-reviewer" }]
                : undefined,
            })}
          />
        </I18nProvider>,
      );

      expect(markup).toContain("tool-presentation-card subagent-orchestration");
      expect(markup).not.toContain('data-tool-kind="generic"');
      expect(markup).toContain(testCase.expectedText);
    });
  });

  it("keeps wait-agent usage statistics visible in the tips chip", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1ToolCard
          item={cardItem({
            seq: 500,
            toolName: "wait_agent",
            input: { timeout_ms: 10000 },
            output: subagentToolOutput("wait_agent", "subagent_wait", {
              message: "Received terminal child feedback.",
              timed_out: false,
              no_active_agents: false,
              terminal_event_count: 1,
              agents: [{
                task_name: "usage-agent",
                status: "completed",
                usage_segments: [{
                  provider_call_count: 2,
                  tool_call_count: 3,
                  total_tokens: 640,
                  cached_input_tokens: 240,
                }],
              }],
            }),
          })}
        />
      </I18nProvider>,
    );

    expect(markup).toContain("subagent-orchestration-compact-usage");
    expect(markup).toContain("2 次调用");
    expect(markup).toContain("3 个工具");
    expect(markup).toContain("640 tokens");
    expect(markup).toContain("缓存 240 tokens");
  });
});
