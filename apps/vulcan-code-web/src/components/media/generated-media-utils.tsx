import React from "react";
import {
  FileText,
  Image as ImageIcon,
  Video,
  Volume2,
} from "lucide-react";
import { localMediaSourceUrl } from "@/resources/localMedia";
import type {
  GeneratedMediaImageOffset,
  GeneratedMediaImageSize,
  GeneratedMediaKind,
} from "./generated-media-types";
import {
  GENERATED_MEDIA_MAX_ZOOM,
  GENERATED_MEDIA_MIN_ZOOM,
} from "./generated-media-types";

// normalizeGeneratedMediaKind maps stored attachment kind labels to WebUI media controls.
// normalizeGeneratedMediaKind 将已存储附件类型标签映射为 WebUI 媒体控件。
export function normalizeGeneratedMediaKind(kind: string): GeneratedMediaKind | null {
  if (kind === "image") return "image";
  if (kind === "audio") return "audio";
  if (kind === "video") return "video";
  if (kind === "pdf" || kind === "document" || kind === "file") return "document";
  return null;
}

// useGeneratedMediaSource resolves a generated local media path into a WebUI-renderable source URL.
// useGeneratedMediaSource 将生成的本地媒体路径解析为 WebUI 可渲染的源 URL。
export function useGeneratedMediaSource(path: string, _mimeType?: string): string {
  return React.useMemo(() => (path ? localMediaSourceUrl(path) : ""), [path]);
}

// generatedMediaIcon returns the icon used by generated media cards and modals.
// generatedMediaIcon 返回生成媒体卡片与弹窗使用的图标。
export function generatedMediaIcon(kind: GeneratedMediaKind) {
  if (kind === "image") return ImageIcon;
  if (kind === "video") return Video;
  if (kind === "audio") return Volume2;
  return FileText;
}

// clampGeneratedMediaZoom keeps image zoom values inside the supported media viewer range.
// clampGeneratedMediaZoom 将图片缩放值限制在媒体查看器支持范围内。
// value is the requested scale; optional fitScale permits a measured image to fit below the ordinary floor.
// value 是请求比例；可选 fitScale 允许实测图片适配到普通下限以下。
// Returns the bounded scale; callers without viewport measurements keep the existing ten-percent minimum.
// 返回限制后的比例；没有视口测量的调用方保留既有百分之十下限。
export function clampGeneratedMediaZoom(value: number, fitScale: number | null = null): number {
  // A measured fit may be below the ordinary floor; missing measurements retain the existing floor.
  // 实测适配比例可低于普通下限；缺少测量时保留既有下限。
  const minimum = Math.min(GENERATED_MEDIA_MIN_ZOOM, fitScale ?? GENERATED_MEDIA_MIN_ZOOM);
  return Math.min(GENERATED_MEDIA_MAX_ZOOM, Math.max(minimum, value));
}

// generatedMediaFitScale returns the best fit scale relative to the original image size.
// generatedMediaFitScale 返回相对图片原始尺寸的最佳适配缩放比例。
// viewportSize and naturalSize are measured dimensions; returns a complete-image fit or null until both are usable.
// viewportSize 与 naturalSize 是实测尺寸；返回完整图片适配比例，任一尺寸不可用时返回 null。
export function generatedMediaFitScale(
  viewportSize: GeneratedMediaImageSize | null,
  naturalSize: GeneratedMediaImageSize | null,
): number | null {
  if (!viewportSize || !naturalSize || viewportSize.width <= 0 || viewportSize.height <= 0 || naturalSize.width <= 0 || naturalSize.height <= 0) {
    return null;
  }
  // Fitting must contain the complete image even below the manual viewer's ordinary ten-percent floor.
  // 即使低于手动查看器普通的百分之十下限，适配也必须容纳完整图片。
  return Math.min(1, viewportSize.width / naturalSize.width, viewportSize.height / naturalSize.height);
}

// clampGeneratedMediaImageOffset keeps a panned image reachable inside the viewport.
// clampGeneratedMediaImageOffset 将拖拽后的图片偏移限制在视口可达范围内。
export function clampGeneratedMediaImageOffset(
  offset: GeneratedMediaImageOffset,
  scale: number,
  viewportSize: GeneratedMediaImageSize | null,
  naturalSize: GeneratedMediaImageSize | null,
): GeneratedMediaImageOffset {
  if (!viewportSize || !naturalSize || viewportSize.width <= 0 || viewportSize.height <= 0 || naturalSize.width <= 0 || naturalSize.height <= 0) {
    return { x: 0, y: 0 };
  }
  const scaledWidth = naturalSize.width * scale;
  const scaledHeight = naturalSize.height * scale;
  const maxX = Math.max(0, (scaledWidth - viewportSize.width) / 2);
  const maxY = Math.max(0, (scaledHeight - viewportSize.height) / 2);
  return {
    x: Math.min(maxX, Math.max(-maxX, offset.x)),
    y: Math.min(maxY, Math.max(-maxY, offset.y)),
  };
}

// generatedMediaZoomOffsetAroundPoint preserves the visual anchor while changing scale.
// generatedMediaZoomOffsetAroundPoint 在缩放时保持鼠标或中心锚点的视觉位置。
export function generatedMediaZoomOffsetAroundPoint(
  offset: GeneratedMediaImageOffset,
  currentScale: number,
  nextScale: number,
  anchor: GeneratedMediaImageOffset,
  viewportSize: GeneratedMediaImageSize | null,
  naturalSize: GeneratedMediaImageSize | null,
): GeneratedMediaImageOffset {
  if (currentScale <= 0) {
    return clampGeneratedMediaImageOffset(offset, nextScale, viewportSize, naturalSize);
  }
  const ratio = nextScale / currentScale;
  return clampGeneratedMediaImageOffset({
    x: anchor.x - (anchor.x - offset.x) * ratio,
    y: anchor.y - (anchor.y - offset.y) * ratio,
  }, nextScale, viewportSize, naturalSize);
}

// generatedMediaHasImageOffset returns whether the image is currently panned.
// generatedMediaHasImageOffset 返回图片当前是否存在拖拽偏移。
export function generatedMediaHasImageOffset(offset: GeneratedMediaImageOffset): boolean {
  return Math.abs(offset.x) > 0.5 || Math.abs(offset.y) > 0.5;
}
