// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type {
  CollaborationEventKind,
  ConversationEventBody,
} from "@/service/conversation-event-window-store";
import type { ConversationAgentInteraction } from "@/types/conversation";
import { ConversationCollaborationCard, ConversationAgentInteractionView, ConversationAgentInteractionActionsContext } from "./conversation-collaboration-card";
import { USER_PROMPT_COLLAPSE_LINE_LIMIT } from "./conversation-bubble-limits";
import { clientActionId } from "@/service/conversation-event-window-store";
import { ConversationCollaborationStateContext, projectConversationCollaborationState } from "./conversation-collaboration-state";

// This file mounts the card directly; the display renderer only proves it renders somewhere.
// 本文件直接挂载该卡片；显示渲染器只证明它被渲染，不证明其自身行为。

let host: HTMLDivElement;
let root: Root;
// Controlled geometry stands in for browser line layout, which jsdom does not implement.
// 可控几何替代 jsdom 未实现的浏览器行布局。
let contentHeight: number;
beforeEach(() => {
  contentHeight = 40;
  const computedStyle = window.getComputedStyle.bind(window);
  vi.spyOn(window, "getComputedStyle").mockImplementation((element) => element.classList.contains("vk-collab-bubble-body")
    ? { lineHeight: "20px" } as CSSStyleDeclaration : computedStyle(element));
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.classList.contains("vk-collab-bubble-body") ? contentHeight : 0;
  });
  vi.stubGlobal("ResizeObserver", class {
    // Geometry is controlled by the test; no asynchronous resize notifications are needed.
    // 几何由测试控制，不需要异步尺寸通知。
    observe() {}
    // Release the inert observer; returns nothing.
    // 释放空观察器；无返回值。
    disconnect() {}
  });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

// participant builds one typed participant coordinate for source or target.
// participant 为来源或目标构建一个有类型的参与者坐标。
function participant(role: "source" | "target") {
  return {
    session_id: `session-${role}`,
    root_session_id: `session-${role}`,
    project_id: "project-1",
    segment_generation: 1,
    ...(role === "source" ? { turn_id: 7 } : { task_id: "task-target" }),
  };
}

// questionEvent builds one pending-question collaboration event.
// questionEvent 构建一条待处理问题的协作事件。
function questionEvent(
  state: "pending" | "answered" = "pending",
  eventKind: CollaborationEventKind = "question_created",
): Extract<ConversationEventBody, { kind: "collaboration" }> {
  return {
    kind: "collaboration",
    version: 1,
    operation_id: "operation-question-1",
    event_kind: eventKind,
    after_state: {
      kind: "question",
      question: {
        operation_id: "operation-question-1",
        question_id: "question-1",
        delivery_id: "delivery-1",
        source: participant("source"),
        target: participant("target"),
        body: "Which file owns this behavior?",
        execution_purpose: "in_turn_question",
        state,
        created_at_ms: 100,
      },
      delivery: {
        delivery_id: "delivery-1",
        question_id: "question-1",
        operation_id: "operation-question-1",
        source: participant("source"),
        target: participant("target"),
        state: "recorded",
        updated_at_ms: 101,
      },
    },
  } as Extract<ConversationEventBody, { kind: "collaboration" }>;
}

// inboundNotificationEvent builds one received notification, which must not offer question controls.
// inboundNotificationEvent 构建一条已接收通知，它不得提供问题控件。
function inboundNotificationEvent(): Extract<ConversationEventBody, { kind: "collaboration" }> {
  return {
    kind: "collaboration",
    version: 1,
    operation_id: "operation-notification-1",
    event_kind: "inbound_notification_accepted",
    after_state: {
      kind: "inbound_notification",
      notification: {
        notification_id: "notification-1",
        delivery_id: "notification-delivery-1",
        source: participant("source"),
        target: participant("target"),
        body: "Phase one finished.",
        state: "accepted",
        created_at_ms: 200,
      },
    },
  } as unknown as Extract<ConversationEventBody, { kind: "collaboration" }>;
}

// interaction wraps one collaboration body in the exact host envelope the card consumes.
// interaction 把一条协作事件体包进卡片所消费的精确宿主 envelope。
function interaction(event: Extract<ConversationEventBody, { kind: "collaboration" }>): ConversationAgentInteraction {
  return {
    seq: 5,
    turn_id: 7,
    created_at_ms: 1_700_000_000_000,
    updated_at_ms: 1_700_000_000_000,
    event_revision: 1,
    visibility: "host_only",
    event,
  };
}

// button finds one rendered control by the English label in a locale-fixed card.
// button 在固定区域语言的卡片中按英文标签查找一个已渲染控件。
function button(text: string): HTMLButtonElement {
  const match = Array.from(host.querySelectorAll("button")).find((element) =>
    element.textContent?.includes(text),
  );
  if (!match) throw new Error(`Missing action: ${text}`);
  return match;
}

// mount renders the production card with the supplied actions in en-US for stable assertions.
// mount 以 en-US 与给定操作渲染生产卡片，以便断言稳定。
async function mount(
  event: Extract<ConversationEventBody, { kind: "collaboration" }>,
  actions: {
    onOpenSession?: (sessionId: string) => void;
    availableSessionIds?: ReadonlySet<string>;
    subagentNames?: ReadonlyMap<string, string>;
  } = {},
) {
  await act(async () =>
    root.render(
      <I18nProvider initialLocale="en-US">
        <ConversationCollaborationCard
          {...actions}
          createdAtMs={1_700_000_000_000}
          event={event}
          seq={5}
        />
      </I18nProvider>,
    ),
  );
  // Existing action assertions inspect the real expanded body; the initial surface must stay folded.
  // 既有操作断言检查真实展开正文；初始界面必须保持收起。
  const toggle = host.querySelector<HTMLButtonElement>(".vk-collab-head .vk-collab-toggle");
  if (toggle?.getAttribute("aria-expanded") === "false") {
    await act(async () => toggle.click());
  }
}

it("renders each collaboration kind under its own card identity", async () => {
  // The renderer-level tests only assert the kinds they happen to produce; this covers the card's own
  // mapping from after-state to card identity, including the notification branch.
  // 渲染器层测试只断言它恰好产生的类型；本用例覆盖卡片自身从后状态到卡片身份的映射，包括通知分支。
  await mount(questionEvent());
  const questionCard = host.querySelector("article")!;
  expect(questionCard.getAttribute("data-collaboration-card-kind")).toBe("interaction");
  expect(questionCard.getAttribute("data-event-kind")).toBe("collaboration");
  expect(questionCard.getAttribute("data-seq")).toBe("5");
  await act(async () => root.unmount());
  root = createRoot(host);
  await mount(inboundNotificationEvent());
  const notificationCard = host.querySelector("article")!;
  expect(notificationCard.getAttribute("data-collaboration-card-kind")).toBe("notification");
});

it("renders the interaction view for a collaboration envelope and the result card for a child return", async () => {
  // ConversationAgentInteractionView is the card's real entry point from the event window; it must route
  // a collaboration envelope to this card and a child return to the distinct task-result card.
  // ConversationAgentInteractionView 是卡片从事件窗口进入的真实入口；它必须把协作 envelope 路由到
  // 本卡片，并把子任务返回路由到独立的任务结果卡片。
  const childReturn: ConversationAgentInteraction = {
    seq: 6,
    turn_id: 7,
    created_at_ms: 1_700_000_000_000,
    updated_at_ms: 1_700_000_000_000,
    event_revision: 1,
    visibility: "provider",
    event: {
      kind: "subagent_return",
      child_session_id: "child-session",
      child_segment_generation: 1,
      child_root_action_id: "child-execution",
      child_task_id: "child-task",
      content: "Child final content",
      source_assistant_text_seq: 3,
      source_from_seq: 1,
      source_to_seq: 5,
      child_turn_stats_seq: 5,
    },
  };
  await act(async () =>
    root.render(
      <I18nProvider initialLocale="en-US">
        <ConversationAgentInteractionView interaction={interaction(questionEvent())} />
      </I18nProvider>,
    ),
  );
  expect(host.querySelector("article")!.getAttribute("data-collaboration-card-kind")).toBe("interaction");
  await act(async () =>
    root.render(
      <I18nProvider initialLocale="en-US">
        <ConversationAgentInteractionActionsContext.Provider value={{ subagentNames: new Map([["child-session", "yes_agent"]]) }}>
          <ConversationAgentInteractionView interaction={childReturn} />
        </ConversationAgentInteractionActionsContext.Provider>
      </I18nProvider>,
    ),
  );
  const resultCard = host.querySelector("article")!;
  expect(resultCard.getAttribute("data-collaboration-card-kind")).toBe("task-result");
  expect(resultCard.textContent).toContain("Agent work completed");
  expect(resultCard.textContent).toContain("yes_agent");
  expect(resultCard.querySelector(".lucide-messages-square")).not.toBeNull();
  expect(resultCard.textContent).not.toContain("Child final content");
  const toggle = resultCard.querySelector<HTMLButtonElement>(".vk-collab-toggle")!;
  expect(toggle.getAttribute("aria-expanded")).toBe("false");
  await act(async () => toggle.click());
  expect(toggle.getAttribute("aria-expanded")).toBe("true");
  expect(resultCard.textContent).toContain("Child final content");
  expect(resultCard.textContent).not.toContain("Cancel question");
  await act(async () => toggle.click());
  expect(toggle.getAttribute("aria-expanded")).toBe("false");
});

it("offers no question control for a received notification", async () => {
  // A18 requires notifications to carry no reply obligation; a cancel control would imply one.
  // A18 要求通知不携带回复义务；撤销控件会暗示存在义务。
  await mount(inboundNotificationEvent());
  expect(Array.from(host.querySelectorAll("button")).map((element) => element.textContent)).not.toContain(
    "Cancel question",
  );
});

it("shows a failed answer delivery as failed after authorization revocation", async () => {
  // A revoked reply is a durable failed delivery, not an answer sent to the source model.
  // 撤权后的回复属于可靠失败交付，不是已经发给来源模型的答案。
  await mount({
    kind: "collaboration",
    version: 1,
    operation_id: "operation-answer-failed",
    event_kind: "answer_delivery_failed",
    after_state: {
      kind: "answer_delivery",
      question_id: "question-1",
      delivery: {
        state: "failed",
        failure: "authorization_revoked",
        delivery_revision: 2,
        last_operation_id: "operation-answer-failed",
        updated_at_ms: 200,
      },
    },
  });
  expect(host.querySelector(".vk-collab-badge")?.textContent).toBe("Failed");
});

it("enables session navigation only for a routable participant", async () => {
  // A route whose target is missing must stay inert rather than navigating to a dead session.
  // 目标缺失的路由必须保持不可用，而不是导航到死会话。
  const onOpenSession = vi.fn();
  const names = new Map([["session-source", "Source"], ["session-target", "Target"]]);
  await mount(questionEvent(), { onOpenSession, availableSessionIds: new Set(["session-source"]), subagentNames: names });
  expect(host.querySelector('[aria-label="Open source session"]')).toBeNull();
  expect(button("Target").disabled).toBe(true);
  await act(async () => button("Target").click());
  expect(onOpenSession).not.toHaveBeenCalled();
  await mount(questionEvent(), { onOpenSession, availableSessionIds: new Set(["session-target"]), subagentNames: names });
  const toggle = host.querySelector<HTMLButtonElement>(".vk-collab-toggle")!;
  const expandedBeforeNavigation = toggle.getAttribute("aria-expanded");
  expect(button("Target").textContent).toBe("[Target]");
  expect(button("Target").querySelector(".lucide-arrow-up-right")).not.toBeNull();
  await act(async () => button("Target").click());
  expect(onOpenSession).toHaveBeenCalledExactlyOnceWith("session-target");
  expect(toggle.getAttribute("aria-expanded")).toBe(expandedBeforeNavigation);
});

it.each([false, true])("shows incoming content immediately, folding only excess lines (long: %s)", async (long) => {
  contentHeight = long ? 20 * (USER_PROMPT_COLLAPSE_LINE_LIMIT + 1) : 40;
  await mount(inboundNotificationEvent());
  const bubble = host.querySelector(".vk-prompt")!;
  expect(bubble.textContent).toContain("Phase one finished.");
  expect(bubble.querySelector("time")).toBeNull();
  expect(host.querySelector("article > .vk-prompt-foot > time")).not.toBeNull();
  expect(host.querySelector(".vk-collab-head .vk-collab-toggle")).toBeNull();
  expect(host.querySelector(".vk-prompt-foot > .vk-icon-btn")).toBeNull();
  const toggle = bubble.querySelector<HTMLButtonElement>(".vk-collab-more");
  expect(Boolean(toggle)).toBe(long);
  if (toggle) {
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    await act(async () => toggle.click());
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    await act(async () => toggle.click());
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(bubble.textContent).toContain("Phase one finished.");
  }
});

it.each(["pending", "answered"] as const)("shows no cancellation control or technical details for a %s question", async (state) => {
  await mount(questionEvent(state));
  expect(host.textContent).toContain("Which file owns this behavior?");
});

// Both received question and received answer reuse the same short/long bubble behavior.
// 收到的问题与答案复用同一套短文和长文气泡行为。
it.each([false, true])("renders received questions and answers as incoming bubbles (long: %s)", async (long) => {
  contentHeight = long ? 20 * (USER_PROMPT_COLLAPSE_LINE_LIMIT + 1) : 40;
  // The producer fixture mirrors HostMessageEnvelope::render, including escaped literal body text.
  // 生产者夹具镜像 HostMessageEnvelope::render，包含转义的原始正文。
  const incoming: ConversationAgentInteraction = { ...interaction(questionEvent()), event: {
    kind: "user_input", client_action_id: clientActionId("agent-message:delivery-1"), input_id: 9,
    input_source: "agent_message", text: '<vulcan_agent_message version="1" event_id="delivery-1" kind="question" source="session-source" target="session-target" question_id="question-1" scope="question">Question `&lt;literal&gt;` &amp; text</vulcan_agent_message>',
  } };
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><ConversationAgentInteractionView interaction={incoming} /></I18nProvider>));
  expect(host.querySelector(".vk-prompt")?.textContent).toContain("Question <literal> & text");
  expect(host.querySelector(".vk-collab-toggle")).toBeNull();
  expect(Boolean(host.querySelector(".vk-collab-more"))).toBe(long);
  // The answer's source snapshot contains the original question, which must never appear in its bubble.
  // 答案来源快照含原问题，但它绝不能出现在答案气泡里。
  const replied = questionEvent("answered", "question_answered");
  if (replied.after_state.kind !== "question") throw new Error("Expected a question fixture");
  replied.after_state.question.answer = { operation_id: "reply", responder: participant("target"), body: "Only the answer.", settled_at_ms: 110,
    delivery: { state: "undelivered", delivery_revision: 1, last_operation_id: "reply", updated_at_ms: 110 } };
  await mount(replied);
  expect(host.querySelector(".vk-prompt")?.textContent).toContain("Only the answer.");
  expect(host.textContent).not.toContain("Which file owns this behavior?");
  expect(host.querySelector(".vk-collab-toggle")).toBeNull();
  expect(Boolean(host.querySelector(".vk-collab-more"))).toBe(long);
  await mount({ ...replied, event_kind: "question_created" });
  expect(host.querySelector(".vk-prompt")).toBeNull();
  expect(host.textContent).toContain("Which file owns this behavior?");
  expect(host.textContent).not.toContain("Only the answer.");
});

// Receipts update the existing answer while unrelated or stale revisions cannot replace its state.
// 回执原位更新现有答案，无关或过期修订不能覆盖其状态。
it("attaches exact answer receipts in place without duplicating or moving the reply", async () => {
  const reply = questionEvent("answered", "question_answered");
  if (reply.after_state.kind !== "question") throw new Error("Expected a question fixture");
  reply.after_state.question.answer = { operation_id: "reply", responder: participant("target"), body: "Reply body", settled_at_ms: 110,
    delivery: { state: "undelivered", delivery_revision: 1, last_operation_id: "reply", updated_at_ms: 110 } };
  // Controlled snapshots use question IDs and receipt revisions from the production contract.
  // 可控快照使用生产契约的问题身份及回执修订。
  const receipt = (questionId: string, revision: number, state: "delivered" | "failed"): ConversationAgentInteraction => ({
    ...interaction(reply), seq: 5 + revision, event: { kind: "collaboration", version: 1, operation_id: `receipt-${questionId}-${revision}`,
      event_kind: state === "delivered" ? "answer_delivered" : "answer_delivery_failed",
      after_state: { kind: "answer_delivery", question_id: questionId, delivery: {
        state, delivery_revision: revision, last_operation_id: `receipt-${revision}`, updated_at_ms: 110 + revision,
      } },
    },
  });
  // Rerender only presentation state; the original event and timestamp remain immutable.
  // 仅重新渲染展示状态；原始事件与时间戳保持不可变。
  const render = async (receipts: ConversationAgentInteraction[]) => {
    await act(async () => root.render(<I18nProvider initialLocale="en-US">
      <ConversationCollaborationStateContext.Provider value={projectConversationCollaborationState([interaction(reply), ...receipts])}>
        <ConversationAgentInteractionView interaction={interaction(reply)} />
      </ConversationCollaborationStateContext.Provider>
    </I18nProvider>));
  };
  await render([]);
  const card = host.querySelector("article");
  const timestamp = host.querySelector("time")?.dateTime;
  await render([receipt("another-question", 9, "failed"), receipt("question-1", 3, "delivered"), receipt("question-1", 2, "failed")]);
  expect(host.querySelector("article")).toBe(card);
  expect(host.querySelectorAll("article")).toHaveLength(1);
  expect(host.querySelector(".vk-collab-badge")?.textContent).toContain("Delivered");
  expect(host.querySelector(".vk-collab-badge")?.textContent).not.toContain("Failed");
  expect(host.querySelector(".vk-prompt")?.textContent).toBe("Reply body");
  expect(host.querySelector("time")?.dateTime).toBe(timestamp);
});

// Incoming execution transitions enrich the same question bubble without leaking across question IDs.
// 接收方执行转换补充同一个提问气泡，不能跨问题身份混入。
it("updates receiver execution in the question bubble and suppresses stale running after settlement", async () => {
  // The received host question is immutable while execution and delivery facts evolve independently.
  // 收到的宿主问题保持不可变，执行和投递事实独立演进。
  const incoming: ConversationAgentInteraction = { ...interaction(questionEvent()), event: {
    kind: "user_input", client_action_id: clientActionId("agent-message:delivery-1"), input_id: 9, input_source: "agent_message",
    text: '<vulcan_agent_message version="1" event_id="delivery-1" kind="question" source="session-source" target="session-target" question_id="question-1" scope="question">Original question</vulcan_agent_message>',
  } };
  // Build typed receiver execution facts for one exact question; returns its persisted envelope.
  // 为一个精确问题构造接收方执行事实；返回持久信封。
  const execution = (questionId: string, state: "running" | "completed", outcome?: "settled" | "failed"): ConversationAgentInteraction => ({
    ...interaction(questionEvent()), event: { kind: "collaboration", version: 1, operation_id: `execution-${state}`,
      event_kind: state === "running" ? "question_execution_started" : "question_execution_completed",
      after_state: { kind: "question_execution", execution: { execution_id: `execution-${questionId}`, operation_id: "start", question_id: questionId,
        participant: participant("target"), purpose: "in_turn_question", state, outcome, started_at_ms: 100 } },
    },
  });
  // Preserve the original event, position and timestamp while updating the presentation context.
  // 更新展示上下文时保留原事件、位置及时间戳。
  const render = async (events: ConversationAgentInteraction[]) => {
    await act(async () => root.render(<I18nProvider initialLocale="en-US">
      <ConversationCollaborationStateContext.Provider value={projectConversationCollaborationState(events)}>
        <ConversationAgentInteractionView interaction={incoming} />
      </ConversationCollaborationStateContext.Provider>
    </I18nProvider>));
  };
  await render([execution("question-1", "running"), execution("unrelated", "completed", "failed")]);
  const card = host.querySelector("article");
  const timestamp = host.querySelector("time")?.dateTime;
  expect(host.querySelector(".vk-collab-badge")?.textContent).toBe("Running");
  for (const outcome of ["settled", "failed"] as const) {
    await render([execution("question-1", "running"), execution("question-1", "completed", outcome)]);
    expect(host.querySelector("article")).toBe(card);
    expect(host.querySelector(".vk-collab-badge")?.textContent).toBe(outcome === "settled" ? "Settled" : "Failed");
  }
  // A persisted settlement wins even when the window still contains the earlier running snapshot.
  // 即使窗口仍含此前运行快照，持久化结算也应优先显示。
  await render([execution("question-1", "running"), { ...interaction(questionEvent()), event: {
    kind: "collaboration", version: 1, operation_id: "settlement", event_kind: "delivery_settled",
    after_state: { kind: "inbound_delivery", delivery: { delivery_id: "delivery-1", question_id: "question-1", operation_id: "delivery",
      source: participant("source"), target: participant("target"), execution_purpose: "in_turn_question", state: "projected", target_revision: 2, updated_at_ms: 200,
      settlement: { state: "answered", source_event_seq: 12, source_operation_id: "reply", target_operation_id: "settlement", acknowledged_at_ms: 200 },
    } },
  } }]);
  expect(host.querySelector("article")).toBe(card);
  expect(host.querySelectorAll("article")).toHaveLength(1);
  expect(host.querySelector(".vk-collab-badge")?.textContent).toContain("Answered");
  expect(host.textContent).not.toContain("Running");
  expect(host.querySelector(".vk-prompt")?.textContent).toBe("Original question");
  expect(host.querySelector("time")?.dateTime).toBe(timestamp);
});

it("retains the original event timestamp across a delivery update", async () => {
  // The card carries the host timestamp, not a reconstructed one; a delivery update must not shift it.
  // 卡片携带宿主时间戳而非重建时间戳；投递更新不得改变它。
  await mount(questionEvent("pending", "delivery_accepted"));
  const markup = host.innerHTML;
  expect(markup).toContain(`data-seq="5"`);
  expect(markup).toContain('data-collaboration-event-kind="delivery_accepted"');
  expect(markup).toContain(new Date(1_700_000_000_000).toISOString());
});
