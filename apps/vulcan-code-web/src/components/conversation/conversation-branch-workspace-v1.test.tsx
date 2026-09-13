// @vitest-environment jsdom
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createRoot } from "react-dom/client";
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
import type { ThinkingDisplayMode } from "./thinking-display";
import { projectConversationTimelineItems } from "./conversation-timeline";
import { resolveToolPresentation } from "./conversation-tool-presentations";

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
      end_seq: 8,
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

// workspaceTree constructs the real restored branch shell with V1 authority.
// workspaceTree 使用 V1 权威构造真实恢复后的分支外壳。
// Parameters: events is the canonical history; reasoningDisplayMode selects its current presentation.
// 参数：events 是规范历史；reasoningDisplayMode 选择当前展示方式。
// Returns: the tree with every process expanded so static and deferred results share one fixture.
// 返回值：展开所有过程的组件树，使同步与延迟结果共用一个夹具。
function workspaceTree(
  events: readonly ConversationEventSummary[],
  reasoningDisplayMode: ThinkingDisplayMode = "inline_output",
  // Explicit card IDs allow testing the production expanded result rather than a detached presentation fixture.
  // 显式卡片标识用于测试生产展开结果，而不是脱离实际路径的展示夹具。
  expandedToolPresentationIds: string[] = [],
): React.ReactElement {
  const conversation: ConversationSummary = {
    id: "session-branch-v1",
    title: "恢复界面",
    status: "completed",
  };
  const commands: SelectedConversationEventWindowCommands = {
    rehydrate: vi.fn(async () => true),
    prependOlder: vi.fn(async () => true),
  };
  // timeline identifies exact process keys so historical folds cannot mask reasoning-mode assertions.
  // timeline 标识精确过程键，防止历史折叠遮蔽思考模式断言。
  const timeline = projectConversationTimelineItems(
    projectConversationBranchPresentationV1(conversation.id, events, emptyResourceCatalog(), (key) => key).renderItems,
    conversation,
    false,
  );
  return (
    <I18nProvider initialLocale="zh-CN">
      <TooltipProvider>
        <ConversationLiveStreamProvider store={new ConversationLiveStreamStore()}>
          <ConversationBranchWorkspaceV1
            commandSessions={[]}
            commands={commands}
            conversation={conversation}
            initialExpandedState={{
              processCollapsedByKey: Object.fromEntries(timeline.flatMap((item) => item.kind === "process"
                ? [[`${conversation.id}:${item.process.id}`, false]]
                : [])),
              expandedUserPromptIds: [],
              expandedToolPresentationIds,
              measuredCollapseById: {},
            }}
            onCopyText={() => undefined}
            onExpandedStateChange={() => undefined}
            onOpenSubagentPanel={async () => true}
            onPreserveHistoryPrepend={(load) => load()}
            onRootInputGenerationAction={() => undefined}
            onScrollStateChange={() => undefined}
            reasoningDisplayMode={reasoningDisplayMode}
            scrollRef={React.createRef<HTMLElement>()}
            sessionId={conversation.id}
            sessionRunning={false}
            snapshot={snapshot(events)}
          />
        </ConversationLiveStreamProvider>
      </TooltipProvider>
    </I18nProvider>
  );
}

// Render synchronous scenarios as static markup; deferred results use the same tree in a mounted DOM.
// 同步场景使用静态标记，延迟结果通过同一棵树挂载到 DOM。
// Parameters match workspaceTree; returns the synchronous markup.
// 参数与 workspaceTree 相同；返回同步标记。
function renderWorkspace(...args: Parameters<typeof workspaceTree>): string {
  return renderToStaticMarkup(workspaceTree(...args));
}

describe("ConversationBranchWorkspaceV1", () => {
  // The real runtime-config call revealed that branch rendering bypassed the direct V1 domain card.
  // 实际 runtime-config 调用暴露了分支渲染绕过直接 V1 领域卡的问题。
  it("renders the runtime descriptor through the full branch projection without eager raw JSON", async () => {
    // Payload follows the real describe result shape; the marker belongs only to raw diagnostics.
    // 载荷遵循实际 describe 返回结构；标记仅属于原始诊断。
    const events = completeTurnEvents().map((summary): ConversationEventSummary => summary.event.kind === "tool_call" ? {
      ...summary,
      event: { ...summary.event, tool_name: "runtime_config", input: { action: "describe" }, output: { model_output: JSON.stringify({ action: "describe", ok: true, error: null, result: [{ skill_id: "vulcan-codekit", skill_version: "0.1.11", complete: true, missing_count: 0, invalid_count: 0, business_issue_count: 0, orphaned_count: 0, items: [], revision: "raw-only-revision" }] }) } },
    } : summary);
    // Card ownership is resolved by the same production adapter used by the live tool row.
    // 卡片归属通过实际工具行使用的同一生产适配器解析。
    const branch = projectConversationBranchPresentationV1("session-branch-v1", events, emptyResourceCatalog(), (key) => key);
    // Only actual call/result blocks enter the same tool-card adapter as the production merger.
    // 只有真实调用/结果块进入与生产合并器相同的工具卡适配器。
    const blocks = branch.renderItems.flatMap((item) => item.blocks).flatMap((block) => block.toolName === "runtime_config" && (block.kind === "tool-call" || block.kind === "tool-result") ? [{ ...block, kind: block.kind as "tool-call" | "tool-result" }] : []);
    const presentation = resolveToolPresentation(blocks, "completed");
    // The mounted production tree must resolve its real deferred module, without mocking the result renderer.
    // 挂载的生产组件树必须解析真实延迟模块，不模拟结果渲染器。
    const host = document.createElement("div");
    document.body.appendChild(host);
    const root = createRoot(host);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    try {
      await React.act(async () => { root.render(workspaceTree(events, "inline_output", [presentation.id])); });
      await React.act(async () => { await vi.dynamicImportSettled(); });
      const markup = host.innerHTML;
      expect(markup).toContain("data-tool-domain-result");
      expect(markup).toContain("运行配置");
      expect(markup).toContain("配置完整");
      expect(markup).toContain("vulcan-codekit");
      expect(markup).toContain("原始详情");
      expect(markup).not.toContain("raw-only-revision");
      expect(markup).not.toContain("更新运行配置");
    } finally {
      await React.act(async () => root.unmount());
      host.remove();
      vi.unstubAllGlobals();
    }
  });

  // Mode changes must reproject the same completed ledger without waiting for a new provider event.
  // 模式变化必须重新投影同一份已结束账本，不能等待新的供应商事件。
  it("applies all display modes to completed history without mutating its events", () => {
    // events includes two reasoning segments in each completed logical Turn.
    // events 在每个已完成逻辑回合中包含两段思考。
    const events = completeTurnEvents().flatMap((summary) => summary.event.kind === "assistant_text"
      ? [eventSummary(0, {
          kind: "assistant_reasoning",
          api_call_id: summary.event.api_call_id,
          text: "工具结束后继续核对。",
          status: "completed",
        }), summary]
      : [summary]).map((summary, index) => ({
        ...summary,
        seq: index + 1,
        event: summary.event.kind === "turn_stats"
          ? { ...summary.event, end_seq: index + 1 }
          : summary.event,
      }));
    // history repeats the display fixture with distinct Turn and sequence identities to verify per-Turn selection.
    // history 以不同回合与序号身份重复展示测试数据，用于验证按回合选择首段。
    const history = [...events, ...events.map((summary) => ({
      ...summary,
      seq: summary.seq + events.length,
      turn_id: 12,
      event: summary.event.kind === "turn_stats"
        ? { ...summary.event, start_seq: events.length + 1, end_seq: events.length * 2 }
        : summary.event,
    }))];
    // original records the immutable ledger before any presentation choice.
    // original 记录任何展示选择之前的不可变账本。
    const original = JSON.stringify(history);
    // cases also returns to inline mode so previously hidden reasoning must remain recoverable.
    // cases 还会返回内联模式，确保此前隐藏的思考仍能恢复。
    const cases: readonly [ThinkingDisplayMode, number][] = [
      ["inline_output", 4],
      ["popup_only", 0],
      ["first_inline_with_popup", 2],
      ["first_inline_only", 2],
      ["inline_output", 4],
    ];
    for (const [mode, expectedCount] of cases) {
      // markup uses the production branch shell, including its exact row filtering.
      // markup 使用生产分支外壳，包含其精确行过滤逻辑。
      const markup = renderWorkspace(history, mode);
      expect(markup.match(/class="thinking-block[" ]/g)?.length ?? 0, mode).toBe(expectedCount);
      expect(markup, mode).toContain("已经完成核对。");
      expect(markup, mode).toContain("rg current-call-chain");
      expect(markup, mode).toContain("turn-stats-row");
    }
    expect(JSON.stringify(history)).toBe(original);
  });

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
