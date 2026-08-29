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
