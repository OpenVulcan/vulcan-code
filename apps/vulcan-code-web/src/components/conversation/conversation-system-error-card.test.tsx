import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { I18nProvider } from "@/i18n";
import type { ConversationMessage } from "@/types";
import {
  isProviderResultReconciliationPending,
  parseSystemProviderError,
  SystemErrorCard,
} from "./conversation-system-error-card";
import { parseConversationBlocks } from "./conversation-transcript-parser";

// SYSTEM_ERROR_BODY mirrors the new JSON public-contract failure shape shown by the user.
// SYSTEM_ERROR_BODY 对应用户提供的新 JSON 公共契约失败正文格式。
const SYSTEM_ERROR_BODY = `Vulcan 公共契约流式执行失败：provider custom:78958aba28114a6babbe55c4e4b5ef1c failure: ${JSON.stringify({
  category: "unclassified",
  stage: "stream",
  dispatch_outcome: "dispatched",
  semantic_output_observed: false,
  side_effect_observed: false,
  terminal: true,
  retryable: false,
  upstream_code: "server_is_overloaded",
  upstream_type: "server_error",
  upstream_message: "upstream detail | key=value",
  request_id: "req-123",
  provider_id: "custom:78958aba28114a6babbe55c4e4b5ef1c",
  entry_id: "entry-1",
  model_id: "model-1",
  message: "Our servers are currently overloaded. Please try again later.",
})}`;

// LEGACY_SYSTEM_ERROR_BODY protects rendering of already persisted text-format failures during migration.
// LEGACY_SYSTEM_ERROR_BODY 保障迁移期间已经持久化的旧文本失败仍可渲染。
const LEGACY_SYSTEM_ERROR_BODY = "Vulcan 公共契约流式执行失败：provider custom:78958aba28114a6babbe55c4e4b5ef1c failure: category=Unclassified | stage=stream | dispatch_outcome=Dispatched | semantic_output_observed=false | retryable=false | upstream_code=server_is_overloaded | Our servers are currently overloaded. Please try again later.";

// EMPTY_MESSAGE_502_BODY mirrors an upstream 502 that supplies retry metadata but no display text.
// EMPTY_MESSAGE_502_BODY 对应携带重试元数据但没有展示文本的上游 502。
const EMPTY_MESSAGE_502_BODY = `Vulcan 公共契约流式执行失败：provider apikeyfan failure: ${JSON.stringify({
  category: "provider_unavailable",
  subcategory: "http",
  stage: "response_headers",
  http_status: 502,
  transport: "http",
  upstream_message: "",
  provider_id: "apikeyfan",
  entry_id: "public-api",
  model_id: "gpt-5.6-luna",
  message: "",
  dispatch_outcome: "dispatched",
  semantic_output_observed: false,
  side_effect_observed: false,
  terminal: true,
  retryable: true,
  retry_after: {
    delay_milliseconds: 60_000,
    source: "header_seconds",
  },
})}。 [provider_route: provider=apikeyfan, model=gpt-5.6-luna, protocol=openai-chat-completions.v1, manifest_revision=0, manifest_generation=2, request_profiles=[], response_profiles=[]]`;

// AGENTSPACE_RECONCILIATION_BODY mirrors the route-aware closed pending-confirmation classification.
// AGENTSPACE_RECONCILIATION_BODY 模拟感知路由的封闭待确认分类。
const AGENTSPACE_RECONCILIATION_BODY = `Vulcan 公共契约流式执行失败：provider agentspace failure: ${JSON.stringify({
  category: "unclassified",
  subcategory: "agentspace_result_reconciliation_required",
  stage: "stream",
  transport: "http",
  upstream_code: "reconciliation_required",
  upstream_type: "invocation.usage_unavailable",
  request_id: "request-1",
  provider_id: "agentspace",
  entry_id: "chatgpt-plan",
  model_id: "gpt-6-astra",
  message: "The request result requires reconciliation.",
  dispatch_outcome: "dispatched",
  semantic_output_observed: false,
  side_effect_observed: false,
  terminal: true,
  retryable: false,
})}`;

describe("system provider error card", () => {
  it("parses canonical provider failure fields without losing the upstream message", () => {
    const error = parseSystemProviderError(SYSTEM_ERROR_BODY);

    expect(error).toMatchObject({
      provider: "custom:78958aba28114a6babbe55c4e4b5ef1c",
      category: "unclassified",
      stage: "stream",
      dispatchOutcome: "dispatched",
      semanticOutputObserved: false,
      retryable: false,
      upstreamCode: "server_is_overloaded",
      upstreamType: "server_error",
      upstreamMessage: "upstream detail | key=value",
      requestId: "req-123",
      message: "Our servers are currently overloaded. Please try again later.",
    });
  });

  it("keeps the legacy text parser available for existing transcript records", () => {
    expect(parseSystemProviderError(LEGACY_SYSTEM_ERROR_BODY)).toMatchObject({
      category: "Unclassified",
      dispatchOutcome: "Dispatched",
      upstreamCode: "server_is_overloaded",
    });
  });

  // Verifies empty upstream messages retain the structured card and exact retry facts.
  // 验证空上游消息仍会保留结构化卡片与精确重试事实。
  it("keeps retryable 502 failures card-ready when message fields are empty", () => {
    expect(parseSystemProviderError(EMPTY_MESSAGE_502_BODY)).toMatchObject({
      provider: "apikeyfan",
      category: "provider_unavailable",
      stage: "response_headers",
      httpStatus: "502",
      retryable: true,
      retryAfterMs: "60000",
      message: "HTTP 502 · provider_unavailable · response_headers",
    });
  });

  it("projects a terminal system transcript into the notice card path", () => {
    const message: ConversationMessage = {
      id: "system-error-message",
      conversationId: "conversation",
      role: "system",
      recordKind: "transcript",
      body: SYSTEM_ERROR_BODY,
    };
    const [block] = parseConversationBlocks(message);

    expect(block?.kind).toBe("notice");
    expect(block?.status).toBe("failed");
    expect(block?.summary).toBe("Our servers are currently overloaded. Please try again later.");
  });

  it("renders AgentSpace reconciliation as pending result and usage without retry", () => {
    const error = parseSystemProviderError(AGENTSPACE_RECONCILIATION_BODY);
    expect(error).not.toBeNull();
    expect(isProviderResultReconciliationPending(error!)).toBe(true);

    const message: ConversationMessage = {
      id: "agentspace-pending",
      conversationId: "conversation",
      role: "system",
      recordKind: "transcript",
      body: AGENTSPACE_RECONCILIATION_BODY,
    };
    const [block] = parseConversationBlocks(message);
    // Pending provider reconciliation remains active in the shared three-state block lifecycle.
    // Provider 待核对状态在共享三态会话块生命周期中保持为进行中。
    expect(block?.status).toBe("running");

    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <SystemErrorCard error={error!} />
      </I18nProvider>,
    );
    expect(markup).toContain("结果与费用待确认");
    expect(markup).toContain("系统不会自动重发");
    expect(markup).toContain("请求标识");
    expect(markup).toContain("request-1");
    expect(markup).toContain("上游错误载荷");
    expect(markup).toContain("结果状态");
    expect(markup).toContain("费用状态");
    expect(markup).toContain("system-error-card-pending");
  });

  // Verifies a pending AgentSpace result keeps a missing upstream identifier explicit.
  // 验证 AgentSpace 待确认结果会显式展示供应商未提供上游标识。
  it("renders a missing AgentSpace request identifier explicitly", () => {
    // body removes only the exact optional request identifier from the canonical fixture.
    // body 只从规范夹具中移除精确的可选请求标识。
    const body = AGENTSPACE_RECONCILIATION_BODY.replace('"request_id":"request-1",', "");
    // error remains a valid pending classification because request identity is observable metadata.
    // error 仍是有效待确认分类，因为请求标识属于可观察元数据。
    const error = parseSystemProviderError(body);
    expect(error).not.toBeNull();
    expect(isProviderResultReconciliationPending(error!)).toBe(true);

    // markup must not invent a response ID or another provider identifier.
    // markup 不得虚构 response ID 或其他供应商标识。
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <SystemErrorCard error={error!} />
      </I18nProvider>,
    );
    expect(markup).toContain("请求标识");
    expect(markup).toContain("供应商未提供");
  });

  it("renders a compact summary with an expandable JSON detail block", () => {
    const error = parseSystemProviderError(SYSTEM_ERROR_BODY);
    expect(error).not.toBeNull();

    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <SystemErrorCard error={error!} />
      </I18nProvider>,
    );

    expect(markup).toContain("system-error-card");
    expect(markup).toContain("系统错误");
    expect(markup).toContain("server_is_overloaded");
    expect(markup).toContain("查看原始错误");
    expect(markup).toContain("system-error-chevron");
    expect(markup).toContain("language-json");
    expect(markup).toContain("Our servers are currently overloaded. Please try again later.");
  });
});
