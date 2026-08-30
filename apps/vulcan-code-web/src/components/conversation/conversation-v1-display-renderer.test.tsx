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
    // inlineMarkup preserves both canonical reasoning events inline.
    // inlineMarkup 在内联区域保留两条 canonical 思考事件。
    const inlineMarkup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ConversationV1DisplayRenderer
          events={events}
          reasoningDisplayMode="inline_output"
        />
      </I18nProvider>,
    );
    // firstInlineMarkup preserves only the first reasoning event in the logical Turn.
    // firstInlineMarkup 仅保留逻辑 Turn 中的第一条思考事件。
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

    expect(seqOffset(inlineMarkup, 1)).toBeLessThan(seqOffset(inlineMarkup, 2));
    expect(seqOffset(inlineMarkup, 2)).toBeLessThan(seqOffset(inlineMarkup, 3));
    expect(firstInlineMarkup).toContain("first reasoning");
    expect(firstInlineMarkup).not.toContain("second reasoning");
    expect(firstInlineMarkup).toContain("answer after reasoning");
    expect(popupOnlyMarkup).not.toContain("first reasoning");
    expect(popupOnlyMarkup).not.toContain("second reasoning");
    expect(popupOnlyMarkup).toContain("answer after reasoning");
    expect(inlineMarkup).toContain('class="thinking-block"');
    expect(inlineMarkup).toContain("思考过程");
    expect(inlineMarkup).not.toContain("处理过程");
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
        outcome: "completed",
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
    expect(markup).toContain("Checking");
    expect(markup).toContain("result");
    expect(markup).toContain("处理过程");
    expect(markup).toContain('class="thinking-block"');
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
    const markup = renderToStaticMarkup(<ConversationV1DisplayRenderer events={events} />);

    expect(markup).not.toContain("host-only content");
    expect(markup).toContain('data-seq="2"');
    expect(markup).toContain("目标状态已更新");
    expect(markup).not.toContain("goal_status");
  });
});
