import React from "react";
import type { MediaUploadProgressChangedDelta } from "@/types";
import { useI18n } from "@/i18n";

// MediaUploadProgressPanel renders one cancellable pre-dispatch upload lifecycle.
// MediaUploadProgressPanel 渲染一条可取消的分派前上传生命周期。
export function MediaUploadProgressPanel(props: {
  // Current ephemeral progress payload from the ordered service stream.
  // 有序服务流中的当前临时进度载荷。
  progress: MediaUploadProgressChangedDelta;
  // Cancels the exact client submission before it is materialized.
  // 在物化前取消精确的客户端提交。
  onCancel: (clientSubmissionId: string) => Promise<boolean>;
}) {
  const { t } = useI18n();
  const [cancelling, setCancelling] = React.useState(false);
  const totalBytes = props.progress.totalBytes ?? 0;
  const percentage = totalBytes > 0
    ? Math.min(100, Math.round((props.progress.bytesSent / totalBytes) * 100))
    : props.progress.phase === "completed" ? 100 : 0;

  // handleCancel keeps the button idempotent while the backend aborts the source stream.
  // handleCancel 在后端中止源流期间保持按钮幂等。
  const handleCancel = React.useCallback(async () => {
    if (cancelling) {
      return;
    }
    setCancelling(true);
    const accepted = await props.onCancel(props.progress.clientSubmissionId);
    if (!accepted) {
      setCancelling(false);
    }
  }, [cancelling, props]);

  return (
    <section className="media-upload-progress-panel" role="status" aria-live="polite">
      <div className="media-upload-progress-heading">
        <span>{props.progress.phase === "preparing" ? t("conversation.submission.preparing") : t("conversation.submission.uploading")}</span>
        <span>{percentage}%</span>
      </div>
      <div className="media-upload-progress-file" title={props.progress.fileName}>
        {props.progress.fileName}
        {props.progress.fileCount > 1 ? ` (${props.progress.fileIndex + 1}/${props.progress.fileCount})` : ""}
      </div>
      <progress value={percentage} max={100} aria-label={t("conversation.submission.uploadProgress")} />
      <button
        type="button"
        className="media-upload-progress-cancel"
        onClick={() => void handleCancel()}
        disabled={cancelling}
      >
        {cancelling ? t("conversation.submission.cancelling") : t("conversation.action.cancel")}
      </button>
    </section>
  );
}
