import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import {
  ConversationV1Workspace,
  type ConversationV1OptimisticUserInput,
} from "./conversation-v1-workspace";
import { I18nProvider } from "@/i18n";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { SelectedConversationEventWindowCommands } from "@/service/selected-conversation-event-window-registry";
import {
  clientActionId,
  type ConversationEventSummary,
} from "@/service/conversation-event-window-store";

/** Builds one canonical V1 event envelope for production-shell tests.
 * 为生产外壳测试构建一条 canonical V1 事件 envelope。
 *
 * @param seq exact persisted sequence identity.
 *            精确的持久化序列身份。
 * @param event persisted event body.
 *              持久化事件正文。
 * @returns one complete event summary.
 *          一条完整事件摘要。 */
function eventSummary(
  seq: number,
  event: ConversationEventSummary["event"],
): ConversationEventSummary {
  return {
    seq,
    turn_id: 11,
    created_at_ms: 100 + seq,
    updated_at_ms: 100 + seq,
    event_revision: 1,
    visibility: "provider",
    event,
  };
}

/** Builds one selected-session event-window snapshot.
 * 构建一份选中会话事件窗口快照。
 *
 * @param sessionId exact owner session identity.
 *                  精确的归属会话身份。
 * @param events canonical events in persisted seq order.
 *               按持久化 seq 顺序排列的 canonical 事件。
 * @returns one complete controller snapshot.
 *          一份完整控制器快照。 */
function snapshot(
  sessionId: string,
  events: readonly ConversationEventSummary[],
): ConversationEventWindowControllerSnapshot {
  return {
    owner: { session_id: sessionId, window_generation: 3 },
    service_epoch: "service-workspace-test",
    memory_revision: 4,
    durable_revision: 4,
    events,
    older_before_seq: null,
    has_more: false,
    open_turn_id: 11,
    open_turn_start_seq: 1,
    session_config: null,
  };
}

/** Renders the production V1 shell with exact owner-bound defaults.
 * 使用精确绑定归属的默认参数渲染生产 V1 外壳。
 *
 * @param sessionId currently selected durable session identity.
 *                  当前选中的持久化会话身份。
 * @param currentSnapshot atomic controller snapshot.
 *                        原子控制器快照。
 * @param optimisticUserInput optional process-local row awaiting V1 reconciliation.
 *                            等待 V1 对账的可选进程内行。
 * @returns static production-shell markup.
 *          生产外壳的静态标记。 */
function renderWorkspace(
  sessionId: string,
  currentSnapshot: ConversationEventWindowControllerSnapshot,
  optimisticUserInput: ConversationV1OptimisticUserInput | null = null,
): string {
  // commands is a stable owner-bound command surface; SSR never invokes it.
  // commands 是稳定的归属绑定命令面；SSR 不会调用它。
  const commands: SelectedConversationEventWindowCommands = {
    rehydrate: vi.fn(async () => true),
    prependOlder: vi.fn(async () => true),
  };
  // scrollRef models the ChatArea-owned exact transcript element reference.
  // scrollRef 模拟由 ChatArea 持有的精确转录元素引用。
  const scrollRef = React.createRef<HTMLElement>();
  return renderToStaticMarkup(
    <I18nProvider initialLocale="zh-CN">
      <ConversationV1Workspace
        commands={commands}
        onCopyText={() => undefined}
        onPreserveHistoryPrepend={(load) => load()}
        onScrollStateChange={() => undefined}
        optimisticUserInput={optimisticUserInput}
        reasoningDisplayMode="inline_output"
        scrollRef={scrollRef}
        sessionId={sessionId}
        sessionRunning={false}
        snapshot={currentSnapshot}
      />
    </I18nProvider>,
  );
}

describe("ConversationV1Workspace", () => {
  it("renders only canonical V1 events for the exact selected owner", () => {
    // events represent the only production transcript authority.
    // events 表示唯一的生产转录权威。
    const events = [
      eventSummary(1, {
        kind: "user_input",
        client_action_id: clientActionId("workspace-root-input"),
        input_id: 1,
        input_source: "root",
        text: "V1 root input",
      }),
      eventSummary(2, {
        kind: "assistant_text",
        api_call_id: 1,
        text: "V1 assistant output",
        status: "completed",
      }),
    ] as const;
    // markup is the exact owner-ready production shell output.
    // markup 是精确归属就绪后的生产外壳输出。
    const markup = renderWorkspace("session-v1", snapshot("session-v1", events));

    expect(markup).toContain('data-v1-owner-ready="true"');
    expect(markup).toContain('class="conversation-v1-display"');
    expect(markup).toContain("V1 root input");
    expect(markup).toContain("V1 assistant output");
    expect(markup).not.toContain("conversation-workspace");
  });

  it("never renders a stale previous-session event window during owner handoff", () => {
    // staleEvents belong to a different durable session and must remain invisible.
    // staleEvents 属于另一个持久化会话，必须保持不可见。
    const staleEvents = [
      eventSummary(1, {
        kind: "assistant_text",
        api_call_id: 9,
        text: "stale previous-session output",
        status: "completed",
      }),
    ] as const;
    // markup renders the handoff state for the newly selected session.
    // markup 渲染新选中会话的归属切换状态。
    const markup = renderWorkspace("session-new", snapshot("session-old", staleEvents));

    expect(markup).toContain('data-v1-owner-ready="false"');
    expect(markup).toContain('data-v1-owner-loading="true"');
    expect(markup).not.toContain("stale previous-session output");
    expect(markup).not.toContain('class="conversation-v1-display"');
  });

  it("shows one process-local input until the exact canonical event replaces it", () => {
    // optimistic is the display-only browser action waiting for durable event projection.
    // optimistic 是等待可靠事件投影的纯展示浏览器动作。
    const optimistic: ConversationV1OptimisticUserInput = {
      clientActionId: "workspace-optimistic-input",
      text: "pending V1 input",
      attachmentNames: ["draft.png"],
    };
    // pendingMarkup includes the process-local row after the durable window.
    // pendingMarkup 在可靠窗口之后包含进程内行。
    const pendingMarkup = renderWorkspace("session-v1", snapshot("session-v1", []), optimistic);
    // canonicalEvents carries the same action after the backend publishes UserInput.
    // canonicalEvents 在后端发布 UserInput 后携带同一动作。
    const canonicalEvents = [
      eventSummary(1, {
        kind: "user_input",
        client_action_id: clientActionId(optimistic.clientActionId),
        input_id: 1,
        input_source: "root",
        text: optimistic.text,
      }),
    ] as const;
    // canonicalMarkup models ChatArea removing the overlay after exact identity reconciliation.
    // canonicalMarkup 模拟 ChatArea 在精确身份对账后移除 overlay。
    const canonicalMarkup = renderWorkspace(
      "session-v1",
      snapshot("session-v1", canonicalEvents),
      null,
    );

    expect(pendingMarkup).toContain('data-v1-optimistic-user-input="true"');
    expect(pendingMarkup).toContain("pending V1 input");
    expect(pendingMarkup).toContain("draft.png");
    expect(canonicalMarkup).not.toContain('data-v1-optimistic-user-input="true"');
    expect(canonicalMarkup).toContain("pending V1 input");
  });
});
