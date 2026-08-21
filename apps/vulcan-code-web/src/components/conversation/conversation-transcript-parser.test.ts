import { describe, expect, it } from "vitest";
import { parseConversationBlocks } from "./conversation-transcript-parser";
import type { ConversationMessage } from "@/types";
import type { TranslationKey } from "@/i18n";

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

  // Verifies persisted route identities are localized at render time and keep next-submission semantics.
  // 验证持久化路由身份会在渲染时本地化，并保持下次用户提交语义。
  it("projects model selection records as localized next-submission notices", () => {
    // message contains only language-neutral backend metadata and exact frozen labels.
    // message 仅包含与语言无关的后端元数据及精确冻结标签。
    const message: ConversationMessage = {
      id: "model-switch-1",
      conversationId: "conversation",
      role: "assistant",
      recordKind: "display-note",
      body: "provider_id=command-code\nconnection_id=command-code-account\nmodel_id=deepseek",
      metadata: {
        notice_kind: "model_switch",
        activation: "next_submission",
        provider_id: "command-code",
        provider_name: "Command Code",
        connection_id: "command-code-account",
        connection_name: "主账号",
        model_id: "deepseek",
        model_name: "DeepSeek",
      },
    };
    // translations proves historical records use the active UI locale instead of persisted prose.
    // translations 证明历史记录使用当前界面语言，而不是持久化文案。
    const translations: Partial<Record<TranslationKey, string>> = {
      "conversation.modelSwitch.title": "模型选择",
      "conversation.modelSwitch.nextSubmission": "已选择 {provider} / {account} / {model}；下次提交生效。",
    };

    const [block] = parseConversationBlocks(
      message,
      (key) => translations[key] ?? key,
    );

    expect(block?.kind).toBe("notice");
    expect(block?.title).toBe("模型选择");
    expect(block?.summary).toBe("已选择 Command Code / 主账号 / DeepSeek；下次提交生效。");
    expect(block?.sourceMessageId).toBe("model-switch-1");
  });
});
