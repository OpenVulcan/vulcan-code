import {
  ComposerAttachmentStrip,
  type ComposerAttachmentPreviewMedia,
} from "@/components/composer/composer-controls";
import { composerAttachmentMedia } from "@/components/composer/composer-attachment-media";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import type {
  ComposerAttachmentPreparationSummary,
  ComposerAttachmentSummary,
  MediaUploadProgressChangedDelta,
} from "@/types";

// ComposerPanelAttachmentPreviewStripProps carries attachment preview callbacks for the composer panel.
// ComposerPanelAttachmentPreviewStripProps 承载输入区面板的附件预览回调。
export interface ComposerPanelAttachmentPreviewStripProps {
  // Attachments currently staged in the composer.
  // 当前在输入区暂存的附件。
  attachments: ComposerAttachmentSummary[];
  // Draft preparation states indexed by stable attachment identity.
  // 按稳定附件身份索引的草稿准备状态。
  preparations: Readonly<Record<string, ComposerAttachmentPreparationSummary>>;
  // Active transfer progress indexed by stable attachment identity.
  // 按稳定附件身份索引的活跃传输进度。
  progresses: Readonly<Record<string, MediaUploadProgressChangedDelta>>;
  // Handler that resolves a local media path into a previewable source URL.
  // 将本地媒体路径解析为可预览 source URL 的处理器。
  resolveMediaSourceUrl: (localPath: string) => string;
  // Handler that opens one generated media preview.
  // 打开单个生成媒体预览的处理器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Handler that removes one staged attachment.
  // 移除单个暂存附件的处理器。
  onRemove: (index: number) => void;
  // Handler that retries one failed provider upload.
  // 重试单个失败供应商上传的处理器。
  onRetry: (attachmentId: string) => void;
}

// ComposerPanelAttachmentPreviewStrip renders staged attachments with media preview support.
// ComposerPanelAttachmentPreviewStrip 渲染带媒体预览能力的暂存附件。
export function ComposerPanelAttachmentPreviewStrip(props: ComposerPanelAttachmentPreviewStripProps) {
  return (
    <ComposerAttachmentStrip
      attachments={props.attachments}
      preparations={props.preparations}
      progresses={props.progresses}
      resolvePreview={(attachment): ComposerAttachmentPreviewMedia | null => {
        const media = composerAttachmentMedia(attachment);
        if (!media) {
          return null;
        }
        return {
          kind: media.kind,
          sourceUrl: media.kind === "image" ? props.resolveMediaSourceUrl(media.localPath) : undefined,
        };
      }}
      onOpenMedia={(attachment) => {
        const media = composerAttachmentMedia(attachment);
        if (media) {
          props.onOpenMedia(media);
        }
      }}
      onRemove={props.onRemove}
      onRetry={props.onRetry}
    />
  );
}
