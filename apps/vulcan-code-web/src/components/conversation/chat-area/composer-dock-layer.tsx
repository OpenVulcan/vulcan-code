import React from "react";
import {
  ComposerDock,
  composerAttachmentMedia,
} from "@/components/composer/composer-dock";
import type { ChatAreaProps } from "@/components/conversation/chat-area/types";

type ComposerDockProps = React.ComponentProps<typeof ComposerDock>;
type ThinkingDisplayMode = ComposerDockProps["thinkingDisplayMode"];

// ChatAreaComposerDockLayerProps groups the already-derived ChatArea state required by the floating composer.
// ChatAreaComposerDockLayerProps 汇总浮动输入区所需的 ChatArea 已派生状态。
export interface ChatAreaComposerDockLayerProps {
  // Full ChatArea props used to forward stable event handlers and workbench snapshots.
  // 完整 ChatArea props，用于转发稳定事件处理器与 workbench 快照。
  chatAreaProps: ChatAreaProps;
  // Project id currently used for prompt submission.
  // 当前提示词提交使用的项目 id。
  projectId: ComposerDockProps["projectId"];
  // Conversation id currently bound to the composer.
  // 当前输入区绑定的会话 id。
  conversationId: ComposerDockProps["conversationId"];
  // Whether the selected conversation is empty.
  // 当前选中会话是否为空。
  isEmptyConversation: boolean;
  // Whether the active conversation is currently running.
  // 当前活动会话是否正在运行。
  isConversationRunning: boolean;
  // Whether the transcript is scrolled away from the latest tail.
  // 转录是否已经离开最新尾部。
  showScrollToBottom: boolean;
  // Runtime cue shown near the composer.
  // 工作区底部状态栏展示的运行时提示。
  runtimeCue: ComposerDockProps["runtimeCue"];
  // Runtime status portal host mounted in the workspace status bar.
  // 挂载在工作区状态栏中的运行状态 Portal 宿主。
  runtimeStatusHost: ComposerDockProps["runtimeStatusHost"];
  // Todolist status portal host mounted beside the workspace version.
  // 挂载在工作区版本号旁的 Todo 状态 Portal 宿主。
  todolistStatusHost: ComposerDockProps["todolistStatusHost"];
  // Current thinking display mode.
  // 当前思考展示模式。
  thinkingDisplayMode: ThinkingDisplayMode;
  // Session review summary visible to the composer.
  // 输入区可见的会话审核摘要。
  sessionReviewSummary: ComposerDockProps["sessionReviewSummary"];
  // Handler for composer dock measured height changes.
  // 输入区测量高度变化处理器。
  onComposerDockHeightChange: ComposerDockProps["onComposerDockHeightChange"];
  // Handler that scrolls the transcript to the newest content.
  // 将转录滚动到最新内容的处理器。
  onScrollToBottom: ComposerDockProps["onScrollToBottom"];
  // Handler for thinking display mode changes.
  // 思考展示模式变化处理器。
  onThinkingDisplayModeChange: ComposerDockProps["onThinkingDisplayModeChange"];
}

// ChatAreaComposerDockLayer renders the floating composer using ChatArea's derived state.
// ChatAreaComposerDockLayer 使用 ChatArea 的已派生状态渲染浮动输入区。
export function ChatAreaComposerDockLayer(props: ChatAreaComposerDockLayerProps) {
  const chatAreaProps = props.chatAreaProps;
  const workbench = chatAreaProps.workbench;
  return (
    <ComposerDock
      attachmentCapabilities={workbench.composerAttachmentCapabilities ?? []}
      projectId={props.projectId}
      conversationId={props.conversationId}
      slashCommands={workbench.composerSlashCommands ?? []}
      contextUsage={props.isEmptyConversation ? null : workbench.contextUsage ?? null}
      emptyMode={props.isEmptyConversation}
      onScrollToBottom={props.onScrollToBottom}
      onSelectWorkbenchModel={chatAreaProps.onSelectWorkbenchModel}
      onListWorkbenchModelSecondaryRoutes={chatAreaProps.onListWorkbenchModelSecondaryRoutes}
      onSelectWorkbenchModelSecondaryRoute={chatAreaProps.onSelectWorkbenchModelSecondaryRoute}
      onSelectWorkbenchReasoningEffort={chatAreaProps.onSelectWorkbenchReasoningEffort}
      onSlashCommand={chatAreaProps.onSlashCommand}
      onOpenKnowledge={chatAreaProps.onOpenKnowledge}
      onComposerDockHeightChange={props.onComposerDockHeightChange}
      onSubmitPrompt={chatAreaProps.onSubmitPrompt}
      onSaveSessionGoal={chatAreaProps.onSaveSessionGoal}
      onResumeSessionGoal={chatAreaProps.onResumeSessionGoal}
      onClearSessionGoal={chatAreaProps.onClearSessionGoal}
      onUpdateSessionGoalStatus={chatAreaProps.onUpdateSessionGoalStatus}
      onSetSessionPlanMode={chatAreaProps.onSetSessionPlanMode}
      onSetSessionVmmPolicy={chatAreaProps.onSetSessionVmmPolicy}
      onStopConversationRun={chatAreaProps.onStopConversationRun}
      onStageAttachmentBytes={chatAreaProps.onStageAttachmentBytes}
      resolveMediaSourceUrl={chatAreaProps.resolveMediaSourceUrl}
      onTrace={chatAreaProps.onTrace}
      providers={workbench.providers}
      pendingPrompts={workbench.pendingPrompts ?? []}
      isConversationRunning={props.isConversationRunning}
      thinkingDisplayMode={props.thinkingDisplayMode}
      reasoningSelection={workbench.reasoningSelection ?? null}
      sessionGoal={props.isEmptyConversation ? null : workbench.sessionGoal ?? null}
      sessionTodolist={props.isEmptyConversation ? null : workbench.sessionTodolist ?? null}
      sessionReviewSummary={props.isEmptyConversation ? null : props.sessionReviewSummary}
      sessionPlanMode={props.isEmptyConversation ? null : workbench.sessionPlanMode ?? null}
      sessionVmmPolicy={props.isEmptyConversation ? null : workbench.sessionVmmPolicy ?? null}
        vmmAvailable={
          workbench.vmmMemory.effective.enabled
          && workbench.vmmMemory.runtime.processReady
        }
      runtimeCue={props.isEmptyConversation ? null : props.runtimeCue}
      runtimeStatusHost={props.runtimeStatusHost}
      todolistStatusHost={props.todolistStatusHost}
      showScrollToBottom={props.showScrollToBottom}
      onEditPendingPrompt={chatAreaProps.onEditPendingPrompt}
      onInsertPendingPrompt={chatAreaProps.onInsertPendingPrompt}
      onRemovePendingPrompt={chatAreaProps.onRemovePendingPrompt}
      onReorderPendingPrompts={chatAreaProps.onReorderPendingPrompts}
      onThinkingDisplayModeChange={props.onThinkingDisplayModeChange}
    />
  );
}

export { composerAttachmentMedia };
