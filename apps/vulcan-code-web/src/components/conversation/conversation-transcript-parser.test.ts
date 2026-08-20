import { describe, expect, it } from "vitest";
import { parseConversationBlocks } from "./conversation-transcript-parser";
import type { ConversationMessage } from "@/types";

// transcriptMessage builds one exact backend message identity for parser projection tests.
// transcriptMessage 为解析器投影测试构建一个精确后端消息身份。
//
// Parameters: id is the authoritative Store key suffix and body is the visible Markdown source.
// 参数：id 是权威 Store 键后缀，body 是可见 Markdown 源码。
// Returns: one assistant transcript message accepted by the production parser.
// 返回值：一条可由生产解析器接受的助手转录消息。
function transcriptMessage(id: string, body: string): ConversationMessage {
  return {
    id,
    conversationId: "conversation",
    role: "assistant",
    recordKind: "assistant-output",
    body,
  };
}

describe("conversation transcript parser source identity", () => {
  it("keeps the backend message id separate from the derived visual block id", () => {
    const [block] = parseConversationBlocks(transcriptMessage("message-42", "streamed body"));

    expect(block?.id).toBe("message-42-markdown-0");
    expect(block?.sourceMessageId).toBe("message-42");
  });

  // Verifies backend-owned Provider retry metadata selects the retained notice-card path.
  // 验证后端持有的 Provider 重试元数据会选择可保留的提示卡路径。
  it("projects Provider retry display notes as structured notices", () => {
    const message: ConversationMessage = {
      id: "provider-retry-1",
      conversationId: "conversation",
      role: "assistant",
      recordKind: "display-note",
      body: "Vulcan 公共契约 第 1/5 次重试将在 3 秒后开始：Provider unavailable",
      metadata: {
        notice_kind: "provider_retry",
        provider_retry_state: "retrying",
      },
    };

    const [block] = parseConversationBlocks(message);

    expect(block?.kind).toBe("notice");
    expect(block?.status).toBe("retrying");
    expect(block?.sourceMessageId).toBe("provider-retry-1");
    expect(block?.rawMetadata).toEqual(message.metadata);
  });
});
