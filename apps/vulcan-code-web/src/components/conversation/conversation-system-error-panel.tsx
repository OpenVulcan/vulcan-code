import * as React from "react";
import { ChevronRight, CircleAlert, Copy } from "lucide-react";

import { StatusPill } from "@/components/shared/status";
import { useI18n } from "@/i18n";
import { formatDurationSeconds } from "@/lib/usage-format";
import { Button } from "@/ui-kit/primitives/button";
import { toast } from "@/ui-kit/primitives/toast";
import ProviderErrorActions from "./conversation-system-error-actions";
import { formatSystemProviderErrorJson, isProviderResultReconciliationPending, type SystemErrorCardProps } from "./conversation-system-error-card";

// Formats the retry delay from its diagnostic value in milliseconds.
// 根据诊断中的毫秒值格式化重试等待时间。
function retryAfterDisplay(retryAfterMs: string): string {
  // Preserve a diagnostic that cannot be parsed as a number.
  // 保留无法解析为数值的诊断内容。
  const milliseconds = Number(retryAfterMs);
  return Number.isFinite(milliseconds) ? formatDurationSeconds(milliseconds) : `${retryAfterMs} ms`;
}

// Renders a compact preview for `error` and provides independent copy and raw-detail controls.
// 为 `error` 渲染紧凑预览，并提供独立的复制及原始详情控件。
export default function ProviderErrorPanel({ error }: SystemErrorCardProps) {
  // Localize preview labels, raw details, and clipboard feedback.
  // 本地化预览标签、原始详情和剪贴板反馈。
  const { t } = useI18n();
  // Preserve the backend's exact pending-confirmation classification.
  // 保留后台精确的待确认分类。
  const pending = isProviderResultReconciliationPending(error);
  // Keep the JSON hidden until requested and associate it with the toggle.
  // 在用户请求前隐藏 JSON，并将其与展开按钮关联。
  const [rawOpen, setRawOpen] = React.useState(false);
  const rawId = React.useId();

  // Copies the full structured error for the visible attempt. On failure, exposes the text for selection.
  // 复制当前尝试的完整结构化错误；复制失败时展开文本以便手动选择。
  async function copyError(): Promise<void> {
    try {
      await navigator.clipboard.writeText(formatSystemProviderErrorJson(error));
      toast.success(t("settings.errors.copied"));
    } catch {
      setRawOpen(true);
      toast.error(t("settings.errors.copyFailedSelectManually"));
    }
  }

  return (
    <section aria-label={t(pending ? "conversation.systemError.pendingTitle" : "conversation.systemError.title")}
      className="vk-error-card" data-pending={pending || undefined}>
      <div className="vk-error-summary">
        <CircleAlert aria-hidden="true" className="vk-error-icon" />
        <div className="vk-error-message">
          <p>{error.message}</p>
          <div className="vk-error-meta">
            <span>{error.provider}{error.modelId ? ` / ${error.modelId}` : null}</span>
            {error.upstreamCode ? <code aria-label={t("conversation.systemError.upstreamCode")}>{error.upstreamCode}</code> : null}
            {error.httpStatus ? <span>{t("conversation.systemError.httpStatus")} {error.httpStatus}</span> : null}
            {error.retryAfterMs ? <span>{t("conversation.systemError.retryAfter")} {retryAfterDisplay(error.retryAfterMs)}</span> : null}
          </div>
        </div>
        <Button aria-label={t("settings.errors.copyError")} title={t("settings.errors.copyError")}
          onClick={() => void copyError()} size="icon-xs" type="button" variant="ghost">
          <Copy aria-hidden="true" size={13} />
        </Button>
      </div>
      {pending ? (
        <div className="vk-error-pending">
          <StatusPill status="pending" label={t("conversation.systemError.pendingStatus")} />
          <p>{t("conversation.systemError.pendingNotice")}</p>
          <dl className="vk-facts">
            <dt>{t("conversation.systemError.requestIdentifier")}</dt>
            <dd>{error.requestId ?? t("conversation.systemError.identifierMissing")}</dd>
            <dt>{t("conversation.systemError.requestIdentifierSource")}</dt>
            <dd>{t(error.requestId ? "conversation.systemError.upstreamErrorPayload" : "conversation.systemError.identifierMissing")}</dd>
            <dt>{t("conversation.systemError.resultConfirmation")}</dt>
            <dd>{t("conversation.systemError.unconfirmed")}</dd>
            <dt>{t("conversation.systemError.usageConfirmation")}</dt>
            <dd>{t("conversation.systemError.unconfirmed")}</dd>
          </dl>
        </div>
      ) : null}
      <div className="vk-error-footer">
        <Button aria-controls={rawId} aria-expanded={rawOpen} onClick={() => setRawOpen(!rawOpen)} size="sm" type="button" variant="ghost">
          <ChevronRight aria-hidden="true" className="vk-error-chev" size={12} />
          {t("conversation.systemError.raw")}
        </Button>
        <ProviderErrorActions error={error} />
      </div>
      <div className="vk-error-raw" hidden={!rawOpen} id={rawId}>
        {rawOpen ? <pre><code className="language-json">{formatSystemProviderErrorJson(error)}</code></pre> : null}
      </div>
    </section>
  );
}
