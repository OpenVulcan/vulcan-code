import { FileWarning } from "lucide-react";
import { useI18n } from "@/i18n";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";

// DocumentViewerLoading covers a document viewer while its file loads, announcing `label` when given.
// DocumentViewerLoading 在文件加载期间覆盖文档查看器；给出 `label` 时播报该文案。
export function DocumentViewerLoading(props: { label?: string }) {
  const { t } = useI18n();
  return (
    <div className="vk-docview-overlay">
      <StreamPlaceholder className="vk-stream-inset" count={8} label={props.label ?? t("workspaceFilePreview.loading")} />
    </div>
  );
}

// DocumentViewerFailure tells the reader the document could not be shown, with the reader library's own message.
// DocumentViewerFailure 告知读者文档无法显示，并附上读取库自身的消息。
export function DocumentViewerFailure(props: { detail: string }) {
  const { t } = useI18n();
  return (
    <div className="vk-docview-overlay failed" role="alert">
      <FileWarning size={24} />
      <strong>{t("documentViewer.failed")}</strong>
      {props.detail ? <span>{props.detail}</span> : null}
    </div>
  );
}
