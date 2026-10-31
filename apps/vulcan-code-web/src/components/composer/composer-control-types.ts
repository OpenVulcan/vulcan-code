// ComposerTraceCallback records local UI interactions without binding composer controls to the app logger.
// ComposerTraceCallback 记录本地 UI 交互，避免输入区控件直接绑定应用埋点实现。
export type ComposerTraceCallback = (action: string, detail?: Record<string, unknown>) => void;

// ComposerAttachmentPreviewMedia describes the minimal media data needed by attachment UI previews.
// ComposerAttachmentPreviewMedia 描述附件 UI 预览所需的最小媒体数据。
export interface ComposerAttachmentPreviewMedia {
  // Stable media family used to select the preview control.
  // 用于选择预览控件的稳定媒体族。
  kind: "image" | "audio" | "video" | "document";
  // Optional browser-safe source URL used for image thumbnails.
  // 用于图片缩略图的可选浏览器安全资源地址。
  sourceUrl?: string;
}
