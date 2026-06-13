import { type GeneratedMediaAttachment } from "@/components/media/generated-media";
import type { ComposerAttachmentSummary } from "@/types";

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
  if (mimeType.startsWith("audio/") || /\.(mp3|wav|m4a|aac|flac|ogg)$/.test(name)) return "audio";
  if (mimeType.startsWith("video/") || /\.(mp4|mov|mkv|webm|avi)$/.test(name)) return "video";
  if (mimeType === "application/pdf" || /\.pdf$/.test(name)) return "pdf";
  return null;
}

