// @vitest-environment jsdom
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";

// Projection tests inspect expanded details; the card suite covers the actual folded default and clicks.
// 投影测试检查展开详情；卡片测试覆盖真实默认收起状态与点击交互。
vi.mock("./conversation-measured-collapse", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./conversation-measured-collapse")>();
  return {
    ...actual,
    // Seed only collaboration folds, leaving unrelated tool and process defaults untouched.
    // 仅初始化协作折叠，不改变无关工具及过程的默认状态。
    useConversationMeasuredExpansion: (id: string, defaultExpanded: boolean) =>
      actual.useConversationMeasuredExpansion(id,
        id.startsWith("collaboration:") || id.startsWith("subagent-return:") || defaultExpanded),
  };
});

import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { SelectedConversationEventWindowCommands } from "@/service/selected-conversation-event-window-registry";
import {
  clientActionId,
  type ConversationEventSummary,
  type CollaborationTaskResult,
} from "@/service/conversation-event-window-store";
import type { ConversationSummary, ToolRequestProgressSnapshot } from "@/types";
import { ConversationLiveStreamProvider } from "@/state/conversation-live-stream-context";
import { ConversationLiveStreamStore } from "@/state/conversation-live-stream-store";
import type { ConversationSessionResourceCatalogV1 } from "@/state/conversation-session-resources-v1";
import { appendToolRequestDraftMessages, createConversationBranchPresentationProjectorV1, projectConversationBranchPresentationV1 } from "./conversation-branch-presentation-v1";
import { ConversationBranchWorkspaceV1 } from "./conversation-branch-workspace-v1";
import type { ThinkingDisplayMode } from "./thinking-display";
import { projectConversationTimelineItems } from "./conversation-timeline";
import { resolveToolPresentation } from "./conversation-tool-presentations";
import type { ConversationV1CollaborationNavigation } from "./conversation-collaboration-navigation";


// collaborationEvents builds a notification and a child result after the parent's real final answer.
// collaborationEvents 构建父会话真实最终回复之后的通知与子任务结果。
// Returns canonical ordered facts including the hidden User protocol mirror.
// 返回规范有序事实，其中包含隐藏的 User 协议镜像。
function collaborationEvents(): ConversationEventSummary[] {
  // source and target preserve the host-declared identities used for navigation assertions.
  // source 与 target 保留用于导航断言的宿主声明身份。
  const source = { session_id: "session-source", root_session_id: "root-source", project_id: "project-1", segment_generation: 1 };
  const target = { session_id: "session-target", root_session_id: "session-target", project_id: "project-1", segment_generation: 1 };
  return [
    completeTurnEvents()[0],
    eventSummary(2, { kind: "assistant_text", api_call_id: 1, status: "completed", text: "父助手的结束回复" }),
    eventSummary(3, {
      kind: "collaboration", version: 1, operation_id: "notify-op", event_kind: "notification_sent",
      after_state: { kind: "outbound_notification", notification: {
        notification_id: "notice-1", delivery_id: "notice-delivery", operation_id: "notify-op", source, target,
        body: "子代理已经完成代码定位", state: "recorded", created_at_ms: 1003, updated_at_ms: 1003,
      } },
    }, "host_only"),
    eventSummary(4, { kind: "user_input", client_action_id: clientActionId("mirror-action"), input_id: 2, input_source: "agent_message", text: "<vulcan_agent_message>协议镜像不得显示</vulcan_agent_message>" }),
    eventSummary(5, { kind: "subagent_return", child_session_id: "session-source", child_segment_generation: 1, child_root_action_id: "child-execution", child_task_id: "child-task", content: "子任务的结束结果", source_assistant_text_seq: 12, source_from_seq: 1, source_to_seq: 14, child_turn_stats_seq: 14 }),
    ...completeTurnEvents().slice(-1),
  ];
}

// questionEvent builds one source-owned pending question without a human Ask interaction.
// questionEvent 构建由来源持有的待处理问题，不包含人类 Ask 交互。
function questionEvent(): ConversationEventSummary {
  // participant preserves a child session whose durable source belongs to a different root.
  // participant 保留持久化来源属于另一根会话的子会话。
  const source = { session_id: "session-source", root_session_id: "root-source", project_id: "project-1", segment_generation: 1 };
  const target = { session_id: "session-target", root_session_id: "session-target", project_id: "project-1", segment_generation: 1 };
  return eventSummary(9, { kind: "collaboration", version: 1, operation_id: "question-op", event_kind: "question_created", after_state: {
    kind: "question",
    question: { question_id: "question-1", delivery_id: "delivery-1", operation_id: "question-op", source, target, body: "请确认归属", execution_purpose: "in_turn_question", state: "pending", created_at_ms: 1009 },
    delivery: { question_id: "question-1", delivery_id: "delivery-1", operation_id: "question-op", source, target, state: "recorded", updated_at_ms: 1009 },
  } }, "host_only");
}

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
    eventSummary(7, { kind: "turn_boundary", reason: "completed", outcome: "completed", purpose: "business" }),
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
  // Running state exercises the production live reasoning expansion path.
  // 运行状态用于验证生产实时思考展开路径。
  sessionRunning = false,
  // Optional host routes exercise real card buttons through the production surface.
  // 可选宿主路由通过生产界面验证真实卡片按钮。
  collaborationNavigation?: ConversationV1CollaborationNavigation,
  // Inject a stable transport store for live construction tests.
  // 为实时构造测试注入稳定传输存储。
  liveStore = new ConversationLiveStreamStore(),
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
    sessionRunning,
  );
  return (
    <I18nProvider initialLocale="zh-CN">
      <TooltipProvider>
        <ConversationLiveStreamProvider store={liveStore}>
          <ConversationBranchWorkspaceV1
            collaborationNavigation={collaborationNavigation}
            commandSessions={[]}
            commands={commands}
            conversation={conversation}
            initialExpandedState={{
              processCollapsedByKey: Object.fromEntries(timeline.flatMap((item) => item.kind === "process"
                ? [[`${conversation.id}:${item.process.id}`, false]]
                : [])),
              expandedUserPromptIds: [],
              expandedToolPresentationIds,
              // Content tests explicitly expand every tool sequence; lifecycle defaults have their own interaction suite.
              // 内容测试明确展开所有工具分组；生命周期默认值由独立交互测试验证。
              measuredCollapseById: Object.fromEntries(timeline.flatMap((entry) => entry.kind === "process"
                ? entry.process.items.map((item) => [`tool-sequence:${entry.process.id}:${item.renderKey ?? item.id}`, { expanded: true }])
                : [])),
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
            sessionRunning={sessionRunning}
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
  // The real branch surface must show one card from API start through terminal replay, without leaking summary prose.
  // 真实分支界面必须从 API 开始到终态重放都显示一张卡，不能把摘要正文泄漏为普通回复。
  it.each(["completed", "failed", "interrupted"] as const)("renders %s compaction as one lifecycle card", (status) => {
    // Events preserve exact API identity across reasoning, summary and terminal facts.
    // 事件在思考、摘要与终态事实之间保留精确 API 身份。
    const events = [completeTurnEvents()[0], eventSummary(2, {
      kind: "api_started", api_call_id: 9, purpose: "compaction", request: { provider: "test", model: "test" },
    })];
    expect(renderWorkspace(events, "inline_output", [], true)).toContain("上下文正在压缩");
    events.push(eventSummary(3, { kind: "assistant_reasoning", api_call_id: 9, text: "压缩专用思考", status: "completed" }));
    events.push(eventSummary(4, { kind: "assistant_text", api_call_id: 9, text: "压缩摘要正文", status: "completed" }));
    expect(renderWorkspace(events, "inline_output", [], true)).toContain("压缩摘要正文");
    events.push(eventSummary(5, { kind: "api_stats", api_call_id: 9, purpose: "compaction",
      provider: "test", model: "test", usage: { input_tokens: 250_000, output_tokens: 1_000 }, status,
      continuation_eligibility: "forbidden", timing: { completed_at_ms: 1_005 } }));
    events.push(eventSummary(6, { kind: "assistant_text", api_call_id: 10, text: "压缩之后的正常回答", status: "completed" }));
    // Markup goes through the production transcript grouping and collapsed card component.
    // markup 经过生产转录分组与默认折叠卡组件。
    const markup = renderWorkspace(events);
    expect(markup.match(/class="context-compaction-block /g)).toHaveLength(1);
    expect(markup).toContain(status === "completed" ? "上下文已压缩" : "上下文压缩失败");
    expect(markup).not.toContain("压缩摘要正文");
    expect(markup).toContain("压缩之后的正常回答");
  });
  it("binds drafts to their exact API and removes them on durable handoff or failure", () => {
    // A draft cannot invent an API or use another API's Turn.
    // 草稿不能伪造 API 或使用其他 API 的回合。
    const draft: ToolRequestProgressSnapshot = { apiCallId: 7, requestStreamId: "handoff",
      toolCallId: "provider-call", toolName: "file_edit", state: "receiving",
      receivedBytes: 0, receivedCharacters: 0, requestRevision: 1 };
    const started = eventSummary(2, { kind: "api_started", api_call_id: 7,
      purpose: "main_generation", request: { provider: "test", model: "test" } });
    const messages = appendToolRequestDraftMessages("session", [], [started], [draft]);
    expect(messages).toHaveLength(1);
    expect(messages[0]?.metadata).toMatchObject({ v1_turn_id: started.turn_id, api_call_id: 7 });
    expect(appendToolRequestDraftMessages("session", [], [], [draft])).toEqual([]);
    expect(appendToolRequestDraftMessages("session", [], [started], [{ ...draft, apiCallId: 8 }])).toEqual([]);
    const tool = eventSummary(3, { kind: "tool_call", api_call_id: 7, tool_call_id: 1,
      request_stream_id: "handoff", tool_name: "file_edit", status: "running", input: {} });
    expect(appendToolRequestDraftMessages("session", [], [started, tool], [draft])).toEqual([]);
    const terminal = completeTurnEvents().find((entry) => entry.event.kind === "api_stats")!;
    if (terminal.event.kind !== "api_stats") throw new Error("expected API terminal fixture");
    const failed = { ...terminal, event: { ...terminal.event, api_call_id: 7, status: "failed" as const } };
    expect(appendToolRequestDraftMessages("session", [], [started, failed], [draft])).toEqual([]);
  });
  it("updates six same-name tool rows in place through durable handoff and completion", async () => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    // Keep the real API owner and six distinct request streams while provider call IDs differ from durable IDs.
    // 保留真实 API 所有者和六个不同请求流，供应商调用标识与持久调用标识不同。
    const rootInput = completeTurnEvents().find((entry) => entry.event.kind === "user_input")!;
    // The unfinished API owns both construction drafts and the eventual tool records.
    // 未结束 API 同时拥有构造草稿和最终工具记录。
    const events = [rootInput, eventSummary(rootInput.seq + 1, {
      kind: "api_started", api_call_id: 7, purpose: "main_generation",
      request: { provider: "test", model: "test" },
    })];
    // Retain stale draft membership during handoff to exercise the projector's exact identity reconciliation.
    // 交接时保留旧草稿成员，以覆盖投影器按精确标识协调的行为。
    const store = new ConversationLiveStreamStore();
    // Same-name calls must never be deduplicated by tool name.
    // 同名调用绝不能按工具名称去重。
    const drafts: ToolRequestProgressSnapshot[] = Array.from({ length: 6 }, (_, index) => ({
      apiCallId: 7, requestStreamId: `stream-file-${index}`, toolCallId: `provider-call-${index}`,
      toolName: "file", state: "receiving", receivedBytes: 0, receivedCharacters: 0, requestRevision: 1,
    }));
    for (const draft of drafts) store.applyEnvelope({ generation: 1, delta: {
      kind: "toolRequestProgressChanged", payload: {
        conversationId: "session-branch-v1", requestStreamId: draft.requestStreamId, progress: draft,
      },
    } });
    store.flushNotifications();
    // Render the production branch and retain its original DOM rows across every state transition.
    // 渲染生产分支，并在各状态转换之间保留原始 DOM 行。
    const host = document.createElement("div");
    const root = createRoot(host);
    try {
      await React.act(async () => root.render(workspaceTree(events, "inline_output", [], true, undefined, store)));
      // Stable stack elements prove in-place updates rather than duplicate insertion followed by removal.
      // 稳定的栈元素证明原地更新，而非插入重复节点后再删除。
      const rows = [...host.querySelectorAll(".tool-execution-stack")];
      expect(rows).toHaveLength(drafts.length);
      for (const status of ["running", "completed"] as const) {
        // Durable preparation carries the original request stream, not the provider-visible call ID.
        // 持久准备携带原始请求流，而非供应商可见调用标识。
        const tools = drafts.map((draft, index) => eventSummary(rootInput.seq + 2 + index, {
          kind: "tool_call", api_call_id: draft.apiCallId, tool_call_id: 100 + index,
          request_stream_id: draft.requestStreamId, tool_name: draft.toolName, status,
          input: { action: "read", path: `src/file-${index}.ts` },
        }));
        await React.act(async () => root.render(workspaceTree([...events, ...tools], "inline_output", [], true, undefined, store)));
        expect(host.querySelectorAll(".tool-execution-stack")).toHaveLength(drafts.length);
        [...host.querySelectorAll(".tool-execution-stack")].forEach((row, index) => {
          expect(row).toBe(rows[index]);
          expect(row.textContent).toContain(`file-${index}.ts`);
        });
      }
      await React.act(async () => {
        drafts.forEach((draft) => store.settleToolRequestProgress("session-branch-v1", draft.requestStreamId));
        store.flushNotifications();
      });
      expect([...host.querySelectorAll(".tool-execution-stack")]).toEqual(rows);
    } finally {
      await React.act(async () => root.unmount());
      vi.unstubAllGlobals();
    }
  });

  it.each(["apply_patch", "file_edit", "file_write", "file"])("shows %s on name discovery and streams counts before any durable ToolCall", async (toolName) => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    // Canonical API ownership exists, but no complete arguments or tool event exists yet.
    // 规范 API 所有权已存在，但完整参数和工具事件尚不存在。
    const rootInput = completeTurnEvents().find((entry) => entry.event.kind === "user_input")!;
    const events = [rootInput, eventSummary(rootInput.seq + 1, {
      kind: "api_started", api_call_id: 7, purpose: "main_generation",
      request: { provider: "test", model: "test" },
    })];
    const store = new ConversationLiveStreamStore();
    const host = document.createElement("div");
    const root = createRoot(host);
    // Emit only the safe parser contract; partial raw JSON never enters the browser.
    // 仅发送安全解析器契约；不完整原始 JSON 绝不进入浏览器。
    const publish = async (progress: ToolRequestProgressSnapshot | null) => {
      await React.act(async () => {
        store.applyEnvelope({ generation: progress?.requestRevision ?? 99,
          delta: { kind: "toolRequestProgressChanged", payload: {
            conversationId: "session-branch-v1", requestStreamId: "draft-patch", progress,
          } } });
        store.flushNotifications();
      });
    };
    const seed: ToolRequestProgressSnapshot = { apiCallId: 7, requestStreamId: "draft-patch",
      toolCallId: "provider-call", toolName, state: "receiving",
      receivedBytes: 0, receivedCharacters: 0, requestRevision: 1 };
    try {
      await React.act(async () => root.render(workspaceTree(events, "inline_output", [], true, undefined, store)));
      await publish(seed);
      expect(host.querySelector(".tool-presentation-summary")).not.toBeNull();
      expect(host.textContent).toContain(toolName === "file_write" ? "写入" : toolName === "file_edit" ? "编辑" : toolName === "apply_patch" ? "应用补丁" : toolName);
      if (toolName === "file_write") expect(host.textContent).toContain("正在接收内容");
      await publish({ ...seed, requestRevision: 2, receivedBytes: 100, receivedCharacters: 100,
        preview: (toolName === "file" || toolName === "file_write") ? { kind: "file_write", revision: 1,
          title_fields: { path: "src/example.ts" }, progress_fields: { new_line_count: 1 } }
          : { kind: toolName, revision: 1, affected_resources: ["src/example.ts"],
          body_preview: "*** Begin Patch\n*** Update File: src/example.ts\n-old\n+new",
          structured_lines: [
            { operation_index: 0, resource_index: 0, side: "old", line_index: 0, content: "old" },
            { operation_index: 0, resource_index: 0, side: "new", line_index: 0, content: "new" },
          ] } });
      expect(host.textContent).toContain("example.ts");
      expect(host.textContent).toContain((toolName === "file" || toolName === "file_write") ? "已生成 1 行" : "+1");
      if ((toolName !== "file" && toolName !== "file_write")) expect(host.textContent).toContain("-1");
      // Byte-only events must not erase semantic headers between independently delivered previews.
      // 独立预览之间的纯字节进度事件不能清空语义卡头。
      const summary = host.querySelector(".tool-presentation-summary");
      // Preserve the exact recognized header, including its path, operation and current counts.
      // 保留完整已识别卡头，包括路径、操作及当前行数。
      const firstHeader = summary?.textContent;
      await publish({ ...seed, requestRevision: 3, receivedBytes: 105, receivedCharacters: 105 });
      expect(host.querySelector(".tool-presentation-summary")).toBe(summary);
      expect(summary?.textContent).toBe(firstHeader);
      await publish({ ...seed, requestRevision: 4, receivedBytes: 110, receivedCharacters: 110,
        preview: (toolName === "file" || toolName === "file_write") ? { kind: "file_write", revision: 2,
          title_fields: { path: "src/example.ts" }, progress_fields: { new_line_count: 2 } }
          : { kind: toolName, revision: 2, affected_resources: ["src/example.ts"],
          body_preview: "*** Begin Patch\n*** Update File: src/example.ts\n-old\n+new\n+next",
          structured_lines: [
            { operation_index: 0, resource_index: 0, side: "old", line_index: 0, content: "old" },
            { operation_index: 0, resource_index: 0, side: "new", line_index: 0, content: "new" },
            { operation_index: 0, resource_index: 0, side: "new", line_index: 1, content: "next" },
          ] } });
      expect(host.textContent).toContain((toolName === "file" || toolName === "file_write") ? "已生成 2 行" : "+2");
      if ((toolName !== "file" && toolName !== "file_write")) expect(host.textContent).toContain("-1");
      expect(host.querySelectorAll(".tool-presentation-summary")).toHaveLength(1);
      expect(host.querySelector(".tool-presentation-state")).toBeNull();
      expect(host.textContent).not.toContain("110B");
      // Assembly and readiness carry no preview; only the durable tool handoff ends this draft.
      // 组装完成与就绪事件不携带预览；仅持久工具交接结束当前草稿。
      const latestHeader = summary?.textContent;
      for (const [index, state] of (["receiving", "arguments_assembled", "ready"] as const).entries()) {
        await publish({ ...seed, state, requestRevision: 5 + index,
          receivedBytes: 120 + index, receivedCharacters: 120 + index, preview: null });
        expect(host.querySelector(".tool-presentation-summary")).toBe(summary);
        expect(summary?.textContent).toBe(latestHeader);
      }
      await publish(null);
      expect(host.querySelector(".tool-presentation-summary")).toBeNull();
    } finally {
      await React.act(async () => root.unmount());
    }
  });
  // Verifies unordered write arguments update one card before and after the path arrives; returns nothing.
  // 验证无序写入参数在路径到达前后更新同一卡片；无返回值。
  it("streams write counts before a trailing path and enriches the same card", async () => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    // The API exists while the write arguments are still incomplete.
    // API 已存在，但写入参数尚未完整。
    const rootInput = completeTurnEvents().find((entry) => entry.event.kind === "user_input")!;
    const events = [rootInput, eventSummary(rootInput.seq + 1, {
      kind: "api_started", api_call_id: 7, purpose: "main_generation",
      request: { provider: "test", model: "test" },
    })];
    // The production store and workspace exercise the actual draft-to-card projection.
    // 生产状态仓库和工作区覆盖实际草稿到卡片投影。
    const store = new ConversationLiveStreamStore();
    const host = document.createElement("div");
    const root = createRoot(host);
    // Publish a bounded semantic snapshot with an optional completed path; returns after rendering.
    // 发布包含可选完整路径的有界语义快照；渲染后返回。
    const publish = async (revision: number, lines: number, path?: string) => {
      await React.act(async () => {
        store.applyEnvelope({ generation: revision, delta: {
          kind: "toolRequestProgressChanged", payload: {
            conversationId: "session-branch-v1", requestStreamId: "late-path",
            progress: { apiCallId: 7, requestStreamId: "late-path", toolCallId: "provider-call",
              toolName: "file_write", state: "receiving", requestRevision: revision,
              receivedBytes: revision * 100, receivedCharacters: revision * 100,
              preview: { kind: "file_write", revision, title_fields: path ? { path } : {},
                progress_fields: { new_line_count: lines } } },
          },
        } });
        store.flushNotifications();
      });
    };
    try {
      await React.act(async () => root.render(workspaceTree(events, "inline_output", [], true, undefined, store)));
      await publish(1, 1);
      // Retain both the request row and the card element while the path is unknown.
      // 在路径未知时保留请求行和卡片元素。
      const row = host.querySelector(".tool-execution-stack");
      const card = host.querySelector(".tool-presentation-card.file-change");
      expect(card?.textContent).toContain("写入");
      expect(card?.textContent).toContain("已生成 1 行");
      expect(card?.textContent).not.toContain("file");
      await publish(2, 51);
      expect(host.querySelector(".tool-presentation-card.file-change")).toBe(card);
      expect(card?.textContent).toContain("已生成 51 行");
      await publish(3, 803, "index.html");
      expect(host.querySelector(".tool-execution-stack")).toBe(row);
      expect(host.querySelector(".tool-presentation-card.file-change")).toBe(card);
      expect(card?.textContent).toContain("index.html");
      expect(card?.textContent).toContain("已生成 803 行");
      expect(card?.textContent).not.toContain("+803");
      expect(host.querySelectorAll(".tool-presentation-summary")).toHaveLength(1);
      expect(host.querySelector(".tool-presentation-state")).toBeNull();
    } finally {
      await React.act(async () => root.unmount());
    }
  });

  // Parameters: current selects the changed setting; returns no value after checking restored history.
  // 参数：current 选择变化的设置；验证恢复后的历史后无返回值。
  it.each([
    { model: "model-b", thinking_level: "max" },
    { model: "model-a", thinking_level: "high" },
  ])("keeps setting changes outside work groups after a completed turn: %j", (current) => {
    // events retains a real completed turn so the regression exercises the terminal-to-notice boundary.
    // events 保留真实已完成回合，使回归覆盖终态到提示的边界。
    const events = completeTurnEvents();
    // nextSeq derives the following identity from the fixture instead of binding to an evolving position.
    // nextSeq 从夹具派生后续身份，不绑定会演进的位置。
    const nextSeq = Math.max(...events.map((event) => event.seq)) + 1;
    // changes repeat the same category so no later notice can restart a phantom work group.
    // changes 重复同类变化，确保后续提示也不会重新创建虚假的工作分组。
    const changes: ConversationEventSummary[] = [nextSeq, nextSeq + 1].map((seq) => ({
      ...eventSummary(seq, {
        kind: "request_config_changed",
        previous: { provider: "provider-a", model: "model-a", thinking_level: "max" },
        current: { provider: "provider-a", ...current },
      }, "ui_only"),
      turn_id: null,
    }));
    // baseline and rendered isolate the extra notices while preserving genuine completed work.
    // baseline 与 rendered 隔离新增提示，同时保留真实已完成工作。
    const baseline = document.createElement("div");
    baseline.innerHTML = renderWorkspace(events);
    const rendered = document.createElement("div");
    rendered.innerHTML = renderWorkspace([...events, ...changes]);
    expect(rendered.querySelectorAll(".work-process-group").length).toBe(baseline.querySelectorAll(".work-process-group").length);
    expect(rendered.querySelectorAll('[data-event-kind="request_config_changed"]')).toHaveLength(changes.length);
    for (const notice of rendered.querySelectorAll('[data-event-kind="request_config_changed"]')) {
      expect(notice.closest(".work-process-group")).toBeNull();
      expect(notice.querySelector("button, [aria-expanded]")).toBeNull();
      expect(notice.textContent).toContain(current.model === "model-b" ? "model-b" : "high");
    }
  });

  it("attaches exact delivery progress to the owning tools", () => {
    // Two independent questions detect accidental association by adjacent event or display name.
    // 两个独立问题用于发现按相邻事件或展示名称错误关联的情况。
    const created = questionEvent();
    if (created.event.kind !== "collaboration" || created.event.after_state.kind !== "question") throw new Error("Invalid question fixture");
    const tool = (seq: number, questionId: string): ConversationEventSummary => eventSummary(seq, {
      kind: "tool_call", api_call_id: 1, tool_call_id: seq, tool_name: "ask_question",
      input: { target: "same-agent", question: questionId }, request_stream_id: `ask-${seq}`, status: "completed",
      output: { model_output: { question_id: questionId, status: "accepted" } },
    });
    const events = [tool(7, "question-1"), tool(8, "question-2"), created,
      eventSummary(10, { kind: "collaboration", version: 1, operation_id: "delivery-progress", event_kind: "delivery_projected",
        after_state: { kind: "outbound_delivery", delivery: { ...created.event.after_state.delivery, state: "projected" } } }, "host_only")];
    const markup = renderWorkspace(events);
    const container = document.createElement("div");
    container.innerHTML = markup;
    const cards = [...container.querySelectorAll(".tool-presentation-card:not(.conversation-v1-collaboration-card)")];
    expect(cards).toHaveLength(2);
    expect(cards.find((card) => card.querySelector("[data-collaboration-attached-status]"))?.textContent).toContain("已投影");
    expect(cards.filter((card) => card.querySelector("[data-collaboration-attached-status]"))).toHaveLength(1);
    expect(container.querySelector('[data-collaboration-event-kind="delivery_projected"]')).toBeNull();
    expect(markup).not.toContain("协作投递");
  });

  it("attaches child completion by execution identity", () => {
    // Reused child names must not let an older execution update the new creation card.
    // 复用子代理名称不得让旧执行更新新创建卡片。
    const create = (seq: number, executionId: string): ConversationEventSummary => eventSummary(seq, {
      kind: "tool_call", api_call_id: 1, tool_call_id: seq, tool_name: "spawn_agent",
      input: { task_id: "same-agent", prompt: "work" }, request_stream_id: `create-${seq}`, status: "completed",
      output: { model_output: "created", host_result: { kind: "subagent_task", payload: {
        task_id: "same-agent", execution_id: executionId, display_title: "same-agent", prompt: "work", execution_status: "running",
      } } },
    });
    const events = [create(1, "old-execution"), create(2, "new-execution"), eventSummary(3, {
      kind: "collaboration", version: 1, operation_id: "terminal-old", event_kind: "child_task_terminal",
      after_state: { kind: "child_task", task: { reservation_operation_id: "reserve-old", source_tool_name: "spawn_agent",
        execution_id: "old-execution", task_id: "same-agent", parent_segment_generation: 1, parent_turn_id: 11,
        child: { session_id: "same-child", root_session_id: "session-branch-v1", project_id: "project-1", segment_generation: 1 },
        state: "terminal", reserved_at_ms: 1000, terminal_outcome: "completed", terminal_at_ms: 1010,
        non_success_result: null, non_success_projection: null } },
    }, "host_only")];
    const markup = renderWorkspace(events);
    const container = document.createElement("div");
    container.innerHTML = markup;
    expect(container.querySelectorAll("[data-collaboration-attached-status]"), markup).toHaveLength(1);
    expect(container.querySelector("[data-collaboration-attached-status]")?.textContent).toContain("已完成");
    expect(container.querySelector('[data-collaboration-card-kind]')).toBeNull();
  });

  // Keep the named outbound question card and hide only its successfully correlated tool receipt.
  // 保留带名称的发出提问卡，仅隐藏与其成功关联的工具回执。
  it.each(["ask_question", "ask_parent"])("replaces the successful %s receipt with its question card", (toolName) => {
    const tool = eventSummary(8, { kind: "tool_call", api_call_id: 1, tool_call_id: 3, request_stream_id: "ask-stream",
      tool_name: toolName, status: "completed", input: { target_session_id: "session-target", question: "请确认归属" },
      output: { model_output: "Question recorded", host_result: { kind: "collaboration_receipt_v1", payload: { format: "markdown", question_id: "question-1" } } },
    });
    const question = questionEvent();
    const container = document.createElement("div");
    container.innerHTML = renderWorkspace([tool, question]);
    expect(container.querySelectorAll('[data-collaboration-event-kind="question_created"]')).toHaveLength(1);
    expect(container.querySelectorAll(".tool-execution-stack")).toHaveLength(0);
    expect(container.textContent).toContain("请确认归属");
    // Reuse cached event objects as the question arrives and leaves a paged window.
    // 在问题进入及离开分页窗口时复用已缓存事件对象。
    const project = createConversationBranchPresentationProjectorV1();
    const resources = emptyResourceCatalog();
    const t: Parameters<typeof project>[3] = (key) => key;
    expect(project("session-branch-v1", [tool], resources, t).messages.some((message) => message.recordKind === "tool-call-output")).toBe(true);
    expect(project("session-branch-v1", [tool, question], resources, t).messages.some((message) => message.recordKind === "tool-call-output")).toBe(false);
    expect(project("session-branch-v1", [tool], resources, t).messages.some((message) => message.recordKind === "tool-call-output")).toBe(true);
  });

  // Failed/active tools and unrelated receipts retain their independent evidence even beside a question.
  // 失败或活动工具以及无关回执保留独立证据，即使旁边存在提问卡。
  it.each([
    { status: "failed", toolName: "ask_question", questionId: "question-1" },
    { status: "running", toolName: "ask_question", questionId: "question-1" },
    { status: "cancelled", toolName: "ask_question", questionId: "question-1" },
    { status: "completed", toolName: "ask_question", questionId: "different-question" },
    { status: "completed", toolName: "reply_question", questionId: "question-1" },
  ] as const)("preserves a tool without a replaceable successful question receipt: %j", ({ status, toolName, questionId }) => {
    const tool = eventSummary(8, { kind: "tool_call", api_call_id: 1, tool_call_id: 3, request_stream_id: "ask-stream",
      tool_name: toolName, status, input: { target_session_id: "session-target", question: "请确认归属", question_id: questionId, answer: "Reply" },
      output: { model_output: "Visible receipt", host_result: { kind: "collaboration_receipt_v1", payload: { format: "markdown", question_id: questionId } } },
    });
    const result = projectConversationBranchPresentationV1("session-branch-v1", [tool, questionEvent()], emptyResourceCatalog(), (key) => key);
    expect(result.messages.some((message) => message.recordKind === "tool-call-output")).toBe(true);
  });

  // Source questions and answers retain separate bodies; receipts enrich only the exact answer.
  // 来源问题和答案保留独立正文；回执仅补充精确对应的答案。
  it("separates question and answer content and merges answer delivery receipts", () => {
    // Build real typed collaboration snapshots instead of parsing display labels.
    // 构造真实强类型协作快照，不解析展示标签。
    const question = questionEvent();
    if (question.event.kind !== "collaboration" || question.event.after_state.kind !== "question") throw new Error("Expected a question fixture");
    const answer = eventSummary(question.seq + 1, { ...question.event, operation_id: "reply", event_kind: "question_answered",
      after_state: { ...question.event.after_state, question: { ...question.event.after_state.question, state: "answered",
        answer: { operation_id: "reply", responder: question.event.after_state.question.target, body: "独立回答正文", settled_at_ms: 1020,
          delivery: { state: "undelivered", delivery_revision: 1, last_operation_id: "reply", updated_at_ms: 1020 } },
      } },
    }, "host_only");
    const receipt = eventSummary(answer.seq + 1, { kind: "collaboration", version: 1, operation_id: "delivered", event_kind: "answer_delivered",
      after_state: { kind: "answer_delivery", question_id: "question-1", delivery: {
        state: "delivered", delivery_revision: 2, last_operation_id: "delivered", updated_at_ms: 1030,
      } },
    }, "host_only");
    const container = document.createElement("div");
    container.innerHTML = renderWorkspace([question, answer, receipt]);
    const questionCard = container.querySelector('[data-collaboration-event-kind="question_created"]');
    const answerCard = container.querySelector('[data-collaboration-event-kind="question_answered"]');
    expect(questionCard?.textContent).toContain("请确认归属");
    expect(questionCard?.textContent).not.toContain("独立回答正文");
    expect(questionCard?.querySelector(".user-bubble")).toBeNull();
    expect(answerCard?.querySelector(".user-bubble")?.textContent).toContain("独立回答正文");
    expect(answerCard?.textContent).not.toContain("请确认归属");
    expect(answerCard?.textContent).toContain("已交付");
    expect(container.querySelector('[data-collaboration-event-kind="answer_delivered"]')).toBeNull();
    // A receipt-only page must not invent an empty answer card.
    // 仅回执分页不能伪造空答案卡片。
    container.innerHTML = renderWorkspace([receipt]);
    expect(container.querySelector("[data-collaboration-card-kind]")).toBeNull();
  });

  // Host question inputs are displayed once while their ordinary protocol mirrors stay hidden.
  // 宿主问题输入只展示一次，其余普通协议镜像仍保持隐藏。
  it("projects a received host question as a collaboration bubble, never a human input", () => {
    const incoming = eventSummary(2, { kind: "user_input", input_source: "agent_message", input_id: 2,
      client_action_id: clientActionId("agent-message:question-delivery"),
      text: '<vulcan_agent_message version="1" event_id="question-delivery" kind="question" source="session-source" target="session-branch-v1" question_id="question-1" scope="question">收到的提问正文</vulcan_agent_message>',
    });
    const container = document.createElement("div");
    // The receiver publishes execution before the question input; both must occupy the same bubble.
    // 接收方先发布执行再发布问题输入，两者必须合并到同一气泡。
    const started = eventSummary(1, { kind: "collaboration", version: 1, operation_id: "question-start", event_kind: "question_execution_started",
      after_state: { kind: "question_execution", execution: { execution_id: "question-execution", operation_id: "question-start",
        question_id: "question-1", participant: { session_id: "session-branch-v1", root_session_id: "session-branch-v1", project_id: "project-1", segment_generation: 1 },
        purpose: "in_turn_question", state: "running", started_at_ms: 1001 } },
    }, "host_only");
    container.innerHTML = renderWorkspace([started, incoming]);
    expect(container.querySelectorAll('[data-collaboration-card-kind="question"]')).toHaveLength(1);
    expect(container.querySelectorAll("[data-collaboration-card-kind]")).toHaveLength(1);
    expect(container.querySelector(".conversation-v1-collaboration-badge")?.textContent).toContain("运行中");
    expect(container.querySelector('[data-collaboration-event-kind="question_execution_started"]')).toBeNull();
    expect(container.querySelector(".user-bubble")?.textContent).toContain("收到的提问正文");
    expect(container.querySelector(".user-message-copy")).toBeNull();
    expect(container.textContent).not.toContain("vulcan_agent_message");
    // Execution-only pagination does not invent a question body or an empty execution card.
    // 仅执行记录分页不伪造问题正文或生成空执行卡。
    container.innerHTML = renderWorkspace([started]);
    expect(container.querySelector("[data-collaboration-card-kind]")).toBeNull();
  });

  it("updates a task message in place without merging identical content", () => {
    // Delivery identity distinguishes identical messages; receipts must not move their first anchor.
    // 投递身份区分相同正文的消息；回执不得移动首个锚点。
    const message = { operation_id: "send-one", delivery_id: "delivery-one", kind: "task_steer" as const,
      source: { session_id: "source", root_session_id: "source", project_id: "p", segment_generation: 1 },
      target: { session_id: "target", root_session_id: "target", project_id: "p", segment_generation: 1 }, body: "相同消息正文" };
    const sent = (seq: number, deliveryId: string): ConversationEventSummary => eventSummary(seq, {
      kind: "collaboration", version: 1, operation_id: `send-${deliveryId}`, event_kind: "task_message_sent",
      after_state: { kind: "outbound_task_message", record: { message: { ...message, delivery_id: deliveryId }, created_at_ms: 1001, acceptance: null } },
    }, "host_only");
    const events = [sent(1, "delivery-one"), sent(2, "delivery-two"), eventSummary(3, {
      kind: "collaboration", version: 1, operation_id: "ack-one", event_kind: "task_message_acknowledged",
      after_state: { kind: "outbound_task_message", record: { message, created_at_ms: 1001,
        acceptance: { message, operation_id: "accept-one", target_revision: 1, event_seq: 3, accepted_at_ms: 1003 } } },
    }, "host_only")];
    const markup = renderWorkspace(events);
    const container = document.createElement("div");
    container.innerHTML = markup;
    const cards = container.querySelectorAll('[data-collaboration-card-kind="task-message"]');
    expect(cards).toHaveLength(2);
    expect(container.querySelector('[data-seq="1"] .conversation-v1-collaboration-badge')?.textContent).toBe("已接纳");
    expect(container.querySelector('[data-seq="2"] .conversation-v1-collaboration-badge')?.textContent).toBe("已记录");
    expect(container.querySelector('[data-seq="3"]')).toBeNull();
    expect(container.querySelectorAll("details, .conversation-v1-collaboration-details")).toHaveLength(0);
    expect([...cards].every((card) => card.textContent?.includes("相同消息正文"))).toBe(true);
  });

  it("attaches result receipts to their exact card", () => {
    // Distinct deliveries may have identical text; only the first receives progress receipts.
    // 不同投递可以正文相同；此处只有首条收到进度回执。
    const result: CollaborationTaskResult = {
      acceptance: { message: { operation_id: "send", delivery_id: "result-one", kind: "task_start", body: "任务",
        source: { session_id: "source", root_session_id: "source", project_id: "p", segment_generation: 1 },
        target: { session_id: "target", root_session_id: "target", project_id: "p", segment_generation: 1 } },
        operation_id: "accept", target_revision: 2, event_seq: 2, accepted_at_ms: 2 },
      effect: { delivery_id: "result-one", operation_id: "claim", target_revision: 3, event_seq: 3, occurred_at_ms: 3,
        outcome: { kind: "claimed", execution_id: "execution", turn_id: 2, input_id: 2, input_seq: 4 } },
      terminal: { turn_id: 2, turn_start_seq: 3, turn_stats_seq: 9, outcome: "completed", completed_at_ms: 9,
        final_assistant: { api_call_id: 2, api_stats_seq: 7, source_text: { event_seq: 6, text: "相同结果正文" } } },
      operation_id: "result", target_revision: 6, event_seq: 10, recorded_at_ms: 9,
    };
    // Build independent result anchors with exact producer identities.
    // 使用精确生产者身份构造独立结果锚点。
    const resultEvent = (seq: number, deliveryId: string): ConversationEventSummary => eventSummary(seq, {
      kind: "collaboration", version: 1, operation_id: `result-${deliveryId}`, event_kind: "task_result_recorded",
      after_state: { kind: "task_result", result: { ...result,
        acceptance: { ...result.acceptance, message: { ...result.acceptance.message, delivery_id: deliveryId } },
        effect: { ...result.effect, delivery_id: deliveryId } } },
    }, "host_only");
    // Receipt progression must preserve both the original result and its timeline position.
    // 回执进度必须保留原始结果及时间线位置。
    const acceptance = { result, disclosure: "full" as const, operation_id: "receive", event_seq: 20, receiver_revision: 8, accepted_at_ms: 10 };
    const acknowledged = eventSummary(3, { kind: "collaboration", version: 1, operation_id: "ack", event_kind: "task_result_acknowledged",
      after_state: { kind: "task_result_acknowledgement", acknowledgement: { acceptance, operation_id: "ack", event_seq: 3, target_revision: 9, acknowledged_at_ms: 11 } } }, "host_only");
    const projected = eventSummary(4, { kind: "collaboration", version: 1, operation_id: "project", event_kind: "task_result_projected",
      after_state: { kind: "task_result_projection", projection: { delivery_id: "result-one", acceptance_seq: 20, turn_id: 1,
        disclosure: "full", inputs: [{ input_seq: 31, input_id: 4 }], operation_id: "project", event_seq: 4, receiver_revision: 10, projected_at_ms: 12 } } }, "host_only");
    const events = [resultEvent(1, "result-one"), resultEvent(2, "result-two"), acknowledged, projected,
      { ...acknowledged, seq: 5 }];
    for (const length of [2, 3, 4, 5]) {
      // The production surface must update the owner without emitting receipt-only cards.
      // 生产展示链路应更新所属卡片，不能生成纯回执卡片。
      const history = events.slice(0, length);
      const markup = renderWorkspace(history);
      const container = document.createElement("div");
      container.innerHTML = markup;
      expect(container.querySelectorAll(".conversation-v1-collaboration-card")).toHaveLength(2);
      expect(container.querySelector('[data-seq="1"] .conversation-v1-collaboration-badge')?.textContent)
        .toBe(length === 2 ? "已完成" : length === 3 ? "已完成 · 已送达" : "已完成 · 已加入上下文");
      expect(container.querySelector('[data-seq="2"] .conversation-v1-collaboration-badge')?.textContent).toBe("已完成");
      expect(container.querySelector('[data-seq="1"]')?.textContent).toContain("相同结果正文");
      expect(container.querySelector('[data-seq="2"]')?.textContent).toContain("相同结果正文");
      expect(markup).not.toContain("任务回执已");
      expect(markup).not.toContain("已投影");
    }
    // Result publication follows durable statistics, but its display belongs before those exact statistics.
    // 结果发布晚于可靠统计，但展示应位于精确归属的统计之前。
    const stats = completeTurnEvents().find((summary) => summary.event.kind === "turn_stats")!;
    const resultHistory = [{ ...stats, seq: 1, turn_id: result.terminal!.turn_id }, { ...resultEvent(2, "result-one"), turn_id: null }];
    const presentation = projectConversationBranchPresentationV1("session-branch-v1", resultHistory, emptyResourceCatalog(), (key) => key);
    expect(presentation.messages.map((message) => message.recordKind)).toEqual(["agent-interaction", "turn-stats"]);
    const resultMarkup = renderWorkspace(resultHistory);
    expect(resultMarkup.indexOf("相同结果正文")).toBeGreaterThanOrEqual(0);
    // Moving the display anchor never rewrites the result's durable sequence identity.
    // 移动展示锚点绝不改写结果的可靠序列身份。
    expect(presentation.messages.find((message) => message.agentInteraction)?.agentInteraction?.seq).toBe(2);
  });

  it("updates historical question status after settlement without offering cancellation", () => {
    // created is the old pending snapshot retained as immutable history.
    // created 是作为不可变历史保留的旧待处理快照。
    const created = questionEvent();
    if (created.event.kind !== "collaboration" || created.event.after_state.kind !== "question") throw new Error("Invalid question fixture");
    // settled supplies the later authoritative state without rewriting the old event.
    // settled 提供后续权威状态，不重写旧事件。
    const settled = eventSummary(10, { ...created.event, operation_id: "cancel-op", event_kind: "question_cancelled", after_state: {
      ...created.event.after_state,
      question: { ...created.event.after_state.question, state: "cancelled", cancellation: { operation_id: "cancel-op", requester: created.event.after_state.question.source, reason: "user_cancelled", cancelled_at_ms: 1010 } },
    } }, "host_only");
    const markup = renderWorkspace([created, settled]);
    expect(markup).not.toContain("conversation-v1-collaboration-cancel-button");
    expect(markup).not.toContain("等待处理");
    expect(markup).not.toContain("技术详情");
    expect(markup).toContain("user_cancelled");
  });

  it.each([
    { kind: "claimed", acceptedWhileBusy: false },
    { kind: "claimed", acceptedWhileBusy: true },
    { kind: "steered", acceptedWhileBusy: false },
  ] as const)("groups cross-session $kind work (busy acceptance: $acceptedWhileBusy) by its canonical turn", ({ kind, acceptedWhileBusy }) => {
    // Reuse real reasoning/tool fixtures but bind the received message through its exact effect.
    // 复用真实思考与工具夹具，通过精确效果绑定收到的消息。
    const previous = completeTurnEvents();
    const turnId = kind === "claimed" ? 22 : 11;
    const message = { operation_id: "incoming", delivery_id: "incoming-delivery", kind: kind === "claimed" ? "task_start" as const : "task_steer" as const,
      source: { session_id: "other-session", root_session_id: "other-session", project_id: "p", segment_generation: 1 },
      target: { session_id: "session-branch-v1", root_session_id: "session-branch-v1", project_id: "p", segment_generation: 1 }, body: "检查仓库，不是用户提示词" };
    const incoming = eventSummary(9, { kind: "collaboration", version: 1, operation_id: "accepted", event_kind: "task_message_accepted",
      after_state: { kind: "inbound_task_message", acceptance: { message, operation_id: "accepted", target_revision: 2, event_seq: 9, accepted_at_ms: 1009 } } }, "host_only");
    const effect = { delivery_id: message.delivery_id, operation_id: "effect", target_revision: 3, event_seq: 10, occurred_at_ms: 1010,
      outcome: kind === "claimed" ? { kind, execution_id: "task-execution", turn_id: turnId, input_id: 12, input_seq: 11 }
        : { kind, turn_id: turnId, input_id: 12, input_seq: 11 } };
    const suffix = [incoming, eventSummary(10, { kind: "collaboration", version: 1, operation_id: "effect", event_kind: "task_message_effect",
      after_state: { kind: "task_message_effect", effect } }, "host_only"),
      eventSummary(11, { kind: "user_input", client_action_id: clientActionId("agent-mirror"), input_id: 12, input_source: "agent_message", text: "隐藏协议镜像" }),
      ...previous.filter((summary) => ["assistant_reasoning", "tool_call", "assistant_text", "turn_stats"].includes(summary.event.kind))
        .map((summary, index) => ({ ...summary, seq: index + 12 })),
    ].map((summary) => ({ ...summary, turn_id: summary.seq === 9 ? null : turnId }));
    // A running turn has no terminal statistics yet; an idle turn keeps its completed predecessor.
    // 运行中的回合尚无终态统计；空闲启动保留已经结束的前一回合。
    // Accepting a queued task before prior statistics must not split its predecessor's process.
    // 在前一回合统计前接纳排队任务，不能拆散前一回合的过程。
    const events = (acceptedWhileBusy
      ? [...previous.flatMap((summary) => summary.event.kind === "turn_stats" ? [suffix[0], summary] : [summary]), ...suffix.slice(1)]
      : [...(kind === "claimed" ? previous : previous.filter((summary) => summary.event.kind === "user_input")), ...suffix])
      .map((summary, index) => ({ ...summary, seq: index + 1 }));
    const presentation = projectConversationBranchPresentationV1("session-branch-v1", events, emptyResourceCatalog(), (key) => key);
    const timeline = projectConversationTimelineItems(presentation.renderItems, undefined, false);
    const processes = timeline.filter((entry) => entry.kind === "process");
    expect(processes).toHaveLength(kind === "claimed" ? 2 : 1);
    const process = processes.at(-1)!;
    if (process.kind !== "process") throw new Error("Expected task process");
    // New tasks precede the work divider; steering remains among that same turn's process items.
    // 新任务位于工作分隔线之前；插队仍位于同一回合的过程项中。
    const incomingRow = presentation.renderItems.find((item) => item.sourceMessage?.agentInteraction?.event.kind === "collaboration"
      && item.sourceMessage.agentInteraction.event.after_state.kind === "inbound_task_message")!;
    expect(process.process.items.some((item) => item.id === incomingRow.id)).toBe(kind === "steered");
    if (kind === "claimed") {
      const startIndex = timeline.findIndex((entry) => entry.kind === "item" && entry.item.id === incomingRow.id);
      expect(startIndex).toBeGreaterThanOrEqual(0);
      expect(startIndex).toBeLessThan(timeline.indexOf(process));
    }
    expect(process.process.items.some((item) => item.blocks.some((block) => block.kind === "thinking"))).toBe(false);
    expect(process.process.items.some((item) => item.leadingThinking?.length)).toBe(true);
    expect(presentation.messages.filter((message) => message.role === "user")).toHaveLength(1);
    expect(presentation.messages.some((message) => message.body === "隐藏协议镜像")).toBe(false);
  });
  it("keeps notification and child-result cards distinct from human input and parent completion", () => {
    // presentation and timeline exercise the same adapter and grouping path used by ChatArea.
    // presentation 与 timeline 验证 ChatArea 使用的相同适配器及分组链路。
    const presentation = projectConversationBranchPresentationV1("session-branch-v1", collaborationEvents(), emptyResourceCatalog(), (key) => key);
    const timeline = projectConversationTimelineItems(presentation.renderItems, undefined, false);
    expect(presentation.messages.filter((message) => message.role === "user")).toHaveLength(1);
    expect(presentation.messages.filter((message) => message.agentInteraction)).toHaveLength(2);
    expect(timeline.filter((entry) => entry.kind === "item" && entry.finalAssistantOutput).map((entry) => entry.kind === "item" && entry.item.sourceMessage?.body)).toEqual(["父助手的结束回复"]);
    // markup proves the actual workspace mounts the shared cards without exposing protocol mirrors.
    // markup 证明真实 workspace 挂载共享卡片，不暴露协议镜像。
    const markup = renderWorkspace(collaborationEvents());
    expect(markup).toContain('data-collaboration-card-kind="notification"');
    expect(markup).toContain('data-collaboration-card-kind="task-result"');
    expect(markup).toContain("子代理已经完成代码定位");
    expect(markup).toContain("子任务的结束结果");
    expect(markup).not.toContain("协议镜像不得显示");
    expect(markup).not.toContain("conversation-v1-collaboration-cancel-button");
    expect(markup.indexOf("父助手的结束回复")).toBeLessThan(markup.indexOf('data-seq="3"'));
    expect(markup.indexOf('data-seq="3"')).toBeLessThan(markup.indexOf('data-seq="5"'));
    expect(markup).not.toContain('message-role-label">SYSTEM');
    // Late notifications alone never invent an executing business process after completed history.
    // 完成历史之后的迟到通知不能凭空创建执行中的业务过程。
    const late = projectConversationBranchPresentationV1("session-branch-v1", [questionEvent()], emptyResourceCatalog(), (key) => key);
    expect(projectConversationTimelineItems(late.renderItems, undefined, true).map((entry) => entry.kind)).toEqual(["item"]);
  });

  it("routes known participants without allowing the UI to cancel agent questions", async () => {
    // container mounts production buttons so callbacks cannot pass on static markup alone.
    // container 挂载生产按钮，避免只用静态标记证明回调有效。
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);
    const onSelectConversation = vi.fn();
    const onOpenSubagentPanel = vi.fn(async () => true);
    try {
      await React.act(async () => { root.render(workspaceTree([questionEvent()], "inline_output", [], false, {
        conversations: [{ id: "session-target", title: "接收会话" }], navigation: null, onSelectConversation, onOpenSubagentPanel,
      })); });
      expect(container.querySelector('[aria-label="打开来源会话"]')).toBeNull();
      const targetButton = container.querySelector<HTMLButtonElement>('[aria-label="打开目标会话"]')!;
      expect(targetButton.disabled).toBe(false);
      const fold = container.querySelector<HTMLButtonElement>(".conversation-v1-collaboration-fold")!;
      const expanded = fold.getAttribute("aria-expanded");
      await React.act(async () => { targetButton.click(); });
      expect(fold.getAttribute("aria-expanded")).toBe(expanded);
      await React.act(async () => { fold.click(); });
      expect(fold.getAttribute("aria-expanded")).not.toBe(expanded);
      await React.act(async () => { fold.click(); });
      expect(fold.getAttribute("aria-expanded")).toBe(expanded);
      expect(onSelectConversation).toHaveBeenCalledExactlyOnceWith("session-target");
      expect(onOpenSubagentPanel).not.toHaveBeenCalled();
      expect(container.querySelector(".conversation-v1-collaboration-cancel-button")).toBeNull();
    } finally {
      await React.act(async () => root.unmount());
      container.remove();
    }
  });
  // The same live sequence must hand reasoning to exactly one tool under either popup preference.
  // 同一实时序列在两种弹窗偏好下都必须仅将思考交给一个工具。
  it.each(["popup_only", "inline_output"] as const)("retains full reasoning through live-to-tool handoff in %s", async (mode) => {
    // Long text catches accidental summary-only rendering and must remain selectable in the portal.
    // 长文本用于发现只渲染摘要的错误，且必须在浮层内保持可选择。
    const fullText = "完整思考段落。".repeat(300) + "末尾校验标记";
    const completed = completeTurnEvents().slice(0, 3).map((summary): ConversationEventSummary => summary.event.kind === "assistant_reasoning"
      ? { ...summary, event: { ...summary.event, text: fullText } } : summary);
    const streaming = completed.slice(0, 2).map((summary): ConversationEventSummary => summary.event.kind === "assistant_reasoning"
      ? { ...summary, event: { ...summary.event, status: "streaming" } } : summary);
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    // jsdom has no geometry observer; production scrolling still receives the real lifecycle calls.
    // jsdom 没有几何观察器；生产滚动仍接收真实生命周期调用。
    vi.stubGlobal("ResizeObserver", class {
      // Observation is inert without browser geometry; no value is returned.
      // 无浏览器几何时观察为空操作，不返回值。
      observe() {}
      // Disconnect releases the inert test observer; no value is returned.
      // 断开空测试观察器，不返回值。
      disconnect() {}
      // Unobserve removes an inert target; no value is returned.
      // 移除空观察目标，不返回值。
      unobserve() {}
    });
    try {
      await React.act(async () => root.render(workspaceTree(streaming, mode, [], true)));
      expect(host.querySelector(".thinking-block")).toBeNull();
      expect(host.querySelector(".thinking-gutter-anchor")).not.toBeNull();
      if (mode === "popup_only") {
        expect(host.querySelector(".thinking-preview-indicator")).not.toBeNull();
        expect(host.querySelector(".thinking-body")).toBeNull();
      } else {
        expect(host.querySelector(".thinking-body")).toBeNull();
        expect(document.querySelector(".thinking-preview-surface")).toBeNull();
        // Reading the first segment uses the same gutter preview as later reasoning.
        // 阅读首段思考时使用与后续思考相同的左侧预览。
        await React.act(async () => host.querySelector<HTMLButtonElement>(".thinking-preview-trigger")!.click());
        expect(document.querySelector(".thinking-preview-body")?.textContent).toContain("末尾校验标记");
      }
      await React.act(async () => root.render(workspaceTree(completed, mode, [], true)));
      expect(host.querySelector(".thinking-block")).toBeNull();
      expect(host.querySelectorAll(".tool-with-thinking")).toHaveLength(1);
      // The header selector must target the real card, without wrapping its expanded detail in a prefix column.
      // 标题选择器必须命中真实卡片，不能把展开详情包进思考入口所在分栏。
      expect(host.querySelector(".tool-with-thinking > .conversation-block-content > .tool-execution-stack:first-child > .vk-tool > .vk-tool-head")).not.toBeNull();
      expect(document.querySelector(".thinking-preview-surface")).toBeNull();
      const trigger = host.querySelector<HTMLButtonElement>(".thinking-preview-trigger.compact")!;
      await React.act(async () => trigger.click());
      const surface = document.querySelector<HTMLElement>(".thinking-preview-surface")!;
      expect(surface.textContent).toContain("末尾校验标记");
      expect(host.contains(surface)).toBe(false);
      // A final answer owns the complete preceding reasoning through the same gutter entry.
      // 最终回答通过相同的左侧入口承接完整前置思考。
      const final = [completed[0], completed[1], completeTurnEvents()[4]];
      await React.act(async () => root.render(workspaceTree(final, mode)));
      expect(host.querySelector(".thinking-toggle")).toBeNull();
      expect(host.querySelector(".thinking-gutter-anchor")).toBeNull();
      expect(host.querySelector(".assistant-final-output .assistant-with-thinking")).not.toBeNull();
      await React.act(async () => host.querySelector<HTMLButtonElement>(".assistant-final-output .thinking-preview-trigger")!.click());
      expect(document.querySelector(".thinking-preview-body")?.textContent).toContain("末尾校验标记");
      expect(host.querySelector(".tool-with-thinking")).toBeNull();
      expect(host.textContent).toContain("已经完成核对。");
      // A later tool makes this assistant text intermediate; its prefix owns the full preceding reasoning.
      // 后续工具使这段助手正文成为中间输出；其前置入口拥有完整的上一段思考。
      await React.act(async () => root.render(workspaceTree([
        completed[0], completed[1],
        { ...completeTurnEvents()[4], seq: 3 },
        { ...completed[2], seq: 4 },
        { ...completeTurnEvents()[4], seq: 5 },
      ], mode)));
      expect(host.querySelectorAll(".assistant-with-thinking")).toHaveLength(1);
      expect(host.querySelector(".tool-with-thinking")).toBeNull();
      expect(host.querySelector(".thinking-block")).toBeNull();
      const assistantTrigger = host.querySelector<HTMLButtonElement>(".assistant-with-thinking > .thinking-preview-trigger");
      await React.act(async () => assistantTrigger?.click());
      expect(document.querySelector(".thinking-preview-body")?.textContent).toContain("末尾校验标记");
    } finally {
      await React.act(async () => root.unmount());
      host.remove();
      vi.unstubAllGlobals();
    }
  });
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
      expect(markup).not.toContain("原始详情");
      expect(Array.from(host.querySelectorAll('[role="tab"]'), tab => tab.textContent)).toEqual(["配置详情", "输入", "输出"]);
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
  it("uses gutter entries for final reasoning and tools under every persisted preference", () => {
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
      ["inline_output", 2],
      ["popup_only", 2],
      ["first_inline_with_popup", 2],
      ["first_inline_only", 2],
      ["inline_output", 2],
    ];
    for (const [mode, expectedCount] of cases) {
      // markup uses the production branch shell, including its exact row filtering.
      // markup 使用生产分支外壳，包含其精确行过滤逻辑。
      const markup = renderWorkspace(history, mode);
      expect(markup).not.toContain("thinking-block");
      expect(markup.match(/assistant-flow assistant-with-thinking/g)?.length ?? 0, mode).toBe(expectedCount);
      expect(markup.match(/thinking-preview-trigger compact/g)?.length ?? 0, mode).toBe(4);
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
    expect(markup).toContain("已工作");
    expect(markup).toContain("rg current-call-chain");
    expect(markup).toContain("vk-tool-head");
    expect(markup).toContain("turn-stats-row");
    expect(markup).not.toContain(">SYSTEM<");
    expect(markup).toContain('aria-label="分叉会话"');
    expect(markup).toContain('aria-label="复制最终输出原文"');
  });

  // Durable tool envelopes must recover branch-native summaries and bodies without exposing wrapper JSON.
  // 可靠工具封装必须恢复分支原生摘要与正文，且不得暴露封装 JSON。
  it("adapts prompt-skill Markdown and file-read ranges from exact V1 tool envelopes", () => {
    // filePayload mirrors the coordinate-only vulcan.file_read.v3 host metadata persisted by the runtime.
    // filePayload 镜像运行时持久化的仅含定位信息的 vulcan.file_read.v3 宿主元数据。
    const filePayload = {
      action: "read",
      path: "D:/openvulcan/tui_test.py",
      range: { line_count: 2, source_start_line: 10 },
      read_status: "content",
      render: { output_start_line: 6 },
      schema: "vulcan.file_read.v3",
      total_line_count: 40,
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
          model_output: "# File: tui_test.py\n\nLines: 40 | Showing: 10-11 | Encoding: utf-8 (detected)\n\n```python\nL10: line ten\nL11: line eleven\n```\n",
          tool_call_id: "branch-file-1",
          tool_name: "file",
        },
        status: "completed",
      }),
    ]);

    expect(markup).toContain("<code>terminal-session</code>");
    // The read card names the exact returned source range and stays collapsed until opened.
    // 读取卡标出实际返回的源码范围，并在打开前保持折叠。
    expect(markup).toContain("D:/openvulcan/tui_test.py<small>L10–11</small>");
    expect(markup).toContain('data-family="read"');
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
