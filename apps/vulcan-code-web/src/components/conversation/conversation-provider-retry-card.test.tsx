import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { I18nProvider } from "@/i18n";
import type { ConversationBlock } from "./conversation-model";
import {
  providerRetryDisplayState,
  ProviderRetryCard,
  type ProviderRetryDisplayState,
} from "./conversation-provider-retry-card";

// PROVIDER_RETRY_BLOCK preserves the original Provider failure reason below the lifecycle label.
// PROVIDER_RETRY_BLOCK 在生命周期标签下保留原始 Provider 失败原因。
const PROVIDER_RETRY_BLOCK: ConversationBlock = {
  id: "provider-retry-1",
  sourceMessageId: "provider-retry-1",
  kind: "notice",
  title: "Provider retry",
  body: "Anthropic retry 1/5 starts in 3 seconds: Provider unavailable",
  summary: "Provider unavailable",
  status: "retrying",
};

// RETRY_LABELS binds each authoritative lifecycle state to its Chinese user-facing label.
// RETRY_LABELS 将每个权威生命周期状态绑定到中文用户界面标签。
const RETRY_LABELS: Record<ProviderRetryDisplayState, string> = {
  retrying: "正在重试",
  succeeded: "重试成功",
  failed: "重试失败",
};

describe("Provider retry card", () => {
  // Verifies the card persists one exact lifecycle state and does not resemble assistant Markdown.
  // 验证卡片会保留一个精确生命周期状态，且不会呈现为助手 Markdown。
  it.each(Object.entries(RETRY_LABELS) as [ProviderRetryDisplayState, string][])(
    "renders the %s lifecycle state",
    (state, label) => {
      const markup = renderToStaticMarkup(
        <I18nProvider initialLocale="zh-CN">
          <ProviderRetryCard block={PROVIDER_RETRY_BLOCK} state={state} />
        </I18nProvider>,
      );

      expect(markup).toContain("provider-retry-card");
      expect(markup).toContain(`data-state="${state}"`);
      expect(markup).toContain(label);
      expect(markup).toContain("Provider unavailable");
    },
  );

  // Verifies only the backend retry lifecycle field can select this dedicated renderer.
  // 验证只有后端重试生命周期字段能够选择这个专用渲染器。
  it("rejects unrelated and unsupported metadata", () => {
    expect(providerRetryDisplayState({ provider_retry_state: "retrying" })).toBe("retrying");
    expect(providerRetryDisplayState({ provider_retry_state: "completed" })).toBeNull();
    expect(providerRetryDisplayState({ status: "retrying" })).toBeNull();
  });
});
