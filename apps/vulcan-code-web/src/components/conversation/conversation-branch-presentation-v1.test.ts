import { describe, expect, it } from "vitest";

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
