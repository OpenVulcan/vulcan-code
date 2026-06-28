import React from "react";
import { Check, CheckCircle2, Circle, Edit3, ListChecks, PauseCircle, PlayCircle, Route, Search, Target, Trash2, X } from "lucide-react";
import { toast } from "sonner";
import {
  ModelPickerControl,
  modelPickerChoiceKey,
  modelPickerChoicesFromProviders,
  type ModelPickerChoice,
} from "@/components/composer/model-picker-control";
import {
  aiCapabilitySignature,
  composerAttachmentAccept,
  composerSlashCommandMatchesInput,
  defaultAiCapabilityTargetOrder,
  defaultDisabledAiCapabilityKinds,
  defaultDisabledAiCapabilityTargetSelectors,
  exactComposerSlashCommand,
  hasBlockingComposerPopup,
  isComposerAiCapabilityKind,
  isComposerPrintableKey,
  isComposerTextEntryElement,
  normalizedComposerSlashCommandId,
  serializeAiCapabilityTargetOrder,
  type AiCapabilityKind,
  type ComposerAiCapabilityConfigurationSave,
  type ComposerSelectOption,
  type ComposerSlashCommandResult,
  type ComposerSubmission,
} from "@/components/composer/composer-model";
import {
  ComposerAiCapabilityControl,
  ComposerAttachmentMenu,
  ComposerSlashCommandPanel,
  PendingPromptQueueStrip,
} from "@/components/composer/composer-controls";
import { useComposerAttachments } from "@/components/composer/composer-attachments";
import type { ConversationSessionReviewSummary } from "@/components/conversation/session-todolist-panel";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { GeneratedMediaPlayerWindow, type GeneratedMediaAttachment } from "@/components/media/generated-media";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { ProgressRing } from "@/components/ui/progress-ring";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type {
  AiCapabilitySummary,
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ContextUsageSnapshot,
  PendingPromptSummary,
  ProviderSummary,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  ReasoningSelectionSummary,
  SessionGoalSummary,
  SessionPlanModeSummary,
  SessionTodolistItemSummary,
  SessionTodolistSummary,
} from "@/types";
import type { ComposerTraceCallback } from "@/components/composer/composer-control-types";

// ComposerSessionMemoryDraft stores unsent composer content for one conversation slot.
// ComposerSessionMemoryDraft 保存单个会话槽中尚未发送的输入区内容。
import {
  composerSessionDraftKey,
  composerShortcutHint,
  defaultAiCapabilityTargetOrderFromRecord,
  readComposerSessionMemoryDraft,
  writeComposerSessionMemoryDraft,
  type ComposerSessionMemoryDraft,
} from "@/components/composer/composer-panel/helpers";
import { ComposerSessionProgressCapsule } from "@/components/composer/composer-panel/progress-capsule";
import { ModelSecondaryRouteControl, parseModelPickerChoiceKey } from "@/components/composer/composer-panel/model-secondary-route-control";
import { ComposerGoalStrip, ComposerPlanModeChip } from "@/components/composer/composer-panel/goal-strip";
import { ComposerPanelAttachmentPreviewStrip } from "@/components/composer/composer-panel/attachment-preview-strip";
import { ComposerGoalDialog } from "@/components/composer/composer-panel/goal-dialog";
import { ComposerPanelToolbarRight } from "@/components/composer/composer-panel/toolbar-right";
export { ComposerSessionProgressCapsule } from "@/components/composer/composer-panel/progress-capsule";
// ComposerPanel renders the first-version message input surface.
// ComposerPanel 渲染第一版消息输入区域。
export function ComposerPanel(props: {
  // AI capability route summaries used by the quick capability tree.
  // 快捷能力树使用的 AI 能力路由摘要。
  aiCapabilities: AiCapabilitySummary[];
  // Default AI capability route summaries used to reset the session draft.
  // 用于重置会话草稿的默认 AI 能力路由摘要。
  defaultAiCapabilities: AiCapabilitySummary[];
  // Attachment capability gates used by add/drop/paste.
  // 添加、拖拽与粘贴使用的附件能力门控。
  attachmentCapabilities: ComposerAttachmentCapabilitySummary[];
  // Owning project id used for attachment staging.
  // 附件暂存使用的所属项目标识。
  projectId: string | null;
  // Owning conversation id used for attachment staging.
  // 附件暂存使用的所属会话标识。
  conversationId: string | null;
  // Context usage snapshot shown by the composer orb.
  // 输入区圆盘展示的上下文占用快照。
  contextUsage: ContextUsageSnapshot | null;
  // Slash commands projected by the shared workbench service.
  // 共享工作台服务投影出的 slash 指令。
  slashCommands: ComposerSlashCommandSummary[];
  // Visible pending prompts retained for the selected conversation.
  // 当前选中会话保留的可见待发送提示词。
  pendingPrompts: PendingPromptSummary[];
  // Current persisted goal for the selected conversation.
  // 当前选中会话的持久化目标。
  sessionGoal: SessionGoalSummary | null;
  // Current model-authored session progress todolist for the selected conversation.
  // 当前选中会话中模型写入的会话进程 todolist。
  sessionTodolist: SessionTodolistSummary | null;
  // Current Git review summary shown beside compact session progress.
  // 当前用于紧凑会话进程旁展示的 Git 审查摘要。
  sessionReviewSummary: ConversationSessionReviewSummary | null;
  // Current persisted Plan mode state for the selected conversation.
  // 当前选中会话的持久化计划模式状态。
  sessionPlanMode: SessionPlanModeSummary | null;
  // Whether the selected conversation has an active backend run.
  // 当前选中会话是否存在活跃后端运行。
  isConversationRunning: boolean;
  // Thinking display mode selected by the WebUI plus menu.
  // WebUI 加号菜单选中的思考展示模式。
  thinkingDisplayMode: ThinkingDisplayMode;
  // Providers exposed by the shared service API for model picking.
  // 共享 service API 暴露给模型选择器的 Provider 列表。
  providers: ProviderSummary[];
  // Persists one selected workbench model.
  // 持久化单个选中的工作台模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Loads backend-managed secondary routes for one provider/model pair.
  // 加载单个 Provider/模型组合的后端二级路由。
  onListWorkbenchModelSecondaryRoutes: (
    providerId: string,
    modelId: string,
  ) => Promise<ProviderModelSecondaryRouteSummary[]>;
  // Persists one backend-managed secondary route selection.
  // 持久化单个后端二级路由选择。
  onSelectWorkbenchModelSecondaryRoute: (
    providerId: string,
    modelId: string,
    route: ProviderModelSecondaryRouteSelection | null,
  ) => Promise<boolean>;
  // Persists one selected workbench reasoning effort.
  // 持久化单个选中的工作台思考档位。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // Routes one projected slash command through the workbench shell.
  // 通过工作台外壳路由单个投影 slash 指令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Sends one validated composer submission through the caller runtime.
  // 通过调用方运行时发送一条已校验输入区提交。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // Persists one session-scoped AI capability configuration draft.
  // 持久化单个会话级 AI 能力配置草稿。
  onSaveAiCapabilityConfiguration: (request: ComposerAiCapabilityConfigurationSave) => Promise<boolean>;
  // Persists one user-authored session goal.
  // 持久化单个用户编写的会话目标。
  onSaveSessionGoal: (objective: string, tokenBudget?: number | null) => Promise<boolean>;
  // Resumes one stopped session goal with a fresh budget stage.
  // 使用新的预算阶段继续单个已停止会话目标。
  onResumeSessionGoal: (objective: string, tokenBudget?: number | null) => Promise<boolean>;
  // Clears the current user-authored session goal.
  // 清理当前用户编写的会话目标。
  onClearSessionGoal: () => Promise<boolean>;
  // Updates the current session goal lifecycle status.
  // 更新当前会话目标生命周期状态。
  onUpdateSessionGoalStatus: (status: string) => Promise<boolean>;
  // Persists the current session Plan-mode toggle.
  // 持久化当前会话计划模式开关。
  onSetSessionPlanMode: (enabled: boolean) => Promise<boolean>;
  // Stops the selected backend conversation stream.
  // 停止当前选中的后端会话流。
  onStopConversationRun: () => Promise<boolean>;
  // Stages one browser file through the caller-owned service boundary.
  // 通过调用方持有的服务边界暂存单个浏览器文件。
  onStageAttachmentBytes: (
    projectId: string | null,
    conversationId: string | null,
    file: File,
  ) => Promise<ComposerAttachmentSummary | null>;
  // Converts a local media path into a browser-safe resource URL.
  // 将本地媒体路径转换为浏览器安全资源地址。
  resolveMediaSourceUrl: (path: string) => string;
  // Optional trace callback for local UI diagnostics.
  // 用于本地 UI 诊断的可选埋点回调。
  onTrace?: ComposerTraceCallback;
  // Restores one pending prompt into the composer.
  // 将单条待发送提示词恢复到输入区。
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  // Routes one pending prompt into the shared insertion queue.
  // 将单条待发送提示词路由进共享引导队列。
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  // Removes one pending prompt from the visible queue.
  // 从可见队列移除单条待发送提示词。
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  // Persists the pending prompt order.
  // 持久化待发送提示词顺序。
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
  // Updates the WebUI-only thinking display mode.
  // 更新仅 WebUI 使用的思考展示模式。
  onThinkingDisplayModeChange: (mode: ThinkingDisplayMode) => void;
  // Backend-resolved reasoning selector view for the active provider and model.
  // 后端根据当前激活 Provider 与模型解析出的思考选择器视图。
  reasoningSelection: ReasoningSelectionSummary | null;
}) {
  // t translates stable composer labels while model names remain provider data.
  // t 翻译稳定输入区标签，模型名称仍保留为 provider 数据。
  const { locale, t } = useI18n();
  // composerDraftKey scopes unsent input to the selected session or current project's new-chat slot.
  // composerDraftKey 将尚未发送的输入限定到当前会话或当前项目的新建会话槽。
  const composerDraftKey = React.useMemo(
    () => composerSessionDraftKey(props.projectId, props.conversationId),
    [props.conversationId, props.projectId],
  );
  // initialComposerDraftRef captures the draft that should seed this component mount.
  // initialComposerDraftRef 捕获本次组件挂载时用于初始化的草稿。
  const initialComposerDraftRef = React.useRef<ComposerSessionMemoryDraft | null>(null);
  if (initialComposerDraftRef.current === null) {
    initialComposerDraftRef.current = readComposerSessionMemoryDraft(composerDraftKey);
  }
  // activeComposerDraftKeyRef tracks which draft slot currently owns local state.
  // activeComposerDraftKeyRef 跟踪当前本地状态所属的草稿槽。
  const activeComposerDraftKeyRef = React.useRef(composerDraftKey);
  // draftInputRef mirrors draftInput for key-switch saves without stale closures.
  // draftInputRef 镜像 draftInput，避免切换会话保存时闭包过期。
  const draftInputRef = React.useRef(initialComposerDraftRef.current.input);
  // draftAttachmentsRef mirrors attachments for key-switch saves without stale closures.
  // draftAttachmentsRef 镜像 attachments，避免切换会话保存时闭包过期。
  const draftAttachmentsRef = React.useRef<ComposerAttachmentSummary[]>(initialComposerDraftRef.current.attachments);
  // composerDraftRestoredRef marks that the initial draft slot has been hydrated.
  // composerDraftRestoredRef 标记初始草稿槽已经完成恢复。
  const composerDraftRestoredRef = React.useRef(false);
  // draftInput stores the current unsent composer text.
  // draftInput 保存当前尚未发送的输入区文本。
  const [draftInput, setDraftInput] = React.useState(() => initialComposerDraftRef.current?.input ?? "");
  // slashHighlightedIndex stores the highlighted command row while the slash panel is visible.
  // slashHighlightedIndex 保存 slash 面板可见时的高亮指令行。
  const [slashHighlightedIndex, setSlashHighlightedIndex] = React.useState(0);
  // slashDismissedInput stores the exact input dismissed by Escape.
  // slashDismissedInput 保存被 Escape 隐藏的精确输入内容。
  const [slashDismissedInput, setSlashDismissedInput] = React.useState<string | null>(null);
  // modelPickerOpenSignal opens the existing model picker from /model.
  // modelPickerOpenSignal 通过 /model 打开现有模型筛选器。
  const [modelPickerOpenSignal, setModelPickerOpenSignal] = React.useState(0);
  // reasoningPickerOpenSignal opens the existing reasoning selector from /reasoning.
  // reasoningPickerOpenSignal 通过 /reasoning 打开现有思考档位选择器。
  const [reasoningPickerOpenSignal, setReasoningPickerOpenSignal] = React.useState(0);
  // composerMediaPlayer stores an attachment preview opened from the composer strip.
  // composerMediaPlayer 保存从输入区附件条打开的附件预览。
  const [composerMediaPlayer, setComposerMediaPlayer] = React.useState<GeneratedMediaAttachment | null>(null);
  // composerTextareaRef focuses the prompt field from global workspace typing.
  // composerTextareaRef 用于从工作台全局输入聚焦提示词输入框。
  const composerTextareaRef = React.useRef<HTMLTextAreaElement | null>(null);
  // composerAttachments owns attachment filtering, picker, paste, drag-drop, and staging behavior.
  // composerAttachments 管理附件过滤、选择器、粘贴、拖拽投放与暂存行为。
  const composerAttachments = useComposerAttachments({
    initialAttachments: initialComposerDraftRef.current.attachments,
    attachmentCapabilities: props.attachmentCapabilities,
    projectId: props.projectId,
    conversationId: props.conversationId,
    onStageAttachmentBytes: props.onStageAttachmentBytes,
  });
  const {
    attachments,
    setAttachments,
    isAttachmentDragOver,
    setIsAttachmentDragOver,
    attachmentFileInputRef,
    enabledAttachmentCapabilities,
    attachmentInputAccept,
    openAttachmentPicker,
    handleAttachmentInputChange,
    handleComposerPaste,
    handleComposerDragOver,
    handleComposerDrop,
    removeComposerAttachment,
  } = composerAttachments;

  React.useLayoutEffect(() => {
    const previousKey = activeComposerDraftKeyRef.current;
    if (composerDraftRestoredRef.current && previousKey === composerDraftKey) {
      return;
    }
    if (composerDraftRestoredRef.current) {
      writeComposerSessionMemoryDraft(previousKey, draftInputRef.current, draftAttachmentsRef.current);
    }
    const restoredDraft = readComposerSessionMemoryDraft(composerDraftKey);
    activeComposerDraftKeyRef.current = composerDraftKey;
    draftInputRef.current = restoredDraft.input;
    draftAttachmentsRef.current = restoredDraft.attachments;
    composerDraftRestoredRef.current = true;
    setDraftInput(restoredDraft.input);
    setAttachments(restoredDraft.attachments);
    setSlashDismissedInput(null);
    setSlashHighlightedIndex(0);
    setComposerMediaPlayer(null);
  }, [composerDraftKey, setAttachments]);

  React.useEffect(() => {
    if (!composerDraftRestoredRef.current) {
      return;
    }
    draftInputRef.current = draftInput;
    writeComposerSessionMemoryDraft(activeComposerDraftKeyRef.current, draftInput, draftAttachmentsRef.current);
  }, [draftInput]);

  React.useEffect(() => {
    if (!composerDraftRestoredRef.current) {
      return;
    }
    draftAttachmentsRef.current = attachments;
    writeComposerSessionMemoryDraft(activeComposerDraftKeyRef.current, draftInputRef.current, attachments);
  }, [attachments]);

  // executionMode stores the local preview execution-mode selection.
  // executionMode 保存本地预览用的执行模式选择。
  const [executionMode, setExecutionMode] = React.useState("build");
  // draftPlanModeEnabled stores Plan mode before a draft conversation is materialized.
  // draftPlanModeEnabled 保存草稿会话实体化之前的计划模式状态。
  const [draftPlanModeEnabled, setDraftPlanModeEnabled] = React.useState(false);
  // modelChoice stores the selected provider/model key shown by the shared model picker.
  // modelChoice 保存共享模型筛选器展示的 provider/model 选择键。
  const [modelChoice, setModelChoice] = React.useState("");
  // disabledAiCapabilityKinds stores the session-synchronized quick-toggle disables.
  // disabledAiCapabilityKinds 保存与会话同步的快捷开关禁用项。
  const [disabledAiCapabilityKinds, setDisabledAiCapabilityKinds] = React.useState<Set<AiCapabilityKind>>(() => new Set());
  // disabledAiCapabilityTargetSelectors stores the session-synchronized target-route disables.
  // disabledAiCapabilityTargetSelectors 保存与会话同步的目标路由禁用项。
  const [disabledAiCapabilityTargetSelectors, setDisabledAiCapabilityTargetSelectors] = React.useState<Set<string>>(() => new Set());
  // aiCapabilityTargetOrder stores the session-synchronized target route ordering.
  // aiCapabilityTargetOrder 保存与会话同步的目标路由顺序。
  const [aiCapabilityTargetOrder, setAiCapabilityTargetOrder] = React.useState<Map<AiCapabilityKind, string[]>>(() => new Map());
  // reasoningEffort stores the backend-synchronized reasoning-effort selection.
  // reasoningEffort 保存与后端同步的思考档位选择。
  const [reasoningEffort, setReasoningEffort] = React.useState("");
  // isComposerRunning stores the local send/stop preview state.
  // isComposerRunning 保存输入区发送/停止的本地预览状态。
  const [isComposerRunning, setIsComposerRunning] = React.useState(false);
  // goalDialogOpen controls the user-facing session goal editor.
  // goalDialogOpen 控制面向用户的会话目标编辑器。
  const [goalDialogOpen, setGoalDialogOpen] = React.useState(false);
  // goalDialogMode distinguishes ordinary edits from budget-stage resumes.
  // goalDialogMode 区分普通编辑与预算阶段继续。
  const [goalDialogMode, setGoalDialogMode] = React.useState<"edit" | "resume">("edit");
  // goalObjectiveDraft stores the editable goal objective.
  // goalObjectiveDraft 保存可编辑的目标文本。
  const [goalObjectiveDraft, setGoalObjectiveDraft] = React.useState("");
  // goalTokenBudgetDraft stores the optional token budget as input text.
  // goalTokenBudgetDraft 以输入文本保存可选 token 预算。
  const [goalTokenBudgetDraft, setGoalTokenBudgetDraft] = React.useState("");
  // goalSaving stores whether a goal mutation is currently in flight.
  // goalSaving 保存当前是否存在飞行中的目标修改。
  const [goalSaving, setGoalSaving] = React.useState(false);
  // visibleSessionGoal hides completed goals from the composer-level control strip.
  // visibleSessionGoal 从输入区控制条中隐藏已完成目标。
  const visibleSessionGoal = props.sessionGoal?.status === "complete" ? null : props.sessionGoal;
  // planModeEnabled follows the backend-persisted session Plan mode state.
  // planModeEnabled 跟随后端持久化的会话计划模式状态。
  const planModeEnabled = props.sessionPlanMode?.executionMode?.toLowerCase() === "plan" || (!props.conversationId && draftPlanModeEnabled);
  // availableModelChoices flattens provider data into selectable workbench models.
  // availableModelChoices 将 Provider 数据扁平化为可选择的工作台模型。
  const availableModelChoices = React.useMemo(
    () => modelPickerChoicesFromProviders(props.providers),
    [props.providers],
  );
  // activeModelChoiceKey follows the service-reported active model when available.
  // activeModelChoiceKey 在可用时跟随 service 上报的激活模型。
  const activeModelChoiceKey = React.useMemo(() => {
    const activeChoice = availableModelChoices.find((choice) => choice.active) ?? availableModelChoices[0];
    return modelPickerChoiceKey(activeChoice);
  }, [availableModelChoices]);
  // activeProviderModelSummary resolves the full API model row for secondary-route controls.
  // activeProviderModelSummary 为二级路由控件解析完整 API 模型行。
  const activeProviderModelSummary = React.useMemo(() => {
    const parsedChoice = parseModelPickerChoiceKey(modelChoice || activeModelChoiceKey);
    const providerId = parsedChoice?.providerId;
    const modelId = parsedChoice?.modelId;
    if (!providerId || !modelId) {
      return null;
    }
    const provider = props.providers.find((candidate) => candidate.id === providerId);
    const model = provider?.models?.find((candidate) => candidate.id === modelId) ?? null;
    return provider && model ? { provider, model } : null;
  }, [activeModelChoiceKey, modelChoice, props.providers]);
  // capabilitySignature prevents unrelated workbench streaming snapshots from resetting session-synced capability state.
  // capabilitySignature 防止无关的工作台流式快照重置会话同步的能力状态。
  const capabilitySignature = React.useMemo(
    () => aiCapabilitySignature(props.aiCapabilities),
    [props.aiCapabilities],
  );

  React.useEffect(() => {
    if (!activeModelChoiceKey) {
      return;
    }
    if (modelChoice !== activeModelChoiceKey) {
      setModelChoice(activeModelChoiceKey);
    }
  }, [activeModelChoiceKey, modelChoice]);

  React.useEffect(() => {
    setDisabledAiCapabilityKinds(defaultDisabledAiCapabilityKinds(props.aiCapabilities));
    setDisabledAiCapabilityTargetSelectors(defaultDisabledAiCapabilityTargetSelectors(props.aiCapabilities));
    setAiCapabilityTargetOrder(defaultAiCapabilityTargetOrder(props.aiCapabilities));
  }, [capabilitySignature]);
  React.useEffect(() => {
    setExecutionMode(planModeEnabled ? "plan" : "build");
  }, [planModeEnabled]);

  React.useEffect(() => {
    if (props.conversationId) {
      setDraftPlanModeEnabled(false);
    }
  }, [props.conversationId]);
  // fallbackReasoningEffortOptions is only used when no backend selector view is available.
  // fallbackReasoningEffortOptions 仅在没有后端选择器视图时使用。
  const fallbackReasoningEffortOptions = React.useMemo<ComposerSelectOption[]>(() => [
    { value: "off", label: t("composer.reasoning.off") },
    { value: "low", label: t("composer.reasoning.low") },
    { value: "medium", label: t("composer.reasoning.medium") },
    { value: "high", label: t("composer.reasoning.high") },
    { value: "x-high", label: t("composer.reasoning.max") },
  ], [t]);
  // reasoningEffortOptions uses the shared backend selector view before falling back to local preview labels.
  // reasoningEffortOptions 优先使用共享后端选择器视图，再回退到本地预览标签。
  const reasoningEffortOptions = React.useMemo<ComposerSelectOption[]>(() => {
    if (props.reasoningSelection?.options.length) {
      return props.reasoningSelection.options.map((option) => ({
        value: option.value,
        label: option.label,
      }));
    }
    return fallbackReasoningEffortOptions;
  }, [fallbackReasoningEffortOptions, props.reasoningSelection]);
  // serviceSelectedReasoningEffort mirrors the active backend-selected effort when available.
  // serviceSelectedReasoningEffort 在可用时镜像后端选中的生效思考档位。
  const serviceSelectedReasoningEffort = props.reasoningSelection?.selectedEffort;
  // submittedReasoningEffort is non-null only when the backend selector confirms the value.
  // submittedReasoningEffort 仅在后端选择器确认该值时才为非空。
  const submittedReasoningEffort = React.useMemo(() => {
    if (!props.reasoningSelection?.options.some((option) => option.value === reasoningEffort)) {
      return null;
    }
    return reasoningEffort;
  }, [props.reasoningSelection, reasoningEffort]);

  React.useEffect(() => {
    if (serviceSelectedReasoningEffort && serviceSelectedReasoningEffort !== reasoningEffort) {
      setReasoningEffort(serviceSelectedReasoningEffort);
      return;
    }
    if (!reasoningEffortOptions.some((option) => option.value === reasoningEffort)) {
      setReasoningEffort(reasoningEffortOptions[0]?.value ?? "medium");
    }
  }, [reasoningEffort, reasoningEffortOptions, serviceSelectedReasoningEffort]);
  // placeholder describes the platform-specific send and newline keyboard behavior.
  // placeholder 描述平台相关的发送与换行键盘行为。
  const placeholder = React.useMemo(() => {
    return t("composer.placeholder").replace("{shortcut}", composerShortcutHint(t("composer.shortcutHint")));
  }, [t]);
  // visibleSlashCommands mirrors WebUI prefix filtering for the slash command panel.
  // visibleSlashCommands 对齐 WebUI 前缀过滤逻辑，供 slash 指令面板展示。
  const visibleSlashCommands = React.useMemo(() => {
    if (!draftInput.startsWith("/") || slashDismissedInput === draftInput) {
      return [];
    }
    return props.slashCommands.filter((command) => composerSlashCommandMatchesInput(command, draftInput));
  }, [draftInput, props.slashCommands, slashDismissedInput]);
  // slashPanelVisible controls whether the current slash input has at least one match.
  // slashPanelVisible 控制当前 slash 输入是否存在至少一个匹配项。
  const slashPanelVisible = visibleSlashCommands.length > 0;
  // highlightedSlashCommand resolves the currently highlighted command row.
  // highlightedSlashCommand 解析当前高亮的指令行。
  const highlightedSlashCommand = visibleSlashCommands[Math.min(slashHighlightedIndex, Math.max(visibleSlashCommands.length - 1, 0))] ?? null;
  // exactSlashCommand mirrors WebUI exact trigger activation before the highlighted row.
  // exactSlashCommand 对齐 WebUI 精确触发优先于高亮项的激活规则。
  const exactSlashCommand = React.useMemo(
    () => exactComposerSlashCommand(visibleSlashCommands, draftInput),
    [draftInput, visibleSlashCommands],
  );

  React.useEffect(() => {
    setSlashHighlightedIndex((current) => {
      if (!visibleSlashCommands.length) {
        return 0;
      }
      return Math.min(current, visibleSlashCommands.length - 1);
    });
  }, [visibleSlashCommands.length]);

  // handleDraftInputChange tracks typed content and re-enables slash matching after edits.
  // handleDraftInputChange 跟踪输入内容，并在编辑后重新启用 slash 匹配。
  const handleDraftInputChange = React.useCallback((value: string) => {
    setDraftInput(value);
    if (slashDismissedInput !== null && value !== slashDismissedInput) {
      setSlashDismissedInput(null);
    }
  }, [slashDismissedInput]);

  React.useEffect(() => {
    // handleGlobalWorkspaceTyping redirects ordinary workspace typing into the composer.
    // handleGlobalWorkspaceTyping 将工作台中的普通键入重定向到输入区。
    const handleGlobalWorkspaceTyping = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !isComposerPrintableKey(event)) {
        return;
      }
      const textarea = composerTextareaRef.current;
      if (!textarea || document.activeElement === textarea) {
        return;
      }
      if (hasBlockingComposerPopup() || isComposerTextEntryElement(document.activeElement)) {
        return;
      }
      event.preventDefault();
      textarea.focus({ preventScroll: true });
      setDraftInput((current) => `${current}${event.key}`);
      if (slashDismissedInput !== null) {
        setSlashDismissedInput(null);
      }
    };
    document.addEventListener("keydown", handleGlobalWorkspaceTyping);
    return () => document.removeEventListener("keydown", handleGlobalWorkspaceTyping);
  }, [slashDismissedInput]);

  // handleComposerSend validates input and forwards it to the real backend runtime.
  // handleComposerSend 校验输入并转发到真实后端运行时。
  const handleComposerSend = React.useCallback(async () => {
    const prompt = draftInput.trim();
    const hasSubmittableContent = Boolean(prompt || attachments.length > 0);
    props.onTrace?.("composer.send_or_stop", {
      isConversationRunning: props.isConversationRunning,
      hasSubmittableContent,
      promptChars: prompt.length,
      attachmentCount: attachments.length,
    });
    if (props.isConversationRunning && !hasSubmittableContent) {
      const stopped = await props.onStopConversationRun();
      if (!stopped) {
        toast.error(t("composer.error.stopCurrentFailed"));
      }
      return;
    }
    if (isComposerRunning) {
      return;
    }
    if (!hasSubmittableContent) {
      toast.info(t("composer.error.promptEmpty"));
      return;
    }
    // submittedInput preserves the exact textarea content for a failure-only restore.
    // submittedInput 保存精确输入内容，仅在发送失败时用于恢复。
    const submittedInput = draftInput;
    // submittedAttachments preserves staged files for a failure-only restore.
    // submittedAttachments 保存已暂存文件，仅在发送失败时用于恢复。
    const submittedAttachments = attachments;
    draftInputRef.current = "";
    draftAttachmentsRef.current = [];
    writeComposerSessionMemoryDraft(activeComposerDraftKeyRef.current, "", []);
    setDraftInput("");
    setAttachments([]);
    setIsComposerRunning(true);
    try {
      const sent = await props.onSubmitPrompt({
        prompt,
        attachments: submittedAttachments,
        executionMode,
        reasoningEffort: submittedReasoningEffort,
        disabledAiCapabilityKinds: Array.from(disabledAiCapabilityKinds),
        disabledAiCapabilityTargetSelectors: Array.from(disabledAiCapabilityTargetSelectors),
        aiCapabilityTargetOrderOverrides: serializeAiCapabilityTargetOrder(aiCapabilityTargetOrder),
      });
      if (!sent) {
        setDraftInput((current) => {
          const restoredInput = current || submittedInput;
          draftInputRef.current = restoredInput;
          writeComposerSessionMemoryDraft(
            activeComposerDraftKeyRef.current,
            restoredInput,
            draftAttachmentsRef.current,
          );
          return restoredInput;
        });
        setAttachments((current) => {
          const restoredAttachments = current.length ? current : submittedAttachments;
          draftAttachmentsRef.current = restoredAttachments;
          writeComposerSessionMemoryDraft(
            activeComposerDraftKeyRef.current,
            draftInputRef.current,
            restoredAttachments,
          );
          return restoredAttachments;
        });
      }
    } finally {
      setIsComposerRunning(false);
    }
  }, [aiCapabilityTargetOrder, attachments, disabledAiCapabilityKinds, disabledAiCapabilityTargetSelectors, draftInput, executionMode, isComposerRunning, props, submittedReasoningEffort, t]);

  // handleComposerStop stops the active backend conversation without consuming draft text.
  // handleComposerStop 在不消费草稿文本的前提下停止活跃后端会话。
  const handleComposerStop = React.useCallback(async () => {
    props.onTrace?.("composer.stop", {
      isConversationRunning: props.isConversationRunning,
      draftChars: draftInput.trim().length,
      attachmentCount: attachments.length,
    });
    const stopped = await props.onStopConversationRun();
    if (!stopped) {
      toast.error(t("composer.error.stopCurrentFailed"));
    }
  }, [attachments.length, draftInput, props, t]);

  // handlePendingPromptEdit restores one queued prompt into the composer draft.
  // handlePendingPromptEdit 将单条排队提示词恢复到输入区草稿。
  const handlePendingPromptEdit = React.useCallback(async (entryId: string) => {
    props.onTrace?.("pending_prompt.edit", { entryId });
    const entry = await props.onEditPendingPrompt(entryId);
    if (!entry) {
      return;
    }
    setDraftInput(entry.prompt);
    setAttachments(entry.attachments ?? []);
    window.requestAnimationFrame(() => composerTextareaRef.current?.focus({ preventScroll: true }));
  }, [props]);

  // handleAiCapabilityConfigurationSave persists the dialog draft and mirrors it locally after success.
  // handleAiCapabilityConfigurationSave 持久化弹窗草稿，并在成功后同步到本地。
  const handleAiCapabilityConfigurationSave = React.useCallback(async (request: ComposerAiCapabilityConfigurationSave) => {
    const saved = await props.onSaveAiCapabilityConfiguration(request);
    if (!saved) {
      return false;
    }
    setDisabledAiCapabilityKinds(new Set(request.disabledAiCapabilityKinds));
    setDisabledAiCapabilityTargetSelectors(new Set(request.disabledAiCapabilityTargetSelectors));
    setAiCapabilityTargetOrder(defaultAiCapabilityTargetOrderFromRecord(request.aiCapabilityTargetOrderOverrides));
    return true;
  }, [props]);

  // openGoalDialog seeds the goal editor from the current persisted goal.
  // openGoalDialog 使用当前持久化目标初始化目标编辑器。
  const openGoalDialog = React.useCallback(async () => {
    if (planModeEnabled) {
      if (!props.conversationId) {
        setDraftPlanModeEnabled(false);
        setExecutionMode("build");
      } else {
        const disabled = await props.onSetSessionPlanMode(false);
        if (!disabled) {
          return;
        }
      }
    }
    setGoalDialogMode("edit");
    setGoalObjectiveDraft(visibleSessionGoal?.objective ?? "");
    setGoalTokenBudgetDraft(visibleSessionGoal?.tokenBudget ? String(visibleSessionGoal.tokenBudget) : "");
    setGoalDialogOpen(true);
  }, [planModeEnabled, props, visibleSessionGoal]);

  // handlePlanModeChange toggles session Plan mode through the persisted service API.
  // handlePlanModeChange 通过持久化服务 API 切换会话计划模式。
  const handlePlanModeChange = React.useCallback(async (enabled: boolean) => {
    if (enabled && visibleSessionGoal) {
      toast.error(t("composer.planMode.goalConflict"));
      return false;
    }
    if (!props.conversationId) {
      setDraftPlanModeEnabled(enabled);
      setExecutionMode(enabled ? "plan" : "build");
      return true;
    }
    const saved = await props.onSetSessionPlanMode(enabled);
    if (saved) {
      setExecutionMode(enabled ? "plan" : "build");
    }
    return saved;
  }, [props, t, visibleSessionGoal]);

  // activateSlashCommand dispatches one selected command and applies its UI route result.
  // activateSlashCommand 分派单个选中的指令并应用其 UI 路由结果。
  const activateSlashCommand = React.useCallback((command: ComposerSlashCommandSummary) => {
    if (normalizedComposerSlashCommandId(command) === "plan") {
      void handlePlanModeChange(!planModeEnabled).then((handled) => {
        if (!handled) {
          return;
        }
        setDraftInput("");
        setSlashDismissedInput(null);
        setSlashHighlightedIndex(0);
      });
      return;
    }
    const result = props.onSlashCommand(command);
    if (!result.handled) {
      return;
    }
    setDraftInput("");
    setSlashDismissedInput(null);
    setSlashHighlightedIndex(0);
    if (result.replacementText !== undefined) {
      setDraftInput(result.replacementText);
    }
    if (result.openTarget === "model") {
      setModelPickerOpenSignal((current) => current + 1);
    }
    if (result.openTarget === "reasoning") {
      setReasoningPickerOpenSignal((current) => current + 1);
    }
  }, [handlePlanModeChange, planModeEnabled, props]);

  // openGoalResumeDialog seeds the next-stage goal editor from the stopped goal.
  // openGoalResumeDialog 使用已停止目标初始化下一阶段目标编辑器。
  const openGoalResumeDialog = React.useCallback(() => {
    if (!visibleSessionGoal) {
      return;
    }
    setGoalDialogMode("resume");
    setGoalObjectiveDraft(visibleSessionGoal.objective);
    setGoalTokenBudgetDraft(visibleSessionGoal.tokenBudget ? String(visibleSessionGoal.tokenBudget) : "");
    setGoalDialogOpen(true);
  }, [visibleSessionGoal]);

  // handleGoalSave persists the current goal dialog draft.
  // handleGoalSave 持久化当前目标弹窗草稿。
  const handleGoalSave = React.useCallback(async () => {
    const objective = goalObjectiveDraft.trim();
    const tokenBudgetText = goalTokenBudgetDraft.trim();
    const tokenBudget = tokenBudgetText ? Number(tokenBudgetText) : null;
    if (!objective) {
      toast.error(t("composer.goal.error.empty"));
      return;
    }
    if (tokenBudget !== null && (!Number.isInteger(tokenBudget) || tokenBudget <= 0)) {
      toast.error(t("composer.goal.error.budgetInvalid"));
      return;
    }
    setGoalSaving(true);
    try {
      const shouldSubmitCreatedGoal = !visibleSessionGoal && !props.isConversationRunning && !isComposerRunning;
      const saved = goalDialogMode === "resume"
        ? await props.onResumeSessionGoal(objective, tokenBudget)
        : await props.onSaveSessionGoal(objective, tokenBudget);
      if (saved) {
        setGoalDialogOpen(false);
        if (goalDialogMode === "edit" && shouldSubmitCreatedGoal) {
          setIsComposerRunning(true);
          try {
            const sent = await props.onSubmitPrompt({
              prompt: objective,
              attachments: [],
              executionMode,
              reasoningEffort: submittedReasoningEffort,
              disabledAiCapabilityKinds: Array.from(disabledAiCapabilityKinds),
              disabledAiCapabilityTargetSelectors: Array.from(disabledAiCapabilityTargetSelectors),
              aiCapabilityTargetOrderOverrides: serializeAiCapabilityTargetOrder(aiCapabilityTargetOrder),
            });
            if (!sent) {
              setDraftInput((current) => current || objective);
            }
          } finally {
            setIsComposerRunning(false);
          }
        }
      }
    } finally {
      setGoalSaving(false);
    }
  }, [aiCapabilityTargetOrder, disabledAiCapabilityKinds, disabledAiCapabilityTargetSelectors, executionMode, goalDialogMode, goalObjectiveDraft, goalTokenBudgetDraft, isComposerRunning, props, submittedReasoningEffort, t, visibleSessionGoal]);

  // handleGoalStatusToggle pauses or resumes the current goal.
  // handleGoalStatusToggle 暂停或恢复当前目标。
  const handleGoalStatusToggle = React.useCallback(async () => {
    if (!visibleSessionGoal) {
      return;
    }
    const nextStatus = visibleSessionGoal.status === "paused" ? "active" : "paused";
    setGoalSaving(true);
    try {
      await props.onUpdateSessionGoalStatus(nextStatus);
    } finally {
      setGoalSaving(false);
    }
  }, [props, visibleSessionGoal]);

  // handleGoalClear clears the current goal after a direct user action.
  // handleGoalClear 在用户直接操作后清理当前目标。
  const handleGoalClear = React.useCallback(async () => {
    setGoalSaving(true);
    try {
      await props.onClearSessionGoal();
    } finally {
      setGoalSaving(false);
    }
  }, [props]);

  // handleModelChoiceChange applies one model choice locally and through the shared backend.
  // handleModelChoiceChange 在本地与共享后端应用单个模型选择。
  const handleModelChoiceChange = React.useCallback(async (choice: ModelPickerChoice) => {
    const previousChoice = modelChoice;
    const nextChoice = modelPickerChoiceKey(choice);
    setModelChoice(nextChoice);
    const ok = await props.onSelectWorkbenchModel(choice);
    if (!ok) {
      setModelChoice(previousChoice);
    }
  }, [modelChoice, props]);

  // handleReasoningEffortChange applies one backend-routed reasoning selection with rollback.
  // handleReasoningEffortChange 应用单个后端路由的思考档位选择并支持失败回滚。
  const handleReasoningEffortChange = React.useCallback(async (value: string) => {
    const previousEffort = reasoningEffort;
    setReasoningEffort(value);
    const ok = await props.onSelectWorkbenchReasoningEffort(value);
    if (!ok) {
      setReasoningEffort(previousEffort);
    }
  }, [props, reasoningEffort]);

  // toggleExecutionMode switches between build and plan without leaving the composer focus flow.
  // toggleExecutionMode 在不离开输入区焦点流的情况下切换构建与计划模式。
  const toggleExecutionMode = React.useCallback(() => {
    void handlePlanModeChange(!planModeEnabled);
  }, [handlePlanModeChange, planModeEnabled]);

  // handleComposerContainerKeyDown captures Shift+Tab as the work-mode shortcut.
  // handleComposerContainerKeyDown 将 Shift+Tab 捕获为工作模式快捷键。
  const handleComposerContainerKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Tab" || !event.shiftKey) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    toggleExecutionMode();
  }, [toggleExecutionMode]);

  // handleComposerKeyDown maps Enter to send while keeping Shift+Enter as a newline.
  // handleComposerKeyDown 将 Enter 映射为发送，并保留 Shift+Enter 换行。
  const handleComposerKeyDown = React.useCallback((event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (slashPanelVisible) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setSlashHighlightedIndex((current) => (current + 1) % visibleSlashCommands.length);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setSlashHighlightedIndex((current) => (current - 1 + visibleSlashCommands.length) % visibleSlashCommands.length);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setSlashDismissedInput(draftInput);
        return;
      }
      if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
        const command = exactSlashCommand ?? highlightedSlashCommand;
        if (command) {
          event.preventDefault();
          activateSlashCommand(command);
          return;
        }
      }
    }
    if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) {
      return;
    }
    event.preventDefault();
    void handleComposerSend();
  }, [
    activateSlashCommand,
    draftInput,
    exactSlashCommand,
    handleComposerSend,
    highlightedSlashCommand,
    slashPanelVisible,
    visibleSlashCommands.length,
  ]);

  return (
    <>
      {props.pendingPrompts.length ? (
        <PendingPromptQueueStrip
          entries={props.pendingPrompts}
          onTrace={props.onTrace}
          onEdit={(entryId) => void handlePendingPromptEdit(entryId)}
          onInsert={(entryId) => void props.onInsertPendingPrompt(entryId)}
          onRemove={(entryId) => void props.onRemovePendingPrompt(entryId)}
          onReorder={(entryIds, draggedEntryId, targetEntryId) => void props.onReorderPendingPrompts(entryIds, draggedEntryId, targetEntryId)}
        />
      ) : null}
      {visibleSessionGoal ? (
        <ComposerGoalStrip
          disabled={goalSaving}
          goal={visibleSessionGoal}
          onClear={() => void handleGoalClear()}
          onEdit={() => void openGoalDialog()}
          onResume={openGoalResumeDialog}
          onToggleStatus={() => void handleGoalStatusToggle()}
        />
      ) : null}
      <section
        className={cn("composer-panel", isAttachmentDragOver && "attachment-drag-over")}
        onDragLeave={() => setIsAttachmentDragOver(false)}
        onDragOver={handleComposerDragOver}
        onDrop={handleComposerDrop}
        onKeyDownCapture={handleComposerContainerKeyDown}
      >
        <input
          accept={attachmentInputAccept}
          className="composer-attachment-input"
          multiple
          onChange={handleAttachmentInputChange}
          ref={attachmentFileInputRef}
          type="file"
        />
        {attachments.length ? (
          <ComposerPanelAttachmentPreviewStrip
            attachments={attachments}
            onOpenMedia={setComposerMediaPlayer}
            onRemove={removeComposerAttachment}
            resolveMediaSourceUrl={props.resolveMediaSourceUrl}
          />
        ) : null}
        {slashPanelVisible ? (
          <ComposerSlashCommandPanel
            commands={visibleSlashCommands}
            highlightedIndex={Math.min(slashHighlightedIndex, visibleSlashCommands.length - 1)}
            locale={locale}
            onActivate={activateSlashCommand}
            onHighlight={setSlashHighlightedIndex}
          />
        ) : null}
        <Textarea
          aria-label={t("composer.aria.input")}
          onChange={(event) => handleDraftInputChange(event.currentTarget.value)}
          onKeyDown={handleComposerKeyDown}
          onPaste={handleComposerPaste}
          placeholder={placeholder}
          ref={composerTextareaRef}
          value={draftInput}
        />
        <div className="composer-toolbar">
          <div className="composer-toolbar-left">
            <ComposerAttachmentMenu
              capabilities={enabledAttachmentCapabilities}
              thinkingDisplayMode={props.thinkingDisplayMode}
              planModeEnabled={planModeEnabled}
              onPick={openAttachmentPicker}
              onThinkingDisplayModeChange={props.onThinkingDisplayModeChange}
              onOpenGoal={() => void openGoalDialog()}
              onPlanModeChange={(enabled) => void handlePlanModeChange(enabled)}
            />
            {planModeEnabled ? (
              <ComposerPlanModeChip
                artifactName={props.sessionPlanMode?.artifact?.name ?? null}
                onClose={() => void handlePlanModeChange(false)}
              />
            ) : null}
            <ModelPickerControl
              ariaLabel={t("composer.aria.model")}
              fallbackLabel={t("composer.selectFallback")}
              filterPlaceholder={t("composer.modelPicker.filter")}
              noMatchesLabel={t("composer.modelPicker.noMatches")}
              onValueChange={(choice) => void handleModelChoiceChange(choice)}
              openSignal={modelPickerOpenSignal}
              providers={props.providers}
              value={modelChoice}
            />
            {activeProviderModelSummary?.model.supportsSecondaryRoutes ? (
              <ModelSecondaryRouteControl
                modelId={activeProviderModelSummary.model.id}
                onListRoutes={props.onListWorkbenchModelSecondaryRoutes}
                onSelectRoute={props.onSelectWorkbenchModelSecondaryRoute}
                providerId={activeProviderModelSummary.provider.id}
                selectedRoute={activeProviderModelSummary.model.secondaryRoute ?? null}
              />
            ) : null}
            <ComposerAiCapabilityControl
              ariaLabel={t("composer.aria.capability")}
              capabilities={props.aiCapabilities}
              defaultCapabilities={props.defaultAiCapabilities}
              disabledKinds={disabledAiCapabilityKinds}
              disabledTargetSelectors={disabledAiCapabilityTargetSelectors}
              targetOrder={aiCapabilityTargetOrder}
              locale={locale}
              onSaveConfiguration={handleAiCapabilityConfigurationSave}
            />
          </div>
          <ComposerPanelToolbarRight
            attachmentCount={attachments.length}
            contextUsage={props.contextUsage}
            draftInput={draftInput}
            isConversationRunning={props.isConversationRunning}
            onReasoningEffortChange={(value) => void handleReasoningEffortChange(value)}
            onSend={() => void handleComposerSend()}
            onStop={() => void handleComposerStop()}
            reasoningAriaLabel={t("composer.aria.reasoning")}
            reasoningEffort={reasoningEffort}
            reasoningEffortOptions={reasoningEffortOptions}
            reasoningPickerOpenSignal={reasoningPickerOpenSignal}
            selectFallbackLabel={t("composer.selectFallback")}
            sendAriaLabel={t("composer.aria.send")}
            stopAriaLabel={t("composer.aria.stop")}
          />
        </div>
        {composerMediaPlayer ? (
          <GeneratedMediaPlayerWindow attachment={composerMediaPlayer} onClose={() => setComposerMediaPlayer(null)} />
        ) : null}
      </section>
      <ComposerGoalDialog
        mode={goalDialogMode}
        objective={goalObjectiveDraft}
        onObjectiveChange={setGoalObjectiveDraft}
        onOpenChange={setGoalDialogOpen}
        onSave={() => void handleGoalSave()}
        onTokenBudgetChange={setGoalTokenBudgetDraft}
        open={goalDialogOpen}
        saving={goalSaving}
        tokenBudget={goalTokenBudgetDraft}
      />
    </>
  );
}
