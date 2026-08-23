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
import {
  StreamingThinkingOverlay,
  TransientReasoningStatusLine,
} from "@/components/conversation/conversation-blocks";
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
} from "@/components/conversation/chat-area-scroll";
import type { ConversationPlanArtifact } from "@/components/conversation/conversation-model";
import { cn } from "@/lib/utils";
import { useI18n } from "@/i18n";
import { projectForConversation } from "@/state/workbench-selectors";
import { useConversationTransientReasoningStatus } from "@/state/conversation-live-stream-context";
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
import { useConversationScrollController } from "@/components/conversation/chat-area/conversation-scroll-controller";
import { hasAuthoritativeSubmissionCorrelation } from "@/components/workspace/draft-submission-state";
import { conversationContinuationBlockedReasonText } from "@/components/conversation/continuation-blocked-reason";
import { ConversationContinueControl } from "@/components/conversation/conversation-continue-control";

// ChatArea renders the conversation workspace and composer.
// ChatArea 渲染会话工作区与输入区域。
export function ChatArea(props: ChatAreaProps) {
  // t and locale resolve localized runtime labels and user-visible Plan action prompts.
  // t 与 locale 解析运行文案和用户可见计划操作提示词。
  const { t, locale } = useI18n();
  // workspaceRef stores the scrollable conversation timeline element.
  // workspaceRef 保存可滚动的会话时间线元素。
  const workspaceRef = React.useRef<HTMLElement | null>(null);
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
  // continuationClaimKey suppresses duplicate claims until the authoritative workbench revision changes.
  // continuationClaimKey 在权威工作台修订变化前阻止重复领取 Continue。
  const [continuationClaimKey, setContinuationClaimKey] = React.useState<string | null>(null);
  // continuationFeedback keeps claim progress and failures visible after transient toasts disappear.
  // continuationFeedback 在瞬时 toast 消失后继续保留领取进度与失败反馈。
  const [continuationFeedback, setContinuationFeedback] = React.useState<{
    key: string;
    kind: "claiming" | "failed" | "accepted";
  } | null>(null);
  // continuationClientIdsRef preserves one retry id across timeout/retry clicks for the same snapshot.
  // continuationClientIdsRef 为同一快照跨超时/重试点击保留一个继续请求标识。
  const continuationClientIdsRef = React.useRef<Map<string, string>>(new Map());
  const continuation = selectedConversation?.continuation;
  const activeContinuationKey = selectedConversation && continuation
    ? `${selectedConversation.id}:${continuation.generation}:${continuation.expectedLedgerRevision}`
    : null;
  React.useEffect(() => {
    if (!continuation?.eligible || continuationClaimKey !== activeContinuationKey) {
      setContinuationClaimKey(null);
    }
    if (continuationFeedback && continuationFeedback.key !== activeContinuationKey) {
      setContinuationFeedback(null);
    }
  }, [activeContinuationKey, continuation?.eligible, continuationClaimKey, continuationFeedback]);
  // handleContinueConversation claims one exact terminal generation without synthesizing user input.
  // handleContinueConversation 在不伪造用户输入的情况下领取一个精确终止代际。
  const handleContinueConversation = React.useCallback(async () => {
    if (
      !selectedConversation
      || !continuation?.eligible
      || !props.onContinueConversation
      || activeContinuationKey === null
      || continuationClaimKey === activeContinuationKey
    ) {
      return;
    }
    setContinuationClaimKey(activeContinuationKey);
    setContinuationFeedback({ key: activeContinuationKey, kind: "claiming" });
    // accepted remains false for every rejected callback so an unexpected Promise rejection cannot leave the button locked.
    // accepted 对所有被拒绝的回调保持 false，避免意外的 Promise 拒绝让按钮永久锁定。
    let accepted = false;
    try {
      // clientContinuationId reuses one stable idempotency identity for the exact visible continuation token.
      // clientContinuationId 为精确可见的继续令牌复用同一个稳定幂等身份。
      const clientContinuationId = continuationClientIdsRef.current.get(activeContinuationKey)
        ?? globalThis.crypto.randomUUID();
      continuationClientIdsRef.current.set(activeContinuationKey, clientContinuationId);
      if (continuationClientIdsRef.current.size > 128) {
        // oldestKey bounds retained idempotency identities without removing the active token.
        // oldestKey 限制保留的幂等身份数量，同时不会移除当前活动令牌。
        const oldestKey = continuationClientIdsRef.current.keys().next().value;
        if (oldestKey) {
          continuationClientIdsRef.current.delete(oldestKey);
        }
      }
      accepted = await props.onContinueConversation(
        selectedConversation.id,
        continuation.generation,
        continuation.expectedLedgerRevision,
        clientContinuationId,
      );
    } catch {
      toast.error(t("conversation.continuation.claimFailed"));
    }
    if (!accepted) {
      setContinuationClaimKey(null);
      setContinuationFeedback({ key: activeContinuationKey, kind: "failed" });
    } else {
      setContinuationFeedback({ key: activeContinuationKey, kind: "accepted" });
    }
  }, [
    activeContinuationKey,
    continuation,
    continuationClaimKey,
    props.onContinueConversation,
    selectedConversation,
    t,
  ]);
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
  // liveLayoutSurfaceIds includes every conversation represented by the main transcript registry.
  // liveLayoutSurfaceIds 包含主转录注册表所表示的全部会话。
  const liveLayoutSurfaceIds = React.useMemo(() => {
    // surfaceIds starts with every live conversation id.
    // surfaceIds 以全部存活会话标识初始化。
    return new Set(props.workbench.conversations.map((conversation) => conversation.id));
  }, [props.workbench.conversations]);
  // Surface layout memory owns scroll and expansion restore records.
  // 界面布局记忆管理滚动与展开恢复记录。
  const {
    read: readConversationSurfaceLayoutState,
    writeScroll: writeSurfaceScrollSnapshot,
    writeExpandedState: handleExpandedStateChange,
  } = useConversationSurfaceLayoutMemory({ liveSurfaceIds: liveLayoutSurfaceIds });
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
  // transientReasoningStatus is request-local and never enters the stable Workbench snapshot.
  // transientReasoningStatus 仅属于当前请求，绝不进入稳定 Workbench 快照。
  const transientReasoningStatus = useConversationTransientReasoningStatus(
    activeSurfaceConversationId,
  );
  // retainedFirstSubmissionKeyRef preserves the exact optimistic correlation across draft ID promotion.
  // retainedFirstSubmissionKeyRef 在草稿标识转正期间保留精确乐观关联身份。
  const retainedFirstSubmissionKeyRef = React.useRef<string | null>(null);
  // pendingFirstSubmissionKey exists only for the first prompt submitted from a draft surface.
  // pendingFirstSubmissionKey 仅存在于从草稿界面提交的首条提示词。
  const pendingFirstSubmissionKey = props.selectedDraftConversation?.pendingSubmission?.clientSubmissionId ?? null;
  if (pendingFirstSubmissionKey) {
    retainedFirstSubmissionKeyRef.current = pendingFirstSubmissionKey;
  }
  // retainedFirstSubmissionKey is accepted after materialization only when the authoritative message carries the same correlation.
  // retainedFirstSubmissionKey 仅在物化后的权威消息携带相同关联时继续生效。
  const retainedFirstSubmissionKey = retainedFirstSubmissionKeyRef.current;
  const firstSubmissionTransitionKey = pendingFirstSubmissionKey
    ?? (retainedFirstSubmissionKey
      && hasAuthoritativeSubmissionCorrelation(messages, retainedFirstSubmissionKey)
      ? retainedFirstSubmissionKey
      : null);
  // transientReasoningStatusVisible tracks insertion and removal; the shared ResizeObserver owns node reflow.
  // transientReasoningStatusVisible 跟踪插入与移除；共享 ResizeObserver 负责节点重排。
  const transientReasoningStatusVisible = transientReasoningStatus !== null;
  const displayedStreamingThinkingBlock = activeThinkingOverlayBlock(thinkingDisplayMode, streamingThinkingBlock);
  // Visual transitions retain empty/overlay surfaces only within the current conversation scope.
  // 视觉过渡仅在当前会话作用域内保留空状态与浮层。
  const {
    visibleThinkingBlock: visibleStreamingThinkingOverlayBlock,
    thinkingOverlayLeaving: streamingThinkingOverlayLeaving,
    emptyPromptVisible,
    emptyPromptLeaving,
    firstSubmissionMaskVisible,
    firstSubmissionMaskLeaving,
  } = useConversationVisualTransitions({
    surfaceKey: visualSurfaceKey,
    isEmptyConversation,
    displayedThinkingBlock: displayedStreamingThinkingBlock,
    firstSubmissionTransitionKey,
    firstSubmissionMaterializing: isMaterializingConversationSurface,
  });
  // projectedHiddenThinkingBlockIds contains the freshly projected hidden thinking identities.
  // projectedHiddenThinkingBlockIds 包含本次新投影出的隐藏思考身份。
  const projectedHiddenThinkingBlockIds = React.useMemo(
    () => Array.from(hiddenThinkingBlockIdsForDisplayMode(
      thinkingDisplayMode,
      renderItems,
      visibleStreamingThinkingOverlayBlock?.id ?? null,
    )).sort(),
    [thinkingDisplayMode, renderItems, visibleStreamingThinkingOverlayBlock?.id],
  );
  // hiddenThinkingSignature is a stable change marker for restoring scroll after filter changes.
  // hiddenThinkingSignature 是过滤变化后恢复滚动位置的稳定变更标记。
  const hiddenThinkingSignature = JSON.stringify(projectedHiddenThinkingBlockIds);
  // hiddenThinkingBlockIds preserves Set identity while the projected values remain equal.
  // hiddenThinkingBlockIds 在投影值保持相等时保留 Set 身份。
  const hiddenThinkingBlockIds = React.useMemo(
    () => new Set<string>(JSON.parse(hiddenThinkingSignature) as string[]),
    [hiddenThinkingSignature],
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
  // readConversationScrollSnapshot returns the exact saved semantic viewport for one surface.
  // readConversationScrollSnapshot 返回单个界面精确保存的语义视口。
  const readConversationScrollSnapshot = React.useCallback((surfaceId: string) => (
    readConversationSurfaceLayoutState(surfaceId)?.scroll ?? null
  ), [readConversationSurfaceLayoutState]);
  // handleConversationFollowTailChange synchronizes the controller ownership state with the visible affordance.
  // handleConversationFollowTailChange 将控制器所有权状态与可见操作入口同步。
  const handleConversationFollowTailChange = React.useCallback((followTail: boolean) => {
    setIsAtConversationBottom(followTail);
  }, []);
  // conversationScrollController is the sole owner of main-transcript scroll position and surface restoration.
  // conversationScrollController 是主转录滚动位置与界面恢复的唯一所有者。
  const conversationScrollController = useConversationScrollController({
    active: Boolean(activeSurfaceConversationId && !isEmptyConversation && !showConversationLoadingOnly),
    onFollowTailChange: handleConversationFollowTailChange,
    readSnapshot: readConversationScrollSnapshot,
    scrollRef: workspaceRef,
    surfaceId: activeSurfaceConversationId ?? null,
    writeSnapshot: writeSurfaceScrollSnapshot,
  });

  // A structural stream change keeps explicit tail ownership or restores the reader's semantic anchor.
  // 结构性流变化会保持显式尾部所有权，或恢复阅读者的语义锚点。
  React.useLayoutEffect(() => {
    conversationScrollController.scheduleStoredRestore();
  }, [conversationScrollController, conversationStreamSignature]);

  // Adding or removing the timeline-tail status follows the current semantic scroll ownership.
  // 在时间线尾部增删状态行时遵循当前语义滚动所有权。
  React.useLayoutEffect(() => {
    conversationScrollController.scheduleStoredRestore();
  }, [conversationScrollController, transientReasoningStatusVisible]);

  // A thinking-filter or external layout key change is restored from the latest pre-change semantic snapshot.
  // 思考过滤或外部布局键变化会从最近一次变化前语义快照恢复。
  React.useLayoutEffect(() => {
    conversationScrollController.scheduleStoredRestore();
  }, [conversationScrollController, hiddenThinkingSignature, props.layoutStabilityKey]);

  // handleOpenSubagentPanelFromTimeline preserves the current scroll before opening a child transcript.
  // handleOpenSubagentPanelFromTimeline 在打开子代理转录前保留当前滚动位置。
  const handleOpenSubagentPanelFromTimeline = React.useCallback((
    parentConversationId: string,
    taskId: string,
  ) => {
    conversationScrollController.remember();
    return props.onOpenSubagentPanel(parentConversationId, taskId);
  }, [conversationScrollController, props.onOpenSubagentPanel]);

  // scrollConversationToBottom transfers the main viewport to explicit tail ownership.
  // scrollConversationToBottom 将主视口切换到显式尾部所有权。
  const scrollConversationToBottom = React.useCallback((behavior: ScrollBehavior = "smooth") => {
    conversationScrollController.scrollToBottom(behavior);
  }, [conversationScrollController]);

  // handleComposerDockHeightChange captures the reader anchor before composer geometry changes.
  // handleComposerDockHeightChange 在输入区几何变化前捕获阅读者锚点。
  const handleComposerDockHeightChange = React.useCallback((height: number) => {
    conversationScrollController.captureAndScheduleLayoutRestore();
    setComposerDockHeight((current) => current === height ? current : height);
  }, [conversationScrollController]);

  // Composer geometry uses the same single-frame semantic restoration path as transcript reflow.
  // 输入区几何与转录重排使用相同的单帧语义恢复路径。
  React.useLayoutEffect(() => {
    conversationScrollController.scheduleStoredRestore();
  }, [composerDockHeight, conversationScrollController]);

  const {
    handlePlanArtifactCancel,
    handlePlanArtifactExecute,
    handlePlanArtifactSuggest,
  } = useChatAreaPlanActions({
    conversationId: props.selectedConversationId,
    executeLabel: t("planArtifact.execute"),
    locale,
    onSetSessionPlanMode: props.onSetSessionPlanMode,
    onSubmitPrompt: props.onSubmitPrompt,
    pendingPlanArtifact,
    reasoningEffort: props.workbench.reasoningSelection?.selectedEffort ?? null,
  });

  // A blocking decision removes the Composer; the height effect restores the last pre-change semantic snapshot.
  // 阻塞式决策移除输入区；高度 effect 会恢复最近一次变化前语义快照。
  React.useLayoutEffect(() => {
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
        parentConversationId={props.selectedConversationId ?? null}
        workspacePath={selectedProject?.path ?? null}
        commandSessions={visibleCommandSessions}
        pendingPlanArtifact={pendingPlanArtifact}
        onPlanArtifactExecute={handlePlanArtifactExecute}
        onPlanArtifactSuggest={handlePlanArtifactSuggest}
        onPlanArtifactCancel={handlePlanArtifactCancel}
        onPreserveHistoryPrepend={conversationScrollController.preserveHistoryPrepend}
        onScrollStateChange={conversationScrollController.handleScroll}
        onOpenSubagentPanel={handleOpenSubagentPanelFromTimeline}
        scrollRef={workspaceRef}
        initialExpandedState={activeSurfaceLayoutState?.expandedState ?? null}
        onExpandedStateChange={handleExpandedStateChange}
        transientReasoningStatus={transientReasoningStatus ? (
          <TransientReasoningStatusLine status={transientReasoningStatus} />
        ) : null}
         footer={selectedConversation
           && continuation
           && !conversationIsRunning(selectedConversation)
           && props.onContinueConversation ? (
              <ConversationContinueControl
                blockedLabel={t("conversation.continuation.blocked")}
                blockedReason={conversationContinuationBlockedReasonText(continuation.blockedReason, t)}
                claiming={continuationClaimKey === activeContinuationKey}
                continueLabel={t("conversation.continue")}
                continuingLabel={t("conversation.continuing")}
                eligible={continuation.eligible}
                feedback={continuationFeedback?.key === activeContinuationKey
                  && continuationFeedback.kind !== "claiming" ? {
                    kind: continuationFeedback.kind,
                    label: continuationFeedback.kind === "failed"
                      ? t("conversation.continuation.claimFailed")
                      : t("conversation.continuing"),
                  } : null}
                onContinue={() => void handleContinueConversation()}
              />
            ) : null}
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
      {firstSubmissionMaskVisible ? (
        <div
          aria-hidden="true"
          className={cn(
            "conversation-first-submission-mask",
            firstSubmissionMaskLeaving && "leaving",
          )}
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
