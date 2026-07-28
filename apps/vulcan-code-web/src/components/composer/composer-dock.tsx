import React from "react";
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
  ComposerRuntimeCue,
  ComposerTraceCallback,
} from "@/components/composer/composer-control-types";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
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

export { composerAttachmentMedia } from "@/components/composer/composer-attachment-media";

// ComposerDock renders composer-related floating controls.
// ComposerDock 渲染输入区相关的浮动控件。
export function ComposerDock(props: {
  // Attachment capability gates used by composer attachment controls.
  // 输入区附件控件使用的附件能力门控。
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
  // Whether the dock is visually centered for an empty conversation.
  // 输入区浮层是否在空会话中居中显示。
  emptyMode: boolean;
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
  // runtimeCue shows the current session in-progress activity near the composer.
  // runtimeCue 在工作区底部状态栏显示当前会话的进行中活动。
  runtimeCue: ComposerRuntimeCue | null;
  // Runtime status portal host mounted in the center of the workspace status bar.
  // 挂载在工作区状态栏中央的运行状态 Portal 宿主。
  runtimeStatusHost: HTMLElement | null;
  // Todolist status portal host mounted immediately after the workspace version.
  // 紧跟工作区版本号挂载的 Todo 状态 Portal 宿主。
  todolistStatusHost: HTMLElement | null;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
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
    <div className={cn("composer-dock", props.emptyMode && "empty-mode")} ref={dockRef}>
      {props.showScrollToBottom ? (
        <div className="composer-floating-status-stack">
          <div className="scroll-to-bottom-layer">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  aria-label={t("composer.scrollToBottom")}
                  className="scroll-to-bottom-button"
                  onClick={props.onScrollToBottom}
                  size="icon"
                  type="button"
                  variant="outline"
                >
                  <ArrowDown size={18} />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="top">{t("composer.scrollToBottom")}</TooltipContent>
            </Tooltip>
          </div>
        </div>
      ) : null}
      <ComposerPanel
        attachmentCapabilities={props.attachmentCapabilities}
        projectId={props.projectId}
        conversationId={props.conversationId}
        contextUsage={props.contextUsage}
        slashCommands={props.slashCommands}
        pendingPrompts={props.pendingPrompts}
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
      />
      {props.todolistStatusHost && visibleSessionTodolist
        ? createPortal(
          <ComposerSessionProgressCapsule
            reviewSummary={props.sessionReviewSummary}
            todolist={visibleSessionTodolist}
          />,
          props.todolistStatusHost,
        )
        : null}
      {props.runtimeStatusHost && props.runtimeCue
        ? createPortal(
          <div className="composer-runtime-cue" aria-live="polite" aria-busy="true">
            <span className="shimmer">{props.runtimeCue.text}</span>
          </div>,
          props.runtimeStatusHost,
        )
        : null}
    </div>
  );
}
