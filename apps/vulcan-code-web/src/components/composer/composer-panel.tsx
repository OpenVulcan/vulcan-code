import React from "react";
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
import { Textarea } from "@/components/ui/textarea";
import { TooltipProvider } from "@/components/ui/tooltip";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type {
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
  SessionGoalUserStatusUpdate,
  SessionPlanModeSummary,
  SessionTodolistSummary,
  SessionVmmPolicySummary,
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
import { useComposerSessionDraft } from "@/components/composer/composer-panel/use-composer-session-draft";
import { useComposerModelSelection } from "@/components/composer/composer-panel/use-composer-model-selection";
import { useComposerSlashCommands } from "@/components/composer/composer-panel/use-composer-slash-commands";
import { useComposerSendController } from "@/components/composer/composer-panel/use-composer-send-controller";
import { useComposerPlanMode } from "@/components/composer/composer-panel/use-composer-plan-mode";
export { ComposerSessionProgressCapsule } from "@/components/composer/composer-panel/progress-capsule";
// ComposerPanel renders the first-version message input surface.
// ComposerPanel 渲染第一版消息输入区域。
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
    replaceDraft,
  } = composerDraft;

  // visibleSessionGoal hides completed goals from the composer-level control strip.
  // visibleSessionGoal 从输入区控制条中隐藏已完成目标。
  const visibleSessionGoal = props.sessionGoal?.status === "complete" ? null : props.sessionGoal;
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
    providers: props.providers,
    reasoningSelection: props.reasoningSelection,
    onSelectModel: props.onSelectWorkbenchModel,
    onSelectReasoningEffort: props.onSelectWorkbenchReasoningEffort,
  });
  // secondaryRouteProviderId is the exact provider adapter id, not the credential-scoped row id.
  // secondaryRouteProviderId 是精确的供应商适配器标识，而不是凭据作用域行标识。
  const secondaryRouteProviderId = activeProviderModelSummary?.provider.sourceProviderId?.trim() ?? "";
  // placeholder describes the platform-specific send and newline keyboard behavior.
  // placeholder 描述平台相关的发送与换行键盘行为。
  const placeholder = React.useMemo(() => {
    return t("composer.placeholder").replace("{shortcut}", composerShortcutHint(t("composer.shortcutHint")));
  }, [t]);
  // Send controller owns duplicate gating, stop routing, draft consumption, and failure recovery.
  // 发送控制器管理重复门控、停止路由、草稿消费与失败恢复。
  const {
    canAutoSubmitCreatedGoal,
    sendDraft: handleComposerSend,
    stopConversation: handleComposerStop,
    submitCreatedGoal,
  } = useComposerSendController({
    draftSlotKey,
    draftInput,
    attachments,
    executionMode,
    reasoningEffort: submittedReasoningEffort,
    modelSelection: activeModelChoice
      ? {
          providerId: activeModelChoice.providerId,
          modelId: activeModelChoice.modelId,
          connectionId: activeModelChoice.connectionId,
        }
      : null,
    isConversationRunning: props.isConversationRunning,
    clearForSubmission,
    restoreFailedSubmission,
    restoreInputToSlot,
    onSubmitPrompt: props.onSubmitPrompt,
    onStopConversationRun: props.onStopConversationRun,
    onTrace: props.onTrace,
  });

  // handlePendingPromptEdit restores one queued prompt into the composer draft.
  // handlePendingPromptEdit 将单条排队提示词恢复到输入区草稿。
  const handlePendingPromptEdit = React.useCallback(async (entryId: string) => {
    props.onTrace?.("pending_prompt.edit", { entryId });
    const entry = await props.onEditPendingPrompt(entryId);
    if (!entry) {
      return;
    }
    replaceDraft(entry.prompt, entry.attachments ?? []);
    window.requestAnimationFrame(() => composerTextareaRef.current?.focus({ preventScroll: true }));
  }, [props.onEditPendingPrompt, props.onTrace, replaceDraft]);

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
    onSaveGoal: props.onSaveSessionGoal,
    onResumeGoal: props.onResumeSessionGoal,
    onClearGoal: props.onClearSessionGoal,
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
      if (event.defaultPrevented || !isComposerPrintableKey(event)) {
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
              conversationId={props.conversationId}
              projectId={props.projectId}
              thinkingDisplayMode={props.thinkingDisplayMode}
              planModeEnabled={planModeEnabled}
              sessionVmmPolicy={props.sessionVmmPolicy}
              onOpenKnowledge={props.onOpenKnowledge}
              onPick={openAttachmentPicker}
              onThinkingDisplayModeChange={props.onThinkingDisplayModeChange}
              onOpenGoal={() => void openGoalDialog()}
              onPlanModeChange={(enabled) => void handlePlanModeChange(enabled)}
              onVmmPolicyChange={(policy, mode) => void props.onSetSessionVmmPolicy(policy, mode)}
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
            <ModelAccountPickerControl
              accounts={activeModelAccounts}
              ariaLabel={t("composer.aria.account")}
              onValueChange={(choice) => void handleModelChoiceChange(choice)}
              value={activeModelChoice?.connectionId ?? ""}
            />
            <ComposerAiCapabilityControl
              ariaLabel={t("composer.aria.capability")}
              conversationId={props.conversationId}
            />
            {activeProviderModelSummary?.model.supportsSecondaryRoutes && secondaryRouteProviderId ? (
              <ModelSecondaryRouteControl
                modelId={activeProviderModelSummary.model.id}
                onListRoutes={props.onListWorkbenchModelSecondaryRoutes}
                onSelectRoute={props.onSelectWorkbenchModelSecondaryRoute}
                providerId={secondaryRouteProviderId}
                selectedRoute={activeProviderModelSummary.model.secondaryRoute ?? null}
              />
            ) : null}
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
    </TooltipProvider>
  );
}
