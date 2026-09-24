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
import { ConversationV1DisplayRenderer } from "./conversation-v1-display-renderer";
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
        <ConversationLiveStreamProvider store={new ConversationLiveStreamStore()}>
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

  it.each(["branch", "native"] as const)("attaches exact delivery progress to the owning tools in %s", (surface) => {
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
    const markup = surface === "branch" ? renderWorkspace(events) : renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN"><ConversationV1DisplayRenderer events={events} defaultExpandedTools /></I18nProvider>,
    );
    const container = document.createElement("div");
    container.innerHTML = markup;
    const cards = [...container.querySelectorAll(".tool-presentation-card:not(.conversation-v1-collaboration-card)")];
    expect(cards).toHaveLength(2);
    expect(cards.find((card) => card.querySelector("[data-collaboration-attached-status]"))?.textContent).toContain("已投影");
    expect(cards.filter((card) => card.querySelector("[data-collaboration-attached-status]"))).toHaveLength(1);
    expect(container.querySelector('[data-collaboration-event-kind="delivery_projected"]')).toBeNull();
    expect(markup).not.toContain("协作投递");
  });

  it.each(["branch", "native"] as const)("attaches child completion by execution identity in %s", (surface) => {
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
    const markup = surface === "branch" ? renderWorkspace(events) : renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN"><ConversationV1DisplayRenderer events={events} defaultExpandedTools /></I18nProvider>,
    );
    const container = document.createElement("div");
    container.innerHTML = markup;
    expect(container.querySelectorAll("[data-collaboration-attached-status]"), markup).toHaveLength(1);
    expect(container.querySelector("[data-collaboration-attached-status]")?.textContent).toContain("已完成");
    expect(container.querySelector('[data-collaboration-card-kind]')).toBeNull();
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
        conversations: [{ id: "session-source" }], navigation: null, onSelectConversation, onOpenSubagentPanel,
      })); });
      const sourceButton = container.querySelector<HTMLButtonElement>('[aria-label="打开来源会话"]')!;
      const targetButton = container.querySelector<HTMLButtonElement>('[aria-label="打开目标会话"]')!;
      expect(sourceButton.disabled).toBe(false);
      expect(targetButton.disabled).toBe(true);
      await React.act(async () => { sourceButton.click(); targetButton.click(); });
      expect(onSelectConversation).toHaveBeenCalledExactlyOnceWith("session-source");
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
      expect(host.querySelector(".tool-with-thinking > .conversation-block-content > .tool-execution-stack:first-child > .tool-presentation-card > .tool-presentation-summary")).not.toBeNull();
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
    expect(markup).toContain("读取");
    expect(markup).toContain("tui_test.py · 第 10-24 行");
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
