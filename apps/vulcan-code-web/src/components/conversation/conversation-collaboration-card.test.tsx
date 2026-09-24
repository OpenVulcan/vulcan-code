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

// This file mounts the card directly; the display renderer only proves it renders somewhere.
// 本文件直接挂载该卡片；显示渲染器只证明它被渲染，不证明其自身行为。

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
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
  const toggle = host.querySelector<HTMLButtonElement>(".tool-presentation-summary")!;
  if (toggle.getAttribute("aria-expanded") === "false") {
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
  expect(notificationCard.className).toContain("conversation-v1-collaboration-notification-card");
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
  const toggle = resultCard.querySelector<HTMLButtonElement>(".tool-presentation-summary")!;
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
  expect(host.querySelector(".conversation-v1-collaboration-status")?.textContent).toBe("Failed");
  expect(host.querySelector(".conversation-v1-collaboration-details")).toBeNull();
});

it("enables session navigation only for a routable participant", async () => {
  // A route whose target is missing must stay inert rather than navigating to a dead session.
  // 目标缺失的路由必须保持不可用，而不是导航到死会话。
  const onOpenSession = vi.fn();
  await mount(questionEvent(), { onOpenSession, availableSessionIds: new Set(["session-source"]) });
  expect(button("Source").disabled).toBe(false);
  expect(button("Target").disabled).toBe(true);
  await act(async () => button("Source").click());
  expect(onOpenSession).toHaveBeenCalledTimes(1);
  expect(onOpenSession).toHaveBeenCalledWith("session-source");
  await act(async () => button("Target").click());
  expect(onOpenSession).toHaveBeenCalledTimes(1);
});

it.each(["pending", "answered"] as const)("shows no cancellation control or technical details for a %s question", async (state) => {
  await mount(questionEvent(state));
  expect(host.querySelector(".conversation-v1-collaboration-cancel-button")).toBeNull();
  expect(host.querySelector(".conversation-v1-collaboration-details")).toBeNull();
  expect(host.textContent).toContain("Which file owns this behavior?");
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
