import React from "react";
import {
  ComposerDock,
  composerAttachmentMedia,
} from "@/components/composer/composer-dock";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type {
  ComposerSlashCommandResult,
  ComposerSubmission,
} from "@/components/composer/composer-model";
import {
  ConversationLoadingCenter,
  EmptyConversationPrompt,
} from "@/components/conversation/conversation-chrome";
import type { ConversationTimelineVirtualRestoreState } from "@/components/conversation/conversation-virtualization";
import { ConversationWorkspace } from "@/components/conversation/conversation-workspace";
import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import { StreamingThinkingOverlay } from "@/components/conversation/conversation-blocks";
import {
  activeStreamingThinkingBlock,
  conversationIsRunning,
  conversationRuntimeCue,
  normalizedSessionRunPhase,
} from "@/components/conversation/conversation-runtime";
import {
  generatedMediaAttachmentsFromToolDetailPayload,
  projectConversationRenderItems,
  thinkingOverlayBlockSignature,
} from "@/components/conversation/conversation-transcript";
import {
  buildConversationStreamSignature,
  conversationDistanceFromBottom,
  conversationScrollToDistanceFromBottom,
  conversationViewportAllowsTailFollow,
  type ConversationScrollSnapshot,
} from "@/components/conversation/chat-area-scroll";
import type { ConversationBlock } from "@/components/conversation/conversation-model";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import { cn } from "@/lib/utils";
import { useI18n } from "@/i18n";
import { projectForConversation } from "@/state/workbench-selectors";
import type {
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ConversationMessage,
  PendingPromptSummary,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";

// THINKING_OVERLAY_STORAGE_KEY persists the WebUI-only thinking display preference.
// THINKING_OVERLAY_STORAGE_KEY 持久化仅 WebUI 使用的思考展示偏好。
const THINKING_OVERLAY_STORAGE_KEY = "vulcan-webui-thinking-overlay-enabled";

// ChatAreaProps contains the state and service edges needed by the conversation surface.
// ChatAreaProps 包含会话展示面所需的状态与服务边界。
interface ChatAreaProps {
  // Current startup state used for parity with the shell contract.
  // 当前启动状态，用于保持与外壳契约一致。
  startup: StartupSnapshot;
  // Current workbench snapshot that owns conversations, messages, providers, and composer data.
  // 当前工作台快照，包含会话、消息、供应商与输入区数据。
  workbench: WorkbenchSnapshot;
  // Draft conversation selected before a persisted session exists.
  // 持久会话创建前选中的草稿会话。
  selectedDraftConversation: DraftConversationSelection | null;
  // Selected parent conversation id.
  // 当前选中的父会话标识。
  selectedConversationId: string | null;
  // Conversation id currently being hydrated from the service.
  // 当前正在从服务水合的会话标识。
  loadingConversationId: string | null;
  // Last conversation body loading error.
  // 最近一次会话正文加载错误。
  conversationLoadError: string | null;
  // Selects the active workbench model.
  // 选择当前工作台模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Selects the active reasoning effort.
  // 选择当前推理强度。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // Routes one composer slash command.
  // 路由一个输入区斜杠命令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Submits one composer prompt.
  // 提交一个输入区提示词。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // Stops the currently running conversation.
  // 停止当前运行中的会话。
  onStopConversationRun: () => Promise<boolean>;
  // Opens a routed subagent transcript panel.
  // 打开已路由的子代理转录面板。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Closes the routed subagent panel.
  // 关闭已路由的子代理面板。
  onCloseSubagentPanel: () => Promise<void>;
  // Stops one subagent task.
  // 停止一个子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Edits one queued prompt.
  // 编辑一个队列中的提示词。
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  // Inserts one queued prompt into the composer.
  // 将一个队列提示词插入输入区。
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  // Removes one queued prompt.
  // 移除一个队列提示词。
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  // Reorders queued prompts.
  // 重排队列提示词。
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
  // Stages a browser-provided file through the service.
  // 通过服务暂存浏览器提供的文件。
  onStageAttachmentBytes: (
    projectId: string | null,
    conversationId: string | null,
    file: File,
  ) => Promise<ComposerAttachmentSummary | null>;
  // Copies transcript text through the shell-provided clipboard path.
  // 通过外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void>;
  // Loads hidden tool details for expanded transcript rows.
  // 为展开的转录行加载隐藏工具详情。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Resolves local media paths into renderable URLs.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Emits lightweight click diagnostics.
  // 输出轻量点击诊断信息。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
}

// ChatArea renders the conversation workspace and composer.
// ChatArea 渲染会话工作区与输入区域。
export function ChatArea(props: ChatAreaProps) {
  // t resolves localized runtime labels for conversation overlays and cues.
  // t 解析会话浮层与提示使用的本地化运行文案。
  const { t } = useI18n();
  // workspaceRef stores the scrollable conversation timeline element.
  // workspaceRef 保存可滚动的会话时间线元素。
  const workspaceRef = React.useRef<HTMLElement | null>(null);
  // shouldFollowConversationTailRef remembers the pre-update tail-follow decision across streaming renders.
  // shouldFollowConversationTailRef 在流式渲染之间记住更新前是否应追随尾部。
  const shouldFollowConversationTailRef = React.useRef(true);
  // selectedConversationTailRef tracks conversation switches so opening a session can jump directly to the bottom.
  // selectedConversationTailRef 跟踪会话切换，使打开会话时可直接跳到底部。
  const selectedConversationTailRef = React.useRef<string | null>(null);
  // isAtConversationBottom tracks whether the user is viewing the latest conversation content.
  // isAtConversationBottom 跟踪用户是否正在查看最新的会话内容。
  const [isAtConversationBottom, setIsAtConversationBottom] = React.useState(true);
  // composerDockHeight tracks the actual floating composer stack height for overlay positioning.
  // composerDockHeight 跟踪浮动输入区堆栈真实高度，用于定位浮层。
  const [composerDockHeight, setComposerDockHeight] = React.useState(132);
  // thinkingOverlayEnabled controls whether streaming thinking uses the floating surface or inline transcript.
  // thinkingOverlayEnabled 控制流式思考使用浮层还是会话内联展示。
  const [thinkingOverlayEnabled, setThinkingOverlayEnabled] = React.useState(() => initialThinkingOverlayEnabled());
  const selectedConversation = props.workbench.conversations.find((conversation) => conversation.id === props.selectedConversationId);
  const subagentNavigation = props.workbench.subagentNavigation?.parentConversationId === props.selectedConversationId
    ? props.workbench.subagentNavigation
    : null;
  const activeSurfaceConversationId = subagentNavigation?.activeConversationId ?? props.selectedConversationId;
  // renderedSurfaceConversationIdRef tracks the surface currently represented by the shared scroll element.
  // renderedSurfaceConversationIdRef 跟踪当前共享滚动元素代表的界面。
  const renderedSurfaceConversationIdRef = React.useRef<string | null>(activeSurfaceConversationId ?? null);
  // conversationScrollSnapshotsRef keeps scroll restoration data per parent or child surface.
  // conversationScrollSnapshotsRef 按主会话或子代理界面保存滚动恢复数据。
  const conversationScrollSnapshotsRef = React.useRef<Map<string, ConversationScrollSnapshot>>(new Map());
  // conversationVirtualizerStatesRef keeps measured row heights per parent or child surface.
  // conversationVirtualizerStatesRef 按主会话或子代理界面保存已测量行高。
  const conversationVirtualizerStatesRef = React.useRef<Map<string, ConversationTimelineVirtualRestoreState>>(new Map());
  // scrollRestoreGuardRef prevents transient restore measurements from overwriting tail-pinned memory.
  // scrollRestoreGuardRef 防止恢复过程中的临时测量覆盖贴底记忆。
  const scrollRestoreGuardRef = React.useRef<{ surfaceId: string; wasAtBottom: boolean } | null>(null);
  const activeSubagentTask = subagentNavigation?.tasks?.find((task) => task.isActive) ?? null;
  const surfaceConversation = activeSubagentTask
    ? {
      id: activeSubagentTask.taskId,
      title: activeSubagentTask.label,
      status: activeSubagentTask.isRunning ? "running" : activeSubagentTask.runState,
      runPhase: activeSubagentTask.isRunning ? "requesting" : activeSubagentTask.runState,
      projectId: selectedConversation?.projectId ?? null,
    }
    : selectedConversation;
  const messages = React.useMemo(
    () => props.workbench.messages.filter((message) => message.conversationId === activeSurfaceConversationId),
    [activeSurfaceConversationId, props.workbench.messages],
  );
  // renderItems stores parsed transcript rows for this active surface.
  // renderItems 保存当前活动界面的已解析转录行。
  const renderItems = React.useMemo(() => projectConversationRenderItems(messages), [messages]);
  const isLoading = props.loadingConversationId === props.selectedConversationId;
  // showConversationLoadingOnly is true only when no cached messages can be shown during hydration.
  // showConversationLoadingOnly 仅在水合期间没有可展示缓存消息时为真。
  const showConversationLoadingOnly = isLoading && messages.length === 0 && !props.conversationLoadError;
  const selectedProject = selectedConversation ? projectForConversation(props.workbench, selectedConversation) : null;
  const emptyProjectName = props.selectedDraftConversation?.projectName ?? selectedProject?.name ?? "当前项目";
  const isEmptyConversation = Boolean(
    props.selectedDraftConversation
      || (!activeSubagentTask && selectedConversation && !showConversationLoadingOnly && !props.conversationLoadError && messages.length === 0),
  );
  // emptyPromptVisible keeps the empty prompt mounted long enough to fade out after first submit.
  // emptyPromptVisible 让空会话提示在首次提交后保留足够时间完成淡出。
  const [emptyPromptVisible, setEmptyPromptVisible] = React.useState(isEmptyConversation);
  // emptyPromptLeaving marks the empty prompt exit animation.
  // emptyPromptLeaving 标记空会话提示退出动画。
  const [emptyPromptLeaving, setEmptyPromptLeaving] = React.useState(false);
  const streamingThinkingBlock = activeStreamingThinkingBlock(surfaceConversation, renderItems);
  const displayedStreamingThinkingBlock = thinkingOverlayEnabled ? streamingThinkingBlock : null;
  // streamingThinkingOverlayBlock keeps the last thinking popup mounted during its delayed exit.
  // streamingThinkingOverlayBlock 在思考弹窗延迟退场期间保留最后一个思考块。
  const [streamingThinkingOverlayBlock, setStreamingThinkingOverlayBlock] = React.useState<ConversationBlock | null>(null);
  // streamingThinkingOverlayLeaving marks the fade-out half of the delayed thinking popup exit.
  // streamingThinkingOverlayLeaving 标记思考弹窗延迟退场中的淡出阶段。
  const [streamingThinkingOverlayLeaving, setStreamingThinkingOverlayLeaving] = React.useState(false);
  const visibleStreamingThinkingOverlayBlock = displayedStreamingThinkingBlock ?? streamingThinkingOverlayBlock;
  const displayedStreamingThinkingOverlaySignature = displayedStreamingThinkingBlock
    ? thinkingOverlayBlockSignature(displayedStreamingThinkingBlock)
    : null;
  const retainedStreamingThinkingOverlayId = streamingThinkingOverlayBlock?.id ?? null;
  // hasRunningSubagents reflects the backend-routed subagent lane rather than local UI inference.
  // hasRunningSubagents 反映后端路由的子代理通道，而不是本地界面推断。
  const hasRunningSubagents = subagentNavigation?.tasks?.some((task) => task.isRunning) ?? false;
  // suppressSubagentOnlyMainCue keeps delegated work in the footer instead of the main request cue.
  // suppressSubagentOnlyMainCue 将纯子代理工作保留在底部栏，不显示为主窗口请求提示。
  const suppressSubagentOnlyMainCue = hasRunningSubagents
    && (Boolean(activeSubagentTask?.isRunning)
      || normalizedSessionRunPhase(surfaceConversation?.runPhase) === "toolrunning");
  const runtimeCue = visibleStreamingThinkingOverlayBlock || suppressSubagentOnlyMainCue
    ? null
    : conversationRuntimeCue(surfaceConversation, renderItems, t);
  const isConversationRunning = conversationIsRunning(selectedConversation) || hasRunningSubagents;
  // conversationStreamSignature changes when streamed content grows inside existing transcript rows.
  // conversationStreamSignature 在现有转录行内部的流式内容增长时变化。
  const conversationStreamSignature = React.useMemo(
    () => buildConversationStreamSignature(messages),
    [messages],
  );
  // rememberConversationScrollSnapshot captures the current shared scroll element before route changes.
  // rememberConversationScrollSnapshot 在路由切换前捕获当前共享滚动元素状态。
  const rememberConversationScrollSnapshot = React.useCallback(() => {
    const element = workspaceRef.current;
    const surfaceId = element?.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
    if (!element || !surfaceId) {
      return;
    }
    const wasAtBottom = conversationViewportAllowsTailFollow(element);
    conversationScrollSnapshotsRef.current.set(surfaceId, {
      distanceFromBottom: conversationDistanceFromBottom(element),
      wasAtBottom,
    });
    shouldFollowConversationTailRef.current = wasAtBottom;
  }, [activeSurfaceConversationId]);

  // updateConversationBottomState refreshes the bottom affordance from the timeline scroll position.
  // updateConversationBottomState 根据时间线滚动位置刷新底部跳转按钮状态。
  const updateConversationBottomState = React.useCallback(() => {
    const element = workspaceRef.current;
    if (!element) {
      shouldFollowConversationTailRef.current = true;
      setIsAtConversationBottom(true);
      return;
    }
    const shouldFollow = conversationViewportAllowsTailFollow(element);
    const surfaceId = element.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
    const restoreGuard = scrollRestoreGuardRef.current;
    if (surfaceId && restoreGuard?.surfaceId === surfaceId && restoreGuard.wasAtBottom && !shouldFollow) {
      shouldFollowConversationTailRef.current = true;
      conversationScrollSnapshotsRef.current.set(surfaceId, {
        distanceFromBottom: 0,
        wasAtBottom: true,
      });
      setIsAtConversationBottom(true);
      return;
    }
    shouldFollowConversationTailRef.current = shouldFollow;
    if (surfaceId) {
      conversationScrollSnapshotsRef.current.set(surfaceId, {
        distanceFromBottom: conversationDistanceFromBottom(element),
        wasAtBottom: shouldFollow,
      });
    }
    setIsAtConversationBottom(shouldFollow);
  }, [activeSurfaceConversationId]);

  // handleOpenSubagentPanelFromTimeline preserves the current scroll before opening a child transcript.
  // handleOpenSubagentPanelFromTimeline 在打开子代理转录前保留当前滚动位置。
  const handleOpenSubagentPanelFromTimeline = React.useCallback((
    parentConversationId: string,
    taskId: string,
  ) => {
    rememberConversationScrollSnapshot();
    return props.onOpenSubagentPanel(parentConversationId, taskId);
  }, [props.onOpenSubagentPanel, rememberConversationScrollSnapshot]);

  // scrollConversationToBottom moves the timeline to the latest content.
  // scrollConversationToBottom 将时间线滚动到最新内容。
  const scrollConversationToBottom = React.useCallback((behavior: ScrollBehavior = "smooth") => {
    const element = workspaceRef.current;
    if (!element) {
      return;
    }
    const safeBehavior = behavior === "auto" || behavior === "instant" || behavior === "smooth" ? behavior : "smooth";
    element.scrollTo({ top: element.scrollHeight, behavior: safeBehavior });
  }, []);

  // jumpConversationToBottom immediately pins the timeline to its latest rendered height.
  // jumpConversationToBottom 立即把时间线固定到最新已渲染高度。
  const jumpConversationToBottom = React.useCallback(() => {
    const element = workspaceRef.current;
    if (!element) {
      return false;
    }
    element.scrollTop = element.scrollHeight;
    return true;
  }, []);

  // handleComposerDockHeightChange updates overlay spacing without mutating transcript scroll.
  // handleComposerDockHeightChange 更新浮层间距但不修改转录滚动位置。
  const handleComposerDockHeightChange = React.useCallback((height: number) => {
    setComposerDockHeight((current) => {
      if (current === height) {
        return current;
      }
      return height;
    });
  }, []);

  // handleTimelineVirtualizerStateChange remembers measured row heights for the active surface.
  // handleTimelineVirtualizerStateChange 记住当前界面的已测量行高。
  const handleTimelineVirtualizerStateChange = React.useCallback((surfaceId: string, state: ConversationTimelineVirtualRestoreState | null) => {
    if (!surfaceId) {
      return;
    }
    if (!state || Object.keys(state.rowHeightsByKey).length === 0) {
      conversationVirtualizerStatesRef.current.delete(surfaceId);
      return;
    }
    conversationVirtualizerStatesRef.current.set(surfaceId, state);
  }, []);

  React.useLayoutEffect(() => {
    if (!props.selectedConversationId || isEmptyConversation || showConversationLoadingOnly) {
      return;
    }

    const element = workspaceRef.current;
    const previousSurfaceId = selectedConversationTailRef.current;
    const selectedChanged = previousSurfaceId !== activeSurfaceConversationId;
    if (
      selectedChanged
      && previousSurfaceId
      && element
      && !conversationScrollSnapshotsRef.current.has(previousSurfaceId)
      && shouldFollowConversationTailRef.current
    ) {
      conversationScrollSnapshotsRef.current.set(previousSurfaceId, {
        distanceFromBottom: 0,
        wasAtBottom: true,
      });
    }
    selectedConversationTailRef.current = activeSurfaceConversationId;
    renderedSurfaceConversationIdRef.current = activeSurfaceConversationId ?? null;
    const savedScrollSnapshot = selectedChanged && activeSurfaceConversationId
      ? conversationScrollSnapshotsRef.current.get(activeSurfaceConversationId)
      : null;
    const restoreSurfaceId = activeSurfaceConversationId;
    if (selectedChanged && savedScrollSnapshot && element && restoreSurfaceId) {
      const restoreSavedScrollPosition = () => {
        if (savedScrollSnapshot.wasAtBottom) {
          element.scrollTop = element.scrollHeight;
          return;
        }
        conversationScrollToDistanceFromBottom(element, savedScrollSnapshot.distanceFromBottom);
      };
      shouldFollowConversationTailRef.current = savedScrollSnapshot.wasAtBottom;
      scrollRestoreGuardRef.current = {
        surfaceId: restoreSurfaceId,
        wasAtBottom: savedScrollSnapshot.wasAtBottom,
      };
      restoreSavedScrollPosition();
      let secondFrame: number | null = null;
      const firstFrame = window.requestAnimationFrame(() => {
        restoreSavedScrollPosition();
        secondFrame = window.requestAnimationFrame(() => {
          restoreSavedScrollPosition();
          updateConversationBottomState();
          if (scrollRestoreGuardRef.current?.surfaceId === restoreSurfaceId) {
            scrollRestoreGuardRef.current = null;
          }
        });
      });
      return () => {
        window.cancelAnimationFrame(firstFrame);
        if (secondFrame !== null) {
          window.cancelAnimationFrame(secondFrame);
        }
        if (scrollRestoreGuardRef.current?.surfaceId === restoreSurfaceId) {
          scrollRestoreGuardRef.current = null;
        }
      };
    }

    const shouldFollowTail = selectedChanged
      || !element
      || shouldFollowConversationTailRef.current
      || conversationViewportAllowsTailFollow(element);
    if (!shouldFollowTail) {
      updateConversationBottomState();
      return;
    }

    shouldFollowConversationTailRef.current = true;
    let secondFrame: number | null = null;
    let thirdFrame: number | null = null;
    jumpConversationToBottom();
    const firstFrame = window.requestAnimationFrame(() => {
      jumpConversationToBottom();
      secondFrame = window.requestAnimationFrame(() => {
        jumpConversationToBottom();
        thirdFrame = window.requestAnimationFrame(() => {
          jumpConversationToBottom();
          updateConversationBottomState();
        });
        updateConversationBottomState();
      });
    });

    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame !== null) {
        window.cancelAnimationFrame(secondFrame);
      }
      if (thirdFrame !== null) {
        window.cancelAnimationFrame(thirdFrame);
      }
    };
  }, [
    conversationStreamSignature,
    isEmptyConversation,
    jumpConversationToBottom,
    activeSurfaceConversationId,
    showConversationLoadingOnly,
    updateConversationBottomState,
    props.selectedConversationId,
  ]);

  React.useEffect(() => {
    updateConversationBottomState();
  }, [activeSurfaceConversationId, conversationStreamSignature, updateConversationBottomState]);

  React.useEffect(() => {
    window.localStorage.setItem(THINKING_OVERLAY_STORAGE_KEY, thinkingOverlayEnabled ? "true" : "false");
  }, [thinkingOverlayEnabled]);

  React.useEffect(() => {
    if (displayedStreamingThinkingBlock) {
      setStreamingThinkingOverlayBlock((current) => (
        current && thinkingOverlayBlockSignature(current) === displayedStreamingThinkingOverlaySignature
          ? current
          : displayedStreamingThinkingBlock
      ));
      setStreamingThinkingOverlayLeaving(false);
      return;
    }
    if (!streamingThinkingOverlayBlock) {
      return;
    }
    const fadeTimer = window.setTimeout(() => {
      setStreamingThinkingOverlayLeaving(true);
    }, 500);
    const removeTimer = window.setTimeout(() => {
      setStreamingThinkingOverlayBlock(null);
      setStreamingThinkingOverlayLeaving(false);
    }, 1000);
    return () => {
      window.clearTimeout(fadeTimer);
      window.clearTimeout(removeTimer);
    };
  }, [displayedStreamingThinkingBlock, displayedStreamingThinkingOverlaySignature, retainedStreamingThinkingOverlayId, streamingThinkingOverlayBlock]);

  React.useEffect(() => {
    if (isEmptyConversation) {
      setEmptyPromptVisible(true);
      setEmptyPromptLeaving(false);
      return;
    }
    if (!emptyPromptVisible) {
      return;
    }
    setEmptyPromptLeaving(true);
    const timeout = window.setTimeout(() => {
      setEmptyPromptVisible(false);
      setEmptyPromptLeaving(false);
    }, 360);
    return () => window.clearTimeout(timeout);
  }, [emptyPromptVisible, isEmptyConversation]);

  // composerDock stays as a floating layer outside the transcript scroll flow.
  // composerDock 作为转录滚动流之外的浮动层保持挂载。
  const composerDock = (
    <ComposerDock
      aiCapabilities={props.workbench.aiCapabilities ?? []}
      attachmentCapabilities={props.workbench.composerAttachmentCapabilities ?? []}
      projectId={selectedProject?.id ?? props.selectedDraftConversation?.projectId ?? null}
      conversationId={selectedConversation?.id ?? null}
      slashCommands={props.workbench.composerSlashCommands ?? []}
      contextUsage={isEmptyConversation ? null : props.workbench.contextUsage ?? null}
      emptyMode={isEmptyConversation}
      onScrollToBottom={() => scrollConversationToBottom()}
      onSelectWorkbenchModel={props.onSelectWorkbenchModel}
      onSelectWorkbenchReasoningEffort={props.onSelectWorkbenchReasoningEffort}
      onSlashCommand={props.onSlashCommand}
      onComposerDockHeightChange={handleComposerDockHeightChange}
      onSubmitPrompt={props.onSubmitPrompt}
      onStopConversationRun={props.onStopConversationRun}
      onStageAttachmentBytes={props.onStageAttachmentBytes}
      resolveMediaSourceUrl={props.resolveMediaSourceUrl}
      onTrace={props.onTrace}
      providers={props.workbench.providers}
      pendingPrompts={props.workbench.pendingPrompts ?? []}
      isConversationRunning={isConversationRunning}
      thinkingOverlayEnabled={thinkingOverlayEnabled}
      reasoningSelection={props.workbench.reasoningSelection ?? null}
      runtimeCue={isEmptyConversation ? null : runtimeCue}
      showScrollToBottom={!isEmptyConversation && !isAtConversationBottom}
      onEditPendingPrompt={props.onEditPendingPrompt}
      onInsertPendingPrompt={props.onInsertPendingPrompt}
      onRemovePendingPrompt={props.onRemovePendingPrompt}
      onReorderPendingPrompts={props.onReorderPendingPrompts}
      onThinkingOverlayEnabledChange={setThinkingOverlayEnabled}
    />
  );

  return (
    <div
      className={cn("area-grid chat-grid", isEmptyConversation && "empty-conversation-grid", showConversationLoadingOnly && "loading-conversation-grid")}
      style={{ "--composer-dock-height": `${composerDockHeight}px` } as React.CSSProperties}
    >
      {showConversationLoadingOnly ? (
        <ConversationLoadingCenter />
      ) : (
        <>
          {emptyPromptVisible ? (
            <EmptyConversationPrompt
              leaving={emptyPromptLeaving}
              projectName={emptyProjectName}
            />
          ) : null}
          <ConversationWorkspace
            conversation={surfaceConversation}
            sessionRunning={conversationIsRunning(surfaceConversation)}
            hiddenThinkingBlockId={displayedStreamingThinkingBlock?.id ?? null}
            messages={messages}
            renderItems={renderItems}
            error={props.conversationLoadError}
            parentConversationId={activeSurfaceConversationId ?? null}
            workspacePath={selectedProject?.path ?? null}
            onScrollStateChange={updateConversationBottomState}
            onOpenSubagentPanel={handleOpenSubagentPanelFromTimeline}
            scrollRef={workspaceRef}
            initialScrollDistanceFromBottom={activeSurfaceConversationId
              ? conversationScrollSnapshotsRef.current.get(activeSurfaceConversationId)?.distanceFromBottom ?? 0
              : 0}
            initialVirtualizerState={activeSurfaceConversationId
              ? conversationVirtualizerStatesRef.current.get(activeSurfaceConversationId) ?? null
              : null}
            onVirtualizerStateChange={handleTimelineVirtualizerStateChange}
            footer={null}
            suppressEmptyPlaceholder={isEmptyConversation}
            onCopyText={props.onCopyText}
            onLoadToolDetail={props.onLoadToolDetail}
            onExtractGeneratedMediaAttachments={generatedMediaAttachmentsFromToolDetailPayload}
            resolveMediaSourceUrl={props.resolveMediaSourceUrl}
            resolveComposerAttachmentMedia={composerAttachmentMedia}
            onTrace={props.onTrace}
          />
          {visibleStreamingThinkingOverlayBlock ? (
            <StreamingThinkingOverlay
              block={visibleStreamingThinkingOverlayBlock}
              leaving={streamingThinkingOverlayLeaving}
            />
          ) : null}
          {composerDock}
        </>
      )}
    </div>
  );
}

// initialThinkingOverlayEnabled returns the persisted thinking display preference.
// initialThinkingOverlayEnabled 返回持久化的思考展示偏好。
function initialThinkingOverlayEnabled(): boolean {
  return window.localStorage.getItem(THINKING_OVERLAY_STORAGE_KEY) !== "false";
}
