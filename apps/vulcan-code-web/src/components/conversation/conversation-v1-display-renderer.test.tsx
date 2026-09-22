import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import {
  collectConversationResourceIdsV1,
  ConversationV1DisplayRenderer,
  ConversationV1EventWindowView,
} from "./conversation-v1-display-renderer";
import { I18nProvider } from "@/i18n";
import type {
  ConversationEventHydrationV1Response,
  ConversationEventWindowRpc,
} from "@/service/conversation-event-window-client";
import { ConversationEventWindowController } from "@/service/conversation-event-window-controller";
import type { ConversationSessionResourceCatalogV1 } from "@/state/conversation-session-resources-v1";
import {
  clientActionId,
  resourceId,
  type ConversationEventBody,
  type ConversationEventSummary,
  type CollaborationTaskResult,
} from "@/service/conversation-event-window-store";

/** Builds one complete V1 event envelope for renderer tests.
 * 为渲染器测试构建一条完整 V1 事件 envelope。 */
function summary(
  seq: number,
  event: ConversationEventBody,
  visibility: ConversationEventSummary["visibility"] = "provider",
): ConversationEventSummary {
  return {
    seq,
    turn_id: 7,
    created_at_ms: 100 + seq,
    updated_at_ms: 100 + seq,
    event_revision: 1,
    visibility,
    event,
  };
}

/** Returns the first markup offset of one persisted sequence identity.
 * 返回一个持久化序列身份在标记中的首次偏移。 */
function seqOffset(markup: string, seq: number): number {
  return markup.indexOf(`data-seq="${seq}"`);
}

describe("ConversationV1DisplayRenderer", () => {
  it("renders an owned child failure as a result with its original diagnostic and source", () => {
    // The record represents the parent-owned result event, not a human-authored message.
    // 此记录表示父侧结果事件，而非人类编写的消息。
    const event: ConversationEventBody = {
      kind: "collaboration", version: 1, operation_id: "child-result:3",
      event_kind: "child_task_non_success_recorded",
      after_state: { kind: "child_task", task: {
        reservation_operation_id: "reserve", source_tool_name: "spawn_agent", execution_id: "failed-execution",
        task_id: "worker", parent_segment_generation: 1, parent_turn_id: 7,
        child: { session_id: "executor", root_session_id: "parent", project_id: "project", segment_generation: 1, task_id: "worker" },
        state: "terminal", reserved_at_ms: 3, terminal_at_ms: 10, terminal_outcome: "failed",
        non_success_projection: null,
        non_success_result: { operation_id: "child-result:3", event_seq: 12, parent_revision: 5, proof: {
          child_session_id: "executor", child_segment_generation: 1, execution_id: "failed-execution",
          child_turn_id: 1, child_start_seq: 1, child_terminal_seq: 4, child_turn_outcome: "stopped", outcome: "failed",
          failure_notes: [{ event_seq: 2, reason_code: "startup_failed", redacted_detail: "原始启动错误" }],
        } },
      } },
    };
    // Render through the actual event timeline and its collaboration navigation context.
    // 通过实际事件时间线及其协作导航上下文渲染。
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
      <ConversationV1DisplayRenderer availableCollaborationSessionIds={new Set(["executor"])}
        onOpenCollaborationSession={() => undefined} events={[summary(12, event, "host_only")]} />
    </I18nProvider>);
    expect(markup).toContain('data-collaboration-card-kind="task-result"');
    expect(markup).toContain('class="conversation-v1-collaboration-body">startup_failed: 原始启动错误');
    expect(markup).toContain("打开来源会话");
    expect(markup).not.toContain("user-bubble");
    expect(markup).not.toContain("assistant-final-output");
    expect(markup).not.toContain("conversation-v1-collaboration-cancel-button");
    // Projection is a delivery control; only the immutable report carries the visible diagnostic.
    // 投影属于交付控制；只有不可变报告携带可见诊断。
    if (event.after_state.kind !== "child_task") throw new Error("Expected a child task fixture");
    const projected: ConversationEventBody = {
      ...event, operation_id: "child-result-project:12", event_kind: "child_task_non_success_projected",
      after_state: { kind: "child_task", task: { ...event.after_state.task, non_success_projection: {
        execution_id: "failed-execution", result_seq: 12, parent_turn_id: 7,
        inputs: [{ input_id: 1, input_seq: 13 }], operation_id: "child-result-project:12",
        event_seq: 14, parent_revision: 6, projected_at_ms: 114,
      } } },
    };
    // Render both durable facts together to detect duplicate result presentation.
    // 一起渲染两个持久事实，以检测重复结果展示。
    const deliveredMarkup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
      <ConversationV1DisplayRenderer events={[
        summary(12, event, "host_only"),
        summary(13, { kind: "user_input", client_action_id: clientActionId("child-result:12:0"),
          input_id: 1, input_source: "agent_message", text: "model-only result envelope" }),
        summary(14, projected, "host_only"),
      ]} />
    </I18nProvider>);
    expect(deliveredMarkup.match(/data-collaboration-card-kind="task-result"/g)).toHaveLength(1);
    expect(deliveredMarkup.match(/class="conversation-v1-collaboration-body">startup_failed: 原始启动错误/g)).toHaveLength(1);
    expect(deliveredMarkup).not.toContain("model-only result envelope");
  });

  it("renders final task facts separately from user prompts and delivery acknowledgements", () => {
    // The target executor returns the result to its original requester; stopped work has no answer.
    // 目标执行者向原请求者回传结果；停止任务没有回答正文。
    const result: CollaborationTaskResult = {
      acceptance: {
        message: {
          operation_id: "send", delivery_id: "delivery", kind: "task_start", body: "Original task",
          source: { session_id: "requester", root_session_id: "requester", project_id: "project", segment_generation: 1, turn_id: 1 },
          target: { session_id: "executor", root_session_id: "executor", project_id: "project", segment_generation: 1 },
        }, operation_id: "accept", target_revision: 2, event_seq: 2, accepted_at_ms: 2,
      },
      effect: { delivery_id: "delivery", operation_id: "claim", target_revision: 3, event_seq: 3, occurred_at_ms: 3,
        outcome: { kind: "claimed", execution_id: "execution", turn_id: 2, input_id: 2, input_seq: 4 } },
      terminal: { turn_id: 2, turn_start_seq: 3, turn_stats_seq: 9, outcome: "completed", completed_at_ms: 9,
        final_assistant: { api_call_id: 2, api_stats_seq: 7, source_text: { event_seq: 6, text: "完整最终结果" } } },
      operation_id: "task-result:3", target_revision: 6, event_seq: 10, recorded_at_ms: 9,
    };
    for (const outcome of ["completed", "stopped", "abandoned"] as const) {
      // Each variant is independently rendered so a stale success cannot leak into stopped output.
      // 每个分支独立渲染，避免旧成功正文混入停止输出。
      const snapshot: CollaborationTaskResult = { ...result, terminal: { ...result.terminal, outcome,
        final_assistant: outcome === "completed" ? result.terminal.final_assistant : null } };
      const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer
          availableCollaborationSessionIds={new Set(["executor"])}
          onOpenCollaborationSession={() => undefined}
          events={[summary(10, {
          kind: "collaboration", version: 1, operation_id: result.operation_id,
          event_kind: "task_result_recorded", after_state: { kind: "task_result", result: snapshot },
        }, "host_only")]} />
      </I18nProvider>);
      expect(markup).toContain('data-collaboration-card-kind="task-result"');
      expect(markup).toContain("代理任务结果");
      expect(markup).toContain(outcome === "completed" ? "已完成" : outcome === "stopped" ? "已停止" : "已放弃");
      expect(markup.includes("完整最终结果")).toBe(outcome === "completed");
      expect(markup).not.toContain("user-bubble");
      expect(markup).not.toContain("assistant-final-output");
      expect(markup).not.toContain("conversation-v1-collaboration-cancel-button");
      // Only the executor route is available: this verifies the reversed result direction.
      // 只有执行者路由可用：据此验证结果回传方向确实反转。
      expect(markup).toMatch(/aria-label="打开来源会话"[^>]*class="conversation-v1-collaboration-session-button"[^>]*type="button">/);
      expect(markup).not.toMatch(/aria-label="打开来源会话"[^>]*disabled/);
      expect(markup).toMatch(/aria-label="打开目标会话"[^>]*disabled/);
      // The requester receives a result card; an internal acknowledgement never becomes another answer.
      // 请求者收到结果卡片；内部确认绝不成为另一份回答。
      const acceptance = { result: snapshot, operation_id: "receive-result", event_seq: 20, receiver_revision: 8, accepted_at_ms: 10 };
      const received = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer
          availableCollaborationSessionIds={new Set(["executor"])}
          onOpenCollaborationSession={() => undefined}
          events={[summary(20, { kind: "collaboration", version: 1, operation_id: acceptance.operation_id,
            event_kind: "task_result_accepted", after_state: { kind: "received_task_result", acceptance },
          }, "host_only")]} />
      </I18nProvider>);
      expect(received).toContain('data-collaboration-card-kind="task-result"');
      expect(received).toContain("代理任务结果");
      expect(received.includes("完整最终结果")).toBe(outcome === "completed");
      expect(received).not.toMatch(/aria-label="打开来源会话"[^>]*disabled/);
      expect(received).toMatch(/aria-label="打开目标会话"[^>]*disabled/);
      expect(received).not.toContain("user-bubble");
      const confirmed = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer events={[summary(30, {
          kind: "collaboration", version: 1, operation_id: "confirm-result", event_kind: "task_result_acknowledged",
          after_state: { kind: "task_result_acknowledgement", acknowledgement: { acceptance,
            operation_id: "confirm-result", event_seq: 30, target_revision: 9, acknowledged_at_ms: 11 } },
        }, "host_only")]} />
      </I18nProvider>);
      expect(confirmed).toContain("任务结果已送达");
      expect(confirmed).not.toContain('data-collaboration-card-kind="task-result"');
      expect(confirmed).not.toContain('class="conversation-v1-collaboration-body"');
      expect(confirmed).not.toContain("conversation-v1-collaboration-cancel-button");
      expect(confirmed).not.toContain("assistant-final-output");
      // Projection is an insertion receipt, with neither a second answer body nor reply controls.
      // 投影是插入回执，不包含第二份回答正文或回复操作。
      const projected = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer events={[summary(32, {
          kind: "collaboration", version: 1, operation_id: "project-result", event_kind: "task_result_projected",
          after_state: { kind: "task_result_projection", projection: { delivery_id: "delivery", acceptance_seq: 20,
            turn_id: 1, inputs: [{ input_seq: 31, input_id: 4 }], operation_id: "project-result", event_seq: 32,
            receiver_revision: 10, projected_at_ms: 12 } },
        }, "host_only")]} />
      </I18nProvider>);
      expect(projected).toContain("任务结果已加入上下文");
      expect(projected).not.toContain('class="conversation-v1-collaboration-body"');
      expect(projected).not.toContain("conversation-v1-collaboration-cancel-button");
      expect(projected).not.toContain("user-bubble");
    }
  });
  it("renders task intent, acceptance and effects as distinct delivery cards", () => {
    // These are the exact Core after-state variants, not parsed provider User envelopes.
    // 这些是精确 Core 后状态分支，不是解析供应商 User 信封所得。
    const message = {
      operation_id: "send-task", delivery_id: "task-delivery", kind: "task_steer" as const,
      source: { session_id: "source", root_session_id: "source", project_id: "project", segment_generation: 1, turn_id: 1 },
      target: { session_id: "target", root_session_id: "target", project_id: "project", segment_generation: 1, turn_id: 2 },
      body: "先检查身份绑定",
    };
    const acceptance = { message, operation_id: "accept-task", target_revision: 3, event_seq: 3, accepted_at_ms: 3 };
    const effect = {
      delivery_id: message.delivery_id, operation_id: "apply-task", target_revision: 4, event_seq: 5, occurred_at_ms: 4,
      outcome: { kind: "steered" as const, turn_id: 2, input_id: 2, input_seq: 4 },
    };
    const events: ConversationEventBody[] = [
      { kind: "collaboration", version: 1, operation_id: "send-task", event_kind: "task_message_sent",
        after_state: { kind: "outbound_task_message", record: { message, created_at_ms: 2, acceptance: null } } },
      { kind: "collaboration", version: 1, operation_id: "accept-task", event_kind: "task_message_accepted",
        after_state: { kind: "inbound_task_message", acceptance } },
      { kind: "collaboration", version: 1, operation_id: "ack-task", event_kind: "task_message_acknowledged",
        after_state: { kind: "outbound_task_message", record: { message, created_at_ms: 2, acceptance } } },
      { kind: "collaboration", version: 1, operation_id: "apply-task", event_kind: "task_message_effect",
        after_state: { kind: "task_message_effect", effect } },
      { kind: "collaboration", version: 1, operation_id: "failed-task", event_kind: "task_message_effect",
        after_state: { kind: "task_message_effect", effect: { ...effect,
          outcome: { kind: "failed", reason: "authorization_revoked" } } } },
      { kind: "collaboration", version: 1, operation_id: "claimed-task", event_kind: "task_message_effect",
        after_state: { kind: "task_message_effect", effect: { ...effect,
          outcome: { kind: "claimed", execution_id: "next-task-execution", turn_id: 3, input_id: 4, input_seq: 6 } } } },
    ];
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
      <ConversationV1DisplayRenderer events={events.map((event, index) => summary(index + 1, event, "host_only"))} />
    </I18nProvider>);
    expect(markup.match(/data-collaboration-card-kind="task-message"/g)).toHaveLength(6);
    expect(markup).toContain("代理任务消息");
    expect(markup).toContain("任务已认领");
    expect(markup).toContain("先检查身份绑定");
    expect(markup).not.toContain('data-collaboration-card-kind="task-result"');
    expect(markup).not.toContain("user-bubble");
    expect(markup).not.toContain("子任务结果");
  });
  it("renders child terminal output through the shared task-result card", () => {
    // markup proves the event-native path no longer drops child results or labels them as a parent final.
    // markup 证明事件原生链路不再丢弃子任务结果，也不将其标记为父助手最终回复。
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
      <ConversationV1DisplayRenderer events={[summary(2, { kind: "subagent_return", child_session_id: "child-session", child_segment_generation: 1, child_root_action_id: "child-execution", child_task_id: "child-task", content: "子任务完成内容", source_assistant_text_seq: 3, source_from_seq: 1, source_to_seq: 5, child_turn_stats_seq: 5 })]} />
    </I18nProvider>);
    expect(markup).toContain('data-collaboration-card-kind="task-result"');
    expect(markup).toContain("子任务完成内容");
    expect(markup).not.toContain("assistant-final-output");
    expect(markup).not.toContain("user-bubble");
  });
  it("renders HostOnly collaboration questions as localized cards with exact session navigation", () => {
    const onOpenSession = vi.fn();
    const onCancelQuestion = vi.fn();
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer
          availableCollaborationSessionIds={new Set(["session-source", "session-target"])}
          events={[
            summary(1, {
              kind: "collaboration",
              version: 1,
              operation_id: "operation-question-1",
              event_kind: "question_created",
              after_state: {
                kind: "question",
                question: {
                  operation_id: "operation-question-1",
                  question_id: "question-1",
                  delivery_id: "delivery-1",
                  source: {
                    session_id: "session-source",
                    root_session_id: "session-source",
                    project_id: "project-1",
                    segment_generation: 1,
                    turn_id: 7,
                  },
                  target: {
                    session_id: "session-target",
                    root_session_id: "session-target",
                    project_id: "project-1",
                    segment_generation: 1,
                    task_id: "task-target",
                  },
                  body: "Which file owns this behavior?",
                  execution_purpose: "in_turn_question",
                  state: "pending",
                  created_at_ms: 100,
                },
                delivery: {
                  delivery_id: "delivery-1",
                  question_id: "question-1",
                  operation_id: "operation-question-1",
                  source: {
                    session_id: "session-source",
                    root_session_id: "session-source",
                    project_id: "project-1",
                    segment_generation: 1,
                    turn_id: 7,
                  },
                  target: {
                    session_id: "session-target",
                    root_session_id: "session-target",
                    project_id: "project-1",
                    segment_generation: 1,
                    task_id: "task-target",
                  },
                  state: "recorded",
                  updated_at_ms: 101,
                },
              },
            }, "host_only"),
          ]}
          onCancelCollaborationQuestion={onCancelQuestion}
          onOpenCollaborationSession={onOpenSession}
        />
      </I18nProvider>,
    );

    expect(markup).toContain('data-event-kind="collaboration"');
    expect(markup).toContain("代理提问");
    expect(markup).toContain("等待处理");
    expect(markup).toContain("Which file owns this behavior?");
    expect(markup).toContain("打开来源会话");
    expect(markup).toContain("打开目标会话");
    expect(markup).toContain("撤销问题");
    expect(markup).toContain("operation-question-1");
    expect(markup).toContain("session-source");
    expect(markup).toContain("session-target");
    expect(markup).not.toContain("ask-question-dialog");
  });

  it("renders phase notifications in a distinct collaboration card without question controls", () => {
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="en-US">
        <ConversationV1DisplayRenderer
          availableCollaborationSessionIds={new Set(["session-source", "session-target"])}
          events={[
            summary(2, {
              kind: "collaboration",
              version: 1,
              operation_id: "operation-notification-1",
              event_kind: "notification_sent",
              after_state: {
                kind: "outbound_notification",
                notification: {
                  notification_id: "notification-1",
                  delivery_id: "delivery-notification-1",
                  operation_id: "operation-notification-1",
                  source: {
                    session_id: "session-source",
                    root_session_id: "session-source",
                    project_id: "project-1",
                    segment_generation: 1,
                    turn_id: 7,
                  },
                  target: {
                    session_id: "session-target",
                    root_session_id: "session-target",
                    project_id: "project-1",
                    segment_generation: 1,
                    task_id: "task-target",
                  },
                  body: "The child agent reached the validation phase.",
                  state: "recorded",
                  created_at_ms: 110,
                  updated_at_ms: 111,
                },
              },
            }, "host_only"),
          ]}
        />
      </I18nProvider>,
    );

    expect(markup).toContain('data-collaboration-card-kind="notification"');
    expect(markup).toContain("conversation-v1-collaboration-notification-card");
    expect(markup).toContain("Agent phase notification");
    expect(markup).toContain("Recorded");
    expect(markup).toContain("The child agent reached the validation phase.");
    expect(markup).toContain("Open source session");
    expect(markup).toContain("Open target session");
    expect(markup).not.toContain("Cancel collaboration question");
  });

  // Goal control facts must render as marked cards, not raw protocol identifiers or assistant paragraphs.
  // 目标控制事实必须渲染为标记卡片，不能成为原始协议编号或 assistant 段落。
  it("renders resumed and blocked goals as localized notice cards", () => {
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN">
      <ConversationV1DisplayRenderer events={[
        summary(1, { kind: "display_note", notice_kind: "goal_status", severity: "info", reason_code: "goal_resumed" }, "ui_only"),
        summary(2, { kind: "display_note", notice_kind: "goal_status", severity: "info", reason_code: "goal_blocked" }, "ui_only"),
      ]} />
    </I18nProvider>);
    expect(markup.match(/class="special-block notice"/g)).toHaveLength(2);
    expect(markup).toContain("目标已恢复");
    expect(markup).toContain("目标受阻");
    expect(markup).not.toContain("goal_resumed");
    expect(markup).not.toContain("goal_blocked");
  });
  it("offers edit at the root input and fork only after the owning Turn completes", () => {
    // events contain one root input, one queued input, one final answer, and terminal TurnStats.
    // events 包含一条根输入、一条排队输入、一条最终回答与终态 TurnStats。
    const events: readonly ConversationEventSummary[] = [
      summary(1, {
        kind: "user_input",
        client_action_id: clientActionId("renderer-root-action"),
        input_id: 1,
        input_source: "root",
        text: "editable root",
      }),
      summary(2, {
        kind: "user_input",
        client_action_id: clientActionId("renderer-queued-action"),
        input_id: 2,
        input_source: "queued_scheduled",
        text: "scheduled input",
      }),
      summary(3, {
        kind: "assistant_text",
        api_call_id: 3,
        text: "turn-ending answer",
        status: "completed",
      }),
      summary(4, {
        kind: "turn_stats",
        start_seq: 1,
        end_seq: 4,
        outcome: "completed",
        duration_ms: 3,
        buckets: {
          direct: { api_call_count: 1, unknown_usage_api_call_count: 0, tool_call_count: 0, subagent_return_count: 0, usage: {} },
          subagent: { api_call_count: 0, unknown_usage_api_call_count: 0, tool_call_count: 0, subagent_return_count: 0, usage: {} },
          total: { api_call_count: 1, unknown_usage_api_call_count: 0, tool_call_count: 0, subagent_return_count: 0, usage: {} },
        },
      }, "ui_only"),
    ];
    // onAction proves action controls require an explicit production owner.
    // onAction 证明动作控件必须拥有显式生产归属。
    const onAction = vi.fn();
    // incompleteMarkup proves a final answer without terminal statistics is not a fork boundary.
    // incompleteMarkup 证明缺少终态统计的最终回答不是分叉边界。
    const incompleteMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer
          events={events.slice(0, -1)}
          onRootInputGenerationAction={onAction}
        />
      </I18nProvider>,
    );
    // markup is visibility evidence; no legacy message identity participates.
    // markup 是可见性证据；不存在旧消息身份参与。
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer
          events={events}
          onRootInputGenerationAction={onAction}
        />
      </I18nProvider>,
    );

    expect(incompleteMarkup).not.toContain('aria-label="分叉会话"');
    expect(markup.match(/aria-label="编辑用户提示词"/g)).toHaveLength(1);
    expect(markup.match(/aria-label="分叉会话"/g)).toHaveLength(1);
    expect(markup).toContain("lucide-pencil");
    expect(markup).toContain("lucide-split");
    expect(seqOffset(markup, 2)).toBeLessThan(seqOffset(markup, 3));
    expect(markup).toContain("editable root");
    expect(markup).toContain("scheduled input");
  });

  it("collects user and generated primary/preview ResourceIds in canonical order", () => {
    const events: readonly ConversationEventSummary[] = [
      summary(1, {
        kind: "user_input",
        client_action_id: clientActionId("renderer-resource-input"),
        input_id: 1,
        input_source: "root",
        text: "render",
        resource_ids: [resourceId(101)],
      }),
      summary(2, {
        kind: "tool_call",
        api_call_id: 1,
      tool_call_id: 1,
      tool_name: "image_generate",
      input: { prompt: "render" },
      request_stream_id: "display-renderer-tool-1",
      status: "completed",
        generated_resources: [
          {
            resource_id: resourceId(201),
            kind: "image",
            preview_resource_id: resourceId(204),
          },
          { resource_id: resourceId(202), kind: "file" },
        ],
      }),
    ];

    expect(collectConversationResourceIdsV1(events)).toEqual([
      resourceId(101),
      resourceId(201),
      resourceId(204),
      resourceId(202),
    ]);
  });

  it("applies every reasoning display mode without reordering canonical events", () => {
    // events contains two reasoning updates in one Turn followed by one assistant response.
    // events 包含同一 Turn 的两条思考更新以及随后的一条 assistant 回复。
    const events: readonly ConversationEventSummary[] = [
      summary(1, {
        kind: "assistant_reasoning",
        api_call_id: 21,
        text: "first reasoning",
        status: "completed",
      }),
      summary(2, {
        kind: "assistant_reasoning",
        api_call_id: 21,
        text: "second reasoning",
        status: "streaming",
      }),
      summary(3, {
        kind: "assistant_text",
        api_call_id: 21,
        text: "answer after reasoning",
        status: "streaming",
      }),
    ];
    // inlineMarkup keeps reasoning in the answer's gutter preview rather than independent rows.
    // inlineMarkup 将思考保留在回答的左侧预览中，而不是独立行。
    const inlineMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer
          events={events}
          reasoningDisplayMode="inline_output"
        />
      </I18nProvider>,
    );
    // Legacy popup intent now preserves both reasoning nodes, with only live content in the portal.
    // 旧弹窗意图现在保留两个思考节点，仅实时内容位于 Portal。
    const firstInlineMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer
          events={events}
          reasoningDisplayMode="first_inline_with_popup"
        />
      </I18nProvider>,
    );
    // popupOnlyMarkup leaves all reasoning to the V1 popup owner while preserving the answer.
    // popupOnlyMarkup 将全部思考交给 V1 浮层，同时保留 assistant 回复。
    const popupOnlyMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer
          events={events}
          reasoningDisplayMode="popup_only"
        />
      </I18nProvider>,
    );

    for (const markup of [inlineMarkup, firstInlineMarkup, popupOnlyMarkup]) {
      expect(seqOffset(markup, 3)).toBeGreaterThan(-1);
      expect(markup).toContain("assistant-flow assistant-with-thinking");
      expect(markup.match(/thinking-preview-trigger compact/g)).toHaveLength(1);
      expect(markup).not.toContain("thinking-block");
      expect(markup).not.toContain("first reasoning");
      expect(markup).not.toContain("second reasoning");
      expect(markup).toContain("answer after reasoning");
      expect(markup).not.toContain("已工作");
    }
  });

  it("connects one hydrated controller to the standalone React test host", async () => {
    // events is the canonical controller window rendered by the host.
    // events 是测试宿主渲染的 canonical 控制器窗口。
    const events: readonly ConversationEventSummary[] = [
      summary(1, {
        kind: "user_input",
        client_action_id: clientActionId("renderer-controller-input"),
        input_id: 1,
        input_source: "root",
        text: "controller-owned input",
      }),
    ];
    // hydration is the complete durable boundary returned by the narrow RPC fixture.
    // hydration 是窄 RPC 夹具返回的完整可靠边界。
    const hydration: ConversationEventHydrationV1Response = {
      session_id: "session-render-host",
      window_generation: 1,
      service_epoch: "service-render-host",
      memory_revision: 1,
      durable_revision: 1,
      events,
      older_before_seq: null,
      has_more: false,
      open_turn_id: 7,
      open_turn_start_seq: 1,
      session_config: {
        current_request: {
          provider: null,
          connection_id: null,
          model: null,
          thinking_level: null,
          temperature: null,
          set_at_ms: null,
        },
        next_request: null,
        context_compaction_window_node_id: null,
      },
    };
    // rpc exposes only the calls required to hydrate this standalone host.
    // rpc 只暴露 hydration 当前独立宿主所需的调用。
    const rpc: ConversationEventWindowRpc = {
      hydrate: async () => hydration,
      page: async () => {
        throw new Error("unexpected page request");
      },
      reconnect: async () => {
        throw new Error("unexpected reconnect request");
      },
      subscribeLive: async () => () => undefined,
    };
    // controller owns the snapshot consumed through useSyncExternalStore.
    // controller 持有通过 useSyncExternalStore 消费的快照。
    const controller = new ConversationEventWindowController(rpc);
    await controller.hydrate(hydration.session_id);
    // markup proves the host reads controller events without a legacy transcript model.
    // markup 证明测试宿主无需旧 transcript 模型即可读取控制器事件。
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1EventWindowView controller={controller} />
      </I18nProvider>,
    );

    expect(markup).toContain("controller-owned input");
    expect(markup).toContain('data-seq="1"');
  });

  it("restores the established transcript hierarchy without exposing structural ledger rows", () => {
    // events models one tool API followed by one final no-tool API in the same logical Turn.
    // events 模拟同一逻辑 Turn 中一个工具 API 以及随后一个无工具最终 API。
    const events: readonly ConversationEventSummary[] = [
      summary(1, {
        kind: "system_context",
        context_key: "goal-mode",
        operation: "upsert",
        scope: "turn",
        provider_role_hint: "developer",
        content: "Continue the active goal.",
      }),
      summary(2, {
        kind: "api_started",
        api_call_id: 31,
        purpose: "main_generation",
        request: { provider: "openai", model: "gpt-v1" },
      }, "host_only"),
      summary(3, {
        kind: "user_input",
        client_action_id: clientActionId("renderer-sequence-input"),
        input_id: 41,
        input_source: "root",
        text: "Inspect the repository",
        resource_ids: [resourceId(101), resourceId(102)],
      }),
      summary(4, {
        kind: "assistant_reasoning",
        api_call_id: 31,
        text: "**Checking** the exact source.",
        status: "completed",
      }),
      summary(5, {
        kind: "assistant_text",
        api_call_id: 31,
        text: "I will inspect the exact source.",
        status: "completed",
      }),
      summary(6, {
        kind: "tool_call",
        api_call_id: 31,
      tool_call_id: 51,
      tool_name: "shell_command",
      input: { command: "rg exact-source" },
      request_stream_id: "display-renderer-tool-51",
      output: { stdout: "one match" },
        status: "completed",
      }),
      summary(7, {
        kind: "api_stats",
        api_call_id: 31,
        purpose: "main_generation",
        provider: "openai",
        model: "gpt-v1",
        thinking_level: "high",
        usage: { input_tokens: 20, output_tokens: 8, total_tokens: 28 },
        status: "completed",
        continuation_eligibility: "forbidden",
        timing: { completed_at_ms: 200 },
      }, "ui_only"),
      summary(8, {
        kind: "api_started",
        api_call_id: 32,
        purpose: "main_generation",
        request: { provider: "openai", model: "gpt-v1" },
      }, "host_only"),
      summary(9, {
        kind: "assistant_text",
        api_call_id: 32,
        text: "The **result** is ready.",
        status: "completed",
      }),
      summary(10, {
        kind: "api_stats",
        api_call_id: 32,
        purpose: "main_generation",
        provider: "openai",
        model: "gpt-v1",
        thinking_level: "high",
        usage: { input_tokens: 24, output_tokens: 6, total_tokens: 30 },
        status: "completed",
        continuation_eligibility: "forbidden",
        timing: { completed_at_ms: 220 },
      }, "ui_only"),
      summary(11, {
        kind: "turn_boundary",
        reason: "completed",
        outcome: "completed",
        purpose: "business",
      }, "provider"),
      summary(12, {
        kind: "turn_stats",
        start_seq: 1,
        end_seq: 11,
        outcome: "completed",
        duration_ms: 1200,
        buckets: {
          direct: {
            api_call_count: 2,
            unknown_usage_api_call_count: 0,
            tool_call_count: 1,
            subagent_return_count: 0,
            usage: {
              uncached_input_tokens: 44,
              output_tokens: 14,
              total_tokens: 58,
              cache_read_tokens: 0,
            },
          },
          subagent: {
            api_call_count: 0,
            unknown_usage_api_call_count: 0,
            tool_call_count: 0,
            subagent_return_count: 0,
            usage: {},
          },
          total: {
            api_call_count: 2,
            unknown_usage_api_call_count: 0,
            tool_call_count: 1,
            subagent_return_count: 0,
            usage: {
              uncached_input_tokens: 44,
              output_tokens: 14,
              total_tokens: 58,
              cache_read_tokens: 0,
            },
          },
        },
      }, "ui_only"),
      summary(13, {
        kind: "display_note",
        notice_kind: "resumed",
        severity: "info",
        reason_code: "connection_resumed",
        redacted_detail: "Connection restored",
      }, "ui_only"),
      summary(14, {
        kind: "request_config_changed",
        previous: { provider: "openai", model: "gpt-v1", thinking_level: "high" },
        current: { provider: "openai", model: "gpt-v2", thinking_level: "max" },
      }, "ui_only"),
      summary(15, {
        kind: "request_config_changed",
        previous: { provider: "openai", model: "gpt-v2", thinking_level: "high" },
        current: { provider: "openai", model: "gpt-v2", thinking_level: "max" },
      }, "ui_only"),
      summary(16, {
        kind: "execution_control",
        control_kind: "recovered",
        api_call_id: 32,
      }, "host_only"),
    ];
    // resourceCatalog is the exact owner-bound metadata projection for the two persisted ids.
    // resourceCatalog 是两个持久化 ID 的精确绑定所有者元数据投影。
    const resourceCatalog: ConversationSessionResourceCatalogV1 = {
      sessionId: "session-render",
      status: "ready",
      resources: new Map([
        [resourceId(101), {
          availability: "available",
          resource_id: resourceId(101),
          original_name: "diagram.png",
          mime: "image/png",
          size: 4096,
        }],
        [resourceId(102), {
          availability: "missing",
          resource_id: resourceId(102),
          placeholder: "[MISSING PROJECT RESOURCE]",
        }],
      ]),
      error: null,
      contentUrl: (currentResourceId) => `http://127.0.0.1/resource/${currentResourceId}`,
    };
    // markup is the standalone V1 renderer output with the native tool card mounted.
    // markup 是挂载原生工具卡后的独立 V1 渲染器输出。
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer
          events={events}
          onCopyText={() => undefined}
          onRootInputGenerationAction={() => undefined}
          resourceCatalog={resourceCatalog}
        />
      </I18nProvider>,
    );
    // Structural facts remain in validation but do not become standalone transcript rows.
    // 结构事实继续参与校验，但不会成为独立转录行。
    expect(seqOffset(markup, 1)).toBe(-1);
    expect(seqOffset(markup, 2)).toBe(-1);
    expect(seqOffset(markup, 7)).toBe(-1);
    expect(seqOffset(markup, 8)).toBe(-1);
    expect(seqOffset(markup, 10)).toBe(-1);
    expect(seqOffset(markup, 11)).toBe(-1);
    expect(seqOffset(markup, 16)).toBe(-1);
    expect(seqOffset(markup, 3)).toBeLessThan(seqOffset(markup, 4));
    expect(seqOffset(markup, 4)).toBeLessThan(seqOffset(markup, 9));
    expect(seqOffset(markup, 9)).toBeLessThan(seqOffset(markup, 12));
    expect(seqOffset(markup, 12)).toBeLessThan(seqOffset(markup, 13));
    expect(seqOffset(markup, 13)).toBeLessThan(seqOffset(markup, 14));
    expect(markup).not.toContain("Continue the active goal.");
    expect(markup).toContain("Inspect the repository");
    expect(markup).toContain("diagram.png");
    expect(markup).toContain("[MISSING PROJECT RESOURCE]");
    expect(markup).not.toContain("资源：101, 102");
    expect(markup).toContain("conversation-v1-process-assistant assistant-with-thinking");
    expect(markup).toContain("thinking-preview-trigger compact");
    expect(markup).not.toContain("Checking");
    expect(markup).toContain("result");
    expect(markup).toContain("已工作");
    expect(markup).not.toContain('class="thinking-block"');
    expect(markup).toContain("思考过程");
    expect(markup).toContain('class="message-row user conversation-v1-event"');
    expect(markup).toContain('class="message-row assistant assistant-final-output conversation-v1-event"');
    expect(markup).toContain('class="tool-presentation-card v1-tool-card');
    expect(markup).toContain("rg exact-source");
    expect(markup).toContain('class="turn-stats-row completed"');
    expect(markup).toContain("回合统计 · API 2 · 工具 1");
    expect(markup).toContain('aria-label="编辑用户提示词"');
    expect(markup).toContain('aria-label="分叉会话"');
    expect(markup).toContain("lucide-pencil");
    expect(markup).toContain("lucide-split");
    expect(markup).not.toContain("回合结束");
    expect(markup).toContain("Connection restored");
    expect(markup).toContain("模型已从 openai / gpt-v1 切换为 openai / gpt-v2。");
    expect(markup).toContain("conversation-model-switch-help");
    expect(markup).toContain("<strong>思考强度</strong><span>high → max</span>");
  });

  it("rejects a missing HostOnly sequence instead of inventing display continuity", () => {
    // events deliberately omits seq 2, which would normally be a HostOnly ApiStarted.
    // events 故意遗漏通常属于 HostOnly ApiStarted 的 seq 2。
    const events: readonly ConversationEventSummary[] = [
      summary(1, {
        kind: "user_input",
        client_action_id: clientActionId("renderer-gap-input"),
        input_id: 1,
        input_source: "root",
        text: "first",
      }),
      summary(3, {
        kind: "assistant_text",
        api_call_id: 1,
        text: "third",
        status: "completed",
      }),
    ];

    expect(() => renderToStaticMarkup(<ConversationV1DisplayRenderer events={events} />))
      .toThrow(/projection gap/);
  });

  it("hides a HostOnly system context only after it has satisfied sequence validation", () => {
    // events proves visibility applies to system context by envelope, not by event kind.
    // events 证明系统上下文的可见性由 envelope 决定，而不是由事件类型决定。
    const events: readonly ConversationEventSummary[] = [
      summary(1, {
        kind: "system_context",
        context_key: "host-secret",
        operation: "upsert",
        scope: "session",
        content: "host-only content",
      }, "host_only"),
      summary(2, {
        kind: "display_note",
        notice_kind: "goal_status",
        severity: "info",
        reason_code: "goal_running",
      }, "ui_only"),
    ];
    // markup includes only the visible note while seq 1 still participated in validation.
    // markup 仅包含可见提示，但 seq 1 仍参与了序列校验。
    const markup = renderToStaticMarkup(<I18nProvider initialLocale="zh-CN"><ConversationV1DisplayRenderer events={events} /></I18nProvider>);

    expect(markup).not.toContain("host-only content");
    expect(markup).toContain('data-seq="2"');
    expect(markup).toContain("目标状态已更新");
    expect(markup).toContain('class="special-block notice"');
    expect(markup).not.toContain(">goal_running<");
  });
});
