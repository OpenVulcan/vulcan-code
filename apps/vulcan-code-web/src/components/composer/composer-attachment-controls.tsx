import React from "react";
import { createPortal } from "react-dom";
import { ComposerSettingsHostContext } from "./composer-settings-host";
import { setConversationToolCollapseOverride, useToolCollapsePreference } from "@/components/conversation/tool-collapse-preferences";
import { setConversationThinkingPopupOverride, useThinkingDisplayPreference } from "@/components/conversation/thinking-display-preferences";
import { composerAttachmentMedia } from "./composer-attachment-media";
import { VceComposerMenu } from "@/components/features/code-context/vce-project-controls";
import { composerAttachmentIdentity } from "@/service/composer-attachment-reference";
import {
  BrainCircuit,
  CircleAlert,
  FileText,
  Image as ImageIcon,
  ListChecks,
  Music2,
  Paperclip,
  Palette,
  Plus,
  RotateCcw,
  Settings,
  Target,
  Video,
  Workflow,
  X,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
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
import { formatMessage, useI18n, type TranslationKey } from "@/i18n";
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
// Parameters: props supplies attachment capabilities, output styles and selection callbacks; returns the accessible add menu.
// 参数：props 提供附件能力、输出风格与选择回调；返回可访问的添加菜单。
export function ComposerAttachmentMenu(props: {
  // Enabled attachment capability gates available for this prompt.
  // 当前提示词可用的已启用附件能力门控。
  capabilities: ComposerAttachmentCapabilitySummary[];
  // Thinking display mode selected by the footer settings menu.
  // 底栏设置菜单选中的思考展示模式。
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
  // Readiness is displayed independently of policy editability; absent status preserves embedded callers.
  // 就绪状态与策略可编辑性分开展示；未传状态时保留嵌入调用方行为。
  vmmStatus?: "disabled" | "not_ready" | "ready";
  // Opens memory configuration without changing any global or session policy.
  // 打开记忆配置，不改变任何全局或会话策略。
  onOpenMemorySettings?: () => void;
  // Opens a file picker for one attachment kind or all kinds.
  // 按附件类型或全部类型打开文件选择器。
  onPick: (kind: string | null) => void;
  // Updates the thinking display mode from the footer settings menu.
  // 从底栏设置菜单更新思考展示模式。
  onThinkingDisplayModeChange: (mode: ThinkingDisplayMode) => void;
  // Updates the session Plan mode from the plus menu.
  // 从加号菜单更新会话计划模式。
  onPlanModeChange: (enabled: boolean) => void;
  // Updates one selected-session VMM memory policy from the footer settings menu.
  // 从底栏设置菜单更新当前会话的单个 VMM 记忆策略。
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
  // settingsHost is the actual footer node; the composer retains all session and draft ownership.
  // settingsHost 是实际底栏节点；输入区继续持有全部会话与草稿状态。
  const settingsHost = React.useContext(ComposerSettingsHostContext);
  const menuCapabilities = props.capabilities.filter((capability) => ["image", "video", "audio", "pdf", "file"].includes(capability.kind));
  return (
    <>
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
        <DropdownMenuCheckboxItem
          checked={props.planModeEnabled}
          onCheckedChange={(checked) => props.onPlanModeChange(Boolean(checked))}
          onSelect={(event) => event.preventDefault()}
        >
          <ListChecks size={15} />
          <span>{t("composer.planMode")}</span>
        </DropdownMenuCheckboxItem>
        <DropdownMenuItem onSelect={props.onOpenGoal}>
          <Target size={15} />
          <span>{t("composer.goal")}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
    {settingsHost ? createPortal(<ComposerSettingsMenu key={props.conversationId ?? props.projectId ?? "new"} {...props} />, settingsHost) : null}
    </>
  );
}

/** Renders the relocated settings menu using the original composer's controlled state and callbacks.
 * 使用原输入区的受控状态和回调渲染迁移后的设置菜单。
 * Parameter props carries the active conversation or draft options; returns the conversation settings menu.
 * 参数 props 携带当前会话或草稿选项；返回会话设置菜单。
 */
export function ComposerSettingsMenu(props: React.ComponentProps<typeof ComposerAttachmentMenu>) {
  // t keeps the footer entry and its options aligned with the active locale.
  // t 使底栏入口及其选项与当前语言一致。
  const { t } = useI18n();
  // The provider identifies the selected draft as well as persisted conversations.
  // Provider 同时标识当前草稿和持久化会话。
  const toolCollapse = useToolCollapsePreference();
  // Explicit selection stays distinct from the effective global value.
  // 明确选择始终与实际生效的全局值区分。
  const thinkingPreference = useThinkingDisplayPreference(toolCollapse.scope, props.thinkingDisplayMode);
  return <DropdownMenu>
    <DropdownMenuTrigger asChild>
      <Button aria-label={t("composer.sessionSettings")} className="workspace-settings-button" size="sm" type="button" variant="ghost"><Settings size={14} /><span>{t("composer.sessionSettings")}</span></Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="start" className="composer-settings-menu" side="top" sideOffset={8}>
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
        <VceComposerMenu />
        <DropdownMenuSub>
          <DropdownMenuSubTrigger><Workflow size={15} /><span>{t("composer.collapseTools")}</span></DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="composer-thinking-menu">
            <DropdownMenuRadioGroup value={toolCollapse.override === undefined ? "inherit" : toolCollapse.override ? "enabled" : "disabled"}>
              <DropdownMenuRadioItem value="disabled" disabled={toolCollapse.scope === null} onSelect={() => {
                if (toolCollapse.scope !== null) setConversationToolCollapseOverride(toolCollapse.scope, false);
              }}>{t("composer.collapseToolsDisabled")}</DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="enabled" disabled={toolCollapse.scope === null} onSelect={() => {
                if (toolCollapse.scope !== null) setConversationToolCollapseOverride(toolCollapse.scope, true);
              }}>{t("composer.collapseToolsEnabled")}</DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="inherit" disabled={toolCollapse.scope === null} onSelect={() => {
                if (toolCollapse.scope !== null) setConversationToolCollapseOverride(toolCollapse.scope, null);
              }}>{t("composer.inheritSystem")}</DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <BrainCircuit size={15} />
            <span>{t("composer.thinkingProcess")}</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="composer-thinking-menu">
            <DropdownMenuRadioGroup value={thinkingPreference.override === undefined ? "inherit" : thinkingPreference.override ? "popup_only" : "inline_output"}>
            {THINKING_DISPLAY_MODE_OPTIONS.map((option) => (
              <DropdownMenuRadioItem key={option.value} value={option.value} disabled={toolCollapse.scope === null} onSelect={() => props.onThinkingDisplayModeChange(option.value)}>
                <span className="composer-thinking-mode-label">
                  <span>{t(option.labelKey)}</span>
                </span>
              </DropdownMenuRadioItem>
            ))}
            <DropdownMenuRadioItem value="inherit" disabled={toolCollapse.scope === null} onSelect={() => {
              if (toolCollapse.scope !== null) setConversationThinkingPopupOverride(toolCollapse.scope, null);
            }}>{t("composer.inheritSystem")}</DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        {props.vmmAvailable || props.onOpenMemorySettings ? (
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>
              <BrainCircuit size={15} />
              <span>{t("composer.vmmMemory")}</span>
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent className="composer-thinking-menu">
              {props.vmmStatus ? <p className="px-2 py-1.5 text-xs text-muted-foreground">{t(`memory.workspace.${props.vmmStatus}`)}</p> : null}
              {props.vmmAvailable ? <>
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
              </> : null}
              {props.onOpenMemorySettings ? <DropdownMenuItem onSelect={props.onOpenMemorySettings}>{t("memory.workspace.settings")}</DropdownMenuItem> : null}
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        ) : null}
    </DropdownMenuContent>
  </DropdownMenu>;
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
  const { locale, t } = useI18n();
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
        <span>{t("composer.outputStyle.menuTitle")}</span>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="composer-thinking-menu">
        <DropdownMenuRadioGroup value={selectedId === null ? "inherit" : `style:${selectedId}`}>
        <DropdownMenuRadioItem value="inherit" onSelect={() => props.onChange(null)}>
          <span className="composer-thinking-mode-label">
            <span>{t("composer.outputStyle.inheritGlobal")}</span>
            <small>
              {formatMessage(t("composer.outputStyle.useSystemSetting"), { label: globalLabel })}
            </small>
          </span>
        </DropdownMenuRadioItem>
        {props.catalog.entries.map((entry) => (
          <DropdownMenuRadioItem key={entry.id} value={`style:${entry.id}`} onSelect={() => props.onChange(entry.id)}>
            <span className="composer-thinking-mode-label">
              <span>{locale === "en-US" ? entry.labelEn : entry.labelZh}</span>
              <small>{formatMessage(t("composer.outputStyle.promptCharacters"), { count: entry.promptCharCount.toLocaleString(locale) })}</small>
            </span>
          </DropdownMenuRadioItem>
        ))}
        </DropdownMenuRadioGroup>
        {props.conversationId && props.sessionOutputStyle?.pendingActivation ? <p role="status" className="px-2 py-1.5 text-xs text-muted-foreground">
          {t("composer.outputStyle.savedPending")}
        </p> : null}
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
        <DropdownMenuRadioGroup value={props.selectedMode}>
        {VMM_POLICY_MODE_OPTIONS.map((option) => (
          <DropdownMenuRadioItem
            key={option.mode}
            value={option.mode}
            onSelect={() => props.onChange(props.policy, option.mode)}
          >
            <span>{t(option.labelKey)}</span>
          </DropdownMenuRadioItem>
        ))}
        </DropdownMenuRadioGroup>
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
// Parameters: props supplies staged files, preparation snapshots and preview/removal callbacks; returns file tiles with truthful progress and actions.
// 参数：props 提供暂存文件、准备快照及预览和移除回调；返回包含真实进度与操作的文件块。
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
  // Returns keyboard focus to the owning composer after the final attachment was actually removed.
  // 最后一个附件实际移除后，将键盘焦点交回所属输入区。
  onEmptyAfterRemoval?: () => void;
  // Retries one failed provider upload by stable attachment identity.
  // 按稳定附件身份重试单个失败供应商上传。
  onRetry?: (attachmentId: string) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // strip and removalFocus retain the exact DOM origin, so delayed removals never steal focus from another control.
  // strip 与 removalFocus 保留精确 DOM 来源，避免延迟移除抢走其他控件的焦点。
  const strip = React.useRef<HTMLDivElement>(null);
  const removalFocus = React.useRef<{ attachment: ComposerAttachmentSummary; index: number; trigger: HTMLButtonElement } | null>(null);
  React.useLayoutEffect(() => {
    // request remains pending while cancellation is unresolved or failed and the original attachment remains present.
    // 取消尚未完成或失败且原附件仍存在时，request 保持等待。
    const request = removalFocus.current;
    if (request === null || props.attachments.includes(request.attachment)) return;
    removalFocus.current = null;
    if (request.trigger.isConnected || document.activeElement !== document.body) return;
    // buttons belong only to this exact strip; removal shifts the next item into the removed item's former position.
    // buttons 仅属于此精确附件条；移除会让下一项进入被移除项原先的位置。
    const buttons = strip.current?.querySelectorAll<HTMLButtonElement>(".composer-attachment-remove");
    if (buttons && buttons.length > 0) buttons[Math.min(request.index, buttons.length - 1)].focus({ preventScroll: true });
    else if (props.attachments.length === 0) props.onEmptyAfterRemoval?.();
  }, [props.attachments, props.onEmptyAfterRemoval]);
  if (props.attachments.length === 0) return null;
  return (
    <div className="composer-attachment-strip" ref={strip}>
      {props.attachments.map((attachment, index) => {
        const attachmentId = composerAttachmentIdentity(attachment, index);
        const preparation = attachmentId ? props.preparations?.[attachmentId] : undefined;
        const progress = attachmentId ? props.progresses?.[attachmentId] : undefined;
        const totalBytes = progress?.totalBytes ?? null;
        const progressPercent = preparation?.phase === "failed"
          || preparation?.phase === "cancelled"
          || preparation?.phase === "awaiting_provider_confirmation"
          ? null
          : totalBytes && totalBytes > 0
            ? Math.min(100, Math.max(0, Math.floor((progress?.bytesSent ?? 0) * 100 / totalBytes)))
            : null;
        // previewAvailable follows the same closed media projection used by the actual viewer.
        // previewAvailable 遵循实际查看器使用的同一封闭媒体投影。
        const previewAvailable = composerAttachmentMedia(attachment) !== null;
        // pendingProgress excludes terminal failures even when their submission barrier remains set.
        // pendingProgress 排除仍保留发送屏障的终止失败状态，避免将失败展示为继续处理中。
        const pendingProgress = preparation?.blocksSubmission
          && preparation.phase !== "failed" && preparation.phase !== "cancelled";
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
            aria-label={previewAvailable ? attachment.fileName : `${attachment.fileName} · ${t("composer.attachment.previewUnavailable")}`}
            className="composer-attachment-open"
            disabled={!previewAvailable}
            title={previewAvailable ? attachment.fileName : t("composer.attachment.previewUnavailable")}
            onClick={() => props.onOpenMedia(attachment)}
            type="button"
          />
          <ComposerAttachmentPreview attachment={attachment} resolvePreview={props.resolvePreview} />
          {attachment.kind === "image" ? null : (
            <span className="composer-attachment-name">{attachment.fileName}</span>
          )}
          {props.onRemove ? (
            <button
              aria-label={`${t("composer.attachment.remove")} · ${attachment.fileName}`}
              className="composer-attachment-remove"
              onClick={(event) => {
                event.stopPropagation();
                if (document.activeElement === event.currentTarget) removalFocus.current = { attachment, index, trigger: event.currentTarget };
                props.onRemove?.(index);
              }}
              type="button"
            >
              <X size={12} />
            </button>
          ) : null}
          {typeof progressPercent === "number" || pendingProgress ? (
            <span
              aria-label={typeof progressPercent === "number" ? `${progressPercent}%` : t("composer.attachment.processing")}
              className="composer-attachment-progress"
              role="status"
            >
              <svg aria-hidden="true" className={progressPercent === null ? "motion-safe:animate-spin" : undefined} viewBox="0 0 36 36">
                <circle className="composer-attachment-progress-track" cx="18" cy="18" r="15" />
                <circle
                  className="composer-attachment-progress-value"
                  cx="18"
                  cy="18"
                  pathLength="100"
                  r="15"
                  style={{ strokeDasharray: `${progressPercent ?? 25} 100` }}
                />
              </svg>
              <span className={progressPercent === null ? "sr-only" : undefined}>{typeof progressPercent === "number" ? `${progressPercent}%` : t("composer.attachment.processing")}</span>
            </span>
          ) : null}
          {degraded ? (
            <>
              <ComposerAttachmentNotice fileName={attachment.fileName} failed={preparation?.phase === "failed"} error={preparation?.error ?? null} />
              {preparation?.phase === "failed"
                && preparation.deliveryPlan === "upload_reference"
                && props.onRetry
                && attachmentId ? (
                <Tooltip>
                  <TooltipTrigger asChild>
                    <button
                      aria-label={`${t("composer.attachment.retry")} · ${attachment.fileName}`}
                      className="composer-attachment-retry"
                      onClick={(event) => {
                        event.stopPropagation();
                        props.onRetry?.(attachmentId);
                      }}
                      type="button"
                    >
                      <RotateCcw aria-hidden="true" size={14} />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent side="top">{t("composer.attachment.retry")}</TooltipContent>
                </Tooltip>
              ) : null}
            </>
          ) : null}
        </div>
        );
      })}
    </div>
  );
}

/** Shows compact attachment advice while keeping raw failures in a keyboard-accessible detail popover.
 * 显示紧凑附件说明，将原始失败信息放在可用键盘访问的详情浮层中。
 * Parameters: fileName identifies the file, failed distinguishes blocked preparation, and error is the declared optional diagnostic; returns the notice control.
 * 参数：fileName 标识文件，failed 区分准备失败，error 是明确可选的诊断内容；返回说明控件。
 */
function ComposerAttachmentNotice(props: { fileName: string; failed: boolean; error: string | null }) {
  // t resolves the short notice and action labels independently of raw service text.
  // t 独立于原始服务文本解析简短说明及操作标签。
  const { t } = useI18n();
  if (!props.failed && props.error === null) {
    return <Tooltip>
      <TooltipTrigger asChild><span aria-label={t("composer.attachment.metadataOnly")} className="composer-attachment-warning" role="status" tabIndex={0}><CircleAlert size={16} /></span></TooltipTrigger>
      <TooltipContent className="composer-control-tooltip composer-attachment-notice-tooltip" side="top">{t("composer.attachment.metadataOnly")}</TooltipContent>
    </Tooltip>;
  }
  return <Popover>
    <Tooltip>
      <TooltipTrigger asChild><PopoverTrigger asChild>
        <button aria-label={`${t("composer.attachment.details")} · ${props.fileName}`} className="composer-attachment-warning" type="button"><CircleAlert size={16} /></button>
      </PopoverTrigger></TooltipTrigger>
      <TooltipContent side="top">{t("composer.attachment.details")}</TooltipContent>
    </Tooltip>
    <PopoverContent aria-label={`${t("composer.attachment.details")} · ${props.fileName}`} className="composer-attachment-detail" collisionPadding={12} side="top">
      <strong className="text-sm [overflow-wrap:anywhere]">{props.fileName}</strong>
      <p className="text-xs text-muted-foreground">{t(props.failed ? "composer.attachment.failed" : "composer.attachment.metadataOnly")}</p>
      {props.error !== null ? <pre className="composer-attachment-detail-text" tabIndex={0}>{props.error}</pre> : null}
    </PopoverContent>
  </Popover>;
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
    return <img alt={props.attachment.fileName} decoding="async" src={media.sourceUrl} />;
  }
  if (media?.kind === "video") {
    return <Video size={20} />;
  }
  if (media?.kind === "audio") {
    return <Music2 size={20} />;
  }
  return <FileText size={20} />;
}
