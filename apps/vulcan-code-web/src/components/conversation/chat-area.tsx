import React from "react";
import {
  ChatAreaComposerDockLayer,
  composerAttachmentMedia,
} from "@/components/conversation/chat-area/composer-dock-layer";
import { UserPromptEditDialog } from "@/components/conversation/chat-area/user-prompt-edit-dialog";
import {
  ConversationLoadingCenter,
  EmptyConversationPrompt,
} from "@/components/conversation/conversation-chrome";
import type {
  ConversationTimelineUserScrollDirection,
  ConversationTimelineVirtualRestoreState,
} from "@/components/conversation/conversation-virtualization";
import { ConversationWorkspace } from "@/components/conversation/conversation-workspace";
import { StreamingThinkingOverlay } from "@/components/conversation/conversation-blocks";
import {
  activeStreamingThinkingBlock,
  conversationIsRunning,
  conversationRuntimeCue,
  normalizedSessionRunPhase,
} from "@/components/conversation/conversation-runtime";
import {
  activeThinkingOverlayBlock,
  hiddenThinkingBlockIdsForDisplayMode,
  normalizeThinkingDisplayMode,
  type ThinkingDisplayMode,
} from "@/components/conversation/thinking-display";
import {
  generatedMediaAttachmentsFromToolDetailPayload,
  projectConversationRenderItems,
  thinkingOverlayBlockSignature,
} from "@/components/conversation/conversation-transcript";
import {
  buildConversationStreamSignature,
  captureConversationViewportAnchor,
  conversationDistanceFromBottom,
  conversationScrollToDistanceFromBottom,
  conversationViewportAllowsTailFollow,
  restoreConversationViewportAnchor,
  type ConversationScrollSnapshot,
  type ConversationViewportAnchorSnapshot,
} from "@/components/conversation/chat-area-scroll";
import {
  CONVERSATION_LAYOUT_WILL_CHANGE_EVENT,
} from "@/components/conversation/conversation-layout-events";
import {
  createConversationSurfaceLayoutRestoreState,
  type ConversationSurfaceExpandedState,
  type ConversationSurfaceLayoutRestoreState,
} from "@/components/conversation/conversation-layout-restore";
import type {
  ConversationBlock,
  ConversationPlanArtifact,
} from "@/components/conversation/conversation-model";
import { cn } from "@/lib/utils";
import { useI18n } from "@/i18n";
import { projectForConversation } from "@/state/workbench-selectors";
import { toast } from "sonner";
import type {
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
} from "@/types";
import type { ChatAreaProps } from "@/components/conversation/chat-area/types";
import { useChatAreaPlanActions } from "@/components/conversation/chat-area/plan-actions";
import { useUserPromptEditState } from "@/components/conversation/chat-area/user-prompt-edit-state";

// CONVERSATION_UPWARD_SCROLL_TAIL_SUPPRESS_MS prevents near-bottom auto-follow from fighting slow upward scrolling.
// CONVERSATION_UPWARD_SCROLL_TAIL_SUPPRESS_MS 防止近底部自动跟随抢夺慢速向上滚动。
const CONVERSATION_UPWARD_SCROLL_TAIL_SUPPRESS_MS = 900;

// CONVERSATION_USER_SCROLL_SETTLE_MS lets native wheel scrolling settle before programmatic tail pinning resumes.
// CONVERSATION_USER_SCROLL_SETTLE_MS 让原生滚轮滚动稳定后再恢复程序化尾部锁定。
const CONVERSATION_USER_SCROLL_SETTLE_MS = 260;

// ChatArea renders the conversation workspace and composer.
// ChatArea 渲染会话工作区与输入区域。
export function ChatArea(props: ChatAreaProps) {
  // t and locale resolve localized runtime labels and user-visible Plan action prompts.
  // t 与 locale 解析运行文案和用户可见计划操作提示词。
  const { t, locale } = useI18n();
  // workspaceRef stores the scrollable conversation timeline element.
  // workspaceRef 保存可滚动的会话时间线元素。
  const workspaceRef = React.useRef<HTMLElement | null>(null);
  // shouldFollowConversationTailRef remembers the pre-update tail-follow decision across streaming renders.
  // shouldFollowConversationTailRef 在流式渲染之间记住更新前是否应追随尾部。
  const shouldFollowConversationTailRef = React.useRef(true);
  // upwardScrollTailSuppressUntilRef records when user upward scroll should override near-bottom tail following.
  // upwardScrollTailSuppressUntilRef 记录用户向上滚动应覆盖近底部尾随的截止时间。
  const upwardScrollTailSuppressUntilRef = React.useRef(0);
  // userScrollSettleUntilRef records a short interval where native user scrolling owns scrollTop.
  // userScrollSettleUntilRef 记录原生用户滚动短暂接管 scrollTop 的时间窗口。
  const userScrollSettleUntilRef = React.useRef(0);
  // selectedConversationTailRef tracks conversation switches so opening a session can jump directly to the bottom.
  // selectedConversationTailRef 跟踪会话切换，使打开会话时可直接跳到底部。
  const selectedConversationTailRef = React.useRef<string | null>(null);
  // isAtConversationBottom tracks whether the user is viewing the latest conversation content.
  // isAtConversationBottom 跟踪用户是否正在查看最新的会话内容。
  const [isAtConversationBottom, setIsAtConversationBottom] = React.useState(true);
  // composerDockHeight tracks the actual floating composer stack height for overlay positioning.
  // composerDockHeight 跟踪浮动输入区堆栈真实高度，用于定位浮层。
  const [composerDockHeight, setComposerDockHeight] = React.useState(132);
  const userPromptEdit = useUserPromptEditState({
    onGetUserPromptEditDraft: props.onGetUserPromptEditDraft,
    onSubmitUserPromptEdit: props.onSubmitUserPromptEdit,
  });
  // thinkingDisplayMode controls how thinking blocks flow between the popup and transcript.
  // thinkingDisplayMode 控制思考块在弹窗与会话面板之间的展示方式。
  const [thinkingDisplayMode, setThinkingDisplayMode] = React.useState<ThinkingDisplayMode>(() => (
    normalizeThinkingDisplayMode(props.workbench.thinkingDisplayMode)
  ));
  const selectedConversation = props.workbench.conversations.find((conversation) => conversation.id === props.selectedConversationId);
  const subagentNavigation = props.workbench.subagentNavigation?.parentConversationId === props.selectedConversationId
    ? props.workbench.subagentNavigation
    : null;
  // handleForkSourceConversationSelect opens the original source only when it is still active.
  // handleForkSourceConversationSelect 仅在原始来源仍处于活动会话时打开它。
  const handleForkSourceConversationSelect = React.useCallback((conversationId: string) => {
    const activeSource = props.workbench.conversations.some((conversation) => conversation.id === conversationId);
    const archivedSource = (props.workbench.archivedConversations ?? []).some((conversation) => conversation.id === conversationId);
    if (!activeSource || archivedSource) {
      toast.info(t("conversation.forkSource.missing"));
      return;
    }
    props.onSelectConversation(conversationId);
  }, [props.onSelectConversation, props.workbench.archivedConversations, props.workbench.conversations, t]);
  const activeSurfaceConversationId = props.selectedConversationId;
  // renderedSurfaceConversationIdRef tracks the surface currently represented by the shared scroll element.
  // renderedSurfaceConversationIdRef 跟踪当前共享滚动元素代表的界面。
  const renderedSurfaceConversationIdRef = React.useRef<string | null>(activeSurfaceConversationId ?? null);
  // conversationSurfaceLayoutStatesRef keeps scroll, virtualizer, and expansion restore data per surface.
  // conversationSurfaceLayoutStatesRef 按界面保存滚动、虚拟列表与展开恢复数据。
  const conversationSurfaceLayoutStatesRef = React.useRef<Map<string, ConversationSurfaceLayoutRestoreState>>(new Map());
  // pendingLayoutViewportRef stores the viewport anchor captured before shell geometry changes.
  // pendingLayoutViewportRef 保存外壳几何变化前捕获的视口锚点。
  const pendingLayoutViewportRef = React.useRef<{
    anchor: ConversationViewportAnchorSnapshot | null;
    distanceFromBottom: number;
    surfaceId: string;
    wasAtBottom: boolean;
  } | null>(null);
  // hiddenThinkingLayoutSnapshotRef stores the viewport before thinking display filters change rows.
  // hiddenThinkingLayoutSnapshotRef 保存思考展示过滤改变行集合前的视口状态。
  const hiddenThinkingLayoutSnapshotRef = React.useRef<{
    // Stable hidden-thinking signature captured with this viewport snapshot.
    // 与该视口快照绑定的稳定思考隐藏签名。
    hiddenThinkingSignature: string;
    // Anchor row visible before the display filter changed the virtualized row set.
    // 展示过滤改变虚拟行集合前可见的锚点行。
    anchor: ConversationViewportAnchorSnapshot | null;
    // Bottom distance used when an anchor is unavailable.
    // 锚点不可用时用于恢复的底部距离。
    distanceFromBottom: number;
    // Surface id that owns the captured viewport.
    // 拥有该视口快照的界面标识。
    surfaceId: string;
    // Whether the transcript should keep following the latest content.
    // 转录是否应继续跟随最新内容。
    wasAtBottom: boolean;
  } | null>(null);
  // layoutRestoreFramesRef stores scheduled viewport correction frames.
  // layoutRestoreFramesRef 保存已排期的视口校正帧。
  const layoutRestoreFramesRef = React.useRef<number[]>([]);
  // scrollRestoreGuardRef prevents transient restore measurements from overwriting tail-pinned memory.
  // scrollRestoreGuardRef 防止恢复过程中的临时测量覆盖贴底记忆。
  const scrollRestoreGuardRef = React.useRef<{ surfaceId: string; wasAtBottom: boolean } | null>(null);
  const surfaceConversation = selectedConversation;
  const messages = React.useMemo(
    () => props.workbench.messages.filter((message) => message.conversationId === activeSurfaceConversationId),
    [activeSurfaceConversationId, props.workbench.messages],
  );
  // visibleCommandSessions limits transcript tool cards to this conversation surface's runtime resources.
  // visibleCommandSessions 将转录工具卡片限定到当前会话界面的运行时资源。
  const visibleCommandSessions = React.useMemo(
    () => activeSurfaceConversationId
      ? (props.workbench.commandSessions ?? []).filter((session) => session.ownerSessionId === activeSurfaceConversationId)
      : [],
    [activeSurfaceConversationId, props.workbench.commandSessions],
  );
  // renderItems stores parsed transcript rows for this active surface.
  // renderItems 保存当前活动界面的已解析转录行。
  const renderItems = React.useMemo(() => projectConversationRenderItems(messages, t), [messages, t]);
  const activeConversationHydration = props.conversationHydration?.sessionId === activeSurfaceConversationId
    ? props.conversationHydration
    : null;
  const isLoadingByRequest = props.loadingConversationId === activeSurfaceConversationId;
  const isLoading = isLoadingByRequest
    || activeConversationHydration?.status === "loading";
  // displayedConversationHydration avoids rendering stale ready progress while a fresh request is active.
  // displayedConversationHydration 避免在新请求进行中渲染旧的完成态进度。
  const displayedConversationHydration = isLoadingByRequest && activeConversationHydration?.status !== "loading"
    ? null
    : activeConversationHydration;
  // showConversationLoadingOnly is true only when no cached messages can be shown during hydration.
  // showConversationLoadingOnly 仅在水合期间没有可展示缓存消息时为真。
  const showConversationLoadingOnly = isLoading && messages.length === 0 && !props.conversationLoadError;
  const selectedProject = selectedConversation ? projectForConversation(props.workbench, selectedConversation) : null;
  const emptyProjectName = props.selectedDraftConversation?.projectName ?? selectedProject?.name ?? t("conversation.unlinkedProject");
  const isEmptyConversation = Boolean(
    props.selectedDraftConversation
      || (selectedConversation && !showConversationLoadingOnly && !props.conversationLoadError && messages.length === 0),
  );
  // emptyPromptVisible keeps the empty prompt mounted long enough to fade out after first submit.
  // emptyPromptVisible 让空会话提示在首次提交后保留足够时间完成淡出。
  const [emptyPromptVisible, setEmptyPromptVisible] = React.useState(isEmptyConversation);
  // emptyPromptLeaving marks the empty prompt exit animation.
  // emptyPromptLeaving 标记空会话提示退出动画。
  const [emptyPromptLeaving, setEmptyPromptLeaving] = React.useState(false);
  const streamingThinkingBlock = activeStreamingThinkingBlock(surfaceConversation, renderItems);
  const displayedStreamingThinkingBlock = activeThinkingOverlayBlock(thinkingDisplayMode, streamingThinkingBlock);
  // streamingThinkingOverlayBlock keeps the last thinking popup mounted during its delayed exit.
  // streamingThinkingOverlayBlock 在思考弹窗延迟退场期间保留最后一个思考块。
  const [streamingThinkingOverlayBlock, setStreamingThinkingOverlayBlock] = React.useState<ConversationBlock | null>(null);
  // streamingThinkingOverlayLeaving marks the fade-out half of the delayed thinking popup exit.
  // streamingThinkingOverlayLeaving 标记思考弹窗延迟退场中的淡出阶段。
  const [streamingThinkingOverlayLeaving, setStreamingThinkingOverlayLeaving] = React.useState(false);
  const visibleStreamingThinkingOverlayBlock = displayedStreamingThinkingBlock ?? streamingThinkingOverlayBlock;
  // hiddenThinkingBlockIds contains thinking blocks removed from the virtualized transcript projection.
  // hiddenThinkingBlockIds 包含从虚拟化会话投影中移除的思考块。
  const hiddenThinkingBlockIds = React.useMemo(
    () => hiddenThinkingBlockIdsForDisplayMode(
      thinkingDisplayMode,
      renderItems,
      visibleStreamingThinkingOverlayBlock?.id ?? null,
    ),
    [thinkingDisplayMode, renderItems, visibleStreamingThinkingOverlayBlock?.id],
  );
  // hiddenThinkingSignature is a stable change marker for restoring scroll after filter changes.
  // hiddenThinkingSignature 是过滤变化后恢复滚动位置的稳定变更标记。
  const hiddenThinkingSignature = React.useMemo(
    () => Array.from(hiddenThinkingBlockIds).sort().join("\u0000"),
    [hiddenThinkingBlockIds],
  );
  const displayedStreamingThinkingOverlaySignature = displayedStreamingThinkingBlock
    ? thinkingOverlayBlockSignature(displayedStreamingThinkingBlock)
    : null;
  const retainedStreamingThinkingOverlayId = streamingThinkingOverlayBlock?.id ?? null;
  // hasRunningSubagents reflects the backend-routed subagent lane rather than local UI inference.
  // hasRunningSubagents 反映后端路由的子代理通道，而不是本地界面推断。
  const hasRunningSubagents = subagentNavigation?.tasks?.some((task) => task.isRunning || task.runState === "running") ?? false;
  // suppressSubagentOnlyMainCue keeps delegated work in the footer instead of the main request cue.
  // suppressSubagentOnlyMainCue 将纯子代理工作保留在底部栏，不显示为主窗口请求提示。
  const suppressSubagentOnlyMainCue = hasRunningSubagents
    && normalizedSessionRunPhase(surfaceConversation?.runPhase) === "toolrunning";
  const runtimeCue = visibleStreamingThinkingOverlayBlock || suppressSubagentOnlyMainCue
    ? null
    : conversationRuntimeCue(surfaceConversation, renderItems, t);
  // isConversationRunning controls only the main composer stop affordance.
  // isConversationRunning 仅控制主输入区的停止操作展示。
  const isConversationRunning = conversationIsRunning(selectedConversation);
  // pendingPlanArtifact tracks the current submit_plan artifact that still requires a user decision.
  // pendingPlanArtifact 跟踪当前仍需要用户决策的 submit_plan 计划产物。
  const pendingPlanArtifact = React.useMemo(() => {
    const artifact = !isEmptyConversation
      ? props.workbench.sessionPlanMode?.artifact ?? null
      : null;
    if (!artifact) {
      return null;
    }
    const content = artifact.content;
    if (typeof content !== "string" || !content.trim()) {
      return null;
    }
    return {
      name: artifact.name,
      path: artifact.path,
      content,
      updatedAt: artifact.updatedAt,
    };
  }, [isEmptyConversation, props.workbench.sessionPlanMode?.artifact]);
  // pendingAskQuestion tracks the selected session's blocking ask request.
  // pendingAskQuestion 跟踪当前选中会话的阻塞式 ask 请求。
  const pendingAskQuestion = React.useMemo(() => {
    const question = props.workbench.pendingQuestion ?? null;
    if (isEmptyConversation || !question || question.sessionId !== props.selectedConversationId) {
      return null;
    }
    return question;
  }, [isEmptyConversation, props.selectedConversationId, props.workbench.pendingQuestion]);
  // composerBlockedByDecision hides the normal composer until the user resolves the blocking decision surface.
  // composerBlockedByDecision 在用户处理完阻塞式决策界面前隐藏普通输入区。
  const composerBlockedByDecision = Boolean(pendingPlanArtifact || pendingAskQuestion);
  // conversationStreamSignature changes when streamed content grows inside existing transcript rows.
  // conversationStreamSignature 在现有转录行内部的流式内容增长时变化。
  const conversationStreamSignature = React.useMemo(
    () => buildConversationStreamSignature(messages),
    [messages],
  );
  React.useEffect(() => {
    const liveSurfaceIds = new Set(props.workbench.conversations.map((conversation) => conversation.id));
    const navigation = props.workbench.subagentNavigation;
    if (navigation) {
      for (const task of navigation.tasks ?? []) {
        liveSurfaceIds.add(task.taskId);
      }
    }
    for (const surfaceId of conversationSurfaceLayoutStatesRef.current.keys()) {
      if (!liveSurfaceIds.has(surfaceId)) {
        conversationSurfaceLayoutStatesRef.current.delete(surfaceId);
      }
    }
  }, [props.workbench.conversations, props.workbench.subagentNavigation]);
  // readConversationSurfaceLayoutState returns the current restore state without creating one.
  // readConversationSurfaceLayoutState 读取当前恢复状态但不主动创建。
  const readConversationSurfaceLayoutState = React.useCallback((surfaceId: string | null): ConversationSurfaceLayoutRestoreState | null => {
    if (!surfaceId) {
      return null;
    }
    return conversationSurfaceLayoutStatesRef.current.get(surfaceId) ?? null;
  }, []);
  // updateConversationSurfaceLayoutState merges one restore-state change for a surface.
  // updateConversationSurfaceLayoutState 合并单个界面的恢复状态变化。
  const updateConversationSurfaceLayoutState = React.useCallback((
    surfaceId: string,
    updater: (current: ConversationSurfaceLayoutRestoreState) => ConversationSurfaceLayoutRestoreState,
  ) => {
    const current = conversationSurfaceLayoutStatesRef.current.get(surfaceId)
      ?? createConversationSurfaceLayoutRestoreState();
    conversationSurfaceLayoutStatesRef.current.set(surfaceId, {
      ...updater(current),
      layoutVersion: current.layoutVersion + 1,
    });
  }, []);
  // writeConversationScrollSnapshot stores the current scroll snapshot inside the surface restore state.
  // writeConversationScrollSnapshot 将当前滚动快照写入界面恢复状态。
  const writeConversationScrollSnapshot = React.useCallback((surfaceId: string, snapshot: ConversationScrollSnapshot) => {
    // pendingSurfaceSwitch blocks pre-restore writes after React swaps the DOM to the next surface.
    // pendingSurfaceSwitch 阻止 React 已换到新界面但恢复流程尚未接管时的预恢复写入。
    const pendingSurfaceSwitch = activeSurfaceConversationId !== null
      && selectedConversationTailRef.current !== activeSurfaceConversationId
      && surfaceId === activeSurfaceConversationId;
    if (pendingSurfaceSwitch) {
      return;
    }
    updateConversationSurfaceLayoutState(surfaceId, (current) => ({
      ...current,
      scroll: snapshot,
    }));
  }, [activeSurfaceConversationId, updateConversationSurfaceLayoutState]);
  // rememberConversationScrollSnapshotForSurface captures the current DOM viewport for one exact surface.
  // rememberConversationScrollSnapshotForSurface 为指定界面精确捕获当前 DOM 视口。
  const rememberConversationScrollSnapshotForSurface = React.useCallback((surfaceId: string) => {
    const element = workspaceRef.current;
    if (!element || element.dataset.conversationSurfaceId !== surfaceId) {
      return false;
    }
    const wasAtBottom = conversationViewportAllowsTailFollow(element);
    writeConversationScrollSnapshot(surfaceId, {
      anchor: wasAtBottom ? null : captureConversationViewportAnchor(element),
      distanceFromBottom: wasAtBottom ? 0 : conversationDistanceFromBottom(element),
      wasAtBottom,
    });
    return true;
  }, [writeConversationScrollSnapshot]);
  // rememberConversationScrollSnapshot captures the current shared scroll element before route changes.
  // rememberConversationScrollSnapshot 在路由切换前捕获当前共享滚动元素状态。
  const rememberConversationScrollSnapshot = React.useCallback(() => {
    const element = workspaceRef.current;
    const surfaceId = element?.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
    if (!surfaceId) {
      return;
    }
    if (rememberConversationScrollSnapshotForSurface(surfaceId)) {
      const snapshot = readConversationSurfaceLayoutState(surfaceId)?.scroll;
      shouldFollowConversationTailRef.current = snapshot?.wasAtBottom ?? shouldFollowConversationTailRef.current;
    }
  }, [activeSurfaceConversationId, readConversationSurfaceLayoutState, rememberConversationScrollSnapshotForSurface]);

  // clearLayoutRestoreFrames cancels pending viewport correction frames.
  // clearLayoutRestoreFrames 取消待执行的视口校正帧。
  const clearLayoutRestoreFrames = React.useCallback(() => {
    for (const frame of layoutRestoreFramesRef.current) {
      window.cancelAnimationFrame(frame);
    }
    layoutRestoreFramesRef.current = [];
  }, []);

  // handleConversationUserScrollStart lets user intent cancel pending automatic viewport correction.
  // handleConversationUserScrollStart 让用户滚动意图取消待执行的自动视口校正。
  const handleConversationUserScrollStart = React.useCallback((direction: ConversationTimelineUserScrollDirection = "unknown") => {
    const element = workspaceRef.current;
    const surfaceId = element?.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
    userScrollSettleUntilRef.current = Date.now() + CONVERSATION_USER_SCROLL_SETTLE_MS;
    if (direction === "up") {
      upwardScrollTailSuppressUntilRef.current = Date.now() + CONVERSATION_UPWARD_SCROLL_TAIL_SUPPRESS_MS;
    } else if (direction === "down") {
      upwardScrollTailSuppressUntilRef.current = 0;
    }
    clearLayoutRestoreFrames();
    pendingLayoutViewportRef.current = null;
    if (surfaceId && scrollRestoreGuardRef.current?.surfaceId === surfaceId) {
      scrollRestoreGuardRef.current = null;
    }
    if (direction !== "down") {
      shouldFollowConversationTailRef.current = false;
    }
  }, [activeSurfaceConversationId, clearLayoutRestoreFrames]);

  // updateConversationBottomState refreshes the bottom affordance from the timeline scroll position.
  // updateConversationBottomState 根据时间线滚动位置刷新底部跳转按钮状态。
  const updateConversationBottomState = React.useCallback(() => {
    const element = workspaceRef.current;
    if (!element) {
      shouldFollowConversationTailRef.current = true;
      setIsAtConversationBottom(true);
      return;
    }
    const suppressNearBottomFollow = Date.now() <= upwardScrollTailSuppressUntilRef.current;
    const alreadyTailLocked = shouldFollowConversationTailRef.current && !suppressNearBottomFollow;
    const surfaceId = element.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
    if (alreadyTailLocked) {
      if (surfaceId) {
        writeConversationScrollSnapshot(surfaceId, {
          anchor: null,
          distanceFromBottom: 0,
          wasAtBottom: true,
        });
      }
      setIsAtConversationBottom(true);
      return;
    }
    const shouldFollow = !suppressNearBottomFollow && conversationViewportAllowsTailFollow(element);
    const restoreGuard = scrollRestoreGuardRef.current;
    if (surfaceId && restoreGuard?.surfaceId === surfaceId && restoreGuard.wasAtBottom && !suppressNearBottomFollow && !shouldFollow) {
      shouldFollowConversationTailRef.current = true;
      writeConversationScrollSnapshot(surfaceId, {
        anchor: null,
        distanceFromBottom: 0,
        wasAtBottom: true,
      });
      setIsAtConversationBottom(true);
      return;
    }
    shouldFollowConversationTailRef.current = shouldFollow;
    if (surfaceId) {
      writeConversationScrollSnapshot(surfaceId, {
        anchor: shouldFollow ? null : captureConversationViewportAnchor(element),
        distanceFromBottom: conversationDistanceFromBottom(element),
        wasAtBottom: shouldFollow,
      });
    }
    setIsAtConversationBottom(shouldFollow);
  }, [activeSurfaceConversationId, writeConversationScrollSnapshot]);

  // capturePendingConversationLayoutViewport stores the current visual anchor before layout moves.
  // capturePendingConversationLayoutViewport 在布局移动前保存当前视觉锚点。
  const capturePendingConversationLayoutViewport = React.useCallback(() => {
    const element = workspaceRef.current;
    const surfaceId = element?.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
    if (!element || !surfaceId || isEmptyConversation || showConversationLoadingOnly) {
      pendingLayoutViewportRef.current = null;
      return;
    }
    const wasAtBottom = shouldFollowConversationTailRef.current || conversationViewportAllowsTailFollow(element);
    pendingLayoutViewportRef.current = {
      anchor: captureConversationViewportAnchor(element),
      distanceFromBottom: wasAtBottom ? 0 : conversationDistanceFromBottom(element),
      surfaceId,
      wasAtBottom,
    };
    writeConversationScrollSnapshot(surfaceId, {
      anchor: pendingLayoutViewportRef.current.anchor,
      distanceFromBottom: pendingLayoutViewportRef.current.distanceFromBottom,
      wasAtBottom,
    });
  }, [activeSurfaceConversationId, isEmptyConversation, showConversationLoadingOnly, writeConversationScrollSnapshot]);
  // restorePendingConversationLayoutViewport restores the captured viewport after layout settles.
  // restorePendingConversationLayoutViewport 在布局稳定后恢复已捕获的视口。
  const restorePendingConversationLayoutViewport = React.useCallback(() => {
    const snapshot = pendingLayoutViewportRef.current;
    const element = workspaceRef.current;
    if (!snapshot || !element || element.dataset.conversationSurfaceId !== snapshot.surfaceId) {
      return;
    }
    if (snapshot.wasAtBottom) {
      element.scrollTop = element.scrollHeight;
    } else if (!snapshot.anchor || !restoreConversationViewportAnchor(element, snapshot.anchor)) {
      conversationScrollToDistanceFromBottom(element, snapshot.distanceFromBottom);
    }
    updateConversationBottomState();
  }, [updateConversationBottomState]);
  // schedulePendingConversationLayoutRestore replays viewport correction across multiple layout frames.
  // schedulePendingConversationLayoutRestore 跨多个布局帧重放视口校正。
  const schedulePendingConversationLayoutRestore = React.useCallback(() => {
    clearLayoutRestoreFrames();
    const runAfterFrame = (remainingFrames: number) => {
      const frame = window.requestAnimationFrame(() => {
        layoutRestoreFramesRef.current = layoutRestoreFramesRef.current.filter((candidate) => candidate !== frame);
        restorePendingConversationLayoutViewport();
        if (remainingFrames > 0) {
          runAfterFrame(remainingFrames - 1);
          return;
        }
        pendingLayoutViewportRef.current = null;
      });
      layoutRestoreFramesRef.current.push(frame);
    };
    runAfterFrame(4);
  }, [clearLayoutRestoreFrames, restorePendingConversationLayoutViewport]);

  React.useEffect(() => () => clearLayoutRestoreFrames(), [clearLayoutRestoreFrames]);

  React.useEffect(() => {
    const handleLayoutWillChange = () => {
      capturePendingConversationLayoutViewport();
      schedulePendingConversationLayoutRestore();
    };
    window.addEventListener(CONVERSATION_LAYOUT_WILL_CHANGE_EVENT, handleLayoutWillChange);
    return () => window.removeEventListener(CONVERSATION_LAYOUT_WILL_CHANGE_EVENT, handleLayoutWillChange);
  }, [capturePendingConversationLayoutViewport, schedulePendingConversationLayoutRestore]);

  // Restore the virtualized transcript viewport when thinking display filters insert or remove rows.
  // 当思考展示过滤插入或移除虚拟化行时恢复会话视口位置。
  React.useLayoutEffect(() => {
    if (!props.layoutStabilityKey) {
      return;
    }
    schedulePendingConversationLayoutRestore();
  }, [props.layoutStabilityKey, schedulePendingConversationLayoutRestore]);

  React.useLayoutEffect(() => {
    const element = workspaceRef.current?.querySelector<HTMLElement>(".conversation-scroll-content");
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }
    const initialRect = element.getBoundingClientRect();
    let previousWidth = initialRect.width;
    let previousHeight = initialRect.height;
    const observer = new ResizeObserver(([entry]) => {
      const nextWidth = entry.contentRect.width;
      const nextHeight = entry.contentRect.height;
      const widthChanged = Math.abs(nextWidth - previousWidth) >= 1;
      const heightChanged = Math.abs(nextHeight - previousHeight) >= 1;
      if (!widthChanged && !heightChanged) {
        return;
      }
      previousWidth = nextWidth;
      previousHeight = nextHeight;
      const scrollElement = workspaceRef.current;
      const surfaceId = scrollElement?.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
      const suppressNearBottomFollow = Date.now() <= upwardScrollTailSuppressUntilRef.current;
      const userScrollSettling = Date.now() <= userScrollSettleUntilRef.current;
      if (heightChanged && scrollElement && shouldFollowConversationTailRef.current && !suppressNearBottomFollow && !userScrollSettling) {
        scrollElement.scrollTop = scrollElement.scrollHeight;
        if (surfaceId) {
          writeConversationScrollSnapshot(surfaceId, {
            anchor: null,
            distanceFromBottom: 0,
            wasAtBottom: true,
          });
        }
        setIsAtConversationBottom(true);
        return;
      }
      updateConversationBottomState();
      if (widthChanged) {
        capturePendingConversationLayoutViewport();
        schedulePendingConversationLayoutRestore();
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [activeSurfaceConversationId, capturePendingConversationLayoutViewport, schedulePendingConversationLayoutRestore, updateConversationBottomState, writeConversationScrollSnapshot]);

  React.useLayoutEffect(() => {
    const element = workspaceRef.current;
    const surfaceId = element?.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
    const previousSnapshot = hiddenThinkingLayoutSnapshotRef.current;
    if (
      previousSnapshot
      && previousSnapshot.hiddenThinkingSignature !== hiddenThinkingSignature
      && element
      && surfaceId
      && previousSnapshot.surfaceId === surfaceId
    ) {
      if (previousSnapshot.wasAtBottom) {
        element.scrollTop = element.scrollHeight;
      } else if (!previousSnapshot.anchor || !restoreConversationViewportAnchor(element, previousSnapshot.anchor)) {
        conversationScrollToDistanceFromBottom(element, previousSnapshot.distanceFromBottom);
      }
      pendingLayoutViewportRef.current = {
        anchor: previousSnapshot.anchor,
        distanceFromBottom: previousSnapshot.distanceFromBottom,
        surfaceId,
        wasAtBottom: previousSnapshot.wasAtBottom,
      };
      schedulePendingConversationLayoutRestore();
    }

    if (!element || !surfaceId || isEmptyConversation || showConversationLoadingOnly) {
      hiddenThinkingLayoutSnapshotRef.current = null;
      return;
    }
    hiddenThinkingLayoutSnapshotRef.current = {
      hiddenThinkingSignature,
      anchor: captureConversationViewportAnchor(element),
      distanceFromBottom: conversationDistanceFromBottom(element),
      surfaceId,
      wasAtBottom: shouldFollowConversationTailRef.current && conversationViewportAllowsTailFollow(element),
    };
  });

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
    upwardScrollTailSuppressUntilRef.current = 0;
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

  // handleComposerDockHeightChange updates overlay spacing; tail pinning is handled after CSS variables settle.
  // handleComposerDockHeightChange 更新浮层间距；尾部锁定在 CSS 变量稳定后处理。
  const handleComposerDockHeightChange = React.useCallback((height: number) => {
    setComposerDockHeight((current) => {
      if (current === height) {
        return current;
      }
      return height;
    });
  }, []);

  React.useLayoutEffect(() => {
    if (!shouldFollowConversationTailRef.current || isEmptyConversation || showConversationLoadingOnly) {
      updateConversationBottomState();
      return;
    }
    const surfaceId = workspaceRef.current?.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
    const pinComposerAdjustedTail = () => {
      const element = workspaceRef.current;
      if (!element) {
        return;
      }
      element.scrollTop = element.scrollHeight;
      if (surfaceId) {
        writeConversationScrollSnapshot(surfaceId, {
          anchor: null,
          distanceFromBottom: 0,
          wasAtBottom: true,
        });
      }
      setIsAtConversationBottom(true);
    };
    let secondFrame: number | null = null;
    pinComposerAdjustedTail();
    const firstFrame = window.requestAnimationFrame(() => {
      pinComposerAdjustedTail();
      secondFrame = window.requestAnimationFrame(pinComposerAdjustedTail);
    });
    return () => {
      window.cancelAnimationFrame(firstFrame);
      if (secondFrame !== null) {
        window.cancelAnimationFrame(secondFrame);
      }
    };
  }, [
    activeSurfaceConversationId,
    composerDockHeight,
    isEmptyConversation,
    showConversationLoadingOnly,
    updateConversationBottomState,
    writeConversationScrollSnapshot,
  ]);

  // handleTimelineVirtualizerStateChange remembers measured row heights for the active surface.
  // handleTimelineVirtualizerStateChange 记住当前界面的已测量行高。
  const handleTimelineVirtualizerStateChange = React.useCallback((surfaceId: string, state: ConversationTimelineVirtualRestoreState | null) => {
    if (!surfaceId) {
      return;
    }
    updateConversationSurfaceLayoutState(surfaceId, (current) => ({
      ...current,
      virtualizer: state,
    }));
  }, [updateConversationSurfaceLayoutState]);
  // handleExpandedStateChange stores row expansion state beside scroll and virtualizer data.
  // handleExpandedStateChange 将行展开状态与滚动、虚拟列表数据一起保存。
  const handleExpandedStateChange = React.useCallback((surfaceId: string, state: ConversationSurfaceExpandedState) => {
    if (!surfaceId) {
      return;
    }
    updateConversationSurfaceLayoutState(surfaceId, (current) => ({
      ...current,
      expandedState: state,
    }));
  }, [updateConversationSurfaceLayoutState]);

  React.useLayoutEffect(() => {
    if (!props.selectedConversationId || isEmptyConversation || showConversationLoadingOnly) {
      return;
    }

    const element = workspaceRef.current;
    const previousSurfaceId = selectedConversationTailRef.current;
    const selectedChanged = previousSurfaceId !== activeSurfaceConversationId;
    selectedConversationTailRef.current = activeSurfaceConversationId;
    renderedSurfaceConversationIdRef.current = activeSurfaceConversationId ?? null;
    const savedScrollSnapshot = selectedChanged && activeSurfaceConversationId
      ? readConversationSurfaceLayoutState(activeSurfaceConversationId)?.scroll
      : null;
    const restoreSurfaceId = activeSurfaceConversationId;
    if (selectedChanged && savedScrollSnapshot && element && restoreSurfaceId) {
      const restoreSavedScrollPosition = () => {
        if (savedScrollSnapshot.wasAtBottom) {
          element.scrollTop = element.scrollHeight;
          return;
        }
        if (savedScrollSnapshot.anchor) {
          if (restoreConversationViewportAnchor(element, savedScrollSnapshot.anchor)) {
            return;
          }
          element.scrollTop = element.scrollHeight;
          return;
        }
        if (!savedScrollSnapshot.anchor) {
          conversationScrollToDistanceFromBottom(element, savedScrollSnapshot.distanceFromBottom);
        }
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

    if (selectedChanged && !savedScrollSnapshot && element && restoreSurfaceId) {
      shouldFollowConversationTailRef.current = true;
      scrollRestoreGuardRef.current = {
        surfaceId: restoreSurfaceId,
        wasAtBottom: true,
      };
      // secondFrame keeps the default tail initialization stable after React paints the new transcript.
      // secondFrame 在 React 绘制新转录后保持默认贴底初始化稳定。
      let secondFrame: number | null = null;
      // restoreDefaultTailPosition initializes unseen surfaces without reusing the previous surface scrollTop.
      // restoreDefaultTailPosition 初始化未见过的界面，避免复用上一个界面的 scrollTop。
      const restoreDefaultTailPosition = () => {
        element.scrollTop = element.scrollHeight;
      };
      restoreDefaultTailPosition();
      const firstFrame = window.requestAnimationFrame(() => {
        restoreDefaultTailPosition();
        secondFrame = window.requestAnimationFrame(() => {
          restoreDefaultTailPosition();
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

    const now = Date.now();
    const suppressNearBottomFollow = now <= upwardScrollTailSuppressUntilRef.current;
    const userScrollSettling = now <= userScrollSettleUntilRef.current;
    // inheritedTailFollow is ignored during surface switches to prevent previous conversations from forcing tail lock.
    // inheritedTailFollow 在界面切换时不继承，避免上一个会话强制污染当前会话锁底。
    const inheritedTailFollow = selectedChanged ? false : shouldFollowConversationTailRef.current;
    const shouldFollowTail = !element
      || (!userScrollSettling && (
        inheritedTailFollow
        || (!suppressNearBottomFollow && conversationViewportAllowsTailFollow(element))
      ));
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
    readConversationSurfaceLayoutState,
    showConversationLoadingOnly,
    updateConversationBottomState,
    writeConversationScrollSnapshot,
    props.selectedConversationId,
  ]);

  React.useEffect(() => {
    updateConversationBottomState();
  }, [activeSurfaceConversationId, conversationStreamSignature, updateConversationBottomState]);

  React.useEffect(() => {
    setThinkingDisplayMode(normalizeThinkingDisplayMode(props.workbench.thinkingDisplayMode));
  }, [props.workbench.thinkingDisplayMode]);

  // handleThinkingDisplayModeChange applies the WebUI preference immediately and persists it through the service.
  // handleThinkingDisplayModeChange 立即应用 WebUI 偏好，并通过服务持久化该设置。
  const handleThinkingDisplayModeChange = React.useCallback((mode: ThinkingDisplayMode) => {
    setThinkingDisplayMode(mode);
    void props.onSelectWorkbenchThinkingDisplayMode(mode).then((saved) => {
      if (!saved) {
        setThinkingDisplayMode(normalizeThinkingDisplayMode(props.workbench.thinkingDisplayMode));
      }
    });
  }, [props.onSelectWorkbenchThinkingDisplayMode, props.workbench.thinkingDisplayMode]);

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

  const {
    handlePlanArtifactCancel,
    handlePlanArtifactExecute,
    handlePlanArtifactSuggest,
  } = useChatAreaPlanActions({
    executeLabel: t("planArtifact.execute"),
    locale,
    onSetSessionPlanMode: props.onSetSessionPlanMode,
    onSubmitPrompt: props.onSubmitPrompt,
    pendingPlanArtifact,
    reasoningEffort: props.workbench.reasoningSelection?.selectedEffort ?? null,
  });

  React.useEffect(() => {
    if (composerBlockedByDecision) {
      setComposerDockHeight(0);
    }
  }, [composerBlockedByDecision]);

  // composerDock stays as a floating layer outside the transcript scroll flow.
  // composerDock 作为转录滚动流之外的浮动层保持挂载。
  const composerDock = composerBlockedByDecision ? null : (
    <ChatAreaComposerDockLayer
      chatAreaProps={props}
      projectId={selectedProject?.id ?? props.selectedDraftConversation?.projectId ?? null}
      conversationId={selectedConversation?.id ?? null}
      isEmptyConversation={isEmptyConversation}
      isConversationRunning={isConversationRunning}
      showScrollToBottom={!isEmptyConversation && !isAtConversationBottom}
      runtimeCue={runtimeCue}
      thinkingDisplayMode={thinkingDisplayMode}
      sessionReviewSummary={props.sessionReviewSummary}
      onComposerDockHeightChange={handleComposerDockHeightChange}
      onScrollToBottom={() => scrollConversationToBottom()}
      onThinkingDisplayModeChange={handleThinkingDisplayModeChange}
    />
  );
  // conversationSurfaceContent keeps transcript, overlays, and composer in one adaptive content coordinate space.
  // conversationSurfaceContent 将转录、浮层与输入区保留在同一个自适应内容坐标空间内。
  // activeSurfaceLayoutState reuses one surface-scoped layout snapshot for this render.
  // activeSurfaceLayoutState 在本次渲染中复用单个界面级布局快照。
  const activeSurfaceLayoutState = activeSurfaceConversationId
    ? readConversationSurfaceLayoutState(activeSurfaceConversationId)
    : null;
  const conversationSurfaceContent = (
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
        hiddenThinkingBlockIds={hiddenThinkingBlockIds}
        messages={messages}
        renderItems={renderItems}
        error={props.conversationLoadError}
        hasMoreHistory={props.conversationHasMoreHistory}
        historyLoading={props.conversationHistoryLoading}
        onLoadMoreHistory={props.onLoadConversationHistory}
        parentConversationId={props.selectedConversationId ?? null}
        workspacePath={selectedProject?.path ?? null}
        commandSessions={visibleCommandSessions}
        pendingPlanArtifact={pendingPlanArtifact}
        onPlanArtifactExecute={handlePlanArtifactExecute}
        onPlanArtifactSuggest={handlePlanArtifactSuggest}
        onPlanArtifactCancel={handlePlanArtifactCancel}
        onScrollStateChange={updateConversationBottomState}
        onUserScrollStart={handleConversationUserScrollStart}
        onOpenSubagentPanel={handleOpenSubagentPanelFromTimeline}
        scrollRef={workspaceRef}
        initialScrollDistanceFromBottom={activeSurfaceLayoutState?.scroll.distanceFromBottom ?? 0}
        suppressInitialDistanceRestore={Boolean(activeSurfaceLayoutState?.scroll.anchor)}
        initialVirtualizerState={activeSurfaceLayoutState?.virtualizer ?? null}
        onVirtualizerStateChange={handleTimelineVirtualizerStateChange}
        initialExpandedState={activeSurfaceLayoutState?.expandedState ?? null}
        onExpandedStateChange={handleExpandedStateChange}
        footer={null}
        suppressEmptyPlaceholder={isEmptyConversation}
        onCopyText={props.onCopyText}
        onEditUserPrompt={userPromptEdit.open}
        onForkConversation={props.onForkConversation}
        onSelectForkSourceConversation={handleForkSourceConversationSelect}
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
      <UserPromptEditDialog
        cancelLabel={t("conversation.action.cancel")}
        draft={userPromptEdit.draft}
        onClose={userPromptEdit.close}
        onSubmit={() => void userPromptEdit.submit()}
        onTextChange={userPromptEdit.setText}
        saving={userPromptEdit.saving}
        submitLabel={t("conversation.action.saveAndReplay")}
        text={userPromptEdit.text}
        title={t("conversation.action.editPrompt")}
      />
    </>
  );
  return (
    <div
      className={cn(
        "area-grid chat-grid",
        isEmptyConversation && "empty-conversation-grid",
        showConversationLoadingOnly && "loading-conversation-grid",
      )}
      style={{ "--composer-dock-height": `${composerDockHeight}px` } as React.CSSProperties}
    >
      {showConversationLoadingOnly ? (
        <ConversationLoadingCenter progress={displayedConversationHydration} />
      ) : (
        conversationSurfaceContent
      )}
    </div>
  );
}
