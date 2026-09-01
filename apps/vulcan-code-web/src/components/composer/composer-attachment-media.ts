import { type GeneratedMediaAttachment } from "@/components/media/generated-media";
import type { ComposerAttachmentSummary } from "@/types";

// COMPOSER_DERIVED_PREVIEW_MAX_BYTES mirrors only the host preview-sidecar file limit, never a delivery threshold.
// COMPOSER_DERIVED_PREVIEW_MAX_BYTES 仅对应宿主预览旁路文件限制，绝不是投递阈值。
const COMPOSER_DERIVED_PREVIEW_MAX_BYTES = 2 * 1024 * 1024;

// COMPOSER_DERIVED_PREVIEW_MAX_EDGE bounds full-frame previews without cropping user content.
// COMPOSER_DERIVED_PREVIEW_MAX_EDGE 在不裁剪用户内容的前提下限制完整画面预览边长。
const COMPOSER_DERIVED_PREVIEW_MAX_EDGE = 1_600;

// composerAttachmentMedia converts a staged attachment into a reusable media preview descriptor.
// composerAttachmentMedia 将已暂存附件转换为可复用的媒体预览描述。
export function composerAttachmentMedia(attachment: ComposerAttachmentSummary): GeneratedMediaAttachment | null {
  const kind = attachment.kind === "pdf" ? "document" : attachment.kind;
  if (kind !== "image" && kind !== "audio" && kind !== "video" && kind !== "document") {
    return null;
  }
  return {
    kind,
    fileName: attachment.fileName,
    localPath: attachment.localPath,
    previewLocalPath: attachment.previewLocalPath ?? undefined,
    mimeType: attachment.mimeType,
  };
}

// inferComposerAttachmentKindFromFile maps MIME type and extension to a composer kind.
// inferComposerAttachmentKindFromFile 将 MIME 类型与扩展名映射到输入区附件类型。
export function inferComposerAttachmentKindFromFile(file: File): string | null {
  const mimeType = file.type.toLowerCase();
  const name = file.name.toLowerCase();
  if (mimeType.startsWith("image/") || /\.(png|jpe?g|webp|gif|bmp)$/.test(name)) return "image";
  if (mimeType.startsWith("audio/") || /\.(mp3|wav|m4a|aac|flac|ogg|oga|opus|pcm|ulaw)$/.test(name)) return "audio";
  if (mimeType.startsWith("video/") || /\.(mp4|mpeg|mpg|mov|m4v|mkv|webm|avi|flv|wmv|3gp|3gpp|3g2)$/.test(name)) return "video";
  if (mimeType === "application/pdf" || /\.pdf$/.test(name)) return "pdf";
  return "file";
}

// createComposerImagePreviewFile creates a proportional full-frame JPEG for oversized browser images.
// createComposerImagePreviewFile 为超大浏览器图片创建等比完整画面的 JPEG 预览。
// Parameters: file is the exact user-selected image and remains the canonical attachment source.
// 参数：file 是用户选择的精确图片，并继续作为规范附件源文件。
// Returns: a bounded derivative file, or null when no derivative is required or decoding fails.
// 返回值：受限的派生文件；无需派生或解码失败时返回 null。
export async function createComposerImagePreviewFile(file: File): Promise<File | null> {
  if (file.size <= COMPOSER_DERIVED_PREVIEW_MAX_BYTES || inferComposerAttachmentKindFromFile(file) !== "image") {
    return null;
  }
  try {
    // bitmap decodes the browser-supported image while preserving its complete frame bounds.
    // bitmap 解码浏览器支持的图片，同时保留完整画面边界。
    const bitmap = await createImageBitmap(file);
    // scale never enlarges the source and always preserves its aspect ratio.
    // scale 永不放大源图片，并始终保持宽高比。
    const scale = Math.min(1, COMPOSER_DERIVED_PREVIEW_MAX_EDGE / Math.max(bitmap.width, bitmap.height));
    // canvas owns only the request-copy derivative and never replaces the canonical source.
    // canvas 仅持有请求副本派生图，永不替换规范源文件。
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(bitmap.width * scale));
    canvas.height = Math.max(1, Math.round(bitmap.height * scale));
    const context = canvas.getContext("2d", { alpha: false });
    if (!context) {
      bitmap.close();
      return null;
    }
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    bitmap.close();
    // blob uses a conservative JPEG quality so the derivative stays within the preview-sidecar storage limit.
    // blob 使用保守 JPEG 质量，使派生图保持在预览旁路存储限制内。
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/jpeg", 0.78));
    if (!blob || blob.size > COMPOSER_DERIVED_PREVIEW_MAX_BYTES) {
      return null;
    }
    const sourceStem = file.name.replace(/\.[^.]+$/, "") || "image";
    return new File([blob], `${sourceStem}.vulcan-preview.jpg`, {
      type: "image/jpeg",
      lastModified: file.lastModified,
    });
  } catch {
    return null;
  }
}

