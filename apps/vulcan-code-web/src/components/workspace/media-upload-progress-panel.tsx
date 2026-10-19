import React from "react";
import type { MediaUploadProgressChangedDelta } from "@/types";
import { useI18n } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";

// MediaUploadProgressPanel renders one pre-dispatch or post-dispatch media transfer lifecycle.
// MediaUploadProgressPanel 渲染一条分派前或分派后的媒体传输生命周期。
export function MediaUploadProgressPanel(props: {
  // Current ephemeral progress payload from the ordered service stream.
  // 有序服务流中的当前临时进度载荷。
  progress: MediaUploadProgressChangedDelta;
  // Cancels the exact pre-dispatch client submission when the progress contract permits cancellation.
  // 当进度契约允许取消时，取消精确的分派前客户端提交。
  onCancel: (clientSubmissionId: string) => Promise<boolean>;
}) {
  // t localizes the transfer state and its retryable cancellation failure.
  // t 翻译传输状态和可重试的取消失败。
  const { t } = useI18n();
  // cancellationPending guards synchronous duplicate events until cancellation is accepted or fails.
  // cancellationPending 在取消被接受或失败前阻止同步重复事件。
  const cancellationPending = React.useRef(false);
  // cancelling and cancellationFailed expose the accepted request and the retry state.
  // cancelling 和 cancellationFailed 展示已接受请求及重试状态。
  const [cancelling, setCancelling] = React.useState(false);
  const [cancellationFailed, setCancellationFailed] = React.useState(false);
  // totalBytes remains unknown when the service has not supplied a length.
  // totalBytes 在服务未提供长度时保持未知。
  const totalBytes = props.progress.totalBytes;
  // percentage omits a fabricated zero for streaming transfers with unknown total size.
  // percentage 不为未知总大小的流式传输编造零进度。
  const percentage = totalBytes != null && totalBytes > 0
    ? Math.min(100, Math.round((props.progress.bytesSent / totalBytes) * 100))
    : props.progress.phase === "completed" ? 100 : undefined;

  // handleCancel keeps the button idempotent while the backend aborts the source stream.
  // handleCancel 在后端中止源流期间保持按钮幂等。
  const handleCancel = React.useCallback(async () => {
    if (cancellationPending.current || props.progress.cancellable === false) {
      return;
    }
    cancellationPending.current = true;
    setCancelling(true);
    setCancellationFailed(false);
    // accepted keeps an acknowledged cancellation disabled until its terminal stream event removes this keyed row.
    // accepted 使已确认取消保持禁用，直到终态流事件移除此带稳定键的行。
    let accepted = false;
    try {
      accepted = await props.onCancel(props.progress.clientSubmissionId);
    } catch {
      setCancellationFailed(true);
    } finally {
      if (!accepted) {
        cancellationPending.current = false;
        setCancellationFailed(true);
        setCancelling(false);
      }
    }
  }, [props]);

  return (
    <section className="media-upload-progress-panel" role="status" aria-live="polite">
      <div className="media-upload-progress-heading">
        <span>{props.progress.phase === "preparing"
          ? t("conversation.submission.preparing")
          : props.progress.phase === "awaiting_provider_confirmation"
            ? t("composer.attachment.processing")
            : t("conversation.submission.uploading")}</span>
        <span title={percentage === undefined ? t("conversation.submission.sizeUnknown") : undefined}>{percentage === undefined ? "—" : `${percentage}%`}</span>
      </div>
      <div className="media-upload-progress-file" title={props.progress.fileName}>
        {props.progress.fileName}
        {props.progress.fileCount > 1 ? ` (${props.progress.fileIndex + 1}/${props.progress.fileCount})` : ""}
      </div>
      <progress value={percentage} max={100} aria-label={t("conversation.submission.uploadProgress")} />
      {props.progress.cancellable !== false ? (
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={() => void handleCancel()}
          disabled={cancelling}
        >
          {cancelling ? t("conversation.submission.cancelling") : t("conversation.action.cancel")}
        </Button>
      ) : null}
      {cancellationFailed ? <small role="alert">{t("conversation.submission.cancelFailed")}</small> : null}
    </section>
  );
}
