// GeneratedMediaKind describes host-generated media families that can be rendered inline.
// GeneratedMediaKind 描述可内联渲染的宿主生成媒体族。
export type GeneratedMediaKind = "image" | "audio" | "video" | "document";

// GeneratedMediaAttachment describes one generated media output carried by tool metadata.
// GeneratedMediaAttachment 描述工具元数据携带的一条生成媒体输出。
export interface GeneratedMediaAttachment {
  // Stable media family used to select the preview control.
  // 用于选择预览控件的稳定媒体族。
  kind: GeneratedMediaKind;
  // Human-readable file name displayed in the media card.
  // 媒体卡片中显示的人类可读文件名。
  fileName: string;
  // Preferred local path for preview and opening.
  // 用于预览和打开的首选本地路径。
  localPath: string;
  // Optional preview path, usually a generated video thumbnail.
  // 可选预览路径，通常是生成视频缩略图。
  previewLocalPath?: string;
  // Optional MIME type persisted by the host.
  // 宿主持久化的可选 MIME 类型。
  mimeType?: string;
}

// GeneratedMediaImageSize stores measured image or viewport dimensions used for true-size zoom math.
// GeneratedMediaImageSize 保存用于真实尺寸缩放计算的图片或视口尺寸。
export interface GeneratedMediaImageSize {
  // Width in CSS pixels or natural image pixels depending on the measurement source.
  // 宽度，单位根据测量来源为 CSS 像素或图片自然像素。
  width: number;
  // Height in CSS pixels or natural image pixels depending on the measurement source.
  // 高度，单位根据测量来源为 CSS 像素或图片自然像素。
  height: number;
}

// GeneratedMediaImageOffset stores the pan offset from the image viewport center.
// GeneratedMediaImageOffset 保存相对图片视口中心的拖拽偏移。
export interface GeneratedMediaImageOffset {
  // Horizontal offset in CSS pixels.
  // 水平方向 CSS 像素偏移。
  x: number;
  // Vertical offset in CSS pixels.
  // 垂直方向 CSS 像素偏移。
  y: number;
}

// GENERATED_MEDIA_MIN_ZOOM is the smallest image zoom ratio allowed in media viewers.
// GENERATED_MEDIA_MIN_ZOOM 是媒体查看器允许的最小图片缩放比例。
export const GENERATED_MEDIA_MIN_ZOOM = 0.1;

// GENERATED_MEDIA_MAX_ZOOM is the largest image zoom ratio allowed in media viewers.
// GENERATED_MEDIA_MAX_ZOOM 是媒体查看器允许的最大图片缩放比例。
export const GENERATED_MEDIA_MAX_ZOOM = 10;

// GENERATED_MEDIA_ZOOM_STEP is the fixed zoom delta used by media viewer controls.
// GENERATED_MEDIA_ZOOM_STEP 是媒体查看器控制按钮使用的固定缩放步长。
export const GENERATED_MEDIA_ZOOM_STEP = 0.1;
