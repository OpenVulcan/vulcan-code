import React from "react";
import { conversationSelectionEqual, useSelectedConversationEventWindowSelector } from "@/state/conversation-event-window-react";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type { AnswerMode, AnswerModeOwner } from "@/service/answer-mode";
import type { PermissionModeOwner } from "@/components/composer/composer-panel/use-composer-permission-mode";
import { useVisiblePendingPrompts } from "@/components/composer/composer-panel/use-visible-pending-prompts";
import { createPortal } from "react-dom";
import { ArrowDown } from "lucide-react";
import { type ModelPickerChoice } from "@/components/composer/model-picker-control";
import {
  type ComposerSlashCommandResult,
  type ComposerSubmission,
} from "@/components/composer/composer-model";
import { ComposerPanel, ComposerSessionProgressCapsule } from "@/components/composer/composer-panel";
import type { ConversationSessionReviewSummary } from "@/components/conversation/session-todolist-panel";
import type {
  ComposerTraceCallback,
} from "@/components/composer/composer-control-types";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";
import { useI18n } from "@/i18n";
import type {
  ComposerAttachmentCapabilitySummary,
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ContextWindowSelection,
  ContextUsageSnapshot,
  SessionProviderUsageSummary,
  PublicAiContextCompactionWindowNode,
  PendingPromptSummary,
  ProviderSummary,
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
  VoiceInputSummary,
} from "@/types";

import { selectDedicatedMemory } from "@/service/dedicated-memory";
import { DraftComposerContext } from "@/components/composer/dedicated/draft-composer-context";
import { useDedicatedDraft } from "@/components/composer/dedicated/dedicated-draft-store";
import { DedicatedDraftStrip } from "@/components/composer/dedicated/dedicated-draft-strip";
import { DedicatedComposerContext, DedicatedMemoryLockBar } from "@/components/composer/dedicated/dedicated-session-controls";

// LazyDedicatedSessionWizard keeps the creation wizard and its styles outside the workbench startup chunk.
// LazyDedicatedSessionWizard 将创建向导及其样式保留在工作台启动分块之外。
const LazyDedicatedSessionWizard = React.lazy(() => import("@/components/composer/dedicated/dedicated-session-wizard").then((module) => ({
  default: module.DedicatedSessionWizard,
})));
// LazyFileWindowTaskbar lists the session's file windows above the composer.
// LazyFileWindowTaskbar 在输入区上方列出会话的文件窗口。
const LazyFileWindowTaskbar = React.lazy(() => import("@/components/file-windows/file-window-taskbar"));

export { composerAttachmentMedia } from "@/components/composer/composer-attachment-media";

/** Select only answer-mode ownership/config so streaming tokens do not rerender the composer dock.
 * 仅选择回答模式归属与配置，使流式 Token 不触发输入区浮层重绘。 */
function selectAnswerMode(snapshot: ConversationEventWindowControllerSnapshot): AnswerModeOwner | null {
  if (snapshot.owner === null || snapshot.session_config === null) return null;
  return { sessionId: snapshot.owner.session_id, generation: snapshot.owner.window_generation, mode: snapshot.session_config.answer_mode };
}

/** Select only permission-mode ownership/config so streaming tokens do not rerender the composer dock.
 * 仅选择权限模式归属与配置，使流式 Token 不触发输入区浮层重绘。 */
function selectPermissionMode(snapshot: ConversationEventWindowControllerSnapshot): PermissionModeOwner | null {
  if (snapshot.owner === null || snapshot.session_config === null) return null;
  return {
    sessionId: snapshot.owner.session_id,
    generation: snapshot.owner.window_generation,
    mode: snapshot.session_config.permission_mode,
    unattended: snapshot.session_config.unattended ?? null,
  };
}

// ComposerDock renders composer-related floating controls.
// ComposerDock 渲染输入区相关的浮动控件。
export function ComposerDock(props: {
  // Attachment capability gates used by composer attachment controls.
  // 输入区附件控件使用的附件能力门控。
  attachmentCapabilities: ComposerAttachmentCapabilitySummary[];
  // Published voice-input state for the composer microphone.
  // 输入区麦克风使用的已发布语音输入状态。
  voiceInput?: VoiceInputSummary | null;
  // Owning project id used for attachment staging.
  // 附件暂存使用的所属项目标识。
  projectId: string | null;
  // Owning conversation id used for attachment staging.
  // 附件暂存使用的所属会话标识。
  conversationId: string | null;
  // Project folder whose files the @ path mention offers; null without a project.
  // @ 路径提及所提供文件所在的项目文件夹；没有项目时为 null。
  workspacePath: string | null;
  // Context usage snapshot shown by the composer orb.
  // 输入区圆盘展示的上下文占用快照。
  contextUsage: ContextUsageSnapshot | null;
  // Session-level Provider accounting shown inside the context-usage card.
  // 会话级 Provider 统计在上下文占用卡片内展示。
  sessionProviderUsage?: SessionProviderUsageSummary | null;
  // Fixed context-compaction nodes available to this composer.
  // 当前输入区可用的固定上下文压缩节点。
  contextCompactionWindowNodes: PublicAiContextCompactionWindowNode[];
  // Stable context-compaction node selected by the current session.
  // 当前会话选择的稳定上下文压缩节点。
  sessionContextWindowSelection: ContextWindowSelection | null;
  // Smallest custom window the backend accepts; null hides custom entry.
  // 后端接受的最小自定义窗口；为空时隐藏自定义输入。
  customContextWindowMinTokens: number | null;
  // Whether the dock is visually centered for an empty conversation.
  // 输入区浮层是否在空会话中居中显示。
  emptyMode: boolean;
  // Start content of an empty conversation, stacked in the dock column around the composer so the opening
  // heading, the draft controls and the composer can never overlap.
  // 空会话的起始内容，叠放在停靠层列中输入区的上下方，使开场标题、草稿控件与输入区永不重叠。
  startHero?: React.ReactNode;
  startFooter?: React.ReactNode;
  // Slash commands projected by the shared workbench service.
  // 共享工作台服务投影出的 slash 指令。
  slashCommands: ComposerSlashCommandSummary[];
  // Visible pending prompts retained for the selected conversation.
  // 当前选中会话保留的可见待发送提示词。
  pendingPrompts: PendingPromptSummary[];
  // Current persisted goal for the selected conversation.
  // 当前选中会话的持久化目标。
  sessionGoal: SessionGoalSummary | null;
  // Current model-authored session progress plan for the selected conversation.
  // 当前选中会话中模型写入的会话进程计划。
  sessionTodolist: SessionTodolistSummary | null;
  // Current Git review summary shown beside compact session progress.
  // 当前用于紧凑会话进程旁展示的 Git 审查摘要。
  sessionReviewSummary: ConversationSessionReviewSummary | null;
  // Whether the selected conversation has an active backend run.
  // 当前选中会话是否存在活跃后端运行。
  isConversationRunning: boolean;
  // Thinking display mode selected by the WebUI plus menu.
  // WebUI 加号菜单选中的思考展示模式。
  thinkingDisplayMode: ThinkingDisplayMode;
  // showScrollToBottom controls whether the bottom-jump button is visible.
  // showScrollToBottom 控制是否显示跳转到底部按钮。
  showScrollToBottom: boolean;
  // onScrollToBottom moves the current conversation to its latest content.
  // onScrollToBottom 将当前会话移动到最新内容。
  onScrollToBottom: () => void;
  // Providers exposed by the shared service API for model picking.
  // 共享 service API 暴露给模型选择器的 Provider 列表。
  providers: ProviderSummary[];
  // onSelectWorkbenchModel persists a selected workbench model.
  // onSelectWorkbenchModel 持久化选中的工作台模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // onListWorkbenchModelSecondaryRoutes loads secondary route choices.
  // onListWorkbenchModelSecondaryRoutes 加载二级路由选项。
  onListWorkbenchModelSecondaryRoutes: (
    providerId: string,
    modelId: string,
    forceRefresh: boolean,
  ) => Promise<ProviderModelSecondaryRouteSummary[]>;
  // onSelectWorkbenchModelSecondaryRoute persists a selected secondary route.
  // onSelectWorkbenchModelSecondaryRoute 持久化选中的二级路由。
  onSelectWorkbenchModelSecondaryRoute: (
    providerId: string,
    modelId: string,
    route: ProviderModelSecondaryRouteSelection | null,
  ) => Promise<boolean>;
  // onSelectWorkbenchReasoningEffort persists a selected reasoning effort.
  // onSelectWorkbenchReasoningEffort 持久化选中的思考档位。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // onSlashCommand routes one projected slash command.
  // onSlashCommand 路由单个投影 slash 指令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // onOpenKnowledge opens the project knowledge-library management surface.
  // onOpenKnowledge 打开项目知识库管理界面。
  onOpenKnowledge: () => void;
  // Reports the measured floating composer stack height.
  // 上报浮动输入区堆栈的实测高度。
  onComposerDockHeightChange: (height: number) => void;
  // Session runtime cue floated over the dock's top-left edge; it is out of flow, so it never changes the
  // measured dock height or the transcript layout.
  // 悬浮在停靠层左上沿的会话运行提示；它脱离布局流，因此不会改变实测停靠层高度或转录布局。
  runtimeStatus?: React.ReactNode;
  // onSubmitPrompt sends one composer prompt through the workbench runtime.
  // onSubmitPrompt 通过工作台运行时发送一条输入区提示词。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // onSaveSessionGoal persists one user-authored session goal.
  // onSaveSessionGoal 持久化单个用户编写的会话目标。
  onSaveSessionGoal: (objective: string, tokenBudget?: number | null) => Promise<boolean>;
  // onResumeSessionGoal resumes one stopped session goal with a fresh budget stage.
  // onResumeSessionGoal 使用新的预算阶段继续单个已停止会话目标。
  onResumeSessionGoal: (objective: string, tokenBudget?: number | null) => Promise<boolean>;
  // onClearSessionGoal clears the current session goal.
  // onClearSessionGoal 清理当前会话目标。
  onClearSessionGoal: () => Promise<boolean>;
  // onUpdateSessionGoalStatus updates the current session goal lifecycle status.
  // onUpdateSessionGoalStatus 更新当前会话目标生命周期状态。
  onUpdateSessionGoalStatus: (status: SessionGoalUserStatusUpdate) => Promise<boolean>;
  // onSetSessionPlanMode persists the current session Plan-mode toggle.
  // onSetSessionPlanMode 持久化当前会话计划模式开关。
  onSetSessionPlanMode: (enabled: boolean) => Promise<boolean>;
  // onSetSessionVmmPolicy persists one current session VMM memory policy mode.
  // onSetSessionVmmPolicy 持久化当前会话的单个 VMM 记忆策略模式。
  onSetSessionVmmPolicy: (policy: string, mode: string) => Promise<boolean>;
  // Persists one selected conversation output-style override.
  // 持久化当前会话的输出风格覆盖。
  onSetSessionOutputStyle: (styleId: string | null) => Promise<boolean>;
  // Persists or clears the selected session context-compaction window node.
  // 持久化或清除当前会话的上下文压缩窗口节点。
  onSetSessionContextCompactionWindow: (selection: ContextWindowSelection | null) => Promise<boolean>;
  // onStopConversationRun stops the selected backend conversation stream.
  // onStopConversationRun 停止当前选中的后端会话流。
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
  // Current session Plan mode state.
  // 当前会话计划模式状态。
  sessionPlanMode: SessionPlanModeSummary | null;
  // Current session VMM memory policy state.
  // 当前会话 VMM 记忆策略状态。
  sessionVmmPolicy: SessionVmmPolicySummary | null;
  // Output-style catalog and persisted global selection.
  // 输出风格目录与已持久化全局选择。
  outputStyleCatalog: OutputStyleCatalogSummary;
  // Global answer mode a new conversation copies unless its draft chooses one.
  // 新会话在草稿未选择时复制的全局回答模式。
  defaultAnswerMode: AnswerMode;
  // Current persisted output-style state for the selected conversation.
  // 当前选中会话的持久化输出风格状态。
  sessionOutputStyle: SessionOutputStyleSummary | null;
  // Whether the effective project configuration makes VMM available to this composer.
  // 当前项目最终生效配置是否允许输入区使用 VMM。
  vmmAvailable: boolean;
  // Memory readiness and configuration navigation are passed through without changing policy state.
  // 记忆就绪状态与配置导航直接透传，不改变策略状态。
  vmmStatus?: "disabled" | "not_ready" | "ready";
  onOpenMemorySettings?: () => void;
  // Todolist status portal host mounted immediately after the workspace version.
  // 紧跟工作区版本号挂载的 Todo 状态 Portal 宿主。
  todolistStatusHost: HTMLElement | null;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // answerModeState stays bound to the exact hydrated owner across selection transitions.
  // answerModeState 在选择转换期间始终绑定精确已加载归属。
  const answerModeState = useSelectedConversationEventWindowSelector(selectAnswerMode, conversationSelectionEqual);
  const permissionModeState = useSelectedConversationEventWindowSelector(selectPermissionMode, conversationSelectionEqual);
  const visiblePendingPrompts = useVisiblePendingPrompts(props.pendingPrompts);
  const dedicatedMemory = useSelectedConversationEventWindowSelector(selectDedicatedMemory, conversationSelectionEqual);
  // Only the exact selected persisted session may expose its dedicated controls.
  // 只有精确选中的持久会话才能显示其专职控件。
  const persistedDedicated = props.conversationId !== null && dedicatedMemory?.sessionId === props.conversationId
    ? dedicatedMemory
    : null;
  // draftOwner exists only while an unsent draft is selected; its kind and wizard progress are local.
  // draftOwner 仅在选中未发送草稿时存在；其种类与向导进度保存在本地。
  const draftContext = React.useContext(DraftComposerContext);
  const draftOwner = props.conversationId === null ? draftContext : null;
  const [draftState, updateDraftState] = useDedicatedDraft(draftOwner?.draftId ?? null);
  const dedicatedWizardShown = draftOwner !== null && draftState.kind === "dedicated";
  const startShown = props.emptyMode && !dedicatedWizardShown;
  const [dedicatedCreating, setDedicatedCreating] = React.useState(false);
  // dockRef measures only in-flow composer content; portaled and floating status controls stay outside this height.
  // dockRef 只测量布局流内的输入区内容；Portal 状态与浮动控件不参与该高度。
  const dockRef = React.useRef<HTMLDivElement | null>(null);
  const onComposerDockHeightChange = props.onComposerDockHeightChange;
  // visibleSessionTodolist hides progress when no todolist exists or every step is complete.
  // visibleSessionTodolist 在没有 todolist 或全部步骤完成时隐藏进程提示。
  const visibleSessionTodolist = props.sessionTodolist?.items.some((item) => item.status !== "completed")
    ? props.sessionTodolist
    : null;
  React.useLayoutEffect(() => {
    const element = dockRef.current;
    if (!element) {
      return;
    }
    const reportHeight = () => onComposerDockHeightChange(Math.ceil(element.getBoundingClientRect().height));
    reportHeight();
    if (typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(reportHeight);
    observer.observe(element);
    return () => observer.disconnect();
  }, [onComposerDockHeightChange]);

  return (
    <div
      className="vk-dock"
      // The wizard spans the conversation height instead of floating like the composer.
      // 向导占满会话高度，而不像输入区那样悬浮。
      data-mode={dedicatedWizardShown ? "wizard" : props.emptyMode ? "empty" : undefined}
      ref={dockRef}
    >
      {props.showScrollToBottom ? (
        <div className="vk-dock-float">
          <Tooltip>
            <TooltipTrigger asChild>
              <button
                aria-label={t("composer.scrollToBottom")}
                className="vk-jump"
                onClick={props.onScrollToBottom}
                type="button"
              >
                <ArrowDown size={18} />
              </button>
            </TooltipTrigger>
            <TooltipContent side="top">{t("composer.scrollToBottom")}</TooltipContent>
          </Tooltip>
        </div>
      ) : null}
      {startShown ? props.startHero : null}
      {draftOwner ? (
        <DedicatedDraftStrip
          disabled={dedicatedCreating || draftOwner.submissionPending}
          draft={draftOwner}
          onChange={updateDraftState}
          state={draftState}
        />
      ) : null}
      {persistedDedicated ? <DedicatedMemoryLockBar memory={persistedDedicated} running={props.isConversationRunning} /> : null}
      {props.conversationId ? <React.Suspense fallback={null}><LazyFileWindowTaskbar sessionId={props.conversationId} /></React.Suspense> : null}
      {draftOwner && dedicatedWizardShown ? (
        <React.Suspense fallback={null}>
        <LazyDedicatedSessionWizard
          draft={draftOwner}
          onBusyChange={setDedicatedCreating}
          onChange={updateDraftState}
          onSelectWorkbenchModel={props.onSelectWorkbenchModel}
          onSubmitPrompt={props.onSubmitPrompt}
          outputStyleCatalog={props.outputStyleCatalog}
          providers={props.providers}
          state={draftState}
        />
        </React.Suspense>
      ) : (
      <>
      {/* The runtime cue sits directly above the composer stack so its tab grows out of that stack's top edge. */}
      {/* 运行提示紧贴输入区堆叠之上，使其标签从该堆叠的上边缘伸出。 */}
      {props.runtimeStatus ? <div className="vk-dock-status">{props.runtimeStatus}</div> : null}
      <DedicatedComposerContext.Provider value={persistedDedicated}>
      <ComposerPanel
        answerModeState={answerModeState}
        defaultAnswerMode={props.defaultAnswerMode}
        permissionModeState={permissionModeState}
        attachmentCapabilities={props.attachmentCapabilities}
        voiceInput={props.voiceInput ?? null}
        projectId={props.projectId}
        conversationId={props.conversationId}
        workspacePath={props.workspacePath}
        contextUsage={props.contextUsage}
        sessionProviderUsage={props.sessionProviderUsage}
        contextCompactionWindowNodes={props.contextCompactionWindowNodes}
        sessionContextWindowSelection={props.sessionContextWindowSelection}
        customContextWindowMinTokens={props.customContextWindowMinTokens}
        slashCommands={props.slashCommands}
        pendingPrompts={visiblePendingPrompts}
        sessionGoal={props.sessionGoal}
        sessionTodolist={props.sessionTodolist}
        sessionReviewSummary={props.sessionReviewSummary}
        isConversationRunning={props.isConversationRunning}
        thinkingDisplayMode={props.thinkingDisplayMode}
        providers={props.providers}
        onSelectWorkbenchModel={props.onSelectWorkbenchModel}
        onListWorkbenchModelSecondaryRoutes={props.onListWorkbenchModelSecondaryRoutes}
        onSelectWorkbenchModelSecondaryRoute={props.onSelectWorkbenchModelSecondaryRoute}
        onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
        onSlashCommand={props.onSlashCommand}
        onOpenKnowledge={props.onOpenKnowledge}
        onSubmitPrompt={props.onSubmitPrompt}
        onSaveSessionGoal={props.onSaveSessionGoal}
        onResumeSessionGoal={props.onResumeSessionGoal}
        onClearSessionGoal={props.onClearSessionGoal}
        onUpdateSessionGoalStatus={props.onUpdateSessionGoalStatus}
        onSetSessionPlanMode={props.onSetSessionPlanMode}
        onSetSessionVmmPolicy={props.onSetSessionVmmPolicy}
        onSetSessionOutputStyle={props.onSetSessionOutputStyle}
        onSetSessionContextCompactionWindow={props.onSetSessionContextCompactionWindow}
        onStopConversationRun={props.onStopConversationRun}
        onStageAttachmentBytes={props.onStageAttachmentBytes}
        resolveMediaSourceUrl={props.resolveMediaSourceUrl}
        onTrace={props.onTrace}
        onEditPendingPrompt={props.onEditPendingPrompt}
        onInsertPendingPrompt={props.onInsertPendingPrompt}
        onRemovePendingPrompt={props.onRemovePendingPrompt}
        onReorderPendingPrompts={props.onReorderPendingPrompts}
        onThinkingDisplayModeChange={props.onThinkingDisplayModeChange}
        reasoningSelection={props.reasoningSelection}
        sessionPlanMode={props.sessionPlanMode}
        sessionVmmPolicy={props.sessionVmmPolicy}
        outputStyleCatalog={props.outputStyleCatalog}
        sessionOutputStyle={props.sessionOutputStyle}
        vmmAvailable={props.vmmAvailable}
        vmmStatus={props.vmmStatus}
        onOpenMemorySettings={props.onOpenMemorySettings}
      />
      </DedicatedComposerContext.Provider>
      </>
      )}
      {startShown ? props.startFooter : null}
      {props.todolistStatusHost && visibleSessionTodolist
        ? createPortal(
          <ComposerSessionProgressCapsule
            reviewSummary={props.sessionReviewSummary}
            todolist={visibleSessionTodolist}
          />,
          props.todolistStatusHost,
        )
        : null}
    </div>
  );
}
