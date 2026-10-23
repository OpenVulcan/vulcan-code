import { describe, expect, it } from "vitest";
import zhCNTranslations from "@catalogs/webui/zh-CN.json";
import { parseConversationBlocks } from "./conversation-transcript-parser";
import { parseSystemProviderError } from "./conversation-system-error-card";

import { projectConversationBranchPresentationV1 } from "./conversation-branch-presentation-v1";
import type { ConversationEventSummary } from "@/service/conversation-event-window-store";
import type { ConversationSessionResourceCatalogV1 } from "@/state/conversation-session-resources-v1";

// EMPTY_RESOURCES is the exact owner-bound empty catalog used by request-configuration tests.
// EMPTY_RESOURCES 是请求配置测试使用的精确绑定所有者空目录。
const EMPTY_RESOURCES: ConversationSessionResourceCatalogV1 = {
  contentUrl: () => "",
  error: null,
  resources: new Map(),
  sessionId: "session-request-config",
  status: "ready",
};

/// Builds one complete V1 envelope for a request-configuration event.
/// 为一条请求配置事件构建完整 V1 envelope。
function requestConfigSummary(
  event: Extract<ConversationEventSummary["event"], { kind: "request_config_changed" }>,
): ConversationEventSummary {
  return {
    created_at_ms: 1,
    event,
    event_revision: 1,
    seq: 1,
    turn_id: null,
    updated_at_ms: 1,
    visibility: "ui_only",
  };
}

describe("V1 branch request configuration presentation", () => {
  // A pre-API host failure must retain its original message without claiming unknown dispatch facts.
  // API 前宿主失败必须保留原始消息，不得声称未知的派发事实。
  it("does not manufacture dispatched provider identity for a host failure", () => {
    const presentation = projectConversationBranchPresentationV1("session-request-config", [{
      seq: 1, turn_id: 1, created_at_ms: 1, updated_at_ms: 1, event_revision: 1, visibility: "ui_only",
      event: { kind: "display_note", notice_kind: "system_error", severity: "error", reason_code: "stream_finished_without_v1_api_terminal", redacted_detail: "Original request preparation failed." },
    }], EMPTY_RESOURCES, (key) => zhCNTranslations[key]);
    const error = parseSystemProviderError(presentation.messages[0]!.body)!;
    expect(error.message).toBe("Original request preparation failed.");
    expect(error.dispatchOutcome).toBe("unknown");
    expect(error.structured).not.toHaveProperty("provider_id");
    expect(error.structured).not.toHaveProperty("model_id");
  });

  // The runtime's existing public-contract failure is the sole authority for the expanded JSON card.
  // 运行时既有公共契约失败是展开 JSON 错误卡的唯一权威。
  it("preserves structured producer errors without nesting or losing their route", () => {
    const detail = 'Provider streaming execution failed: provider example failure: {"category":"invalid_request","stage":"prepare","dispatch_outcome":"not_dispatched","semantic_output_observed":false,"side_effect_observed":false,"terminal":true,"retryable":false,"provider_id":"example","model_id":"model-a","message":"Exact original failure."}';
    const presentation = projectConversationBranchPresentationV1("session-request-config", [{
      seq: 1, turn_id: 1, created_at_ms: 1, updated_at_ms: 1, event_revision: 1, visibility: "ui_only",
      event: { kind: "display_note", notice_kind: "system_error", severity: "error", reason_code: "stream_finished_without_v1_api_terminal", redacted_detail: detail },
    }], EMPTY_RESOURCES, (key) => zhCNTranslations[key]);
    expect(presentation.messages[0]!.body).toBe(detail);
    expect(parseSystemProviderError(presentation.messages[0]!.body)?.modelId).toBe("model-a");
    expect(parseConversationBlocks(presentation.messages[0]!)[0]?.kind).toBe("notice");
  });
  // Tool persistence failures after a successful API retain applied effects in the permanent error card.
  // API 成功后工具持久化失败时，永久错误卡保留已应用副作用。
  it("retains applied file effects after the provider request has already completed", () => {
    // This is the registered public-error format carried by the terminal report into redacted_detail.
    // 这是由终态报告传入 redacted_detail 的已登记公共错误格式。
    const detail = 'Provider streaming execution failed: provider `provider-a` failure: ' + JSON.stringify({
      category: "unclassified", subcategory: "tool_result_persistence", stage: "terminal",
      dispatch_outcome: "dispatched", semantic_output_observed: false, side_effect_observed: true,
      terminal: true, retryable: false, provider_id: "provider-a", entry_id: "entry-a", model_id: "model-a",
      upstream_code: "tool_result_persistence_failed", upstream_message: "duplicate resource identifier",
      message: "The file changes were already applied. Do not repeat the operation to recover its receipt.",
    });
    const presentation = projectConversationBranchPresentationV1("session-request-config", [{
      seq: 1, turn_id: 1, created_at_ms: 1, updated_at_ms: 1, event_revision: 1, visibility: "ui_only",
      event: { kind: "display_note", notice_kind: "system_error", severity: "error", reason_code: "stream_failed_after_v1_api_terminal", redacted_detail: detail },
    }], EMPTY_RESOURCES, (key) => zhCNTranslations[key]);
    const error = parseSystemProviderError(presentation.messages[0]!.body)!;
    expect(error.sideEffectObserved).toBe(true);
    expect(error.retryable).toBe(false);
    expect(error.upstreamCode).toBe("tool_result_persistence_failed");
    expect(error.message).toContain("already applied");
    expect(error.modelId).toBe("model-a");
  });

  // The real branch adapter and parser must select the notice-card path for historical Goal events too.
  // 真实分支适配器与解析器也必须为历史目标事件选择提示卡路径。
  it("routes goal states into localized notice blocks", () => {
    const presentation = projectConversationBranchPresentationV1("session-request-config", [{
      seq: 1, turn_id: null, created_at_ms: 1, updated_at_ms: 1, event_revision: 1, visibility: "ui_only",
      event: {kind: "display_note", notice_kind: "goal_status", severity: "info", reason_code: "goal_resumed"},
    }], EMPTY_RESOURCES, (key) => zhCNTranslations[key]);
    const message = presentation.messages[0]!;
    expect(message.body).toContain("目标已恢复");
    expect(message.body).not.toContain("goal_resumed");
    expect(parseConversationBlocks(message)[0]?.kind).toBe("notice");
  });
  // A memory recall becomes one work-process notice carrying its queries, memories and failure state.
  // 记忆召回成为一条工作过程提示，携带检索词、记忆与失败状态。
  it("routes memory recall records into notice blocks", () => {
    const t = (key: keyof typeof zhCNTranslations) => zhCNTranslations[key];
    const recall = (event: Extract<ConversationEventSummary["event"], { kind: "memory_recall" }>) =>
      projectConversationBranchPresentationV1("session-request-config", [{
        seq: 1, turn_id: 1, created_at_ms: 1, updated_at_ms: 1, event_revision: 1, visibility: "ui_only", event,
      }], EMPTY_RESOURCES, t).messages[0]!;
    const matched = recall({ kind: "memory_recall", outcome: "matched", queries: ["deploy `steps`"], memories: [{ memory_id: 7, text: "Use blue-green deploys." }] });
    const [block] = parseConversationBlocks(matched);
    expect(block?.kind).toBe("notice");
    expect(block?.title).toBe(t("conversation.memoryRecall.title"));
    expect(block?.status).toBe("completed");
    expect(matched.body).toContain("`` deploy `steps` ``");
    expect(matched.body).toContain("Use blue-green deploys.");
    const failed = recall({ kind: "memory_recall", outcome: "failed" });
    expect(parseConversationBlocks(failed)[0]?.status).toBe("failed");
    expect(failed.body).toBe(t("conversation.memoryRecall.failedDetail"));
  });
  // Prompt injections become categorized notices carrying the exact model text; automatic turn-end
  // expiry and memory text already shown by the recall card stay hidden.
  // 提示词注入成为带模型原文的分类提示；回合结束的自动失效与召回卡已显示的记忆正文保持隐藏。
  it("routes prompt injections into categorized notice blocks", () => {
    type SystemContext = Extract<ConversationEventSummary["event"], { kind: "system_context" }>;
    const context = (seq: number, turn: number, event: Partial<SystemContext>): ConversationEventSummary => ({
      seq, turn_id: turn, created_at_ms: seq, updated_at_ms: seq, event_revision: 1, visibility: "provider",
      event: { kind: "system_context", context_key: "plan_mode", operation: "upsert", scope: "session",
        content: "Plan mode is active. Do not edit files.", ...event },
    });
    const presentation = projectConversationBranchPresentationV1("session-context", [
      context(1, 1, {}),
      context(2, 1, { context_key: "dynamic.provider_tools", content: "Model extensions changed: image_generate uses ```fixed```." }),
      context(3, 1, { context_key: "continuation_guidance/a", operation: "remove", scope: "turn", content: "expired" }),
      { seq: 4, turn_id: 2, created_at_ms: 4, updated_at_ms: 4, event_revision: 1, visibility: "ui_only",
        event: { kind: "memory_recall", outcome: "matched", memories: [{ memory_id: 1, text: "Remembered." }] } },
      context(5, 2, { context_key: "injection.transient", scope: "turn", content: "Remembered." }),
      context(6, 2, { operation: "remove", content: "Plan mode ended." }),
    ], EMPTY_RESOURCES, (key) => key);
    const meta = (message: (typeof presentation.messages)[number]) => message.metadata as Record<string, unknown> | undefined;
    const notices = presentation.messages.filter((message) => meta(message)?.notice_kind === "context_injection");
    expect(notices.map((message) => meta(message)?.context_category)).toEqual(["plan", "tools", "plan"]);
    expect(notices.map((message) => meta(message)?.summary)).toEqual([
      "conversation.contextInjection.upsert", "conversation.contextInjection.upsert", "conversation.contextInjection.remove",
    ]);
    const [block] = parseConversationBlocks(notices[1]!);
    expect(block?.kind).toBe("notice");
    // The fence outlasts backtick runs inside the content, so the exact text stays one code block.
    // 围栏长于正文中的反引号串，使原文保持为一个代码块。
    expect(notices[1]!.body).toContain("````text\nModel extensions changed: image_generate uses ```fixed```.\n````");
  });
  // Parameters: none. Returns: none; tests the actual branch adapter's initial suppression and display-name fields.
  // 参数：无。返回值：无；测试真实分支适配器对首次提示的抑制以及展示名称字段。
  it("projects configured names and omits initial route notices in the branch UI", () => {
    // providers includes account identity that must never appear in the switch sentence.
    // providers 包含绝不能出现在切换句子中的账号身份。
    const providers = [{ id: "credential-id", sourceProviderId: "provider-a", providerDisplayName: "供应商甲", name: "供应商甲 · 账号", status: "ready", models: [{ id: "model-a", name: "模型甲" }] }];
    // presentation uses both historical initialization and a real later switch.
    // presentation 同时使用历史首次初始化与之后的真实切换。
    const presentation = projectConversationBranchPresentationV1("session-request-config", [
      requestConfigSummary({ kind: "request_config_changed", previous: {}, current: { provider: "provider-a", model: "model-a" } }),
      { ...requestConfigSummary({ kind: "request_config_changed", previous: { provider: "provider-a", model: "model-a", connection_id: "credential-id" }, current: { provider: "removed-provider", model: "other-model" } }), seq: 2 },
    ], EMPTY_RESOURCES, (key) => key, providers);
    expect(presentation.messages).toHaveLength(1);
    expect(presentation.messages[0]?.metadata).toMatchObject({ notice_kind: "model_switch", previous_model_label: "供应商甲 / 模型甲", current_model_label: "removed-provider / other-model" });
    expect(JSON.stringify(presentation.messages)).not.toContain("credential-id");
  });

  // Thinking-only changes must stay out of the model-switch category used by the restored branch UI.
  // 仅思考强度变化不得进入恢复后分支 UI 使用的模型切换类别。
  it("classifies thinking-only changes without emitting a model switch", () => {
    const presentation = projectConversationBranchPresentationV1(
      "session-request-config",
      [requestConfigSummary({
        kind: "request_config_changed",
        previous: { provider: "provider-a", connection_id: "account-a", model: "model-a", thinking_level: "high" },
        current: { provider: "provider-a", connection_id: "account-a", model: "model-a", thinking_level: "max" },
      })],
      EMPTY_RESOURCES,
      (key) => key,
    );

    expect(presentation.messages).toHaveLength(1);
    expect(presentation.messages[0]?.metadata).toMatchObject({
      notice_kind: "reasoning_effort_switch",
      previous_thinking_level: "high",
      current_thinking_level: "max",
    });
    expect(presentation.messages[0]?.metadata).not.toMatchObject({ notice_kind: "model_switch" });
  });
});
