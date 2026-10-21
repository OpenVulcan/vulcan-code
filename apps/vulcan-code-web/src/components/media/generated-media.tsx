import React from "react";
import {
  FileText,
  Play,
} from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import type { GeneratedMediaAttachment } from "./generated-media-types";
import { GENERATED_MEDIA_ZOOM_STEP } from "./generated-media-types";
import {
  clampGeneratedMediaZoom,
  generatedMediaIcon,
  useGeneratedMediaSource,
} from "./generated-media-utils";
import { useI18n } from "@/i18n";
import { MediaPathContextMenu } from "./media-path-context-menu";

export { GeneratedMediaPlayerWindow } from "./generated-media-player";
export type {
  GeneratedMediaAttachment,
  GeneratedMediaKind,
} from "./generated-media-types";
export { normalizeGeneratedMediaKind } from "./generated-media-utils";

// GeneratedMediaStrip renders generated image, audio, and video outputs directly under their tool row.
// GeneratedMediaStrip 在工具行下直接渲染生成图片、音频和视频输出。
export function GeneratedMediaStrip(props: {
  // attachments are the generated media outputs for one tool execution.
  // attachments 是单次工具执行的生成媒体输出。
  attachments: GeneratedMediaAttachment[];
  // onOpenDetail opens the full tool detail modal.
  // onOpenDetail 打开完整工具详情弹窗。
  onOpenDetail: () => void;
  // onOpenMedia opens the independent media playback window.
  // onOpenMedia 打开独立媒体播放窗口。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  return (
    <div className="generated-media-strip">
      {props.attachments.map((attachment) => (
        <GeneratedMediaInlineCard
          attachment={attachment}
          key={`${attachment.kind}:${attachment.localPath}`}
          onOpenDetail={props.onOpenDetail}
          onOpenMedia={props.onOpenMedia}
        />
      ))}
    </div>
  );
}

// GeneratedMediaInlineCard renders one compact media preview inside the transcript.
// GeneratedMediaInlineCard 在转录中渲染单条紧凑媒体预览。
function GeneratedMediaInlineCard(props: {
  // attachment describes the generated file to preview.
  // attachment 描述需要预览的生成文件。
  attachment: GeneratedMediaAttachment;
  // onOpenDetail opens the parent tool detail modal.
  // onOpenDetail 打开父级工具详情弹窗。
  onOpenDetail: () => void;
  // onOpenMedia opens the independent media playback window.
  // onOpenMedia 打开独立媒体播放窗口。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  const source = useGeneratedMediaSource(
    props.attachment.previewLocalPath ?? props.attachment.localPath,
    props.attachment.mimeType,
  );
  // mediaSource plays the original file; hooks run on every render, so it is resolved before the kind branches.
  // mediaSource 播放原始文件；钩子须在每次渲染执行，因此在按类型分支之前解析。
  const mediaSource = useGeneratedMediaSource(props.attachment.localPath, props.attachment.mimeType);
  const MediaIcon = generatedMediaIcon(props.attachment.kind);
  if (props.attachment.kind === "image") {
    return (
      <MediaPathContextMenu locator={props.attachment.localPath}>
        <button className="generated-media-card image" onClick={() => props.onOpenMedia(props.attachment)} type="button">
          <img alt={props.attachment.fileName} decoding="async" loading="lazy" src={source} />
          <span>{props.attachment.fileName}</span>
        </button>
      </MediaPathContextMenu>
    );
  }
  if (props.attachment.kind === "video") {
    return (
      <MediaPathContextMenu locator={props.attachment.localPath}>
      <button
        className="generated-media-card video"
        onClick={() => props.onOpenMedia(props.attachment)}
        type="button"
      >
        <span className="generated-media-video-thumb">
          {props.attachment.previewLocalPath ? (
            <img alt={props.attachment.fileName} decoding="async" loading="lazy" src={source} />
          ) : (
            <video
              aria-label={props.attachment.fileName}
              muted
              playsInline
              preload="metadata"
              src={mediaSource}
            />
          )}
          <span className="generated-media-play-mark"><Play size={18} fill="currentColor" /></span>
        </span>
        <span>{props.attachment.fileName}</span>
      </button>
      </MediaPathContextMenu>
    );
  }
  if (props.attachment.kind === "audio") {
    return (
      <MediaPathContextMenu locator={props.attachment.localPath}>
        <div className="generated-media-card audio">
          <audio aria-label={props.attachment.fileName} controls preload="metadata" src={mediaSource} />
        </div>
      </MediaPathContextMenu>
    );
  }
  return (
    <button className="generated-media-card document" onClick={props.onOpenDetail} type="button">
      <MediaIcon size={18} />
      <span>{props.attachment.fileName}</span>
    </button>
  );
}

// GeneratedMediaDetail renders media outputs in a zoomable/player-capable detail modal section.
// GeneratedMediaDetail 在详情弹窗中渲染可缩放或可播放的媒体输出区域。
export function GeneratedMediaDetail(props: {
  // attachments are generated media outputs carried by the opened tool detail.
  // attachments 是当前工具详情携带的生成媒体输出。
  attachments: GeneratedMediaAttachment[];
  // onOpenMedia opens the independent media playback window.
  // onOpenMedia 打开独立媒体播放窗口。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const [zoom, setZoom] = React.useState(1);
  const primary = props.attachments[0];
  const primarySource = useGeneratedMediaSource(primary?.localPath ?? "", primary?.mimeType);
  if (!primary) {
    return null;
  }
  const MediaIcon = generatedMediaIcon(primary.kind);
  return (
    <section className="generated-media-detail">
      <header className="generated-media-detail-header">
        <span><MediaIcon size={17} />{primary.fileName}</span>
        {primary.kind === "image" ? (
          <span className="generated-media-zoom-controls">
            <Button aria-label={t("mediaPlayer.zoomOut")} onClick={() => setZoom((value) => clampGeneratedMediaZoom(value - GENERATED_MEDIA_ZOOM_STEP))} size="sm" type="button" variant="outline">-</Button>
            <strong>{Math.round(zoom * 100)}%</strong>
            <Button aria-label={t("mediaPlayer.zoomIn")} onClick={() => setZoom((value) => clampGeneratedMediaZoom(value + GENERATED_MEDIA_ZOOM_STEP))} size="sm" type="button" variant="outline">+</Button>
          </span>
        ) : null}
      </header>
      {primary.kind === "image" ? (
        <MediaPathContextMenu locator={primary.localPath}>
          <div className="generated-media-image-preview">
            <img
              alt={primary.fileName}
              src={primarySource}
              style={{ transform: `scale(${zoom})` }}
            />
          </div>
        </MediaPathContextMenu>
      ) : primary.kind === "video" ? (
        <button
          className="generated-media-open-player"
          onClick={() => props.onOpenMedia(primary)}
          type="button"
        >
          <span className="generated-media-open-player-icon">
            <Play size={22} fill="currentColor" />
          </span>
          <span>
            <strong>{t("media.openVideoPlayer")}</strong>
            <small>{primary.fileName}</small>
          </span>
        </button>
      ) : primary.kind === "audio" ? (
        <MediaPathContextMenu locator={primary.localPath}>
          <audio aria-label={primary.fileName} className="generated-media-audio-preview" controls src={primarySource} />
        </MediaPathContextMenu>
      ) : (
        <a className="generated-media-document-link" href={primarySource} rel="noreferrer" target="_blank">
          <FileText size={18} />
          {primary.fileName}
        </a>
      )}
    </section>
  );
}
