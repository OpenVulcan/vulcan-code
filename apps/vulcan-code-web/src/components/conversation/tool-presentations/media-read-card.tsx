import { FileInput } from "lucide-react";
import { useI18n, type TranslationKey } from "@/i18n";
import type { MediaReadPresentation, ToolPresentationDisplayState } from "./types";

// Every lifecycle has explicit wording so stopped or unknown calls never claim a successful read.
// 每种生命周期都有明确文案，避免已停止或未知调用宣称读取成功。
const STATUS_KEYS: Record<ToolPresentationDisplayState, TranslationKey> = {
  running: "toolPresentation.mediaRead.running",
  completed: "toolPresentation.mediaRead.completed",
  failed: "toolPresentation.mediaRead.failed",
  stopped: "toolPresentation.file.executionStopped",
  unknown: "toolPresentation.fileRead.title",
};

/** Renders `presentation` as one status-and-resource row; returns no preview, expandable body or raw payload controls.
 * 将`presentation`渲染为一行状态和资源；不返回预览、可展开正文或原始载荷控件。
 */
export function MediaReadPresentationCard({ presentation }: {
  // Exact invocation receipt produced by the media-read presentation adapter.
  // 媒体读取展示适配器产出的精确调用回执。
  presentation: MediaReadPresentation;
}) {
  // Localize the status while preserving the actual user resource name.
  // 本地化状态，同时保留实际用户资源名称。
  const { t } = useI18n();
  return (
    <div className="tool-presentation-card media-read">
      <div className="tool-presentation-summary" title={presentation.path}>
        <FileInput aria-hidden="true" size={15} />
        <span
          className={presentation.state === "failed" ? "file-change-error-badge" : "tool-presentation-state"}
          title={presentation.error || undefined}
        >{t(STATUS_KEYS[presentation.state])}</span>
        <span className="tool-presentation-command">
          <span className="tool-presentation-command-text">{presentation.fileName}</span>
        </span>
      </div>
    </div>
  );
}
