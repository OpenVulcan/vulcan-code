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
});
