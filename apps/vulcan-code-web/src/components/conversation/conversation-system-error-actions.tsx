import * as React from "react";

import { FeedbackCenterContext } from "@/components/features/feedback/feedback-center-context";
import { useI18n } from "@/i18n";
import { PROVIDER_RESPONSE_HEADER_TIMEOUT_PATTERN, PROVIDER_RETRY_REGEX_PREFIX } from "@/lib/provider-retry-rules";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";
import { Button } from "@/ui-kit/primitives/button";
import { toast } from "@/ui-kit/primitives/toast";
import { isProviderResultReconciliationPending, snakeCase, type SystemProviderError } from "./conversation-system-error-card";

// UNREPORTABLE_ERROR_CATEGORIES lists failures caused by the account, the provider, the network or the user, not by
// Vulcan Code: credentials, permissions, balance, rate limits (429), provider overload, outage or an unreachable
// endpoint (5xx and transport failures), timeouts, content policy and cancellation.
// UNREPORTABLE_ERROR_CATEGORIES 列出由账号、供应商、网络或用户造成、而非 Vulcan Code 造成的失败：凭据、权限、余额、
// 限流（429）、供应商过载、故障或无法访问（5xx 与传输失败）、超时、内容策略与取消。
const UNREPORTABLE_ERROR_CATEGORIES = new Set([
  "authentication",
  "permission_denied",
  "payment_required",
  "rate_limited",
  "provider_overloaded",
  "provider_unavailable",
  "timeout",
  "content_policy",
  "cancelled",
]);

// CUSTOM_PROVIDER_PREFIX marks providers the user configured; their endpoint, model and limits are the user's settings.
// CUSTOM_PROVIDER_PREFIX 标记用户自定义的供应商；其地址、模型与上限都是用户的配置。
const CUSTOM_PROVIDER_PREFIX = "custom:";

// isReportableSystemError returns whether a failure may point at a Vulcan Code defect worth reporting. Failures in
// UNREPORTABLE_ERROR_CATEGORIES never are. A custom provider is reportable only when the request failed inside Vulcan
// Code before it was sent; anything its endpoint answered (bad request, wrong path, context limit) is configuration.
// Other categories of system providers, including future ones, stay reportable.
// isReportableSystemError 返回该失败是否可能指向值得报告的 Vulcan Code 缺陷。UNREPORTABLE_ERROR_CATEGORIES 中的失败
// 一律不报告。自定义供应商只有在请求发出前于 Vulcan Code 内失败时才可报告；其地址返回的任何错误（错误请求、路径
// 错误、上下文上限）都属于配置问题。系统供应商的其他类别（包括今后新增的）仍可报告。
export function isReportableSystemError(error: SystemProviderError): boolean {
  if (UNREPORTABLE_ERROR_CATEGORIES.has(snakeCase(error.category))) {
    return false;
  }
  if (error.provider.startsWith(CUSTOM_PROVIDER_PREFIX)) {
    return snakeCase(error.dispatchOutcome) === "not_dispatched";
  }
  return true;
}

// Derives an addable retry rule from the selected error, or null when it offers no usable rule.
// 从当前选中的错误生成可添加的重试规则；没有可用规则时返回 null。
function providerRetryRuleForError(error: SystemProviderError): string | null {
  if (error.sideEffectObserved || isProviderResultReconciliationPending(error)
    || /^(authentication|permission_denied|payment_required|invalid_request|context_length_exceeded|content_policy|cancelled)$/.test(error.category)
    || /^4[0-9]{2}$/.test(error.httpStatus ?? "")) {
    return null;
  }
  // Read the provider diagnostic instead of copying the card's JSON or formatted presentation.
  // 使用供应商诊断文本，避免复制卡片 JSON 或格式化展示内容。
  const diagnostic = (error.upstreamMessage ?? error.message).trim();
  if (PROVIDER_RESPONSE_HEADER_TIMEOUT_PATTERN.test(diagnostic)) {
    return `${PROVIDER_RETRY_REGEX_PREFIX}${PROVIDER_RESPONSE_HEADER_TIMEOUT_PATTERN.source}`;
  }
  return diagnostic && !diagnostic.startsWith(PROVIDER_RETRY_REGEX_PREFIX) && !/[\r\n]/.test(diagnostic)
    ? diagnostic
    : null;
}

// Renders the action for one `rule` and reports its service result.
// 渲染单条 `rule` 的添加操作并反馈服务结果。
function AddProviderRetryRuleButton({ rule }: { rule: string }) {
  // Localize the button and status feedback for this rule.
  // 本地化当前规则的按钮及状态反馈。
  const { t } = useI18n();
  // Disable repeat submission while the current add request is pending.
  // 当前添加请求处理期间禁止重复提交。
  const [pending, setPending] = React.useState(false);

  // Persists the selected rule and reports the outcome without changing the selected attempt.
  // 持久化选中的规则并反馈结果，保持当前选择的尝试。
  async function addRule(): Promise<void> {
    setPending(true);
    try {
      await serviceClient.request<null>("service_add_provider_retry_rule", { rule });
      toast.success(t("settings.retry.saved"));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t("settings.retry.couldnTSaveYour"));
    } finally {
      setPending(false);
    }
  }

  return (
    <Button disabled={pending} size="sm" type="button" variant="outline" onClick={() => void addRule()}>
      {t("conversation.systemError.addRetryRule")}
    </Button>
  );
}

// Renders the actions available for the expanded diagnostic `error`, or null when it offers none.
// 为已展开的诊断 `error` 渲染可用操作；没有操作时返回 null。
export default function ProviderErrorActions({ error }: { error: SystemProviderError }) {
  // Localize the report action and use the workbench's feedback controller when provided.
  // 本地化报告操作，并在工作台提供反馈控制器时使用它。
  const { t } = useI18n();
  const feedback = React.useContext(FeedbackCenterContext);
  // Compute the retry rule and report eligibility for the visible attempt.
  // 计算当前可见尝试的重试规则与报告资格。
  const rule = hasServiceRuntime() ? providerRetryRuleForError(error) : null;
  const canReport = feedback && isReportableSystemError(error);
  if (!rule && !canReport) return null;
  return (
    <div className="vk-error-actions">
      {rule ? <AddProviderRetryRuleButton key={rule} rule={rule} /> : null}
      {canReport ? (
        <Button className="vk-error-report" size="sm" type="button" variant="outline" onClick={() => feedback.openFeedback({ source: "error_card", systemError: error })}>
          {t("feedback.reportError")}
        </Button>
      ) : null}
    </div>
  );
}
