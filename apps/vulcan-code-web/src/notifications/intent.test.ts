import { describe, expect, it } from "vitest";
import { createConversationNotificationIntent } from "@/notifications/intent";
import type { ConversationRunSettledEvent } from "@/types";

// settledEvent builds one exact lifecycle event for intent tests.
// settledEvent 为意图测试构建一个精确生命周期事件。
function settledEvent(outcome: ConversationRunSettledEvent["outcome"]): ConversationRunSettledEvent {
  return {
    eventId: "conversation-1:run-1:settled",
    conversationId: "conversation-1",
    projectId: "project-1",
    runId: "run-1",
    outcome,
    occurredAt: "1000",
  };
}

describe("createConversationNotificationIntent", () => {
  it("builds deterministic completed and failed intent identities", () => {
    // completed is the privacy-safe success intent under test.
    // completed 是待测试的隐私安全成功意图。
    const completed = createConversationNotificationIntent(settledEvent("completed"), null);
    // failed is the title-bearing failure intent under test.
    // failed 是待测试的含标题失败意图。
    const failed = createConversationNotificationIntent(settledEvent("failed"), "  Build docs  ");
    expect(completed.intentId).toBe("notification:conversation-1:run-1:settled");
    expect(completed.category).toBe("conversation_completed");
    expect(completed.privacyLevel).toBe("generic");
    expect(completed.expiresAt).toBe(String(1000 + 24 * 60 * 60 * 1000));
    expect(failed.category).toBe("conversation_failed");
    expect(failed.priority).toBe("high");
    expect(failed.localizationArgs).toEqual({ conversationTitle: "Build docs" });
    expect(failed.route).toEqual({
      kind: "conversation",
      projectId: "project-1",
      conversationId: "conversation-1",
    });
  });
});
