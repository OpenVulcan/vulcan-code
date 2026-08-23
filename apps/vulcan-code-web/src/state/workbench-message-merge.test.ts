import { describe, expect, it } from "vitest";

import { patchConversationMessages } from "@/state/workbench-message-merge";
import type { ConversationMessage } from "@/types";

// message builds one minimal timeline row for exact patch-order assertions.
// message 构建一条最小时间线消息，用于精确补丁顺序断言。
//
// Parameters: id is the stable row identity, conversationId is its owner, and body is visible text.
// 参数：id 是稳定行标识，conversationId 是所属会话，body 是可见正文。
// Returns: one complete ConversationMessage test value.
// 返回值：返回一个完整的 ConversationMessage 测试值。
function message(id: string, conversationId: string, body: string): ConversationMessage {
  return {
    id,
    conversationId,
    role: "assistant",
    body,
  };
}

// Verifies the upsert-only hot path replaces exact rows and appends new rows without rebuilding order.
// 验证仅写入热路径会替换精确行并追加新行，且不会重建顺序。
describe("patchConversationMessages exact upserts", () => {
  it("updates one tool card and appends the next card inside the owning conversation", () => {
    const unrelatedBefore = message("other-before", "other", "before");
    const firstTool = message("tool-1", "target", "receiving");
    const assistantStream = message("assistant-stream", "target", "streaming");
    const unrelatedAfter = message("other-after", "other", "after");

    const patched = patchConversationMessages(
      [unrelatedBefore, firstTool, assistantStream, unrelatedAfter],
      {
        conversationId: "target",
        upsertMessages: [
          message("tool-1", "target", "ready"),
          message("tool-2", "target", "receiving"),
        ],
        removeMessageIds: [],
        messageOrder: null,
      },
    );

    expect(patched.map((entry) => `${entry.id}:${entry.body}`)).toEqual([
      "other-before:before",
      "tool-1:ready",
      "assistant-stream:streaming",
      "tool-2:receiving",
      "other-after:after",
    ]);
    expect(patched[0]).toBe(unrelatedBefore);
    expect(patched[2]).toBe(assistantStream);
    expect(patched[4]).toBe(unrelatedAfter);
  });
});
