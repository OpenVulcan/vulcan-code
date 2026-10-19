import React from "react";
import { createPortal } from "react-dom";
import { ComposerSettingsHostContext } from "./composer-settings-host";
import { setConversationToolCollapseOverride, useToolCollapsePreference } from "@/components/conversation/tool-collapse-preferences";
import { setConversationThinkingPopupOverride, useThinkingDisplayPreference } from "@/components/conversation/thinking-display-preferences";
import { composerAttachmentMedia } from "./composer-attachment-media";
import { MediaPathContextMenu } from "@/components/media/media-path-context-menu";
import { MemoryBrainIcon } from "@/components/shell/memory-brain-icon";
import { composerAttachmentIdentity } from "@/service/composer-attachment-reference";
import {
  CircleAlert,
  FileText,
  Image as ImageIcon,
  LayoutPanelTop,
  ListChecks,
  Music2,
  Paperclip,
  Palette,
  Plus,
  RotateCcw,
  Settings,
  Sparkles,
  Target,
  Video,
  Workflow,
  X,
} from "lucide-react";

import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { Popover, PopoverContent, PopoverTrigger } from "@/ui-kit/primitives/popover";
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
} from "@/ui-kit/primitives/dropdown-menu";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";
import {
  THINKING_DISPLAY_MODE_OPTIONS,
  type ThinkingDisplayMode,
} from "@/components/conversation/thinking-display";
import { formatMessage, pickBilingualText, useI18n, type TranslationKey } from "@/i18n";
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
import {
  ComposerSettingsRequestContext,
  RequestedSettingsSub,
  type ComposerSettingsRequest,
  type ComposerSettingsSection,
} from "./composer-settings-request";
import { useDedicatedComposer } from "./dedicated/dedicated-session-controls";
import type { ComposerAnswerModeController } from "./composer-panel/use-composer-answer-mode";

// ComposerAttachmentMenu renders the plus button and attachment submenu.
// ComposerAttachmentMenu 渲染加号按钮与附件子菜单。
// Parameters: props supplies attachment capabilities, output styles and selection callbacks; returns the accessible add menu.
// 参数：props 提供附件能力、输出风格与选择回调；返回可访问的添加菜单。
export function ComposerAttachmentMenu(props: {
  // Owner-scoped answer mode for the current conversation or draft.
  // 当前会话或草稿的按归属限定回答模式。
  answerMode: ComposerAnswerModeController;
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
  // Latest slash-command request to open the settings menu, optionally at one submenu.
  // 最近一次由 slash 指令发起的设置菜单打开请求，可指定子菜单。
  settingsRequest?: ComposerSettingsRequest | null;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // settingsHost is the actual footer node; the composer retains all session and draft ownership.
  // settingsHost 是实际底栏节点；输入区继续持有全部会话与草稿状态。
  const settingsHost = React.useContext(ComposerSettingsHostContext);
  // Dedicated sessions freeze Plan mode and goals at creation, so their entries are not offered.
  // 专职会话在创建时冻结计划模式与目标，因此不提供这些入口。
  const dedicated = useDedicatedComposer();
  const menuCapabilities = props.capabilities.filter((capability) => ["image", "video", "audio", "pdf", "file"].includes(capability.kind));
  return (
    <>
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button
              aria-label={t("composer.attachment.add")}
              className="vk-pick-add"
              type="button"
            >
              <Plus size={17} />
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent className="vk-pick-tip" side="top">
          <strong>{t("composer.attachment.extension")}</strong>
        </TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="start" className="vk-pick-menu" side="top" sideOffset={8}>
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
        {dedicated ? null : <>
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
        </>}
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
  // Settings frozen with a dedicated session (style, answer mode, knowledge, memory) are hidden.
  // 随专职会话冻结的设置（风格、回答模式、知识库、记忆）不再显示。
  const dedicated = useDedicatedComposer();
  // Slash commands open this menu, optionally at one submenu; closing it clears the request.
  // slash 指令可打开本菜单并直接展开某个子菜单；关闭菜单即清除该请求。
  const [menuOpen, setMenuOpen] = React.useState(false);
  const [requestedSection, setRequestedSection] = React.useState<ComposerSettingsSection | null>(null);
  React.useEffect(() => {
    if (props.settingsRequest) {
      setRequestedSection(props.settingsRequest.section);
      setMenuOpen(true);
    }
  }, [props.settingsRequest]);
  return <DropdownMenu open={menuOpen} onOpenChange={(open) => {
    setMenuOpen(open);
    if (!open) setRequestedSection(null);
  }}>
    <DropdownMenuTrigger asChild>
      <button aria-label={t("composer.sessionSettings")} className="vk-status-item" data-action="session-settings" type="button"><Settings /><span>{t("composer.sessionSettings")}</span></button>
    </DropdownMenuTrigger>
    <ComposerSettingsRequestContext.Provider value={requestedSection}>
    <DropdownMenuContent align="start" className="vk-pick-menu" side="top" sideOffset={8}>
        {dedicated ? null : <>
        <OutputStyleSubMenu
          catalog={props.outputStyleCatalog}
          conversationId={props.conversationId}
          draftStyleId={props.draftOutputStyleId}
          onChange={props.onOutputStyleChange}
          sessionOutputStyle={props.sessionOutputStyle}
        />
        <AnswerModeSubMenu controller={props.answerMode} />
        <ComposerKnowledgeControl
          conversationId={props.conversationId}
          draftSelection={props.draftKnowledgeSelection}
          onDraftSelectionChange={props.onDraftKnowledgeSelectionChange}
          onOpenKnowledge={props.onOpenKnowledge}
          projectId={props.projectId}
        />
        </>}
        <RequestedSettingsSub section="tools">
          <DropdownMenuSubTrigger><Workflow size={15} /><span>{t("composer.collapseTools")}</span></DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="vk-pick-menu">
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
        </RequestedSettingsSub>
        <RequestedSettingsSub section="thinking">
          <DropdownMenuSubTrigger>
            {/* Same sparkle as the conversation's thinking trigger. */}
            {/* 与对话中思考入口相同的闪光星形图标。 */}
            <Sparkles size={15} />
            <span>{t("composer.thinkingProcess")}</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="vk-pick-menu">
            <DropdownMenuRadioGroup value={thinkingPreference.override === undefined ? "inherit" : thinkingPreference.override ? "popup_only" : "inline_output"}>
            {THINKING_DISPLAY_MODE_OPTIONS.map((option) => (
              <DropdownMenuRadioItem key={option.value} value={option.value} disabled={toolCollapse.scope === null} onSelect={() => props.onThinkingDisplayModeChange(option.value)}>
                <span className="vk-menu-label">
                  <span>{t(option.labelKey)}</span>
                </span>
              </DropdownMenuRadioItem>
            ))}
            <DropdownMenuRadioItem value="inherit" disabled={toolCollapse.scope === null} onSelect={() => {
              if (toolCollapse.scope !== null) setConversationThinkingPopupOverride(toolCollapse.scope, null);
            }}>{t("composer.inheritSystem")}</DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </RequestedSettingsSub>
        {!dedicated && (props.vmmAvailable || props.onOpenMemorySettings) ? (
          <RequestedSettingsSub section="memory">
            <DropdownMenuSubTrigger>
              {/* Same brain as the memory area in the activity rail. */}
              {/* 与活动栏记忆区域相同的大脑图标。 */}
              <MemoryBrainIcon size={15} />
              <span>{t("composer.vmmMemory")}</span>
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent className="vk-pick-menu">
              {props.vmmStatus ? <p className="vk-menu-note">{t(`memory.workspace.${props.vmmStatus}`)}</p> : null}
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
          </RequestedSettingsSub>
        ) : null}
    </DropdownMenuContent>
    </ComposerSettingsRequestContext.Provider>
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
    ? pickBilingualText(locale, globalEntry.labelZh, globalEntry.labelEn)
    : props.catalog.selectedId;
  return (
    <RequestedSettingsSub section="style">
      <DropdownMenuSubTrigger>
        <Palette size={15} />
        <span>{t("composer.outputStyle.menuTitle")}</span>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="vk-pick-menu">
        <DropdownMenuRadioGroup value={selectedId === null ? "inherit" : `style:${selectedId}`}>
        <DropdownMenuRadioItem value="inherit" onSelect={() => props.onChange(null)}>
          <span className="vk-menu-label">
            <span>{t("composer.outputStyle.inheritGlobal")}</span>
            <small>
              {formatMessage(t("composer.outputStyle.useSystemSetting"), { label: globalLabel })}
            </small>
          </span>
        </DropdownMenuRadioItem>
        {props.catalog.entries.map((entry) => (
          <DropdownMenuRadioItem key={entry.id} value={`style:${entry.id}`} onSelect={() => props.onChange(entry.id)}>
            <span className="vk-menu-label">
              <span>{pickBilingualText(locale, entry.labelZh, entry.labelEn)}</span>
              <small>{formatMessage(t("composer.outputStyle.promptCharacters"), { count: entry.promptCharCount.toLocaleString(locale) })}</small>
            </span>
          </DropdownMenuRadioItem>
        ))}
        </DropdownMenuRadioGroup>
        {props.conversationId && props.sessionOutputStyle?.pendingActivation ? <p role="status" className="vk-menu-note">
          {t("composer.outputStyle.savedPending")}
        </p> : null}
      </DropdownMenuSubContent>
    </RequestedSettingsSub>
  );
}

// AnswerModeSubMenu chooses whether answers may use native answer components.
// AnswerModeSubMenu 选择回答是否可使用原生回答组件。
function AnswerModeSubMenu(props: { controller: ComposerAnswerModeController }) {
  const { t } = useI18n();
  const { mode, saving, available, awaitingRequest, select } = props.controller;
  const options = [
    { value: "standard", label: "answerMode.standard", hint: "answerMode.standardHint" },
    { value: "interactive", label: "answerMode.interactive", hint: "answerMode.interactiveHint" },
  ] as const;
  return (
    <RequestedSettingsSub section="answer">
      <DropdownMenuSubTrigger disabled={!available}>
        <LayoutPanelTop size={15} />
        <span>{t("answerMode.label")}</span>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="vk-pick-menu" aria-busy={saving}>
        <DropdownMenuRadioGroup value={mode ?? ""}>
          {options.map((option) => (
            <DropdownMenuRadioItem key={option.value} value={option.value} disabled={saving} onSelect={() => select(option.value)}>
              <span className="vk-menu-label">
                <span>{t(option.label)}</span>
                <small>{t(option.hint)}</small>
              </span>
            </DropdownMenuRadioItem>
          ))}
        </DropdownMenuRadioGroup>
        {awaitingRequest ? <p role="status" className="vk-menu-note">
          {t("answerMode.pending")}
        </p> : null}
      </DropdownMenuSubContent>
    </RequestedSettingsSub>
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
  // Editable image-compression choice; omitted for read-only strips such as sent messages.
  // 可编辑的图片压缩选择；已发送消息等只读附件条省略此项。
  imageCompression?: { enabled: boolean; onChange: (enabled: boolean) => void };
  // Whether image tiles without a plan are currently being compressed by route planning.
  // 尚无规划的图片块当前是否正由路线规划压缩。
  compressing?: boolean;
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
    const buttons = strip.current?.querySelectorAll<HTMLButtonElement>(".vk-attach-remove");
    if (buttons && buttons.length > 0) buttons[Math.min(request.index, buttons.length - 1)].focus({ preventScroll: true });
    else if (props.attachments.length === 0) props.onEmptyAfterRemoval?.();
  }, [props.attachments, props.onEmptyAfterRemoval]);
  if (props.attachments.length === 0) return null;
  return (
    <div className="vk-attach" ref={strip}>
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
        <MediaPathContextMenu
          key={`${attachment.localPath}-${index}`}
          locator={attachment.kind === "image" || attachment.kind === "video" || attachment.kind === "audio" ? attachment.localPath : null}
        >
        <div
          className="vk-attach-tile"
          data-kind={attachment.kind}
          data-preparing={preparation?.blocksSubmission ? "" : undefined}
          data-degraded={degraded ? "" : undefined}
        >
          <button
            aria-label={previewAvailable ? attachment.fileName : `${attachment.fileName} · ${t("composer.attachment.previewUnavailable")}`}
            className="vk-attach-open"
            disabled={!previewAvailable}
            title={previewAvailable ? attachment.fileName : t("composer.attachment.previewUnavailable")}
            onClick={() => props.onOpenMedia(attachment)}
            type="button"
          />
          <ComposerAttachmentPreview attachment={attachment} resolvePreview={props.resolvePreview} />
          {attachment.kind === "image" ? null : (
            <span className="vk-attach-name">{attachment.fileName}</span>
          )}
          {props.onRemove ? (
            <button
              aria-label={`${t("composer.attachment.remove")} · ${attachment.fileName}`}
              className="vk-attach-remove"
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
          {props.compressing && attachment.kind === "image" && !preparation ? (
            <ComposerAttachmentCooldown label={t("composer.attachment.compressing")} />
          ) : null}
          {typeof progressPercent === "number" || pendingProgress ? (
            <span
              aria-label={typeof progressPercent === "number" ? `${progressPercent}%` : t("composer.attachment.processing")}
              className="vk-attach-progress"
              role="status"
            >
              <svg aria-hidden="true" data-spin={progressPercent === null ? "" : undefined} viewBox="0 0 36 36">
                <circle className="vk-attach-progress-track" cx="18" cy="18" r="15" />
                <circle
                  className="vk-attach-progress-value"
                  cx="18"
                  cy="18"
                  pathLength="100"
                  r="15"
                  style={{ strokeDasharray: `${progressPercent ?? 25} 100` }}
                />
              </svg>
              <span className={progressPercent === null ? "vk-sr-only" : undefined}>{typeof progressPercent === "number" ? `${progressPercent}%` : t("composer.attachment.processing")}</span>
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
                      className="vk-attach-retry"
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
        </MediaPathContextMenu>
        );
      })}
      {props.imageCompression && props.attachments.some((attachment) => attachment.kind === "image") ? (
        <Tooltip>
          <TooltipTrigger asChild>
            <label className="vk-attach-compress">
              <Checkbox
                checked={props.imageCompression.enabled}
                onCheckedChange={(checked) => props.imageCompression?.onChange(checked === true)}
              />
              {t("composer.attachment.compressImages")}
            </label>
          </TooltipTrigger>
          <TooltipContent className="vk-pick-tip" side="top">{t("composer.attachment.compressImagesHint")}</TooltipContent>
        </Tooltip>
      ) : null}
    </div>
  );
}

// COOLDOWN_TURN_MS is one full sweep of the compression overlay.
// COOLDOWN_TURN_MS 是压缩遮罩完整转一圈的时长。
const COOLDOWN_TURN_MS = 1200;

/** Shows a game-style cooldown sweep over an image while it is being compressed.
 * 在图片压缩期间显示类似游戏技能冷却的扫描遮罩。
 * Parameters: label is the accessible status text; returns a non-interactive overlay that reveals the image in one turn.
 * 参数：label 是无障碍状态文本；返回一个不可交互遮罩，转一圈后完全露出图片。
 */
function ComposerAttachmentCooldown(props: { label: string }) {
  // wedge is the shrinking dark sector; the Web Animations API keeps this feature out of the eager stylesheet.
  // wedge 是逐渐缩小的暗色扇区；Web Animations API 让此功能不进入首屏样式表。
  const wedge = React.useRef<SVGCircleElement>(null);
  React.useEffect(() => {
    const element = wedge.current;
    if (!element || typeof element.animate !== "function") return;
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;
    const animation = element.animate(
      [{ strokeDashoffset: 0 }, { strokeDashoffset: -100 }],
      { duration: COOLDOWN_TURN_MS, easing: "linear", iterations: Infinity },
    );
    return () => animation.cancel();
  }, []);
  return (
    <span aria-label={props.label} className="vk-attach-cooldown" role="status">
      <svg aria-hidden="true" preserveAspectRatio="xMidYMid slice" viewBox="0 0 100 100">
        {/* A radius-40 circle with stroke width 80 paints a disc that also covers the corners, so its dash is the remaining sector. */}
        {/* 半径 40、描边宽 80 的圆会画出覆盖四角的圆盘，因此虚线就是剩余扇区。 */}
        <circle
          cx="50"
          cy="50"
          fill="none"
          pathLength="100"
          r="40"
          ref={wedge}
          stroke="var(--vk-ink)"
          strokeDasharray="100 100"
          strokeOpacity="0.45"
          strokeWidth="80"
          transform="rotate(-90 50 50)"
        />
      </svg>
      <span className="vk-sr-only">{props.label}</span>
    </span>
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
      <TooltipTrigger asChild><span aria-label={t("composer.attachment.metadataOnly")} className="vk-attach-warn" role="status" tabIndex={0}><CircleAlert size={16} /></span></TooltipTrigger>
      <TooltipContent side="top">{t("composer.attachment.metadataOnly")}</TooltipContent>
    </Tooltip>;
  }
  return <Popover>
    <Tooltip>
      <TooltipTrigger asChild><PopoverTrigger asChild>
        <button aria-label={`${t("composer.attachment.details")} · ${props.fileName}`} className="vk-attach-warn" type="button"><CircleAlert size={16} /></button>
      </PopoverTrigger></TooltipTrigger>
      <TooltipContent side="top">{t("composer.attachment.details")}</TooltipContent>
    </Tooltip>
    <PopoverContent aria-label={`${t("composer.attachment.details")} · ${props.fileName}`} className="vk-attach-detail" collisionPadding={12} side="top">
      <strong>{props.fileName}</strong>
      <p>{t(props.failed ? "composer.attachment.failed" : "composer.attachment.metadataOnly")}</p>
      {props.error !== null ? <pre className="vk-attach-detail-text" tabIndex={0}>{props.error}</pre> : null}
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
