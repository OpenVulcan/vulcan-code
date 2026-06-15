import React from "react";
import {
  BrainCircuit,
  CheckCircle2,
  FileText,
  Image as ImageIcon,
  Music2,
  Paperclip,
  Plus,
  Video,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import type { ComposerAttachmentCapabilitySummary, ComposerAttachmentSummary } from "@/types";

import type { ComposerAttachmentPreviewMedia } from "./composer-control-types";

// ComposerAttachmentMenu renders the plus button and attachment submenu.
// ComposerAttachmentMenu 渲染加号按钮与附件子菜单。
export function ComposerAttachmentMenu(props: {
  // Enabled attachment capability gates available for this prompt.
  // 当前提示词可用的已启用附件能力门控。
  capabilities: ComposerAttachmentCapabilitySummary[];
  // Whether streaming thinking currently uses the floating thinking surface.
  // 当前流式思考是否使用浮动思考面板。
  thinkingOverlayEnabled: boolean;
  // Opens a file picker for one attachment kind or all kinds.
  // 按附件类型或全部类型打开文件选择器。
  onPick: (kind: string | null) => void;
  // Updates the thinking display preference from the plus menu.
  // 从加号菜单更新思考展示偏好。
  onThinkingOverlayEnabledChange: (enabled: boolean) => void;
}) {
  const menuCapabilities = props.capabilities.filter((capability) => ["image", "video", "audio", "pdf"].includes(capability.kind));
  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <Button
              aria-label="添加附件"
              className="composer-add-button"
              size="icon"
              type="button"
              variant="secondary"
            >
              <Plus size={17} />
            </Button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent className="composer-control-tooltip" side="top">
          <strong>扩展</strong>
        </TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="start" className="composer-attachment-menu" side="top" sideOffset={8}>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <Paperclip size={15} />
            <span>附件</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            {menuCapabilities.length ? (
              menuCapabilities.map((capability) => (
                <DropdownMenuItem
                  key={capability.kind}
                  onSelect={() => props.onPick(capability.kind)}
                >
                  <ComposerAttachmentKindIcon kind={capability.kind} />
                  <span>{capability.label}</span>
                </DropdownMenuItem>
              ))
            ) : (
              <DropdownMenuItem disabled>
                <span>当前无可用附件类型</span>
              </DropdownMenuItem>
            )}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <BrainCircuit size={15} />
            <span>思考弹窗</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="composer-thinking-menu">
            <DropdownMenuItem onSelect={() => props.onThinkingOverlayEnabledChange(true)}>
              <span>开启</span>
              {props.thinkingOverlayEnabled ? <CheckCircle2 size={14} /> : null}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => props.onThinkingOverlayEnabledChange(false)}>
              <span>关闭</span>
              {!props.thinkingOverlayEnabled ? <CheckCircle2 size={14} /> : null}
            </DropdownMenuItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// ComposerAttachmentKindIcon renders a compact icon for one attachment type.
// ComposerAttachmentKindIcon 渲染单种附件类型的紧凑图标。
function ComposerAttachmentKindIcon(props: {
  // Attachment kind projected by the service capability gate.
  // 服务能力门控投影出的附件类型。
  kind: string;
}) {
  if (props.kind === "image") return <ImageIcon size={15} />;
  if (props.kind === "video") return <Video size={15} />;
  if (props.kind === "audio") return <Music2 size={15} />;
  return <FileText size={15} />;
}

// ComposerAttachmentStrip renders staged attachments before the prompt text.
// ComposerAttachmentStrip 在提示词文本之前渲染已暂存附件。
export function ComposerAttachmentStrip(props: {
  // Staged attachments shown in the strip.
  // 附件条中展示的已暂存附件。
  attachments: ComposerAttachmentSummary[];
  // Resolves the minimal preview media for one attachment.
  // 解析单个附件的最小预览媒体信息。
  resolvePreview: (attachment: ComposerAttachmentSummary) => ComposerAttachmentPreviewMedia | null;
  // Opens an attachment preview.
  // 打开附件预览。
  onOpenMedia: (attachment: ComposerAttachmentSummary) => void;
  // Removes one attachment by index.
  // 按索引移除单个附件。
  onRemove?: (index: number) => void;
}) {
  return (
    <div className="composer-attachment-strip">
      {props.attachments.map((attachment, index) => (
        <button
          className={cn("composer-attachment-tile", {
            "image-attachment": attachment.kind === "image",
          })}
          key={`${attachment.localPath}-${index}`}
          onClick={() => props.onOpenMedia(attachment)}
          type="button"
        >
          <ComposerAttachmentPreview attachment={attachment} resolvePreview={props.resolvePreview} />
          {attachment.kind === "image" ? null : (
            <span className="composer-attachment-name">{attachment.fileName}</span>
          )}
          {props.onRemove ? (
            <span
              aria-label="移除附件"
              className="composer-attachment-remove"
              onClick={(event) => {
                event.stopPropagation();
                props.onRemove?.(index);
              }}
              role="button"
              tabIndex={0}
            >
              <X size={12} />
            </span>
          ) : null}
        </button>
      ))}
    </div>
  );
}

// ComposerAttachmentPreview renders one staged attachment thumbnail or media icon.
// ComposerAttachmentPreview 渲染单个已暂存附件缩略图或媒体图标。
function ComposerAttachmentPreview(props: {
  // Staged attachment rendered by this tile.
  // 当前附件块渲染的已暂存附件。
  attachment: ComposerAttachmentSummary;
  // Resolves the minimal preview media for this attachment.
  // 解析当前附件的最小预览媒体信息。
  resolvePreview: (attachment: ComposerAttachmentSummary) => ComposerAttachmentPreviewMedia | null;
}) {
  const media = props.resolvePreview(props.attachment);
  if (media?.kind === "image" && media.sourceUrl) {
    return <img alt={props.attachment.fileName} src={media.sourceUrl} />;
  }
  if (media?.kind === "video") {
    return <Video size={20} />;
  }
  if (media?.kind === "audio") {
    return <Music2 size={20} />;
  }
  return <FileText size={20} />;
}
