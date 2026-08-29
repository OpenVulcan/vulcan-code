import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { SelectedConversationEventWindowCommands } from "@/service/selected-conversation-event-window-registry";
import {
  clientActionId,
  type ConversationEventSummary,
} from "@/service/conversation-event-window-store";
import type { ConversationSummary } from "@/types";
import { ConversationLiveStreamProvider } from "@/state/conversation-live-stream-context";
import { ConversationLiveStreamStore } from "@/state/conversation-live-stream-store";
import type { ConversationSessionResourceCatalogV1 } from "@/state/conversation-session-resources-v1";
import { projectConversationBranchPresentationV1 } from "./conversation-branch-presentation-v1";
import { ConversationBranchWorkspaceV1 } from "./conversation-branch-workspace-v1";

// eventSummary builds one typed V1 envelope for restored-workspace tests.
// eventSummary 为恢复后 workspace 测试构建一条强类型 V1 envelope。
function eventSummary(
  seq: number,
  event: ConversationEventSummary["event"],
  visibility: ConversationEventSummary["visibility"] = "provider",
): ConversationEventSummary {
  return {
    seq,
    turn_id: 11,
    created_at_ms: 1_000 + seq,
    updated_at_ms: 1_000 + seq,
    event_revision: 1,
    visibility,
    event,
  };
}

// emptyResourceCatalog builds an owner-bound catalog for presentation-only tests with no attachments.
// emptyResourceCatalog 为不含附件的纯展示测试构建一份绑定所有者的资源目录。
function emptyResourceCatalog(): ConversationSessionResourceCatalogV1 {
  return {
    sessionId: "session-branch-v1",
    status: "ready",
    resources: new Map(),
    error: null,
    contentUrl: (resourceId) => `/resource/${resourceId}`,
  };
}

// completeTurnEvents builds one complete Turn containing reasoning, a tool card, final output, and statistics.
// completeTurnEvents 构建一个包含思考、工具卡、最终输出与统计的完整 Turn。
function completeTurnEvents(): readonly ConversationEventSummary[] {
  const emptyBucket = {
    api_call_count: 0,
    unknown_usage_api_call_count: 0,
    tool_call_count: 0,
    subagent_return_count: 0,
    usage: {},
  };
  const totalBucket = {
    api_call_count: 2,
    unknown_usage_api_call_count: 0,
    tool_call_count: 1,
    subagent_return_count: 0,
    usage: {
      uncached_input_tokens: 40,
      output_tokens: 12,
      reasoning_tokens: 5,
      total_tokens: 52,
      cache_read_tokens: 8,
    },
  };
  return [
    eventSummary(1, {
      kind: "user_input",
      client_action_id: clientActionId("branch-workspace-root"),
      input_id: 1,
      input_source: "root",
      text: "检查当前实现",
    }),
    eventSummary(2, {
      kind: "assistant_reasoning",
      api_call_id: 1,
      text: "先核对真实调用链。",
      status: "completed",
    }),
    eventSummary(3, {
      kind: "tool_call",
      api_call_id: 1,
      tool_call_id: 1,
      tool_name: "shell_command",
      input: { command: "rg current-call-chain" },
      request_stream_id: "branch-tool-1",
      output: { stdout: "one match" },
      status: "completed",
    }),
    eventSummary(4, {
      kind: "api_stats",
      api_call_id: 1,
      purpose: "main_generation",
      provider: "openai",
      model: "gpt-v1",
      usage: { input_tokens: 20, output_tokens: 5, total_tokens: 25 },
      status: "completed",
      continuation_eligibility: "forbidden",
      timing: { completed_at_ms: 1_100 },
    }, "ui_only"),
    eventSummary(5, {
      kind: "assistant_text",
      api_call_id: 2,
      text: "已经完成核对。",
      status: "completed",
    }),
    eventSummary(6, {
      kind: "api_stats",
      api_call_id: 2,
      purpose: "main_generation",
      provider: "openai",
      model: "gpt-v1",
      usage: { input_tokens: 20, output_tokens: 7, total_tokens: 27 },
      status: "completed",
      continuation_eligibility: "forbidden",
      timing: { completed_at_ms: 1_200 },
    }, "ui_only"),
    eventSummary(7, { kind: "turn_boundary", outcome: "completed" }),
    eventSummary(8, {
      kind: "turn_stats",
      start_seq: 1,
      end_seq: 7,
      outcome: "completed",
      duration_ms: 200,
      buckets: { direct: totalBucket, subagent: emptyBucket, total: totalBucket },
    }, "ui_only"),
  ];
}

// snapshot builds one exact owner-ready event-window snapshot.
// snapshot 构建一份精确归属就绪的事件窗口快照。
function snapshot(events: readonly ConversationEventSummary[]): ConversationEventWindowControllerSnapshot {
  return {
    owner: { session_id: "session-branch-v1", window_generation: 3 },
    service_epoch: "service-branch-v1",
    memory_revision: 8,
    durable_revision: 8,
    events,
    older_before_seq: null,
    has_more: false,
    open_turn_id: null,
    open_turn_start_seq: null,
    session_config: null,
  };
}

// renderWorkspace renders the real restored branch shell with V1 authority.
// renderWorkspace 使用 V1 权威渲染真实恢复后的分支外壳。
function renderWorkspace(events: readonly ConversationEventSummary[]): string {
  const conversation: ConversationSummary = {
    id: "session-branch-v1",
    title: "恢复界面",
    status: "completed",
  };
  const commands: SelectedConversationEventWindowCommands = {
    rehydrate: vi.fn(async () => true),
    prependOlder: vi.fn(async () => true),
  };
  return renderToStaticMarkup(
    <I18nProvider initialLocale="zh-CN">
      <TooltipProvider>
        <ConversationLiveStreamProvider store={new ConversationLiveStreamStore()}>
          <ConversationBranchWorkspaceV1
            commandSessions={[]}
            commands={commands}
            conversation={conversation}
            initialExpandedState={null}
            onCopyText={() => undefined}
            onExpandedStateChange={() => undefined}
            onOpenSubagentPanel={async () => true}
            onPreserveHistoryPrepend={(load) => load()}
            onRootInputGenerationAction={() => undefined}
            onScrollStateChange={() => undefined}
            reasoningDisplayMode="inline_output"
            scrollRef={React.createRef<HTMLElement>()}
            sessionId={conversation.id}
            sessionRunning={false}
            snapshot={snapshot(events)}
          />
        </ConversationLiveStreamProvider>
      </TooltipProvider>
    </I18nProvider>,
  );
}

describe("ConversationBranchWorkspaceV1", () => {
  it("uses the durable user-input update time after an edited generation", () => {
    const editedRoot = {
      ...eventSummary(1, {
        kind: "user_input",
        client_action_id: clientActionId("branch-workspace-edited-root"),
        input_id: 1,
        input_source: "root",
        text: "编辑后的提示词",
      }),
      created_at_ms: 1_000,
      updated_at_ms: 9_000,
      event_revision: 2,
    } satisfies ConversationEventSummary;
    const presentation = projectConversationBranchPresentationV1(
      "session-branch-v1",
      [editedRoot],
      emptyResourceCatalog(),
      (key) => key,
    );

    expect(presentation.messages[0]?.createdAt).toBe("9000");
  });

  it("restores the complete branch transcript suite while retaining V1 as the only authority", () => {
    const markup = renderWorkspace(completeTurnEvents());

    expect(markup).toContain('class="conversation-panel"');
    expect(markup).not.toContain("conversation-v1-display");
    expect(markup).toContain("检查当前实现");
    expect(markup).toContain('aria-label="复制用户提示词"');
    expect(markup).toContain('aria-label="编辑用户提示词"');
    expect(markup).toContain("处理过程");
    expect(markup).toContain("rg current-call-chain");
    expect(markup).toContain("tool-presentation-card");
    expect(markup).toContain("turn-stats-row");
    expect(markup).not.toContain(">SYSTEM<");
    expect(markup).toContain('aria-label="分叉会话"');
    expect(markup).toContain('aria-label="复制最终输出原文"');
  });

  // Durable tool envelopes must recover branch-native summaries and bodies without exposing wrapper JSON.
  // 可靠工具封装必须恢复分支原生摘要与正文，且不得暴露封装 JSON。
  it("adapts prompt-skill Markdown and file-read ranges from exact V1 tool envelopes", () => {
    // filePayload mirrors the strongly typed vulcan.file_access.v2 host result persisted by the runtime.
    // filePayload 镜像运行时持久化的强类型 vulcan.file_access.v2 宿主结果。
    const filePayload = {
      action: "read",
      actual_range: { start_line: 10, end_line: 24 },
      content: "line ten\nline eleven",
      end_line: 24,
      lines: [
        { content: "line ten", line_number: 10 },
        { content: "line eleven", line_number: 11 },
      ],
      ok: true,
      path: "D:/openvulcan/tui_test.py",
      request_path: "tui_test.py",
      schema: "vulcan.file_access.v2",
      start_line: 10,
    };
    const markup = renderWorkspace([
      eventSummary(1, {
        kind: "tool_call",
        api_call_id: 1,
        tool_call_id: 1,
        request_stream_id: "branch-skill-1",
        tool_name: "skill",
        input: { name: "terminal-session" },
        output: {
          host_result: null,
          is_error: false,
          model_output: "<skill_content name=\"terminal-session\">\n# Terminal Session\n\nUse the persistent terminal safely.\n</skill_content>",
          tool_call_id: "branch-skill-1",
          tool_name: "skill",
        },
        status: "completed",
      }),
      eventSummary(2, {
        kind: "tool_call",
        api_call_id: 1,
        tool_call_id: 2,
        request_stream_id: "branch-file-1",
        tool_name: "file",
        input: { action: "read", path: "tui_test.py" },
        output: {
          host_result: { kind: "file_access", payload: filePayload },
          is_error: false,
          model_output: JSON.stringify(filePayload),
          tool_call_id: "branch-file-1",
          tool_name: "file",
        },
        status: "completed",
      }),
    ]);

    expect(markup).toContain("查看技能 · terminal-session");
    expect(markup).toContain("读取文件 · tui_test.py · 第 10-24 行");
    expect(markup).toContain("file-access-operation-icon");
    expect(markup).toContain('aria-expanded="false"');
    expect(markup).not.toContain("model_output");
    expect(markup).not.toContain("host_result");
  });

  // Only durable error notes retain the system role and they render through the established error card.
  // 只有可靠错误提示保留 system 角色，并通过既有错误卡片渲染。
  it("renders system identity only together with an error card", () => {
    const markup = renderWorkspace([
      eventSummary(1, {
        kind: "api_stats",
        api_call_id: 7,
        purpose: "main_generation",
        provider: "test-provider",
        model: "test-model",
        usage: {},
        status: "failed",
        end_reason: "provider_error",
        error: { category: "provider", message: "request failed" },
        continuation_eligibility: "forbidden",
        timing: { completed_at_ms: 1_100 },
      }, "ui_only"),
      eventSummary(2, {
        kind: "display_note",
        notice_kind: "provider_error",
        severity: "error",
        reason_code: "provider_request_failed",
        redacted_detail: "request failed",
        api_call_id: 7,
      }, "ui_only"),
    ]);

    expect(markup).toContain(">SYSTEM<");
    expect(markup).toContain("system-error-card");
    expect(markup).toContain("request failed");
  });

  it("never renders stale events from a previous selected owner", () => {
    const stale = snapshot([
      eventSummary(1, {
        kind: "assistant_text",
        api_call_id: 9,
        text: "不应显示的旧会话正文",
        status: "completed",
      }),
    ]);
    stale.owner = { session_id: "different-session", window_generation: 4 };
    const conversation: ConversationSummary = {
      id: "session-branch-v1",
      title: "恢复界面",
      status: "completed",
    };
    const commands: SelectedConversationEventWindowCommands = {
      rehydrate: vi.fn(async () => true),
      prependOlder: vi.fn(async () => true),
    };
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <TooltipProvider>
          <ConversationLiveStreamProvider store={new ConversationLiveStreamStore()}>
            <ConversationBranchWorkspaceV1
              commandSessions={[]}
              commands={commands}
              conversation={conversation}
              initialExpandedState={null}
              onCopyText={() => undefined}
              onExpandedStateChange={() => undefined}
              onOpenSubagentPanel={async () => true}
              onPreserveHistoryPrepend={(load) => load()}
              onScrollStateChange={() => undefined}
              reasoningDisplayMode="inline_output"
              scrollRef={React.createRef<HTMLElement>()}
              sessionId={conversation.id}
              sessionRunning={false}
              snapshot={stale}
            />
          </ConversationLiveStreamProvider>
        </TooltipProvider>
      </I18nProvider>,
    );

    expect(markup).not.toContain("不应显示的旧会话正文");
  });
});
