import React from "react";
import {
  BrainCircuit,
  CheckCircle2,
  FileText,
  Image as ImageIcon,
  ListChecks,
  Music2,
  Paperclip,
  Plus,
  Target,
  Video,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
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
import {
  THINKING_DISPLAY_MODE_OPTIONS,
  type ThinkingDisplayMode,
} from "@/components/conversation/thinking-display";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type {
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentSummary,
  SessionVmmPolicySummary,
} from "@/types";

import type { ComposerAttachmentPreviewMedia } from "./composer-control-types";
import type { ComposerKnowledgeSelectionSnapshot } from "./composer-model";
import { ComposerKnowledgeControl } from "./composer-knowledge-control";

// ComposerAttachmentMenu renders the plus button and attachment submenu.
// ComposerAttachmentMenu 渲染加号按钮与附件子菜单。
export function ComposerAttachmentMenu(props: {
  // Enabled attachment capability gates available for this prompt.
  // 当前提示词可用的已启用附件能力门控。
  capabilities: ComposerAttachmentCapabilitySummary[];
  // Thinking display mode selected by the WebUI plus menu.
  // WebUI 加号菜单选中的思考展示模式。
  thinkingDisplayMode: ThinkingDisplayMode;
  // Whether session Plan mode is currently enabled.
  // 当前会话计划模式是否已启用。
  planModeEnabled: boolean;
  // Current selected-session VMM memory policy state.
  // 当前选中会话的 VMM 记忆策略状态。
  sessionVmmPolicy: Pick<
    SessionVmmPolicySummary,
    "precheckMode" | "memoryWritebackMode"
  > | null;
  // Whether the effective project configuration makes VMM available to this composer.
  // 当前项目最终生效配置是否允许输入区使用 VMM。
  vmmAvailable: boolean;
  // Opens a file picker for one attachment kind or all kinds.
  // 按附件类型或全部类型打开文件选择器。
  onPick: (kind: string | null) => void;
  // Updates the thinking display mode from the plus menu.
  // 从加号菜单更新思考展示模式。
  onThinkingDisplayModeChange: (mode: ThinkingDisplayMode) => void;
  // Updates the session Plan mode from the plus menu.
  // 从加号菜单更新会话计划模式。
  onPlanModeChange: (enabled: boolean) => void;
  // Updates one selected-session VMM memory policy from the plus menu.
  // 从加号菜单更新当前会话的单个 VMM 记忆策略。
  onVmmPolicyChange: (policy: string, mode: string) => void;
  // Opens the session goal editor from the plus menu.
  // 从加号菜单打开会话目标编辑器。
  onOpenGoal: () => void;
  // Project identifier used to list authorized knowledge libraries.
  // 用于列出已授权知识库的项目标识符。
  projectId: string | null;
  // Conversation identifier whose knowledge selection is edited.
  // 要编辑知识库选择的会话标识符。
  conversationId: string | null;
  // Opens the project knowledge-library management surface.
  // 打开项目知识库管理界面。
  onOpenKnowledge: () => void;
  // Optional new-conversation knowledge selection.
  // 可选的新会话知识库选择。
  draftKnowledgeSelection?: ComposerKnowledgeSelectionSnapshot | null;
  // Saves one new-conversation knowledge selection.
  // 保存一份新会话知识库选择。
  onDraftKnowledgeSelectionChange?: (selection: ComposerKnowledgeSelectionSnapshot) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const menuCapabilities = props.capabilities.filter((capability) => ["image", "video", "audio", "pdf"].includes(capability.kind));
  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <Button
              aria-label={t("composer.attachment.add")}
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
          <strong>{t("composer.attachment.extension")}</strong>
        </TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="start" className="composer-attachment-menu" side="top" sideOffset={8}>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <Paperclip size={15} />
            <span>{t("composer.attachment.menu")}</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            {menuCapabilities.length ? (
              menuCapabilities.map((capability) => {
                const labelKey = composerAttachmentKindLabelKey(capability.kind);
                return (
                  <DropdownMenuItem
                    key={capability.kind}
                    onSelect={() => props.onPick(capability.kind)}
                  >
                    <ComposerAttachmentKindIcon kind={capability.kind} />
                    <span>{labelKey ? t(labelKey) : capability.label}</span>
                  </DropdownMenuItem>
                );
              })
            ) : (
              <DropdownMenuItem disabled>
                <span>{t("composer.attachment.empty")}</span>
              </DropdownMenuItem>
            )}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <ComposerKnowledgeControl
          conversationId={props.conversationId}
          draftSelection={props.draftKnowledgeSelection}
          onDraftSelectionChange={props.onDraftKnowledgeSelectionChange}
          onOpenKnowledge={props.onOpenKnowledge}
          projectId={props.projectId}
        />
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <BrainCircuit size={15} />
            <span>{t("composer.thinkingProcess")}</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="composer-thinking-menu">
            {THINKING_DISPLAY_MODE_OPTIONS.map((option) => (
              <DropdownMenuItem key={option.value} onSelect={() => props.onThinkingDisplayModeChange(option.value)}>
                <span className="composer-thinking-mode-label">
                  <span>{t(option.labelKey)}</span>
                </span>
                {props.thinkingDisplayMode === option.value ? <CheckCircle2 size={14} /> : null}
              </DropdownMenuItem>
            ))}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuCheckboxItem
          checked={props.planModeEnabled}
          onCheckedChange={(checked) => props.onPlanModeChange(Boolean(checked))}
          onSelect={(event) => event.preventDefault()}
        >
          <ListChecks size={15} />
          <span>{t("composer.planMode")}</span>
        </DropdownMenuCheckboxItem>
        {props.vmmAvailable ? (
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <BrainCircuit size={15} />
              <span>{t("composer.vmmMemory")}</span>
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent className="composer-thinking-menu">
              <VmmPolicySubMenu
                label={t("composer.vmm.precheckInjection")}
                policy="precheck"
                selectedMode={props.sessionVmmPolicy?.precheckMode ?? "inherit"}
                onChange={props.onVmmPolicyChange}
              />
              <VmmPolicySubMenu
                label={t("composer.vmm.memoryWriteback")}
                policy="writeback"
                selectedMode={props.sessionVmmPolicy?.memoryWritebackMode ?? "inherit"}
                onChange={props.onVmmPolicyChange}
              />
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        ) : null}
        <DropdownMenuItem onSelect={props.onOpenGoal}>
          <Target size={15} />
          <span>{t("composer.goal")}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// VmmPolicySubMenu renders one VMM policy's tri-state choice menu.
// VmmPolicySubMenu 渲染单个 VMM 策略的三态选择菜单。
function VmmPolicySubMenu(props: {
  // User-facing VMM policy label.
  // 面向用户的 VMM 策略标签。
  label: string;
  // Backend VMM policy key.
  // 后端 VMM 策略键。
  policy: string;
  // Currently selected tri-state mode.
  // 当前选中的三态模式。
  selectedMode: string;
  // Applies one selected policy mode.
  // 应用单个选中的策略模式。
  onChange: (policy: string, mode: string) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <span>{props.label}</span>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent>
        {VMM_POLICY_MODE_OPTIONS.map((option) => (
          <DropdownMenuItem
            key={option.mode}
            onSelect={() => props.onChange(props.policy, option.mode)}
          >
            <span>{t(option.labelKey)}</span>
            {props.selectedMode === option.mode ? <CheckCircle2 size={14} /> : null}
          </DropdownMenuItem>
        ))}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  );
}

// VMM_POLICY_MODE_OPTIONS lists the session-level VMM policy modes.
// VMM_POLICY_MODE_OPTIONS 列出会话级 VMM 策略模式。
const VMM_POLICY_MODE_OPTIONS: Array<{ mode: string; labelKey: TranslationKey }> = [
  { mode: "inherit", labelKey: "composer.vmm.mode.inheritProject" },
  { mode: "enabled", labelKey: "composer.vmm.mode.enabled" },
  { mode: "disabled", labelKey: "composer.vmm.mode.disabled" },
];

// composerAttachmentKindLabelKey returns the localized label key for one attachment kind.
// composerAttachmentKindLabelKey 返回单个附件类型的本地化标签键。
function composerAttachmentKindLabelKey(kind: string): TranslationKey | null {
  if (kind === "image") return "composer.attachment.kind.image";
  if (kind === "video") return "composer.attachment.kind.video";
  if (kind === "audio") return "composer.attachment.kind.audio";
  if (kind === "pdf") return "composer.attachment.kind.pdf";
  return null;
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
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
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
              aria-label={t("composer.attachment.remove")}
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
