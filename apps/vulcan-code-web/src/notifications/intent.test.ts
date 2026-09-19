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
    notificationClientId: "client-1",
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
    expect(completed.intentId).toBe("notification:run-1");
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

  it("keeps a real run UUID within the native notification identity limit", () => {
    // event carries the full conversation and run UUIDs emitted by Core.
    // event 携带 Core 实际发出的完整会话 UUID 与运行 UUID。
    const event = settledEvent("completed");
    event.conversationId = "a9c1327b-85b4-4e26-b395-2f258d892e4c";
    event.runId = "7602a51b-dcca-40d7-9b47-3e17ff4c35e0";
    event.eventId = `${event.conversationId}:${event.runId}:settled`;
    const intent = createConversationNotificationIntent(event, null);
    expect(intent.intentId).toBe(`notification:${event.runId}`);
    expect(intent.intentId).toMatch(/^[A-Za-z0-9:._-]{1,64}$/);
    expect(intent.sourceEventId).toBe(event.eventId);
  });
});
