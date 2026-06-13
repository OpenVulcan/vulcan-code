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

// ComposerRuntimeCue describes the compact running-state badge shown above the composer.
// ComposerRuntimeCue 描述输入区上方展示的紧凑运行状态徽标。
export interface ComposerRuntimeCue {
  // Stable cue family used to pick the icon and visual treatment.
  // 用于选择图标与视觉表现的稳定提示类型。
  kind: "default";
  // Localized text shown in the badge.
  // 徽标中展示的本地化文本。
  text: string;
}
