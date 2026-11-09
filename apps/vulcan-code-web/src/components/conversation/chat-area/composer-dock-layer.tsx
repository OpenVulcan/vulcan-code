import React from "react";
import {
  ComposerDock,
  composerAttachmentMedia,
} from "@/components/composer/composer-dock";
import type { ChatAreaProps } from "@/components/conversation/chat-area/types";
import { ConversationStartFooter, ConversationStartHero } from "@/components/conversation/conversation-start";

type ComposerDockProps = React.ComponentProps<typeof ComposerDock>;
type ThinkingDisplayMode = ComposerDockProps["thinkingDisplayMode"];

// ChatAreaComposerDockLayerProps groups the already-derived ChatArea state required by the floating composer.
// ChatAreaComposerDockLayerProps 汇总浮动输入区所需的 ChatArea 已派生状态。
export interface ChatAreaComposerDockLayerProps {
  // Full ChatArea props; only the fields picked by composerDockInputs are read or compared.
  // 完整 ChatArea props；只读取和比较 composerDockInputs 选出的字段。
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
  // Project named by the start page of an empty conversation.
  // 空会话起始页中显示的项目名称。
  startProjectName: string;
  // Whether the active conversation is currently running.
  // 当前活动会话是否正在运行。
  isConversationRunning: boolean;
  // Whether the transcript is scrolled away from the latest tail.
  // 转录是否已经离开最新尾部。
  showScrollToBottom: boolean;
  // Todolist status portal host mounted beside the workspace version.
  // 挂载在工作区版本号旁的 Todo 状态 Portal 宿主。
  todolistStatusHost: ComposerDockProps["todolistStatusHost"];
  // Latest todolist projected from the selected durable V1 event generation.
  // 从选中可靠 V1 事件代次投影出的最近待办清单。
  sessionTodolist: ComposerDockProps["sessionTodolist"];
  // Current thinking display mode.
  // 当前思考展示模式。
  thinkingDisplayMode: ThinkingDisplayMode;
  // Session review summary visible to the composer.
  // 输入区可见的会话审核摘要。
  sessionReviewSummary: ComposerDockProps["sessionReviewSummary"];
  // Handler for composer dock measured height changes.
  // 输入区测量高度变化处理器。
  onComposerDockHeightChange: ComposerDockProps["onComposerDockHeightChange"];
  // Session runtime cue floated over the composer's top-left edge, or null when no cue is active.
  // 悬浮在输入区左上沿的会话运行提示；没有活动提示时为 null。
  runtimeStatus: ComposerDockProps["runtimeStatus"];
  // Handler that scrolls the transcript to the newest content.
  // 将转录滚动到最新内容的处理器。
  onScrollToBottom: ComposerDockProps["onScrollToBottom"];
  // Handler for thinking display mode changes.
  // 思考展示模式变化处理器。
  onThinkingDisplayModeChange: ComposerDockProps["onThinkingDisplayModeChange"];
  // Submission boundary selected by ChatArea for an ordinary or continued Turn.
  // ChatArea 为普通 Turn 或继续 Turn 选择的提交边界。
  onSubmitPrompt: ComposerDockProps["onSubmitPrompt"];
}

// composerDockInputs picks every ChatArea and Workbench field the composer reads. It is the single list used both to
// render and to decide whether a new Workbench snapshot changes the composer, so a delta that touches only other
// fields (another session's run phase, command sessions, the transcript) does not re-render it.
// composerDockInputs 选出输入区读取的全部 ChatArea 与 Workbench 字段。它是渲染与判断新 Workbench 快照是否改变输入区的
// 唯一清单，因此只涉及其他字段（其他会话的运行阶段、命令会话、转录）的增量不会重新渲染输入区。
// Parameters: props are the layer props; the session list is picked only while the start footer shows it.
// 参数：props 是该层属性；会话列表只在起始页底部显示它时选出。
// Returns: a flat record compared field by field by identity.
// 返回值：按字段引用逐一比较的扁平记录。
function composerDockInputs(props: ChatAreaComposerDockLayerProps) {
  const chatArea = props.chatAreaProps;
  const workbench = chatArea.workbench;
  const draft = chatArea.selectedDraftConversation;
  return {
    draft,
    startFooterConversations: props.isEmptyConversation && draft ? workbench.conversations : null,
    reviewSummary: chatArea.sessionReviewSummary,
    sessionVmmPolicy: workbench.sessionVmmPolicy,
    vmmMemory: workbench.vmmMemory,
    attachmentCapabilities: workbench.composerAttachmentCapabilities,
    voiceInput: workbench.voiceInput,
    projects: workbench.projects,
    slashCommands: workbench.composerSlashCommands,
    contextUsage: workbench.contextUsage,
    sessionProviderUsage: workbench.sessionProviderUsage,
    contextCompactionWindowNodes: workbench.contextCompactionWindowNodes,
    sessionContextWindowSelection: workbench.sessionContextWindowSelection,
    customContextWindowMinTokens: workbench.customContextWindowMinTokens,
    providers: workbench.providers,
    pendingPrompts: workbench.pendingPrompts,
    reasoningSelection: workbench.reasoningSelection,
    sessionGoal: workbench.sessionGoal,
    sessionPlanMode: workbench.sessionPlanMode,
    outputStyleCatalog: workbench.outputStyleCatalog,
    defaultAnswerMode: workbench.defaultAnswerMode,
    sessionOutputStyle: workbench.sessionOutputStyle,
    onOpenKnowledge: chatArea.onOpenKnowledge,
    onOpenMemorySettings: chatArea.onOpenMemorySettings,
    onOpenReviewChanges: chatArea.onOpenReviewChanges,
    onSelectConversation: chatArea.onSelectConversation,
    onSelectWorkbenchModel: chatArea.onSelectWorkbenchModel,
    onListWorkbenchModelSecondaryRoutes: chatArea.onListWorkbenchModelSecondaryRoutes,
    onSelectWorkbenchModelSecondaryRoute: chatArea.onSelectWorkbenchModelSecondaryRoute,
    onSelectWorkbenchReasoningEffort: chatArea.onSelectWorkbenchReasoningEffort,
    onSlashCommand: chatArea.onSlashCommand,
    onSaveSessionGoal: chatArea.onSaveSessionGoal,
    onResumeSessionGoal: chatArea.onResumeSessionGoal,
    onClearSessionGoal: chatArea.onClearSessionGoal,
    onUpdateSessionGoalStatus: chatArea.onUpdateSessionGoalStatus,
    onSetSessionPlanMode: chatArea.onSetSessionPlanMode,
    onSetSessionVmmPolicy: chatArea.onSetSessionVmmPolicy,
    onSetSessionOutputStyle: chatArea.onSetSessionOutputStyle,
    onSetSessionContextCompactionWindow: chatArea.onSetSessionContextCompactionWindow,
    onStopConversationRun: chatArea.onStopConversationRun,
    onStageAttachmentBytes: chatArea.onStageAttachmentBytes,
    resolveMediaSourceUrl: chatArea.resolveMediaSourceUrl,
    onTrace: chatArea.onTrace,
    onEditPendingPrompt: chatArea.onEditPendingPrompt,
    onInsertPendingPrompt: chatArea.onInsertPendingPrompt,
    onRemovePendingPrompt: chatArea.onRemovePendingPrompt,
    onReorderPendingPrompts: chatArea.onReorderPendingPrompts,
  };
}

// sameRecord compares two flat records field by field by identity.
// sameRecord 按引用逐字段比较两个扁平记录。
function sameRecord<T extends object>(left: T, right: T): boolean {
  const keys = Object.keys(left) as (keyof T)[];
  return keys.length === Object.keys(right).length && keys.every((key) => Object.is(left[key], right[key]));
}

// composerDockLayerPropsEqual lets the memo skip a render when neither the layer props nor the picked inputs changed.
// composerDockLayerPropsEqual 在该层属性与选出的输入均未变化时让缓存跳过渲染。
export function composerDockLayerPropsEqual(previous: ChatAreaComposerDockLayerProps, next: ChatAreaComposerDockLayerProps): boolean {
  const { chatAreaProps: _previousChatArea, ...previousLayer } = previous;
  const { chatAreaProps: _nextChatArea, ...nextLayer } = next;
  return sameRecord(previousLayer, nextLayer) && sameRecord(composerDockInputs(previous), composerDockInputs(next));
}

// ChatAreaComposerDockLayerView renders the floating composer using ChatArea's derived state.
// ChatAreaComposerDockLayerView 使用 ChatArea 的已派生状态渲染浮动输入区。
function ChatAreaComposerDockLayerView(props: ChatAreaComposerDockLayerProps) {
  const inputs = composerDockInputs(props);
  const draft = inputs.draft;
  // Resolve visibility from the local session/project; another client may own the shared snapshot selection.
  // 从本地会话/项目解析可见性；共享快照的选择可能由另一客户端持有。
  const memoryEnabled = props.conversationId
    ? inputs.sessionVmmPolicy?.memoryEnabled
    : props.projectId
      ? inputs.vmmMemory.projectScopes.find((project) => project.projectId === props.projectId)?.memoryEnabled
      : inputs.vmmMemory.globalDraft.enabled;
  // Native host readiness is process-wide; external endpoint health is valid only for its projected scope. A scope
  // blocker such as stale vector generations stops memory even while the process is ready.
  // 原生宿主就绪属于进程级；外链端点健康仅对其投影作用域有效。旧向量代次等作用域阻断即使进程就绪也会停止记忆。
  const memoryRuntime = inputs.vmmMemory.runtime;
  const memoryReady = inputs.vmmMemory.globalDraft.backendMode === "native"
    || (inputs.vmmMemory.selectedProjectId === props.projectId && inputs.vmmMemory.selectedConversationId === props.conversationId)
    ? memoryRuntime.processReady && !memoryRuntime.scopeError : undefined;
  return (
    <ComposerDock
      attachmentCapabilities={inputs.attachmentCapabilities ?? []}
      voiceInput={inputs.voiceInput ?? null}
      projectId={props.projectId}
      conversationId={props.conversationId}
      workspacePath={inputs.projects.find((project) => project.id === props.projectId)?.path ?? null}
      slashCommands={inputs.slashCommands ?? []}
      contextUsage={props.isEmptyConversation ? null : inputs.contextUsage ?? null}
      sessionProviderUsage={props.isEmptyConversation ? null : inputs.sessionProviderUsage ?? null}
      contextCompactionWindowNodes={inputs.contextCompactionWindowNodes ?? []}
      sessionContextWindowSelection={
        props.conversationId ? inputs.sessionContextWindowSelection ?? null : null
      }
      customContextWindowMinTokens={inputs.customContextWindowMinTokens ?? null}
      emptyMode={props.isEmptyConversation}
      startHero={props.isEmptyConversation ? (
        <ConversationStartHero
          branchName={inputs.reviewSummary?.branchName ?? null}
          projectName={props.startProjectName}
          showLead={Boolean(draft)}
        />
      ) : null}
      // Only an unsent draft offers ways to begin; a persisted empty session keeps just its heading.
      // 只有未发送草稿提供开始方式；已持久化的空会话只保留标题。
      startFooter={draft && inputs.startFooterConversations ? (
        <ConversationStartFooter
          conversations={inputs.startFooterConversations}
          onOpenKnowledge={inputs.onOpenKnowledge}
          onOpenMemory={inputs.onOpenMemorySettings}
          onOpenReviewChanges={inputs.onOpenReviewChanges}
          onSelectConversation={inputs.onSelectConversation}
          projectId={draft.projectId}
          reviewSummary={inputs.reviewSummary}
        />
      ) : null}
      onScrollToBottom={props.onScrollToBottom}
      onSelectWorkbenchModel={inputs.onSelectWorkbenchModel}
      onListWorkbenchModelSecondaryRoutes={inputs.onListWorkbenchModelSecondaryRoutes}
      onSelectWorkbenchModelSecondaryRoute={inputs.onSelectWorkbenchModelSecondaryRoute}
      onSelectWorkbenchReasoningEffort={inputs.onSelectWorkbenchReasoningEffort}
      onSlashCommand={inputs.onSlashCommand}
      onOpenKnowledge={inputs.onOpenKnowledge}
      onComposerDockHeightChange={props.onComposerDockHeightChange}
      runtimeStatus={props.runtimeStatus}
      onSubmitPrompt={props.onSubmitPrompt}
      onSaveSessionGoal={inputs.onSaveSessionGoal}
      onResumeSessionGoal={inputs.onResumeSessionGoal}
      onClearSessionGoal={inputs.onClearSessionGoal}
      onUpdateSessionGoalStatus={inputs.onUpdateSessionGoalStatus}
      onSetSessionPlanMode={inputs.onSetSessionPlanMode}
      onSetSessionVmmPolicy={inputs.onSetSessionVmmPolicy}
      onSetSessionOutputStyle={inputs.onSetSessionOutputStyle}
      onSetSessionContextCompactionWindow={inputs.onSetSessionContextCompactionWindow}
      onStopConversationRun={inputs.onStopConversationRun}
      onStageAttachmentBytes={inputs.onStageAttachmentBytes}
      resolveMediaSourceUrl={inputs.resolveMediaSourceUrl}
      onTrace={inputs.onTrace}
      providers={inputs.providers}
      pendingPrompts={inputs.pendingPrompts ?? []}
      isConversationRunning={props.isConversationRunning}
      thinkingDisplayMode={props.thinkingDisplayMode}
      reasoningSelection={inputs.reasoningSelection ?? null}
      // Saved settings belong to a durable session even before its first visible message.
      // 已保存设置归属持久会话，在第一条可见消息之前也必须保留。
      sessionGoal={props.conversationId ? inputs.sessionGoal ?? null : null}
      sessionTodolist={props.isEmptyConversation ? null : props.sessionTodolist}
      sessionReviewSummary={props.isEmptyConversation ? null : props.sessionReviewSummary}
      sessionPlanMode={props.conversationId ? inputs.sessionPlanMode ?? null : null}
      sessionVmmPolicy={props.conversationId ? inputs.sessionVmmPolicy ?? null : null}
      outputStyleCatalog={inputs.outputStyleCatalog}
      defaultAnswerMode={inputs.defaultAnswerMode}
      sessionOutputStyle={props.conversationId ? inputs.sessionOutputStyle ?? null : null}
      vmmAvailable={memoryEnabled === true}
      vmmStatus={memoryEnabled === false ? "disabled" : memoryEnabled !== true || memoryReady === undefined ? undefined : memoryReady ? "ready" : "not_ready"}
      onOpenMemorySettings={inputs.onOpenMemorySettings}
      todolistStatusHost={props.todolistStatusHost}
      showScrollToBottom={props.showScrollToBottom}
      onEditPendingPrompt={inputs.onEditPendingPrompt}
      onInsertPendingPrompt={inputs.onInsertPendingPrompt}
      onRemovePendingPrompt={inputs.onRemovePendingPrompt}
      onReorderPendingPrompts={inputs.onReorderPendingPrompts}
      onThinkingDisplayModeChange={props.onThinkingDisplayModeChange}
    />
  );
}

// ChatAreaComposerDockLayer is the memoized floating composer. It re-renders only when a layer prop or one of the
// fields picked by composerDockInputs changes, never for unrelated Workbench deltas.
// ChatAreaComposerDockLayer 是记忆化的浮动输入区。只有该层属性或 composerDockInputs 选出的字段变化时才重新渲染，
// 无关的 Workbench 增量不会触发。
export const ChatAreaComposerDockLayer = React.memo(ChatAreaComposerDockLayerView, composerDockLayerPropsEqual);

export { composerAttachmentMedia };
