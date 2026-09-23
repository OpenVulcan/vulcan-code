import React from "react";
import { toast } from "sonner";
import {
  enqueueComposerPersistence,
  flushLatestComposerPersistence,
} from "@/components/composer/composer-persistence-queue";
import {
  ModelAccountPickerControl,
  ModelPickerControl,
  type ModelPickerChoice,
} from "@/components/composer/model-picker-control";
import {
  composerAttachmentAccept,
  hasBlockingComposerPopup,
  isComposerPrintableKey,
  isComposerTextEntryElement,
  type ComposerSlashCommandResult,
  type ComposerSubmission,
} from "@/components/composer/composer-model";
import {
  ComposerAiCapabilityControl,
  ComposerAttachmentMenu,
  ComposerSlashCommandPanel,
  PendingPromptQueueStrip,
} from "@/components/composer/composer-controls";
import type { ConversationSessionReviewSummary } from "@/components/conversation/session-todolist-panel";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { GeneratedMediaPlayerWindow, type GeneratedMediaAttachment } from "@/components/media/generated-media";
import { ComposerTextContextMenu } from "@/components/composer/composer-text-context-menu";
import { Textarea } from "@/components/ui/textarea";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useI18n } from "@/i18n";
import { isElectronRuntime } from "@/shell";
import { cn } from "@/lib/utils";
import { getWorkspaceInputOwner } from "@/components/workspace/workspace-input-owner";
import type {
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ContextCompactionWindowNodeId,
  ContextUsageSnapshot,
  SessionProviderUsageSummary,
  PendingPromptSummary,
  ProviderSummary,
  PublicAiContextCompactionWindowNode,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  ReasoningSelectionSummary,
  SessionGoalSummary,
  SessionGoalUserStatusUpdate,
  SessionPlanModeSummary,
  SessionOutputStyleSummary,
  SessionTodolistSummary,
  SessionVmmPolicySummary,
  OutputStyleCatalogSummary,
} from "@/types";
import type { ComposerTraceCallback } from "@/components/composer/composer-control-types";

import { composerShortcutHint } from "@/components/composer/composer-panel/helpers";
import { ComposerSessionProgressCapsule } from "@/components/composer/composer-panel/progress-capsule";
import { ModelSecondaryRouteControl } from "@/components/composer/composer-panel/model-secondary-route-control";
import { ComposerGoalStrip, ComposerPlanModeChip } from "@/components/composer/composer-panel/goal-strip";
import { ComposerPanelAttachmentPreviewStrip } from "@/components/composer/composer-panel/attachment-preview-strip";
import { ComposerGoalDialog } from "@/components/composer/composer-panel/goal-dialog";
import { ComposerPanelToolbarRight } from "@/components/composer/composer-panel/toolbar-right";
import { useComposerGoalController } from "@/components/composer/composer-panel/use-composer-goal-controller";
import {
  useComposerSessionDraft,
  type ComposerQueuedDraftReplacementToken,
} from "@/components/composer/composer-panel/use-composer-session-draft";
import { useComposerModelSelection } from "@/components/composer/composer-panel/use-composer-model-selection";
import { useComposerSlashCommands } from "@/components/composer/composer-panel/use-composer-slash-commands";
import { useComposerSendController } from "@/components/composer/composer-panel/use-composer-send-controller";
import { useComposerPlanMode } from "@/components/composer/composer-panel/use-composer-plan-mode";
import { useComposerDraftSessionSettings } from "@/components/composer/composer-panel/use-composer-draft-session-settings";
import { useComposerAttachmentPreparation } from "@/components/composer/composer-panel/use-composer-attachment-preparation";
import { useComposerAttachmentRemoval } from "@/components/composer/composer-panel/use-composer-attachment-removal";
export { ComposerSessionProgressCapsule } from "@/components/composer/composer-panel/progress-capsule";
// ComposerPanel renders the first-version message input surface.
// ComposerPanel 渲染第一版消息输入区域。
// Parameters: props supplies the current input owner, services and controlled conversation state; returns the composer and its attachment, goal and toolbar surfaces.
// 参数：props 提供当前输入归属、服务及受控会话状态；返回输入区及附件、目标和工具栏界面。
export function ComposerPanel(props: {
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
  // Session-level Provider accounting shown inside the context-usage card.
  // 会话级 Provider 统计在上下文占用卡片内展示。
  sessionProviderUsage?: SessionProviderUsageSummary | null;
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
  // Current persisted VMM memory policy for the selected conversation.
  // 当前选中会话的持久化 VMM 记忆策略。
  sessionVmmPolicy: SessionVmmPolicySummary | null;
  // Output-style catalog and persisted global selection.
  // 输出风格目录与已持久化全局选择。
  outputStyleCatalog: OutputStyleCatalogSummary;
  // Current persisted output-style state for the selected conversation.
  // 当前选中会话的持久化输出风格状态。
  sessionOutputStyle: SessionOutputStyleSummary | null;
  // Fixed context-compaction nodes available to the selected session.
  // 当前选中会话可用的固定上下文压缩节点。
  contextCompactionWindowNodes: PublicAiContextCompactionWindowNode[];
  // Stable context-compaction node selected by the current session.
  // 当前会话选择的稳定上下文压缩节点。
  sessionContextCompactionWindowNodeId: ContextCompactionWindowNodeId | null;
  // Whether the effective project configuration makes VMM available to this composer.
  // 当前项目最终生效配置是否允许输入区使用 VMM。
  vmmAvailable: boolean;
  // Memory readiness and configuration navigation remain independent from executable tool admission.
  // 记忆就绪状态及配置导航独立于可执行工具准入。
  vmmStatus?: "disabled" | "not_ready" | "ready";
  onOpenMemorySettings?: () => void;
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
    forceRefresh: boolean,
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
  // Opens the project knowledge-library management surface.
  // 打开项目知识库管理界面。
  onOpenKnowledge: () => void;
  // Sends one validated composer submission through the caller runtime.
  // 通过调用方运行时发送一条已校验输入区提交。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
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
  onUpdateSessionGoalStatus: (status: SessionGoalUserStatusUpdate) => Promise<boolean>;
  // Persists the current session Plan-mode toggle.
  // 持久化当前会话计划模式开关。
  onSetSessionPlanMode: (enabled: boolean) => Promise<boolean>;
  // Persists one current session VMM memory policy mode.
  // 持久化当前会话的单个 VMM 记忆策略模式。
  onSetSessionVmmPolicy: (policy: string, mode: string) => Promise<boolean>;
  // Persists one selected conversation output-style override.
  // 持久化当前会话的输出风格覆盖。
  onSetSessionOutputStyle: (styleId: string | null) => Promise<boolean>;
  // Persists or clears the selected session context-compaction window node.
  // 持久化或清除当前会话的上下文压缩窗口节点。
  onSetSessionContextCompactionWindow: (nodeId: ContextCompactionWindowNodeId | null) => Promise<boolean>;
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
  // composerMediaPlayer stores an attachment preview opened from the composer strip.
  // composerMediaPlayer 保存从输入区附件条打开的附件预览。
  const [composerMediaPlayer, setComposerMediaPlayer] = React.useState<GeneratedMediaAttachment | null>(null);
  // composerTextareaRef focuses the prompt field from global workspace typing.
  // composerTextareaRef 用于从工作台全局输入聚焦提示词输入框。
  const composerTextareaRef = React.useRef<HTMLTextAreaElement | null>(null);
  // pendingPromptEditRef synchronously excludes duplicate edits and competing submissions.
  // pendingPromptEditRef 同步排除重复编辑与竞争提交。
  const pendingPromptEditRef = React.useRef(false);
  // isPendingPromptEditing disables interactive draft mutation while one queue replacement is in flight.
  // isPendingPromptEditing 在一次队列替换进行期间禁用交互式草稿变更。
  const [isPendingPromptEditing, setIsPendingPromptEditing] = React.useState(false);
  // pendingPromptRecoveryToken keeps an ambiguous acknowledgement frozen until authoritative reload succeeds.
  // pendingPromptRecoveryToken 在权威重载成功前保持不明确确认处于冻结状态。
  const [pendingPromptRecoveryToken, setPendingPromptRecoveryToken] = React.useState<ComposerQueuedDraftReplacementToken | null>(null);
  // isPendingPromptEditActive exposes the synchronous replacement gate to the send controller.
  // isPendingPromptEditActive 向发送控制器暴露同步替换门控。
  const isPendingPromptEditActive = React.useCallback(() => pendingPromptEditRef.current, []);
  // composerDraft owns text, attachments, slot persistence, and submission rollback as one transaction.
  // composerDraft 将文本、附件、草稿槽持久化与提交回滚作为一个事务管理。
  const composerDraft = useComposerSessionDraft({
    attachmentCapabilities: props.attachmentCapabilities,
    projectId: props.projectId,
    conversationId: props.conversationId,
    onStageAttachmentBytes: props.onStageAttachmentBytes,
  });
  const {
    draftSlotKey,
    draftInput,
    setDraftInput,
    attachments,
    pendingAttachmentCount,
    hasPendingAttachmentStaging,
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
    clearForSubmission,
    restoreFailedSubmission,
    restoreInputToSlot,
    prepareQueuedDraftReplacement,
    completeQueuedDraftReplacement,
    recoverQueuedDraftReplacement,
  } = composerDraft;
  // releasePendingPromptEditGate releases both synchronous and rendered mutation barriers.
  // releasePendingPromptEditGate 同时释放同步与已渲染的变更屏障。
  const releasePendingPromptEditGate = React.useCallback(() => {
    pendingPromptEditRef.current = false;
    setIsPendingPromptEditing(false);
  }, []);

  React.useEffect(() => {
    if (!pendingPromptRecoveryToken) {
      return;
    }
    // disposed prevents a recovery result from releasing a later component owner.
    // disposed 防止恢复结果释放较晚的组件owner。
    let disposed = false;
    // retryTimer owns only the next bounded reconnect attempt.
    // retryTimer 仅持有下一次有界重连尝试。
    let retryTimer: number | null = null;
    // retryDelayMs backs off repeated local-service outages without allowing stale draft writes.
    // retryDelayMs 对重复本地服务中断进行退避，同时不允许陈旧草稿写入。
    let retryDelayMs = 250;
    // recover reloads the exact captured owner and releases the gate only after an authoritative response.
    // recover 重新加载精确捕获owner，并仅在获得权威响应后释放门控。
    const recover = async () => {
      try {
        await recoverQueuedDraftReplacement(pendingPromptRecoveryToken);
        if (disposed) {
          return;
        }
        setPendingPromptRecoveryToken(null);
        releasePendingPromptEditGate();
      } catch (error) {
        if (disposed) {
          return;
        }
        props.onTrace?.("pending_prompt.edit_recovery_waiting", {
          draftSlotKey: pendingPromptRecoveryToken.owner.draftSlotKey,
          error: error instanceof Error ? error.message : String(error),
        });
        retryTimer = window.setTimeout(() => { void recover(); }, retryDelayMs);
        retryDelayMs = Math.min(retryDelayMs * 2, 5_000);
      }
    };
    void recover();
    return () => {
      disposed = true;
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
      }
    };
  }, [
    pendingPromptRecoveryToken,
    props.onTrace,
    recoverQueuedDraftReplacement,
    releasePendingPromptEditGate,
  ]);
  // draftSessionSettings owns overrides that cannot be persisted until a new conversation materializes.
  // draftSessionSettings 持有新会话物化前无法持久化的覆盖项。
  const draftSessionSettings = useComposerDraftSessionSettings(
    draftSlotKey,
    props.conversationId,
  );
  // providerToolSettingsBlocked mirrors explicit session conflicts into the send boundary.
  // providerToolSettingsBlocked 将显式会话冲突映射到发送边界。
  const [providerToolSettingsBlocked, setProviderToolSettingsBlocked] = React.useState(false);

  // visibleSessionGoal hides completed goals from the composer-level control strip.
  // visibleSessionGoal 从输入区控制条中隐藏已完成目标。
  const persistedVisibleSessionGoal =
    props.sessionGoal?.status === "complete" ? null : props.sessionGoal;
  // visibleSessionGoal uses the draft goal before materialization and the persisted goal afterwards.
  // visibleSessionGoal 在物化前使用草稿目标，物化后使用持久目标。
  const visibleSessionGoal = props.conversationId
    ? persistedVisibleSessionGoal
    : draftSessionSettings.initialGoalSummary;
  // Plan controller unifies scoped draft state, persisted snapshots, goal exclusion, and shortcuts.
  // Plan 控制器统一作用域草稿状态、持久化快照、目标互斥与快捷键。
  const {
    planModeEnabled,
    executionMode,
    setPlanMode: handlePlanModeChange,
    prepareGoalEditor,
    handleContainerKeyDown: handleComposerContainerKeyDown,
  } = useComposerPlanMode({
    scopeKey: draftSlotKey,
    conversationId: props.conversationId,
    sessionPlanMode: props.sessionPlanMode,
    goal: visibleSessionGoal,
    onSetSessionPlanMode: props.onSetSessionPlanMode,
  });
  // Model selection controller synchronizes service snapshots, optimistic choices, and slash-open signals.
  // 模型选择控制器同步服务快照、乐观选择与 slash 打开信号。
  const {
    modelChoice,
    activeModelChoice,
    activeModelAccounts,
    activeProviderModel: activeProviderModelSummary,
    modelPickerOpenSignal,
    reasoningEffort,
    reasoningEffortOptions,
    submittedReasoningEffort,
    reasoningPickerOpenSignal,
    selectModel: handleModelChoiceChange,
    selectReasoningEffort: handleReasoningEffortChange,
    openModelPicker,
    openReasoningPicker,
  } = useComposerModelSelection({
    conversationScopeKey: draftSlotKey,
    persistReasoningSelection: Boolean(props.conversationId),
    providers: props.providers,
    reasoningSelection: props.reasoningSelection,
    onSelectModel: props.onSelectWorkbenchModel,
    onSelectReasoningEffort: props.onSelectWorkbenchReasoningEffort,
  });
  // activeModelSelection freezes the exact rendered route tuple for draft media planning and final submission.
  // activeModelSelection 为草稿媒体规划与最终提交冻结当前渲染的精确路线元组。
  const activeModelSelection = React.useMemo(() => activeModelChoice
    ? {
        providerId: activeModelChoice.providerId,
        modelId: activeModelChoice.modelId,
        connectionId: activeModelChoice.connectionId,
      }
    : null, [activeModelChoice]);
  // Existing sessions persist picker changes through dedicated RPCs, so submissions must not echo
  // rendered model or reasoning values back into Meta. Drafts still carry their initial selection.
  // 已有会话通过专用 RPC 持久化选择器变更，因此提交不得把渲染出的模型或思考值回写 Meta；
  // 草稿仍携带首次选择。
  const submittedModelSelection = props.conversationId ? null : activeModelSelection;
  const submittedSessionReasoningEffort = props.conversationId ? null : submittedReasoningEffort;
  // pendingModelSelectionRef retains the latest model/account persistence request until submission.
  // pendingModelSelectionRef 将最新模型或账号持久化请求保留到提交边界。
  const pendingModelSelectionRef = React.useRef<Promise<boolean> | null>(null);
  // modelSelectionPersistencePending blocks media planning while an optimistic model is not durable.
  // modelSelectionPersistencePending 在乐观模型尚未持久化期间阻止媒体规划。
  const [modelSelectionPersistencePending, setModelSelectionPersistencePending] = React.useState(false);
  // pendingSecondaryRouteSelectionRef retains the latest provider/model route write until submission.
  // pendingSecondaryRouteSelectionRef 将最新供应商/模型路由写入保留到提交边界。
  const pendingSecondaryRouteSelectionRef = React.useRef<Promise<boolean> | null>(null);
  // secondaryRoutePersistencePending blocks media planning until the route write reaches a fixed point.
  // secondaryRoutePersistencePending 在路由写入到达不动点前阻止媒体规划。
  const [secondaryRoutePersistencePending, setSecondaryRoutePersistencePending] = React.useState(false);
  // secondaryRoutePersistenceQueueRef orders overlapping route writes by user intent.
  // secondaryRoutePersistenceQueueRef 按用户意图顺序排列重叠的二级路由写入。
  const secondaryRoutePersistenceQueueRef = React.useRef<Promise<void>>(Promise.resolve());
  // routePlanningRevision is the backend-resolved content identity of the rendered executable route.
  // routePlanningRevision 是后端解析出的当前可执行路线内容身份。
  const routePlanningRevision = activeProviderModelSummary?.model.routeRevision?.trim() ?? "";
  // routeSelectionPending closes the optimistic-selection window before any upload plan is issued.
  // routeSelectionPending 在签发任何上传计划前封闭乐观选择窗口。
  const routeSelectionPending = modelSelectionPersistencePending
    || secondaryRoutePersistencePending;
  // attachmentPreparation owns draft upload, retry, cancellation, and send-barrier state.
  // attachmentPreparation 管理草稿上传、重试、取消与发送屏障状态。
  const attachmentPreparation = useComposerAttachmentPreparation({
    draftSlotKey,
    projectId: props.projectId,
    conversationId: props.conversationId,
    modelSelection: activeModelSelection,
    routePlanningRevision,
    routeSelectionPending,
    attachments,
  });
  // removePreparedAttachment binds asynchronous removal to the exact draft owner and object.
  // removePreparedAttachment 将异步移除绑定到精确草稿所有者与对象。
  const removePreparedAttachment = useComposerAttachmentRemoval({
    draftSlotKey,
    attachments,
    preparation: attachmentPreparation,
    remove: removeComposerAttachment,
  });
  // handleRemoveComposerAttachment reports failed cancellation without discarding its retryable attachment.
  // handleRemoveComposerAttachment 报告取消失败，但不丢弃可重试附件。
  const handleRemoveComposerAttachment = React.useCallback((index: number) => {
    if (pendingPromptEditRef.current) {
      return;
    }
    void removePreparedAttachment(index).catch((error: unknown) => {
      toast.error(error instanceof Error ? error.message : String(error));
    });
  }, [removePreparedAttachment]);
  // handleAttachmentPicker blocks new file selection while a queued prompt owns the draft gate.
  // handleAttachmentPicker 在队列提示词持有草稿门控期间阻止新的文件选择。
  const handleAttachmentPicker = React.useCallback((kind: string | null) => {
    if (!pendingPromptEditRef.current) {
      openAttachmentPicker(kind);
    }
  }, [openAttachmentPicker]);
  // handleAttachmentRetry blocks transfer retries that would mutate the replacement draft interval.
  // handleAttachmentRetry 阻止会在替换草稿区间内产生变更的传输重试。
  const handleAttachmentRetry = React.useCallback((attachmentId: string) => {
    if (!pendingPromptEditRef.current) {
      void attachmentPreparation.retry(attachmentId);
    }
  }, [attachmentPreparation]);
  // handlePersistedModelChoiceChange tracks the exact optimistic write started by either picker.
  // handlePersistedModelChoiceChange 跟踪任一选择器启动的精确乐观写入。
  const handlePersistedModelChoiceChange = React.useCallback((choice: ModelPickerChoice) => {
    const pending = handleModelChoiceChange(choice);
    pendingModelSelectionRef.current = pending;
    setModelSelectionPersistencePending(true);
    // releasePendingModel clears this exact write without creating an unhandled rejected finally promise.
    // releasePendingModel 清理此次精确写入，避免 finally 派生未处理的拒绝 Promise。
    const releasePendingModel = () => {
      if (pendingModelSelectionRef.current === pending) {
        pendingModelSelectionRef.current = null;
        setModelSelectionPersistencePending(false);
      }
    };
    void pending.then(releasePendingModel, releasePendingModel);
    return pending;
  }, [handleModelChoiceChange]);
  // secondaryRouteProviderId is the exact provider adapter id, not the credential-scoped row id.
  // secondaryRouteProviderId 是精确的供应商适配器标识，而不是凭据作用域行标识。
  const secondaryRouteProviderId = activeProviderModelSummary?.provider.sourceProviderId?.trim() ?? "";
  // handleSecondaryRouteSelection tracks the exact persistence request started by the route control.
  // handleSecondaryRouteSelection 跟踪路由控件启动的精确持久化请求。
  const handleSecondaryRouteSelection = React.useCallback((
    providerId: string,
    modelId: string,
    route: ProviderModelSecondaryRouteSelection | null,
  ) => {
    // queuedWrite prevents a slower former RPC from overwriting the latest route choice.
    // queuedWrite 防止较慢的旧 RPC 覆盖最新路由选择。
    const queuedWrite = enqueueComposerPersistence(
      secondaryRoutePersistenceQueueRef.current,
      () => props.onSelectWorkbenchModelSecondaryRoute(providerId, modelId, route),
    );
    secondaryRoutePersistenceQueueRef.current = queuedWrite.tail;
    // pending exposes the exact queued result to the submission barrier.
    // pending 向提交屏障暴露精确的排队结果。
    const pending = queuedWrite.result;
    pendingSecondaryRouteSelectionRef.current = pending;
    setSecondaryRoutePersistencePending(true);
    // releasePendingRoute observes both outcomes while the original promise retains its error contract.
    // releasePendingRoute 观察两种结果，同时由原始 Promise 保留其错误契约。
    const releasePendingRoute = () => {
      if (pendingSecondaryRouteSelectionRef.current === pending) {
        pendingSecondaryRouteSelectionRef.current = null;
        setSecondaryRoutePersistencePending(false);
      }
    };
    void pending.then(releasePendingRoute, releasePendingRoute);
    return pending;
  }, [props.onSelectWorkbenchModelSecondaryRoute]);
  // flushSecondaryRouteSelection waits only for the currently visible route write.
  // flushSecondaryRouteSelection 仅等待当前可见的路由写入。
  const flushSecondaryRouteSelection = React.useCallback(async () => {
    return await flushLatestComposerPersistence(() => pendingSecondaryRouteSelectionRef.current);
  }, []);
  // flushComposerRouteSelections prevents provider dispatch from racing an explicit model/account
  // write, then waits for the selected model's secondary-route write.
  // flushComposerRouteSelections 防止供应商分派与显式模型或账号写入竞争，随后等待所选模型的
  // 二级路由写入。
  const flushComposerRouteSelections = React.useCallback(async () => {
    while (true) {
      // modelSaved drains every model/account intent observable at this submission boundary.
      // modelSaved 排空本次提交边界可观察到的全部模型或账号意图。
      const modelSaved = await flushLatestComposerPersistence(() => pendingModelSelectionRef.current);
      if (!modelSaved) {
        return false;
      }
      // secondaryRouteSaved drains the route queue after the latest model intent has settled.
      // secondaryRouteSaved 在最新模型意图结束后排空路由队列。
      const secondaryRouteSaved = await flushSecondaryRouteSelection();
      if (!secondaryRouteSaved) {
        return false;
      }
      // A model change can arrive while the secondary route is settling, so both queues must be
      // empty at the same synchronous fixed point before provider dispatch begins.
      // 模型变更可能在二级路由结束期间到达，因此供应商分派前两个队列必须在同一个同步
      // 不动点上均为空。
      if (!pendingModelSelectionRef.current && !pendingSecondaryRouteSelectionRef.current) {
        return true;
      }
    }
  }, [flushSecondaryRouteSelection]);
  // placeholder describes the platform-specific send and newline keyboard behavior.
  // placeholder 描述平台相关的发送与换行键盘行为。
  const placeholder = React.useMemo(() => {
    return t("composer.placeholder").replace("{shortcut}", composerShortcutHint(t("composer.shortcutHint")));
  }, [t]);
  // Send controller owns duplicate gating, stop routing, draft consumption, and failure recovery.
  // 发送控制器管理重复门控、停止路由、草稿消费与失败恢复。
  const {
    canAutoSubmitCreatedGoal,
    isSubmissionActive,
    isStopping,
    sendDraft,
    submitCreatedGoal,
  } = useComposerSendController({
    draftSlotKey,
    draftInput,
    attachments,
    projectAttachmentsForSubmission: attachmentPreparation.projectForSubmission,
    executionMode,
    reasoningEffort: submittedSessionReasoningEffort,
    modelSelection: submittedModelSelection,
    captureSessionSettings: draftSessionSettings.captureSubmissionSettings,
    clearSessionSettings: draftSessionSettings.clearSubmissionSettings,
    beforeSubmit: flushComposerRouteSelections,
    isConversationRunning: props.isConversationRunning,
    isDraftReplacementActive: isPendingPromptEditActive,
    clearForSubmission,
    restoreFailedSubmission,
    restoreInputToSlot,
    onSubmitPrompt: props.onSubmitPrompt,
    onStopConversationRun: props.onStopConversationRun,
    onTrace: props.onTrace,
  });
  // handleComposerSend routes the visible stop action before applying barriers shared by button and keyboard submission.
  // handleComposerSend 先路由界面显示的停止操作，再应用按钮和键盘提交共同遵守的屏障。
  // Parameters: none; returns after stopping the current run or validating and submitting the current draft.
  // 参数：无；停止当前运行或校验并提交当前草稿后返回。
  const handleComposerSend = React.useCallback(async () => {
    // Match the toolbar's stop action: unsent staging and draft replacement must not block stopping a running turn.
    // 与工具栏的停止操作保持一致：未完成的暂存和草稿替换不得阻止停止运行中的轮次。
    if (props.isConversationRunning && !draftInput.trim() && attachments.length === 0) {
      await sendDraft();
      return;
    }
    if (pendingPromptEditRef.current) return;
    if (hasPendingAttachmentStaging()) {
      toast.info(t("composer.attachment.stagingPending"));
      return;
    }
    if (providerToolSettingsBlocked) return;
    if (attachmentPreparation.blocksSubmission) {
      toast.info(t("composer.attachment.preparationBlocked").replace(
        "{count}",
        String(attachmentPreparation.blockingCount),
      ));
      return;
    }
    await sendDraft();
  }, [
    attachments.length,
    attachmentPreparation.blockingCount,
    attachmentPreparation.blocksSubmission,
    draftInput,
    props.isConversationRunning,
    providerToolSettingsBlocked,
    sendDraft,
    hasPendingAttachmentStaging,
    t,
  ]);

  // handleOutputStyleChange keeps draft changes local and routes persisted-session changes through the safe boundary API.
  // handleOutputStyleChange 将草稿修改保留在本地，并通过安全边界 API 路由已持久化会话修改。
  const handleOutputStyleChange = React.useCallback((styleId: string | null) => {
    if (props.conversationId) {
      void props.onSetSessionOutputStyle(styleId);
      return;
    }
    if (styleId) {
      draftSessionSettings.setOutputStyle(styleId);
    } else {
      draftSessionSettings.clearOutputStyle();
    }
  }, [draftSessionSettings, props.conversationId, props.onSetSessionOutputStyle]);

  // handlePendingPromptEdit restores one queued prompt into the composer draft.
  // handlePendingPromptEdit 将单条排队提示词恢复到输入区草稿。
  const handlePendingPromptEdit = React.useCallback(async (entryId: string) => {
    if (pendingPromptEditRef.current || isSubmissionActive()) {
      return;
    }
    pendingPromptEditRef.current = true;
    setIsPendingPromptEditing(true);
    props.onTrace?.("pending_prompt.edit", { entryId });
    // token exists only after every earlier draft write has reached its durable boundary.
    // token 仅在全部较早草稿写入到达持久边界后存在。
    let token: ComposerQueuedDraftReplacementToken | null = null;
    // recoveryScheduled keeps the mutation gate closed across an ambiguous RPC acknowledgement.
    // recoveryScheduled 在RPC确认不明确时持续关闭变更门控。
    let recoveryScheduled = false;
    try {
      token = await prepareQueuedDraftReplacement();
      const entry = await props.onEditPendingPrompt(entryId);
      if (!entry) {
        recoveryScheduled = true;
        setPendingPromptRecoveryToken(token);
        return;
      }
      const replacementAttachments = entry.attachments ?? [];
      const appliedToActiveOwner = completeQueuedDraftReplacement(
        token,
        entry.prompt,
        replacementAttachments,
      );
      if (appliedToActiveOwner) {
        try {
          await attachmentPreparation.reconcileDraft(replacementAttachments);
        } catch (error) {
          // The durable queued target is already applied; a lease projection fault remains retryable in place.
          // 持久队列目标已经应用；租约投影故障保留为原位可重试状态。
          toast.error(error instanceof Error ? error.message : String(error));
        }
        window.requestAnimationFrame(() => composerTextareaRef.current?.focus({ preventScroll: true }));
      }
    } catch (error) {
      if (token) {
        recoveryScheduled = true;
        setPendingPromptRecoveryToken(token);
      }
      toast.error(t("composer.error.draftPersistFailed"), {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      if (!recoveryScheduled) {
        releasePendingPromptEditGate();
      }
    }
  }, [
    attachmentPreparation,
    completeQueuedDraftReplacement,
    isSubmissionActive,
    prepareQueuedDraftReplacement,
    props.onEditPendingPrompt,
    props.onTrace,
    releasePendingPromptEditGate,
    t,
  ]);
  // handlePendingPromptInsert excludes insertion while an exact queue edit owns the queue.
  // handlePendingPromptInsert 在精确队列编辑持有队列期间排除插入操作。
  const handlePendingPromptInsert = React.useCallback((entryId: string) => {
    if (!pendingPromptEditRef.current) {
      void props.onInsertPendingPrompt(entryId);
    }
  }, [props.onInsertPendingPrompt]);
  // handlePendingPromptRemove excludes removal while an exact queue edit owns the queue.
  // handlePendingPromptRemove 在精确队列编辑持有队列期间排除移除操作。
  const handlePendingPromptRemove = React.useCallback((entryId: string) => {
    if (!pendingPromptEditRef.current) {
      void props.onRemovePendingPrompt(entryId);
    }
  }, [props.onRemovePendingPrompt]);
  // handlePendingPromptReorder excludes reordering while an exact queue edit owns the queue.
  // handlePendingPromptReorder 在精确队列编辑持有队列期间排除重排操作。
  const handlePendingPromptReorder = React.useCallback((
    entryIds: string[],
    draggedEntryId: string,
    targetEntryId: string | null,
  ) => {
    if (!pendingPromptEditRef.current) {
      void props.onReorderPendingPrompts(entryIds, draggedEntryId, targetEntryId);
    }
  }, [props.onReorderPendingPrompts]);

  // Goal controller state and actions isolate goal lifecycle orchestration from the composer input flow.
  // 目标控制器状态与操作将目标生命周期编排从输入区输入流程中隔离出来。
  const {
    dialogOpen: goalDialogOpen,
    dialogMode: goalDialogMode,
    objectiveDraft: goalObjectiveDraft,
    tokenBudgetDraft: goalTokenBudgetDraft,
    saving: goalSaving,
    setDialogOpen: setGoalDialogOpen,
    setObjectiveDraft: setGoalObjectiveDraft,
    setTokenBudgetDraft: setGoalTokenBudgetDraft,
    openEditor: openGoalDialog,
    openResumeEditor: openGoalResumeDialog,
    save: handleGoalSave,
    toggleStatus: handleGoalStatusToggle,
    clear: handleGoalClear,
  } = useComposerGoalController({
    scopeKey: draftSlotKey,
    goal: visibleSessionGoal,
    canAutoSubmitCreatedGoal,
    onPrepareEditor: prepareGoalEditor,
    onSaveGoal: props.conversationId
      ? props.onSaveSessionGoal
      : async (objective, tokenBudget) => {
          draftSessionSettings.setInitialGoal(objective, tokenBudget ?? null);
          return true;
        },
    onResumeGoal: props.onResumeSessionGoal,
    onClearGoal: props.conversationId
      ? props.onClearSessionGoal
      : async () => {
          draftSessionSettings.clearInitialGoal();
          return true;
        },
    onUpdateGoalStatus: props.onUpdateSessionGoalStatus,
    onSubmitCreatedGoal: submitCreatedGoal,
  });

  // Slash controller owns filtering, navigation, activation, and Enter-key priority.
  // slash 控制器管理筛选、导航、激活与 Enter 键优先级。
  const {
    visibleCommands: visibleSlashCommands,
    panelVisible: slashPanelVisible,
    highlightedIndex: slashHighlightedIndex,
    setHighlightedIndex: setSlashHighlightedIndex,
    handleInputChange: handleDraftInputChange,
    appendGlobalInput,
    activateCommand: activateSlashCommand,
    handleKeyDown: handleComposerKeyDown,
    reset: resetSlashCommands,
  } = useComposerSlashCommands({
    input: draftInput,
    setInput: setDraftInput,
    commands: props.slashCommands,
    planModeEnabled,
    onSetPlanMode: handlePlanModeChange,
    onRouteCommand: props.onSlashCommand,
    onOpenModelPicker: openModelPicker,
    onOpenReasoningPicker: openReasoningPicker,
    onSend: handleComposerSend,
  });

  React.useLayoutEffect(() => {
    resetSlashCommands();
    setComposerMediaPlayer(null);
  }, [draftSlotKey, resetSlashCommands]);

  React.useEffect(() => {
    // handleGlobalWorkspaceTyping redirects ordinary workspace typing into the composer.
    // handleGlobalWorkspaceTyping 将工作台中的普通键入重定向到输入区。
    const handleGlobalWorkspaceTyping = (event: KeyboardEvent) => {
      if (
        pendingPromptEditRef.current
        || event.defaultPrevented
        || !isComposerPrintableKey(event)
      ) {
        return;
      }
      const inputOwner = getWorkspaceInputOwner();
      if (inputOwner.kind === "project_terminal" || inputOwner.kind === "modal") {
        return;
      }
      // textarea is the unique composer input that receives redirected typing.
      // textarea 是接收重定向输入的唯一输入区文本框。
      const textarea = composerTextareaRef.current;
      if (!textarea || document.activeElement === textarea) {
        return;
      }
      if (hasBlockingComposerPopup() || isComposerTextEntryElement(document.activeElement)) {
        return;
      }
      event.preventDefault();
      textarea.focus({ preventScroll: true });
      appendGlobalInput(event.key);
    };
    document.addEventListener("keydown", handleGlobalWorkspaceTyping);
    return () => document.removeEventListener("keydown", handleGlobalWorkspaceTyping);
  }, [appendGlobalInput]);

  return (
    <TooltipProvider delayDuration={500}>
      {props.pendingPrompts.length ? (
        <PendingPromptQueueStrip
          disabled={isPendingPromptEditing}
          entries={props.pendingPrompts}
          onTrace={props.onTrace}
          onEdit={(entryId) => void handlePendingPromptEdit(entryId)}
          onInsert={handlePendingPromptInsert}
          onRemove={handlePendingPromptRemove}
          onReorder={handlePendingPromptReorder}
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
        data-workspace-input-owner="composer"
        onDragLeave={() => setIsAttachmentDragOver(false)}
        onDragOver={isPendingPromptEditing ? undefined : handleComposerDragOver}
        onDrop={isPendingPromptEditing ? undefined : handleComposerDrop}
        onKeyDownCapture={handleComposerContainerKeyDown}
      >
        <input
          accept={attachmentInputAccept}
          className="composer-attachment-input"
          disabled={isPendingPromptEditing}
          multiple
          onChange={handleAttachmentInputChange}
          ref={attachmentFileInputRef}
          type="file"
        />
        {/* Keep the owner-mounted empty strip long enough to restore focus; it renders no empty surface.
            保留所属附件条的空状态挂载以恢复焦点；空状态不渲染空白区域。 */}
        <ComposerPanelAttachmentPreviewStrip
          key={draftSlotKey}
          attachments={attachments}
          preparations={attachmentPreparation.preparations}
          progresses={attachmentPreparation.progresses}
          onOpenMedia={setComposerMediaPlayer}
          onRemove={handleRemoveComposerAttachment}
          onEmptyAfterRemoval={() => composerTextareaRef.current?.focus({ preventScroll: true })}
          onRetry={handleAttachmentRetry}
          resolveMediaSourceUrl={props.resolveMediaSourceUrl}
        />
        {slashPanelVisible && !isPendingPromptEditing ? (
          <ComposerSlashCommandPanel
            commands={visibleSlashCommands}
            highlightedIndex={Math.min(slashHighlightedIndex, visibleSlashCommands.length - 1)}
            locale={locale}
            onActivate={activateSlashCommand}
            onHighlight={setSlashHighlightedIndex}
          />
        ) : null}
        <ComposerTextContextMenu
          disabled={isPendingPromptEditing}
          enabled={isElectronRuntime()}
          onChange={handleDraftInputChange}
          ownerKey={draftSlotKey}
          textareaRef={composerTextareaRef}
          value={draftInput}
        >
          <Textarea
            aria-label={t("composer.aria.input")}
            disabled={isPendingPromptEditing}
            onChange={(event) => handleDraftInputChange(event.currentTarget.value)}
            onKeyDown={handleComposerKeyDown}
            onPaste={isPendingPromptEditing ? undefined : handleComposerPaste}
            placeholder={placeholder}
            ref={composerTextareaRef}
            value={draftInput}
          />
        </ComposerTextContextMenu>
        {pendingAttachmentCount > 0 ? <p role="status" aria-busy="true" className="px-3 text-xs text-muted-foreground">
          {t("composer.attachment.stagingPending")} ({pendingAttachmentCount})
        </p> : null}
        <div className="composer-toolbar">
          <div className="composer-toolbar-left">
            <ComposerAttachmentMenu
              capabilities={enabledAttachmentCapabilities}
              conversationId={props.conversationId}
              draftOutputStyleId={draftSessionSettings.outputStyleId}
              onOutputStyleChange={handleOutputStyleChange}
              outputStyleCatalog={props.outputStyleCatalog}
              projectId={props.projectId}
              thinkingDisplayMode={props.thinkingDisplayMode}
              planModeEnabled={planModeEnabled}
              sessionVmmPolicy={
                props.conversationId
                  ? props.sessionVmmPolicy
                  : draftSessionSettings.vmmPolicy
              }
              sessionOutputStyle={props.sessionOutputStyle}
              vmmAvailable={props.vmmAvailable}
              vmmStatus={props.vmmStatus}
              onOpenMemorySettings={props.onOpenMemorySettings}
              draftKnowledgeSelection={draftSessionSettings.knowledgeSelection}
              onDraftKnowledgeSelectionChange={draftSessionSettings.setKnowledgeSelection}
              onOpenKnowledge={props.onOpenKnowledge}
              onPick={handleAttachmentPicker}
              onThinkingDisplayModeChange={props.onThinkingDisplayModeChange}
              onOpenGoal={() => void openGoalDialog()}
              onPlanModeChange={(enabled) => void handlePlanModeChange(enabled)}
              onVmmPolicyChange={(policy, mode) => {
                if (props.conversationId) {
                  void props.onSetSessionVmmPolicy(policy, mode);
                } else {
                  draftSessionSettings.setVmmPolicy(policy, mode);
                }
              }}
            />
            {planModeEnabled ? (
              <ComposerPlanModeChip
                artifactName={props.sessionPlanMode?.artifact?.name ?? null}
                onClose={() => void handlePlanModeChange(false)}
              />
            ) : null}
            <ModelPickerControl
              ariaLabel={t("composer.aria.model")}
              fallbackLabel={t("composer.modelPicker.placeholder")}
              filterPlaceholder={t("composer.modelPicker.filter")}
              noMatchesLabel={t("composer.modelPicker.noMatches")}
              onValueChange={(choice) => void handlePersistedModelChoiceChange(choice)}
              openSignal={modelPickerOpenSignal}
              providers={props.providers}
              value={modelChoice}
            />
            <ModelAccountPickerControl
              accounts={activeModelAccounts}
              ariaLabel={t("composer.aria.account")}
              onValueChange={(choice) => void handlePersistedModelChoiceChange(choice)}
              value={activeModelChoice?.connectionId ?? ""}
            />
            <ComposerAiCapabilityControl
              ariaLabel={t("composer.aria.capability")}
              conversationId={props.conversationId}
              draftSettings={draftSessionSettings.providerToolSettings}
              onConflictChange={setProviderToolSettingsBlocked}
              onDraftSettingsChange={draftSessionSettings.setProviderToolSettings}
            />
            {activeProviderModelSummary?.model.supportsSecondaryRoutes && secondaryRouteProviderId ? (
              <ModelSecondaryRouteControl
                modelId={activeProviderModelSummary.model.id}
                onListRoutes={props.onListWorkbenchModelSecondaryRoutes}
                onSelectRoute={handleSecondaryRouteSelection}
                providerId={secondaryRouteProviderId}
                selectedRoute={activeProviderModelSummary.model.secondaryRoute ?? null}
              />
            ) : null}
          </div>
          <ComposerPanelToolbarRight
            attachmentCount={attachments.length}
            contextUsage={props.contextUsage}
            draftSlotKey={draftSlotKey}
            sessionProviderUsage={props.sessionProviderUsage}
            contextCompactionWindowNodes={props.contextCompactionWindowNodes}
            sessionContextCompactionWindowNodeId={props.sessionContextCompactionWindowNodeId}
            modelContextLimit={activeProviderModelSummary?.model.contextLimit ?? null}
            draftInput={draftInput}
            isConversationRunning={props.isConversationRunning}
            sendBlocked={isPendingPromptEditing || pendingAttachmentCount > 0 || providerToolSettingsBlocked || attachmentPreparation.blocksSubmission}
            stopPending={isStopping}
            onReasoningEffortChange={(value) => void handleReasoningEffortChange(value)}
            onSetSessionContextCompactionWindow={props.onSetSessionContextCompactionWindow}
            onSend={() => void handleComposerSend()}
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
    </TooltipProvider>
  );
}
