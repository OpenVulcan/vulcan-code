import React from "react";
import {
  BrainCircuit,
  CheckCircle2,
  CircleAlert,
  FileText,
  Image as ImageIcon,
  ListChecks,
  Music2,
  Paperclip,
  Palette,
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
  ComposerAttachmentPreparationSummary,
  ComposerAttachmentSummary,
  MediaUploadProgressChangedDelta,
  OutputStyleCatalogSummary,
  SessionOutputStyleSummary,
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
  // Output-style catalog projected by the shared service.
  // 共享服务投影的输出风格目录。
  outputStyleCatalog: OutputStyleCatalogSummary;
  // Current persisted session output-style state, when editing an existing conversation.
  // 编辑已有会话时的持久化输出风格状态。
  sessionOutputStyle?: SessionOutputStyleSummary | null;
  // Draft-only output-style override for a new conversation.
  // 新会话草稿的输出风格覆盖。
  draftOutputStyleId?: string | null;
  // Applies one output-style selection without changing the active prompt context.
  // 应用输出风格选择，但不直接改变当前提示词上下文。
  onOutputStyleChange: (styleId: string | null) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const menuCapabilities = props.capabilities.filter((capability) => ["image", "video", "audio", "pdf", "file"].includes(capability.kind));
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
        <OutputStyleSubMenu
          catalog={props.outputStyleCatalog}
          conversationId={props.conversationId}
          draftStyleId={props.draftOutputStyleId}
          onChange={props.onOutputStyleChange}
          sessionOutputStyle={props.sessionOutputStyle}
        />
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

// OutputStyleSubMenu renders the selectable prompt-style variants and their persistence cost.
// OutputStyleSubMenu 渲染可选择的提示词风格变体及其持久化成本。
function OutputStyleSubMenu(props: {
  catalog: OutputStyleCatalogSummary;
  conversationId: string | null;
  draftStyleId?: string | null;
  sessionOutputStyle?: SessionOutputStyleSummary | null;
  onChange: (styleId: string | null) => void;
}) {
  const { locale } = useI18n();
  const selectedId = props.conversationId
    ? props.sessionOutputStyle?.overrideStyleId ?? null
    : props.draftStyleId ?? null;
  const globalEntry = props.catalog.entries.find((entry) => entry.id === props.catalog.selectedId);
  const globalLabel = globalEntry
    ? locale === "en-US" ? globalEntry.labelEn : globalEntry.labelZh
    : props.catalog.selectedId;
  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger>
        <Palette size={15} />
        <span>{locale === "en-US" ? "Output style" : "输出风格"}</span>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="composer-thinking-menu">
        <DropdownMenuItem onSelect={() => props.onChange(null)}>
          <span className="composer-thinking-mode-label">
            <span>{locale === "en-US" ? "Inherit global" : "继承全局"}</span>
            <small>
              {locale === "en-US"
                ? `Use system setting: ${globalLabel}`
                : `使用系统设置：${globalLabel}`}
            </small>
          </span>
          {selectedId === null ? <CheckCircle2 size={14} /> : null}
        </DropdownMenuItem>
        {props.catalog.entries.map((entry) => (
          <DropdownMenuItem key={entry.id} onSelect={() => props.onChange(entry.id)}>
            <span className="composer-thinking-mode-label">
              <span>{locale === "en-US" ? entry.labelEn : entry.labelZh}</span>
              <small>{`${entry.constraintLevel} · ${entry.promptCharCount} chars`}</small>
            </span>
            {selectedId === entry.id ? <CheckCircle2 size={14} /> : null}
          </DropdownMenuItem>
        ))}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
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
  if (kind === "file") return "composer.attachment.kind.file";
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
  // Draft preparation states indexed by stable attachment identity.
  // 按稳定附件身份索引的草稿准备状态。
  preparations?: Readonly<Record<string, ComposerAttachmentPreparationSummary>>;
  // Active transfer progress indexed by stable attachment identity.
  // 按稳定附件身份索引的活跃传输进度。
  progresses?: Readonly<Record<string, MediaUploadProgressChangedDelta>>;
  // Resolves the minimal preview media for one attachment.
  // 解析单个附件的最小预览媒体信息。
  resolvePreview: (attachment: ComposerAttachmentSummary) => ComposerAttachmentPreviewMedia | null;
  // Opens an attachment preview.
  // 打开附件预览。
  onOpenMedia: (attachment: ComposerAttachmentSummary) => void;
  // Removes one attachment by index.
  // 按索引移除单个附件。
  onRemove?: (index: number) => void;
  // Retries one failed provider upload by stable attachment identity.
  // 按稳定附件身份重试单个失败供应商上传。
  onRetry?: (attachmentId: string) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <div className="composer-attachment-strip">
      {props.attachments.map((attachment, index) => {
        const attachmentId = attachment.assetId?.trim() ?? "";
        const preparation = attachmentId ? props.preparations?.[attachmentId] : undefined;
        const progress = attachmentId ? props.progresses?.[attachmentId] : undefined;
        const totalBytes = progress?.totalBytes ?? null;
        const progressPercent = preparation?.phase === "failed"
          || preparation?.phase === "cancelled"
          ? null
          : preparation?.phase === "awaiting_provider_confirmation"
          ? 99
          : totalBytes && totalBytes > 0
            ? Math.min(99, Math.max(0, Math.floor((progress?.bytesSent ?? 0) * 100 / totalBytes)))
            : preparation?.blocksSubmission ? 0 : null;
        const degraded = preparation?.deliveryPlan === "metadata_only"
          || preparation?.phase === "failed";
        return (
        <div
          className={cn("composer-attachment-tile", {
            "image-attachment": attachment.kind === "image",
            "attachment-preparing": preparation?.blocksSubmission,
            "attachment-degraded": degraded,
          })}
          key={`${attachment.localPath}-${index}`}
        >
          <button
            aria-label={attachment.fileName}
            className="composer-attachment-open"
            onClick={() => props.onOpenMedia(attachment)}
            type="button"
          />
          <ComposerAttachmentPreview attachment={attachment} resolvePreview={props.resolvePreview} />
          {attachment.kind === "image" ? null : (
            <span className="composer-attachment-name">{attachment.fileName}</span>
          )}
          {props.onRemove ? (
            <button
              aria-label={t("composer.attachment.remove")}
              className="composer-attachment-remove"
              onClick={(event) => {
                event.stopPropagation();
                props.onRemove?.(index);
              }}
              type="button"
            >
              <X size={12} />
            </button>
          ) : null}
          {typeof progressPercent === "number" ? (
            <span
              aria-label={preparation?.phase === "awaiting_provider_confirmation"
                ? t("composer.attachment.processing")
                : `${progressPercent}%`}
              className="composer-attachment-progress"
              role="status"
            >
              <svg aria-hidden="true" viewBox="0 0 36 36">
                <circle className="composer-attachment-progress-track" cx="18" cy="18" r="15" />
                <circle
                  className="composer-attachment-progress-value"
                  cx="18"
                  cy="18"
                  pathLength="100"
                  r="15"
                  style={{ strokeDasharray: `${progressPercent} 100` }}
                />
              </svg>
              <span>{progressPercent}%</span>
            </span>
          ) : null}
          {degraded ? (
            <>
              <span
                aria-label={t("composer.attachment.metadataOnly")}
                className="composer-attachment-warning"
                role="status"
                title={preparation?.error ?? t("composer.attachment.metadataOnly")}
              >
                <CircleAlert size={16} />
              </span>
              {preparation?.phase === "failed"
                && preparation.deliveryPlan === "upload_reference"
                && props.onRetry
                && attachmentId ? (
                <button
                  className="composer-attachment-retry"
                  onClick={(event) => {
                    event.stopPropagation();
                    props.onRetry?.(attachmentId);
                  }}
                  type="button"
                >
                  {t("composer.attachment.retry")}
                </button>
              ) : null}
            </>
          ) : null}
        </div>
        );
      })}
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
