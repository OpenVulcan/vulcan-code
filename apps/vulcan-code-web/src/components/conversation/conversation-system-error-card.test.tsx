// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { FeedbackCenterContext, type FeedbackCenterController } from "@/components/features/feedback/feedback-center-context";
import { I18nProvider } from "@/i18n";
import type { ConversationMessage } from "@/types";
import {
  isProviderResultReconciliationPending,
  parseSystemProviderError,
  SystemErrorCard,
} from "./conversation-system-error-card";
import ProviderErrorActions, { isReportableSystemError } from "./conversation-system-error-actions";
import ProviderErrorPanel from "./conversation-system-error-panel";
import { parseConversationBlocks } from "./conversation-transcript-parser";

// Capture the exact append request while leaving the persisted settings untouched.
// 捕获精确的追加请求，同时保持持久化设置不变。
const retryService = vi.hoisted(() => ({
  request: vi.fn<(method: string, params: { rule: string }) => Promise<null>>().mockResolvedValue(null),
}));
vi.mock("@/service/webui-service-runtime", () => ({
  hasServiceRuntime: () => true,
  serviceClient: retryService,
}));

// SYSTEM_ERROR_BODY mirrors the new JSON public-contract failure shape shown by the user.
// SYSTEM_ERROR_BODY 对应用户提供的新 JSON 公共契约失败正文格式。
const SYSTEM_ERROR_BODY = `Provider streaming execution failed: provider custom:78958aba28114a6babbe55c4e4b5ef1c failure: ${JSON.stringify({
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
const LEGACY_SYSTEM_ERROR_BODY = "Provider streaming execution failed: provider custom:78958aba28114a6babbe55c4e4b5ef1c failure: category=Unclassified | stage=stream | dispatch_outcome=Dispatched | semantic_output_observed=false | retryable=false | upstream_code=server_is_overloaded | Our servers are currently overloaded. Please try again later.";

// EMPTY_MESSAGE_502_BODY mirrors an upstream 502 that supplies retry metadata but no display text.
// EMPTY_MESSAGE_502_BODY 对应携带重试元数据但没有展示文本的上游 502。
const EMPTY_MESSAGE_502_BODY = `Provider streaming execution failed: provider apikeyfan failure: ${JSON.stringify({
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
const AGENTSPACE_RECONCILIATION_BODY = `Provider streaming execution failed: provider agentspace failure: ${JSON.stringify({
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
  // Verify that the card generates a portable regex from both historical and current timeout diagnostics.
  // 验证错误卡可以从历史及当前超时诊断生成可复用的正则。
  it("adds a retry rule that matches response-header timeouts across endpoints", async () => {
    // Use the structured V1 error shape with the historical placeholder as the source.
    // 以带有历史占位符的 V1 结构化错误为来源。
    const original = "Provider request to `<redacted-url>` exceeded the registered 60000 ms response-header timeout.";
    // Parse the same provider error body that supplies the action in a conversation.
    // 解析会话中为该操作提供数据的同一种供应商错误正文。
    const error = parseSystemProviderError(`Provider streaming execution failed: provider alibaba failure: ${JSON.stringify({
      category: "provider",
      stage: "terminal",
      dispatch_outcome: "dispatched",
      semantic_output_observed: false,
      retryable: false,
      upstream_code: "provider_error",
      upstream_message: original,
      provider_id: "alibaba",
      message: original,
    })}`)!;
    // Mount the real card and capture copies without writing to the system clipboard.
    // 挂载真实卡片，并捕获复制内容而不写入系统剪贴板。
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const host = document.createElement("div");
    const root = createRoot(host);
    const writeText = vi.fn<(value: string) => Promise<void>>().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    try {
      await act(async () => root.render(<I18nProvider initialLocale="en-US"><SystemErrorCard error={error} /></I18nProvider>));
      await act(async () => vi.dynamicImportSettled());
      expect(host.querySelector("pre")).toBeNull();
      await act(async () => host.querySelector<HTMLButtonElement>(".vk-error-summary button")!.click());
      expect(JSON.parse(writeText.mock.calls[0][0])).toEqual(error.structured);
      expect(host.querySelector("pre")).toBeNull();
      await act(async () => host.querySelector<HTMLButtonElement>(".vk-error-actions button")!.click());
      expect(retryService.request).toHaveBeenCalledExactlyOnceWith(
        "service_add_provider_retry_rule", { rule: expect.stringMatching(/^regex:/) },
      );
      // The persisted expression must match both historical and current timeout messages.
      // 持久化表达式必须同时匹配历史和当前的超时消息。
      const [, { rule }] = retryService.request.mock.calls[0];
      const matcher = new RegExp(rule.slice("regex:".length));
      expect(matcher.test(original)).toBe(true);
      expect(matcher.test("Provider request to `https://a.invalid/v1/chat` exceeded the registered 1000 ms response-header timeout.")).toBe(true);
      expect(matcher.test("Provider request to `https://a.invalid/v1/chat` failed: unexpected response")).toBe(false);
      await act(async () => host.querySelector<HTMLButtonElement>(".vk-error-footer button[aria-expanded]")!.click());
      expect(JSON.parse(host.querySelector("pre")!.textContent!)).toEqual(error.structured);
    } finally {
      await act(async () => root.unmount());
      Reflect.deleteProperty(navigator, "clipboard");
      vi.unstubAllGlobals();
    }
  });
  // English runtime errors retain structured diagnostics with and without route context.
  // 英文运行时错误无论是否携带路由上下文，都保留结构化诊断。
  it("parses runtime failures with and without route context", () => {
    for (const original of [SYSTEM_ERROR_BODY, EMPTY_MESSAGE_502_BODY]) {
      // English mirrors the exact new runtime wrapper and suffix separator.
      // english 对应准确的新运行时外壳及后缀分隔符。
      const english = original.replace("。 [provider_route: ", " [provider_route: ");
      expect(parseSystemProviderError(english)).toEqual({ ...parseSystemProviderError(original), raw: english });
      expect(parseConversationBlocks({
        id: "english-error", conversationId: "conversation", role: "system", recordKind: "transcript", body: english,
      })[0]?.kind).toBe("notice");
    }
  });
  // Quoted legacy markers remain payload text when the real suffix uses the English format.
  // 当真实后缀使用英文格式时，被引用的旧标记仍属于载荷正文。
  it("does not truncate an upstream message containing a legacy route marker", () => {
    const detail = "An upstream message quoted 。 [provider_route: old] verbatim.";
    const body = SYSTEM_ERROR_BODY
      .replace("upstream detail | key=value", detail)
      + " [provider_route: provider=current, model=current-model]";
    expect(parseSystemProviderError(body)?.upstreamMessage).toBe(detail);
  });
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
    // The retry wait renders as seconds with one decimal, never as raw milliseconds.
    // 重试等待时长渲染为保留一位小数的秒数，而不是原始毫秒。
    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ProviderErrorPanel error={parseSystemProviderError(EMPTY_MESSAGE_502_BODY)!} />
      </I18nProvider>,
    );
    expect(markup).toContain("60.0 s");
    expect(markup).not.toContain("60000 ms");
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
        <ProviderErrorPanel error={error!} />
      </I18nProvider>,
    );
    expect(markup).toContain("结果与费用待确认");
    expect(markup).toContain("系统不会自动重发");
    expect(markup).toContain("请求标识");
    expect(markup).toContain("request-1");
    expect(markup).toContain("上游错误载荷");
    expect(markup).toContain("结果状态");
    expect(markup).toContain("费用状态");
    expect(markup).toContain("data-pending");
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
        <ProviderErrorPanel error={error!} />
      </I18nProvider>,
    );
    expect(markup).toContain("请求标识");
    expect(markup).toContain("供应商未提供");
  });

  it("renders a compact summary with the raw diagnostic hidden", () => {
    const error = parseSystemProviderError(SYSTEM_ERROR_BODY);
    expect(error).not.toBeNull();

    const markup = renderToStaticMarkup(
      <I18nProvider initialLocale="zh-CN">
        <ProviderErrorPanel error={error!} />
      </I18nProvider>,
    );

    expect(markup).toContain("vk-error-card");
    expect(markup).toContain("系统错误");
    expect(markup).toContain("server_is_overloaded");
    expect(markup).toContain("查看原始错误");
    expect(markup).toContain("vk-error-chev");
    expect(markup).not.toContain("language-json");
    expect(markup).toContain("Our servers are currently overloaded. Please try again later.");
  });

  it("offers a report only for failures that may come from Vulcan Code", () => {
    const CUSTOM = "custom:78958aba28114a6babbe55c4e4b5ef1c";
    // failure rewrites the canonical body to one provider, category, HTTP status and dispatch outcome.
    // failure 把规范正文改写为指定供应商、类别、HTTP 状态与派发结果。
    const failure = (provider: string, category: string, status: number | null, dispatch = "dispatched") => SYSTEM_ERROR_BODY
      .replaceAll(CUSTOM, provider)
      .replace('"category":"unclassified"', `"category":"${category}"${status === null ? "" : `,"http_status":${status}`}`)
      .replace('"dispatch_outcome":"dispatched"', `"dispatch_outcome":"${dispatch}"`);
    const reportButton = (body: string) => renderToStaticMarkup(
      <I18nProvider initialLocale="en-US">
        <FeedbackCenterContext.Provider value={{ openFeedback: () => undefined } as unknown as FeedbackCenterController}>
          <ProviderErrorActions error={parseSystemProviderError(body)!} />
        </FeedbackCenterContext.Provider>
      </I18nProvider>,
    ).includes("vk-error-report");

    // System providers: request-shape and unknown failures stay reportable; account, provider and network ones do not.
    // 系统供应商：请求形状与未知失败仍可报告；账号、供应商与网络失败不可报告。
    expect(reportButton(failure("deepseek", "unclassified", null))).toBe(true);
    expect(reportButton(failure("deepseek", "invalid_request", 400))).toBe(true);
    expect(reportButton(failure("deepseek", "context_length_exceeded", 400))).toBe(true);
    expect(reportButton(failure("deepseek", "rate_limited", 429))).toBe(false);
    expect(reportButton(failure("deepseek", "payment_required", 429))).toBe(false);
    expect(reportButton(failure("deepseek", "provider_unavailable", null, "unknown"))).toBe(false);
    expect(reportButton(failure("deepseek", "timeout", null, "unknown"))).toBe(false);
    expect(reportButton(EMPTY_MESSAGE_502_BODY)).toBe(false);

    // Custom providers: whatever the endpoint answered is configuration; only a failure before sending is reportable.
    // 自定义供应商：地址返回的任何错误都属于配置；只有发送前的失败可报告。
    expect(reportButton(SYSTEM_ERROR_BODY)).toBe(false);
    expect(reportButton(failure(CUSTOM, "invalid_request", 404))).toBe(false);
    expect(reportButton(failure(CUSTOM, "context_length_exceeded", 400))).toBe(false);
    expect(reportButton(failure(CUSTOM, "invalid_request", null, "not_dispatched"))).toBe(true);
    expect(reportButton(failure(CUSTOM, "rate_limited", null, "not_dispatched"))).toBe(false);

    // Legacy text records spell categories and outcomes in PascalCase.
    // 旧文本记录以 PascalCase 书写类别与派发结果。
    const legacy = (body: string) => isReportableSystemError(parseSystemProviderError(body)!);
    expect(legacy(LEGACY_SYSTEM_ERROR_BODY.replace("category=Unclassified", "category=RateLimited"))).toBe(false);
    expect(legacy(LEGACY_SYSTEM_ERROR_BODY)).toBe(false);
    expect(legacy(LEGACY_SYSTEM_ERROR_BODY.replace("dispatch_outcome=Dispatched", "dispatch_outcome=NotDispatched"))).toBe(true);
    expect(legacy(LEGACY_SYSTEM_ERROR_BODY.replace(CUSTOM, "deepseek"))).toBe(true);
  });
});
