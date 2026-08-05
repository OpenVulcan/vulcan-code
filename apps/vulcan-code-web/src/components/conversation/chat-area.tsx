import React from "react";
import {
  ChatAreaComposerDockLayer,
  composerAttachmentMedia,
} from "@/components/conversation/chat-area/composer-dock-layer";
import { UserPromptEditDialog } from "@/components/conversation/chat-area/user-prompt-edit-dialog";
import {
  ConversationLoadingCenter,
  ConversationSubmissionStatus,
  EmptyConversationPrompt,
} from "@/components/conversation/conversation-chrome";
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
} from "@/components/conversation/thinking-display";
import {
  generatedMediaAttachmentsFromToolDetailPayload,
  projectConversationRenderItems,
} from "@/components/conversation/conversation-transcript";
import {
  buildConversationStreamSignature,
  captureConversationViewportAnchor,
  conversationDistanceFromBottom,
  conversationViewportAllowsTailFollow,
  restoreConversationScrollSnapshot,
  type ConversationScrollSnapshot,
  type ConversationTimelineUserScrollDirection,
  type ConversationViewportAnchorSnapshot,
} from "@/components/conversation/chat-area-scroll";
import {
  CONVERSATION_LAYOUT_WILL_CHANGE_EVENT,
} from "@/components/conversation/conversation-layout-events";
import type { ConversationPlanArtifact } from "@/components/conversation/conversation-model";
import { cn } from "@/lib/utils";
import { useI18n } from "@/i18n";
import { projectForConversation } from "@/state/workbench-selectors";
import { toast } from "sonner";
import type {
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ConversationMessage,
} from "@/types";
import type { ChatAreaProps } from "@/components/conversation/chat-area/types";
import { useChatAreaPlanActions } from "@/components/conversation/chat-area/plan-actions";
import { useUserPromptEditState } from "@/components/conversation/chat-area/user-prompt-edit-state";
import {
  useConversationVisualTransitions,
  useThinkingDisplayMode,
} from "@/components/conversation/chat-area/visual-state";
import { useConversationSurfaceLayoutMemory } from "@/components/conversation/chat-area/surface-layout-memory";
import { useConversationViewportFrameScheduler } from "@/components/conversation/chat-area/viewport-frame-scheduler";
import { hasAuthoritativeSubmissionCorrelation } from "@/components/workspace/draft-submission-state";

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
  // Thinking display state synchronizes the backend preference with stale-safe optimistic selection.
  // 思考展示状态将后端偏好与过期安全的乐观选择保持同步。
  const {
    mode: thinkingDisplayMode,
    selectMode: handleThinkingDisplayModeChange,
  } = useThinkingDisplayMode({
    authoritativeMode: props.workbench.thinkingDisplayMode,
    onSelectMode: props.onSelectWorkbenchThinkingDisplayMode,
  });
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
  // liveLayoutSurfaceIds includes every conversation and currently projected subagent task.
  // liveLayoutSurfaceIds 包含全部会话与当前投影的子代理任务。
  const liveLayoutSurfaceIds = React.useMemo(() => {
    // surfaceIds starts with every live conversation id.
    // surfaceIds 以全部存活会话标识初始化。
    const surfaceIds = new Set(props.workbench.conversations.map((conversation) => conversation.id));
    // task contributes one routed subagent surface whose layout can be restored.
    // task 表示一个可恢复布局的已路由子代理界面。
    for (const task of props.workbench.subagentNavigation?.tasks ?? []) {
      surfaceIds.add(task.taskId);
    }
    return surfaceIds;
  }, [props.workbench.conversations, props.workbench.subagentNavigation]);
  // Surface layout memory owns scroll and expansion restore records.
  // 界面布局记忆管理滚动与展开恢复记录。
  const {
    read: readConversationSurfaceLayoutState,
    retainTurnHeights: retainSurfaceTurnHeights,
    writeScroll: writeSurfaceScrollSnapshot,
    writeExpandedState: handleExpandedStateChange,
    writeTurnHeight: handleTurnHeightChange,
  } = useConversationSurfaceLayoutMemory({ liveSurfaceIds: liveLayoutSurfaceIds });
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
  // Viewport frame scheduler makes every automatic scroll correction cancellable by user intent.
  // 视口帧调度器使每个自动滚动校正都可被用户意图取消。
  const {
    cancelAll: cancelAllConversationViewportFrames,
    schedule: scheduleConversationViewportFrames,
  } = useConversationViewportFrameScheduler();
  // pendingLayoutRestoreCancelRef owns only the repeated layout-event restore sequence.
  // pendingLayoutRestoreCancelRef 仅持有重复布局事件恢复序列。
  const pendingLayoutRestoreCancelRef = React.useRef<(() => void) | null>(null);
  // scrollRestoreGuardRef prevents transient restore measurements from overwriting tail-pinned memory.
  // scrollRestoreGuardRef 防止恢复过程中的临时测量覆盖贴底记忆。
  const scrollRestoreGuardRef = React.useRef<{ surfaceId: string; wasAtBottom: boolean } | null>(null);
  const surfaceConversation = selectedConversation;
  const messages = React.useMemo(() => {
    const authoritativeMessages = props.workbench.messages
      .filter((message) => message.conversationId === activeSurfaceConversationId);
    const pendingSubmission = props.selectedDraftConversation?.pendingSubmission;
    if (!pendingSubmission || props.selectedDraftConversation?.id !== activeSurfaceConversationId) {
      return authoritativeMessages;
    }
    if (hasAuthoritativeSubmissionCorrelation(authoritativeMessages, pendingSubmission.clientSubmissionId)) {
      return authoritativeMessages;
    }
    // optimisticMessage uses the draft id as its temporary conversation scope until the ACK supplies the durable id.
    // optimisticMessage 在确认返回持久化标识前使用草稿标识作为临时会话作用域。
    const optimisticMessage: ConversationMessage = {
      id: `optimistic:${pendingSubmission.clientSubmissionId}`,
      conversationId: props.selectedDraftConversation.id,
      createdAt: String(pendingSubmission.startedAt),
      role: "user",
      body: pendingSubmission.prompt,
      attachments: pendingSubmission.attachments,
      editableUserPrompt: false,
      metadata: {
        client_submission_id: pendingSubmission.clientSubmissionId,
        optimistic: true,
      },
    };
    return [...authoritativeMessages, optimisticMessage];
  }, [activeSurfaceConversationId, props.selectedDraftConversation, props.workbench.messages]);
  // pendingSubmissionVisible remains true only until the exact authoritative user record replaces the optimistic row.
  // pendingSubmissionVisible 仅在精确权威用户记录替代乐观行之前保持为真。
  const pendingSubmissionVisible = Boolean(
    props.selectedDraftConversation?.pendingSubmission
    && !hasAuthoritativeSubmissionCorrelation(
      props.workbench.messages.filter((message) => message.conversationId === activeSurfaceConversationId),
      props.selectedDraftConversation.pendingSubmission.clientSubmissionId,
    ),
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
    (props.selectedDraftConversation && !props.selectedDraftConversation.pendingSubmission)
      || (selectedConversation && !showConversationLoadingOnly && !props.conversationLoadError && messages.length === 0),
  );
  // isMaterializingConversationSurface identifies the brief draft-to-summary frame that must remain visually blank.
  // isMaterializingConversationSurface 标识草稿转为真实摘要期间必须保持空白的短暂帧。
  const isMaterializingConversationSurface = Boolean(
    activeSurfaceConversationId && !surfaceConversation && !props.selectedDraftConversation,
  );
  // visualSurfaceKey distinguishes persisted sessions from project-scoped new-conversation surfaces.
  // visualSurfaceKey 区分已持久化会话与项目范围的新建会话界面。
  const visualSurfaceKey = activeSurfaceConversationId
    ?? `draft:${props.selectedDraftConversation?.projectId ?? "workspace"}`;
  const streamingThinkingBlock = activeStreamingThinkingBlock(surfaceConversation, renderItems);
  const displayedStreamingThinkingBlock = activeThinkingOverlayBlock(thinkingDisplayMode, streamingThinkingBlock);
  // Visual transitions retain empty/overlay surfaces only within the current conversation scope.
  // 视觉过渡仅在当前会话作用域内保留空状态与浮层。
  const {
    visibleThinkingBlock: visibleStreamingThinkingOverlayBlock,
    thinkingOverlayLeaving: streamingThinkingOverlayLeaving,
    emptyPromptVisible,
    emptyPromptLeaving,
  } = useConversationVisualTransitions({
    surfaceKey: visualSurfaceKey,
    isEmptyConversation,
    displayedThinkingBlock: displayedStreamingThinkingBlock,
  });
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
  // conversationStreamSignature tracks transcript structure without rescanning cumulative streamed bodies.
  // conversationStreamSignature 跟踪转录结构，且不重复扫描累计流式正文。
  const conversationStreamSignature = React.useMemo(
    () => buildConversationStreamSignature(messages),
    [messages],
  );
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
    writeSurfaceScrollSnapshot(surfaceId, snapshot);
  }, [activeSurfaceConversationId, writeSurfaceScrollSnapshot]);
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

  // clearPendingLayoutRestoreFrames cancels only the layout-event restore sequence before replacement.
  // clearPendingLayoutRestoreFrames 仅在替换前取消布局事件恢复序列。
  const clearPendingLayoutRestoreFrames = React.useCallback(() => {
    pendingLayoutRestoreCancelRef.current?.();
    pendingLayoutRestoreCancelRef.current = null;
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
    cancelAllConversationViewportFrames();
    pendingLayoutRestoreCancelRef.current = null;
    pendingLayoutViewportRef.current = null;
    if (surfaceId && scrollRestoreGuardRef.current?.surfaceId === surfaceId) {
      scrollRestoreGuardRef.current = null;
    }
    if (direction !== "down") {
      shouldFollowConversationTailRef.current = false;
    }
  }, [activeSurfaceConversationId, cancelAllConversationViewportFrames]);

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
    const userScrollSettling = Date.now() <= userScrollSettleUntilRef.current;
    if (userScrollSettling || !snapshot || !element || element.dataset.conversationSurfaceId !== snapshot.surfaceId) {
      return;
    }
    restoreConversationScrollSnapshot(element, snapshot);
    updateConversationBottomState();
  }, [updateConversationBottomState]);
  // schedulePendingConversationLayoutRestore replays viewport correction across multiple layout frames.
  // schedulePendingConversationLayoutRestore 跨多个布局帧重放视口校正。
  const schedulePendingConversationLayoutRestore = React.useCallback(() => {
    clearPendingLayoutRestoreFrames();
    pendingLayoutRestoreCancelRef.current = scheduleConversationViewportFrames(
      5,
      restorePendingConversationLayoutViewport,
      () => {
        pendingLayoutViewportRef.current = null;
        pendingLayoutRestoreCancelRef.current = null;
      },
    );
  }, [clearPendingLayoutRestoreFrames, restorePendingConversationLayoutViewport, scheduleConversationViewportFrames]);

  React.useEffect(() => {
    const handleLayoutWillChange = () => {
      capturePendingConversationLayoutViewport();
      schedulePendingConversationLayoutRestore();
    };
    window.addEventListener(CONVERSATION_LAYOUT_WILL_CHANGE_EVENT, handleLayoutWillChange);
    return () => window.removeEventListener(CONVERSATION_LAYOUT_WILL_CHANGE_EVENT, handleLayoutWillChange);
  }, [capturePendingConversationLayoutViewport, schedulePendingConversationLayoutRestore]);

  // Restore the turn-based transcript viewport when thinking display filters insert or remove rows.
  // 当思考展示过滤插入或移除轮次行时恢复会话视口位置。
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
    // resizeFrame coalesces geometry-driven scroll writes into one animation frame.
    // resizeFrame 将几何变化触发的滚动写入合并到一个动画帧。
    let resizeFrame: number | null = null;
    // pendingWidthChange retains width invalidation across coalesced observer callbacks.
    // pendingWidthChange 在合并的观察器回调间保留宽度失效标记。
    let pendingWidthChange = false;
    // pendingHeightChange retains height invalidation across coalesced observer callbacks.
    // pendingHeightChange 在合并的观察器回调间保留高度失效标记。
    let pendingHeightChange = false;
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
      pendingWidthChange ||= widthChanged;
      pendingHeightChange ||= heightChanged;
      if (resizeFrame !== null) {
        return;
      }
      resizeFrame = window.requestAnimationFrame(() => {
        resizeFrame = null;
        const widthInvalidated = pendingWidthChange;
        const heightInvalidated = pendingHeightChange;
        pendingWidthChange = false;
        pendingHeightChange = false;
        const scrollElement = workspaceRef.current;
        const surfaceId = scrollElement?.dataset.conversationSurfaceId ?? renderedSurfaceConversationIdRef.current ?? activeSurfaceConversationId ?? null;
        const suppressNearBottomFollow = Date.now() <= upwardScrollTailSuppressUntilRef.current;
        const userScrollSettling = Date.now() <= userScrollSettleUntilRef.current;
        if (heightInvalidated && scrollElement && shouldFollowConversationTailRef.current && !suppressNearBottomFollow && !userScrollSettling) {
          scrollElement.scrollTop = scrollElement.scrollHeight;
          if (surfaceId) {
            writeConversationScrollSnapshot(surfaceId, {
              anchor: null,
              distanceFromBottom: 0,
              wasAtBottom: true,
            });
          }
          setIsAtConversationBottom(true);
        } else {
          updateConversationBottomState();
        }
        if (widthInvalidated) {
          capturePendingConversationLayoutViewport();
          schedulePendingConversationLayoutRestore();
        }
      });
    });
    observer.observe(element);
    return () => {
      observer.disconnect();
      if (resizeFrame !== null) {
        window.cancelAnimationFrame(resizeFrame);
      }
    };
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
      restoreConversationScrollSnapshot(element, previousSnapshot);
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
    cancelAllConversationViewportFrames();
    pendingLayoutRestoreCancelRef.current = null;
    pendingLayoutViewportRef.current = null;
    const element = workspaceRef.current;
    if (!element) {
      return;
    }
    upwardScrollTailSuppressUntilRef.current = 0;
    const safeBehavior = behavior === "auto" || behavior === "instant" || behavior === "smooth" ? behavior : "smooth";
    element.scrollTo({ top: element.scrollHeight, behavior: safeBehavior });
  }, [cancelAllConversationViewportFrames]);

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
      const userScrollSettling = Date.now() <= userScrollSettleUntilRef.current;
      if (userScrollSettling || !element) {
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
    pinComposerAdjustedTail();
    return scheduleConversationViewportFrames(2, pinComposerAdjustedTail);
  }, [
    activeSurfaceConversationId,
    composerDockHeight,
    isEmptyConversation,
    scheduleConversationViewportFrames,
    showConversationLoadingOnly,
    updateConversationBottomState,
    writeConversationScrollSnapshot,
  ]);

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
        restoreConversationScrollSnapshot(element, savedScrollSnapshot);
      };
      shouldFollowConversationTailRef.current = savedScrollSnapshot.wasAtBottom;
      scrollRestoreGuardRef.current = {
        surfaceId: restoreSurfaceId,
        wasAtBottom: savedScrollSnapshot.wasAtBottom,
      };
      restoreSavedScrollPosition();
      const cancelSavedScrollRestore = scheduleConversationViewportFrames(
        2,
        restoreSavedScrollPosition,
        () => {
          updateConversationBottomState();
          if (scrollRestoreGuardRef.current?.surfaceId === restoreSurfaceId) {
            scrollRestoreGuardRef.current = null;
          }
        },
      );
      return () => {
        cancelSavedScrollRestore();
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
      // restoreDefaultTailPosition initializes unseen surfaces without reusing the previous surface scrollTop.
      // restoreDefaultTailPosition 初始化未见过的界面，避免复用上一个界面的 scrollTop。
      const restoreDefaultTailPosition = () => {
        element.scrollTop = element.scrollHeight;
      };
      restoreDefaultTailPosition();
      const cancelDefaultTailRestore = scheduleConversationViewportFrames(
        2,
        restoreDefaultTailPosition,
        () => {
          updateConversationBottomState();
          if (scrollRestoreGuardRef.current?.surfaceId === restoreSurfaceId) {
            scrollRestoreGuardRef.current = null;
          }
        },
      );
      return () => {
        cancelDefaultTailRestore();
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
    jumpConversationToBottom();
    return scheduleConversationViewportFrames(3, (frameIndex) => {
      jumpConversationToBottom();
      if (frameIndex >= 1) {
        updateConversationBottomState();
      }
    });
  }, [
    conversationStreamSignature,
    isEmptyConversation,
    jumpConversationToBottom,
    activeSurfaceConversationId,
    readConversationSurfaceLayoutState,
    scheduleConversationViewportFrames,
    showConversationLoadingOnly,
    updateConversationBottomState,
    props.selectedConversationId,
  ]);

  React.useEffect(() => {
    updateConversationBottomState();
  }, [activeSurfaceConversationId, conversationStreamSignature, updateConversationBottomState]);

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
      runtimeStatusHost={props.runtimeStatusHost ?? null}
      todolistStatusHost={props.todolistStatusHost ?? null}
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
        autoExpandStreamingThinking={thinkingDisplayMode !== "first_inline_with_popup"}
        conversation={surfaceConversation}
        sessionRunning={conversationIsRunning(surfaceConversation)}
        hiddenThinkingBlockIds={hiddenThinkingBlockIds}
        messages={messages}
        renderItems={renderItems}
        error={props.conversationLoadError}
        hasMoreHistory={props.conversationHasMoreHistory}
        historyLoading={props.conversationHistoryLoading}
        onLoadMoreHistory={props.onLoadConversationHistory}
        onRetryConversationRequest={props.onRetryConversationRequest}
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
        restoreInitialScrollDistance={false}
        initialExpandedState={activeSurfaceLayoutState?.expandedState ?? null}
        initialTurnHeightsByKey={activeSurfaceLayoutState?.turnHeightsByKey ?? null}
        onExpandedStateChange={handleExpandedStateChange}
        onRetainTurnHeights={retainSurfaceTurnHeights}
        onTurnHeightChange={handleTurnHeightChange}
        footer={null}
        suppressEmptyPlaceholder={isEmptyConversation || isMaterializingConversationSurface}
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
      {props.selectedDraftConversation?.pendingSubmission && pendingSubmissionVisible ? (
        <ConversationSubmissionStatus stage={props.selectedDraftConversation.pendingSubmission.stage} />
      ) : null}
      {visibleStreamingThinkingOverlayBlock && activeSurfaceConversationId ? (
        <StreamingThinkingOverlay
          block={visibleStreamingThinkingOverlayBlock}
          conversationId={activeSurfaceConversationId}
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
