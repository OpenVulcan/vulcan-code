import React from "react";
import { createClientOwnedActionId } from "@/notifications/client-identity";
import { AnswerReplyContext, AnswerReplyStore } from "@/components/conversation/answer-components/answer-reply";
import { ChatAreaComposerDockLayer } from "@/components/conversation/chat-area/composer-dock-layer";
import {
  ConversationLoadingCenter,
  ConversationSubmissionStatus,
  DraftSubmissionTranscript,
} from "@/components/conversation/conversation-chrome";
import {
  ConversationBranchWorkspaceV1,
  type ConversationBranchGenerationActionV1,
} from "@/components/conversation/conversation-branch-workspace-v1";
import {
  ConversationGenerationDialogV1,
  type ConversationGenerationDialogActionV1,
} from "@/components/conversation/conversation-generation-dialog-v1";
import {
  StreamingThinkingOverlay,
  TransientReasoningStatusLine,
} from "@/components/conversation/conversation-live-reasoning";
import { ProviderRetryCountdownLine } from "@/components/conversation/conversation-provider-retry-card";
import {
  conversationIsRunning,
  conversationRuntimeCue,
  normalizedSessionRunPhase,
} from "@/components/conversation/conversation-runtime";
import type {
  ConversationBlock,
} from "@/components/conversation/conversation-model";
import type { ComposerSubmission } from "@/components/composer/composer-model";
import { useI18n, type Translate } from "@/i18n";
import { selectConversationSessionTodolistV1 } from "@/service/conversation-session-tools-v1";
import { projectForConversation } from "@/state/workbench-selectors";
import {
  useConversationProviderRetryStatus,
  useConversationTransientReasoningStatus,
} from "@/state/conversation-live-stream-context";
import { toast } from "@/ui-kit/primitives/toast";
import type { ChatAreaProps } from "@/components/conversation/chat-area/types";
import { useChatAreaPlanActions } from "@/components/conversation/chat-area/plan-actions";
import {
  useConversationVisualTransitions,
  useThinkingDisplayMode,
} from "@/components/conversation/chat-area/visual-state";
import { useConversationSurfaceLayoutMemory } from "@/components/conversation/chat-area/surface-layout-memory";
import { useConversationScrollController } from "@/components/conversation/chat-area/conversation-scroll-controller";
import { ConversationContinueControl } from "@/components/conversation/conversation-continue-control";
import { activeThinkingOverlayBlock } from "@/components/conversation/thinking-display";
import { ConversationFindBar } from "@/components/conversation/conversation-find-bar";
import { selectConversationContinueCandidateV1 } from "@/service/conversation-continuation-selector-v1";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import type {
  ConversationEventSummary,
} from "@/service/conversation-event-window-store";
import {
  useSelectedConversationEventWindowCommands,
  useSelectedConversationEventWindowSnapshot,
} from "@/state/conversation-event-window-react";

/** Process-local target for one exact V1 edit or fork action.
 * 一次精确 V1 编辑或分叉动作的进程内目标。
 */
type ConversationGenerationTargetV1 = ConversationBranchGenerationActionV1 & {
  /** Stable child identity reused by every fork retry from this dialog.
   * 当前对话框中每次分叉重试复用的稳定子会话身份。
   */
  childSessionId: string | null;
};

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
  // Thinking display state synchronizes the backend preference with stale-safe optimistic selection.
  // 思考展示状态将后端偏好与过期安全的乐观选择保持同步。
  const {
    mode: thinkingDisplayMode,
    selectMode: handleThinkingDisplayModeChange,
  } = useThinkingDisplayMode({
    authoritativeMode: props.workbench.thinkingDisplayMode,
    conversationId: props.selectedConversationId,
  });
  const selectedConversation = props.workbench.conversations.find((conversation) => conversation.id === props.selectedConversationId);
  // eventWindowSnapshot is the sole durable authority for Continue eligibility and consumption.
  // eventWindowSnapshot 是 Continue 资格与消费状态的唯一可靠权威。
  const eventWindowSnapshot = useSelectedConversationEventWindowSnapshot();
  // sessionTodolist is projected from the same durable V1 generation as the visible transcript.
  // sessionTodolist 从与可见转录相同的可靠 V1 代次投影。
  const sessionTodolist = React.useMemo(
    () => eventWindowSnapshot.owner?.session_id === selectedConversation?.id
      ? selectConversationSessionTodolistV1(eventWindowSnapshot)
      : null,
    [eventWindowSnapshot, selectedConversation?.id],
  );
  // eventWindowCommands stays bound to the exact owner generation represented by the selected store.
  // eventWindowCommands 始终绑定到选中 store 所表示的精确归属代次。
  const eventWindowCommands = useSelectedConversationEventWindowCommands();
  // selectedConversationIdRef prevents a completed action from retargeting a newly selected owner.
  // selectedConversationIdRef 防止已完成动作重新定向到新选中的归属。
  const selectedConversationIdRef = React.useRef(props.selectedConversationId);
  selectedConversationIdRef.current = props.selectedConversationId;
  // generationTarget owns the exact seq and stable fork child identity until success or cancellation.
  // generationTarget 在成功或取消前持有精确 seq 与稳定分叉子会话身份。
  const [generationTarget, setGenerationTarget] = React.useState<ConversationGenerationTargetV1 | null>(null);
  // generationSubmitting blocks duplicate dialog submissions while the durable boundary is in flight.
  // generationSubmitting 在可靠边界执行期间阻止重复提交对话框。
  const [generationSubmitting, setGenerationSubmitting] = React.useState(false);
  // continuationCandidate identifies the latest eligible physical API in the selected V1 open Turn.
  // continuationCandidate 标识选中 V1 开放 Turn 中最近的可继续物理 API。
  const continuationCandidate = React.useMemo(
    () => selectConversationContinueCandidateV1(eventWindowSnapshot),
    [eventWindowSnapshot],
  );
  // activeContinuationKey is UI-only and never becomes backend authorization.
  // activeContinuationKey 仅供界面使用，绝不会成为后端授权信息。
  const activeContinuationKey = selectedConversation
    && continuationCandidate?.session_id === selectedConversation.id
    ? continuationCandidate.ui_identity
    : null;
  // continuationClaimKey suppresses duplicate claims until the authoritative V1 candidate identity changes.
  // continuationClaimKey 在权威 V1 候选身份变化前阻止重复领取 Continue。
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
  React.useEffect(() => {
    if (continuationClaimKey !== activeContinuationKey) {
      setContinuationClaimKey(null);
    }
    if (continuationFeedback && continuationFeedback.key !== activeContinuationKey) {
      setContinuationFeedback(null);
    }
  }, [activeContinuationKey, continuationClaimKey, continuationFeedback]);
  React.useEffect(() => {
    setGenerationTarget(null);
    setGenerationSubmitting(false);
  }, [props.selectedConversationId]);

  /** Opens one generation dialog from a canonical root-input event.
   * 从一条 canonical 根输入事件打开 generation 对话框。
   */
  const handleRootInputGenerationAction = React.useCallback((
    action: ConversationBranchGenerationActionV1,
  ) => {
    setGenerationTarget({
      ...action,
      childSessionId: action.kind === "fork" ? crypto.randomUUID() : null,
    });
  }, []);

  /** Commits the currently frozen edit or fork boundary exactly once.
   * 精确一次提交当前冻结的编辑或分叉边界。
   */
  const handleConfirmGenerationAction = React.useCallback(async (value: string) => {
    const target = generationTarget;
    const owner = eventWindowSnapshot.owner;
    const session = selectedConversation;
    if (
      target === null
      || owner === null
      || session === undefined
      || owner.session_id !== session.id
    ) {
      return;
    }
    setGenerationSubmitting(true);
    try {
      if (target.kind === "edit") {
        const edited = await props.onEditConversationGeneration(
          session.id,
          owner.window_generation,
          target.targetSeq,
          value,
        );
        if (!edited) return;
        if (selectedConversationIdRef.current === session.id) {
          const hydrated = await eventWindowCommands.rehydrate();
          if (!hydrated) {
            throw new Error("ERR_CONVERSATION_EDIT_GENERATION_REHYDRATE_FAILED");
          }
        }
      } else {
        const childSessionId = target.childSessionId;
        if (childSessionId === null) {
          throw new Error("ERR_CONVERSATION_FORK_CHILD_ID_MISSING");
        }
        const forked = await props.onForkConversationGeneration(
          session.id,
          owner.window_generation,
          target.sourceTurnEndSeq,
          childSessionId,
          value,
        );
        if (forked === null) return;
      }
      setGenerationTarget((current) => current === target ? null : current);
    } catch (nextError) {
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      toast.error(message);
    } finally {
      setGenerationSubmitting(false);
    }
  }, [
    eventWindowCommands,
    eventWindowSnapshot.owner,
    generationTarget,
    props,
    selectedConversation,
  ]);

  // generationDialogAction contains only the editable field; seq and identities remain frozen outside it.
  // generationDialogAction 仅包含可编辑字段；seq 与身份继续在其外部冻结。
  const generationDialogAction = React.useMemo<ConversationGenerationDialogActionV1 | null>(
    () => generationTarget
      ? generationTarget.kind === "edit"
        ? { kind: "edit", text: generationTarget.text }
        : {
            kind: "fork",
            title: `${selectedConversation?.title ?? ""}${t("conversation.action.forkTitleSuffix")}`,
          }
      : null,
    [generationTarget, selectedConversation?.title, t],
  );
  // handleContinueConversation resumes one exact V1 Turn with an optional in-Turn supplement.
  // handleContinueConversation 使用可选的 Turn 内补充输入继续一个精确 V1 Turn。
  const handleContinueConversation = React.useCallback(async (
    supplement: ComposerSubmission | null,
  ): Promise<boolean> => {
    if (
      !selectedConversation
      || continuationCandidate?.session_id !== selectedConversation.id
      || !props.onContinueConversation
      || activeContinuationKey === null
      || continuationClaimKey === activeContinuationKey
    ) {
      return false;
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
        ?? createClientOwnedActionId();
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
        clientContinuationId,
        supplement,
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
    return accepted;
  }, [
    activeContinuationKey,
    continuationCandidate,
    continuationClaimKey,
    props.onContinueConversation,
    selectedConversation,
    t,
  ]);
  // handleComposerSubmit keeps an interrupted Turn open when the user supplies a supplement.
  // handleComposerSubmit 在用户提供补充内容时保持中断的 Turn 不变。
  const handleComposerSubmit = React.useCallback((submission: ComposerSubmission): Promise<boolean> => {
    if (activeContinuationKey !== null) {
      return handleContinueConversation(submission);
    }
    return props.onSubmitPrompt(submission);
  }, [activeContinuationKey, handleContinueConversation, props.onSubmitPrompt]);
  const subagentNavigation = props.workbench.subagentNavigation?.parentConversationId === props.selectedConversationId
    ? props.workbench.subagentNavigation
    : null;
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
    writeExpandedState: writeSurfaceExpandedState,
    writeScroll: writeSurfaceScrollSnapshot,
  } = useConversationSurfaceLayoutMemory({ liveSurfaceIds: liveLayoutSurfaceIds });
  const surfaceConversation = selectedConversation;
  // eventWindowOwnerReady permits rendering only when the selected durable session owns the V1 snapshot.
  // eventWindowOwnerReady 仅在选中的持久化会话拥有 V1 快照时允许渲染。
  const eventWindowOwnerReady = Boolean(
    selectedConversation
    && eventWindowSnapshot.owner?.session_id === selectedConversation.id,
  );
  // authoritativeEvents is empty during owner handoff and never falls back to legacy messages.
  // authoritativeEvents 在归属切换期间保持为空，绝不回退到旧 messages。
  const authoritativeEvents = eventWindowOwnerReady ? eventWindowSnapshot.events : [];
  // pendingSubmissionVisible remains true only until the exact authoritative user record replaces the optimistic row.
  // pendingSubmissionVisible 仅在精确权威用户记录替代乐观行之前保持为真。
  const pendingSubmissionVisible = Boolean(
    props.selectedDraftConversation?.pendingSubmission
    && !eventWindowHasAuthoritativeSubmissionCorrelation(
      authoritativeEvents,
      props.selectedDraftConversation.pendingSubmission.clientSubmissionId,
    ),
  );
  // optimisticUserInput is process-local and disappears only after exact V1 ClientActionId reconciliation.
  // optimisticUserInput 仅属于进程内，并且只在精确 V1 ClientActionId 对账后消失。
  const optimisticUserInput = props.selectedDraftConversation?.pendingSubmission && pendingSubmissionVisible
    ? {
      clientActionId: props.selectedDraftConversation.pendingSubmission.clientSubmissionId,
      text: props.selectedDraftConversation.pendingSubmission.prompt,
      attachmentNames: props.selectedDraftConversation.pendingSubmission.attachments.map(
        (attachment) => attachment.fileName,
      ),
    }
    : null;
  // showConversationLoadingOnly covers only the exact V1 owner handoff for a persisted selection.
  // showConversationLoadingOnly 仅覆盖持久化选择的精确 V1 归属切换阶段。
  const showConversationLoadingOnly = Boolean(selectedConversation && !eventWindowOwnerReady && !props.selectedDraftConversation?.pendingSubmission);
  const selectedProject = selectedConversation ? projectForConversation(props.workbench, selectedConversation) : null;
  const emptyProjectName = props.selectedDraftConversation?.projectName ?? selectedProject?.name ?? t("conversation.unlinkedProject");
  const isEmptyConversation = Boolean(
    (props.selectedDraftConversation && !props.selectedDraftConversation.pendingSubmission)
      || (selectedConversation && eventWindowOwnerReady && !props.selectedDraftConversation?.pendingSubmission && !eventWindowHasVisibleTranscript(authoritativeEvents)),
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
  // answerReplyStore keeps the reader's decisions and comments on answer components until the composer takes them;
  // each conversation surface starts with an empty store.
  // answerReplyStore 保存读者在回答组件上的决策与批注，直到输入区取走；每个会话界面都从空存储开始。
  const answerReplyStore = React.useMemo(() => new AnswerReplyStore(), [visualSurfaceKey]);
  // transientReasoningStatus is request-local and never enters the stable Workbench snapshot.
  // transientReasoningStatus 仅属于当前请求，绝不进入稳定 Workbench 快照。
  const transientReasoningStatus = useConversationTransientReasoningStatus(
    activeSurfaceConversationId,
  );
  // providerRetryStatus is the request-local wait before the next automatic Provider retry.
  // providerRetryStatus 是下一次 Provider 自动重试前的请求局部等待。
  const providerRetryStatus = useConversationProviderRetryStatus(activeSurfaceConversationId);
  // Canonical correlation retires only this accepted first prompt after it is already renderable.
  // 规范关联只在该首条提示词已经可渲染后移除同一乐观记录。
  React.useEffect(() => {
    const pending = props.selectedDraftConversation?.pendingSubmission;
    if (selectedConversation && pending && eventWindowOwnerReady && !pendingSubmissionVisible) {
      props.onFirstSubmissionReconciled?.(selectedConversation.id, pending.clientSubmissionId);
    }
  }, [eventWindowOwnerReady, pendingSubmissionVisible, props.onFirstSubmissionReconciled, props.selectedDraftConversation?.pendingSubmission, selectedConversation]);
  // transientReasoningStatusVisible tracks insertion and removal; the shared ResizeObserver owns node reflow.
  // transientReasoningStatusVisible 跟踪插入与移除；共享 ResizeObserver 负责节点重排。
  const transientReasoningStatusVisible = transientReasoningStatus !== null || providerRetryStatus !== null;
  // streamingReasoningBlock is derived only from the latest canonical V1 streaming reasoning event.
  // streamingReasoningBlock 仅从最新 canonical V1 流式思考事件派生。
  const streamingReasoningBlock = React.useMemo(
    () => latestStreamingReasoningBlock(authoritativeEvents, t),
    [authoritativeEvents, t],
  );
  // displayedStreamingReasoningBlock drives the popup modes without reading legacy transcript rows.
  // displayedStreamingReasoningBlock 在不读取旧转录行的情况下驱动浮层模式。
  const displayedStreamingReasoningBlock = activeThinkingOverlayBlock(
    thinkingDisplayMode,
    streamingReasoningBlock,
  );
  // Visual transitions retain empty, popup, and first-submission surfaces within the current conversation scope.
  // 视觉过渡在当前会话作用域内保留空状态、思考浮层与首条提交界面。
  const {
    visibleThinkingBlock: visibleStreamingThinkingOverlayBlock,
    thinkingOverlayLeaving: streamingThinkingOverlayLeaving,
  } = useConversationVisualTransitions({
    surfaceKey: visualSurfaceKey,
    displayedThinkingBlock: displayedStreamingReasoningBlock,
  });
  // reasoningDisplaySignature is a stable scroll-restoration marker for V1 reasoning filtering.
  // reasoningDisplaySignature 是 V1 思考过滤使用的稳定滚动恢复标记。
  const reasoningDisplaySignature = `${thinkingDisplayMode}:${eventWindowSnapshot.owner?.window_generation ?? "none"}`;
  // hasRunningSubagents reflects the backend-routed subagent lane rather than local UI inference.
  // hasRunningSubagents 反映后端路由的子代理通道，而不是本地界面推断。
  const hasRunningSubagents = subagentNavigation?.tasks?.some((task) => task.isRunning || task.runState === "running") ?? false;
  // suppressSubagentOnlyMainCue keeps delegated work in the footer instead of the main request cue.
  // suppressSubagentOnlyMainCue 将纯子代理工作保留在底部栏，不显示为主窗口请求提示。
  const suppressSubagentOnlyMainCue = hasRunningSubagents
    && normalizedSessionRunPhase(surfaceConversation?.runPhase) === "toolrunning";
  const runtimeCue = visibleStreamingThinkingOverlayBlock || suppressSubagentOnlyMainCue
    ? null
    : conversationRuntimeCue(surfaceConversation, t);
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
    () => buildConversationEventWindowStreamSignature(eventWindowSnapshot),
    [eventWindowSnapshot],
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
  }, [conversationScrollController, props.layoutStabilityKey, reasoningDisplaySignature]);

  // handleOpenSubagentPanelFromTimeline preserves the current scroll before opening a child transcript.
  // handleOpenSubagentPanelFromTimeline 在打开子代理转录前保留当前滚动位置。
  const handleOpenSubagentPanelFromTimeline = React.useCallback((
    parentConversationId: string,
    taskId: string,
  ) => {
    conversationScrollController.remember();
    return props.onOpenSubagentPanel(parentConversationId, taskId);
  }, [conversationScrollController, props.onOpenSubagentPanel]);
  // revealFindMatch brings a find match into view without smooth scrolling, so stepping through matches keeps up.
  // revealFindMatch 不经平滑滚动地显示查找匹配，使逐个浏览匹配时跟得上。
  const revealFindMatch = React.useCallback((range: Range) => {
    conversationScrollController.revealAnchor(range, "auto");
  }, [conversationScrollController]);
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
      startProjectName={emptyProjectName}
      isConversationRunning={isConversationRunning}
      showScrollToBottom={!isEmptyConversation && !isAtConversationBottom}
      runtimeCue={runtimeCue}
      runtimeStatusHost={props.runtimeStatusHost ?? null}
      todolistStatusHost={props.todolistStatusHost ?? null}
      sessionTodolist={sessionTodolist}
      thinkingDisplayMode={thinkingDisplayMode}
      sessionReviewSummary={props.sessionReviewSummary}
      onComposerDockHeightChange={handleComposerDockHeightChange}
      onScrollToBottom={() => scrollConversationToBottom()}
      onThinkingDisplayModeChange={handleThinkingDisplayModeChange}
      onSubmitPrompt={handleComposerSubmit}
    />
  );
  // activeSurfaceLayoutState reuses one surface-scoped layout snapshot for this render.
  // activeSurfaceLayoutState 在本次渲染中复用一个界面级布局快照。
  const activeSurfaceLayoutState = activeSurfaceConversationId
    ? readConversationSurfaceLayoutState(activeSurfaceConversationId)
    : null;
  // handleExpandedStateChange stores branch process, prompt, and tool-card expansion state per session.
  // handleExpandedStateChange 按会话保存分支处理过程、提示词与工具卡展开状态。
  const handleExpandedStateChange = React.useCallback((
    surfaceId: string,
    state: Parameters<typeof writeSurfaceExpandedState>[1],
  ): void => {
    writeSurfaceExpandedState(surfaceId, state);
  }, [writeSurfaceExpandedState]);
  // conversationSurfaceContent keeps transcript, overlays, and composer in one adaptive content coordinate space.
  // conversationSurfaceContent 将转录、浮层与输入区保留在同一个自适应内容坐标空间内。
  const conversationSurfaceContent = (
    <>
      {selectedConversation ? (
        <ConversationBranchWorkspaceV1
          collaborationNavigation={{
            conversations: props.workbench.conversations,
            navigation: subagentNavigation,
            onSelectConversation: props.onSelectConversation,
            onOpenSubagentPanel: handleOpenSubagentPanelFromTimeline,
          }}
          providers={props.workbench.providers}
          commandSessions={props.workbench.commandSessions ?? []}
          commands={eventWindowCommands}
          conversation={selectedConversation}
          footer={continuationCandidate?.session_id === selectedConversation.id
            && !conversationIsRunning(selectedConversation)
            && props.onContinueConversation ? (
              <ConversationContinueControl
                blockedLabel={t("conversation.continuation.blocked")}
                blockedReason=""
                claiming={continuationClaimKey === activeContinuationKey}
                continueLabel={t("conversation.continue")}
                continuingLabel={t("conversation.continuing")}
                eligible
                feedback={continuationFeedback?.key === activeContinuationKey
                  && continuationFeedback.kind !== "claiming" ? {
                    kind: continuationFeedback.kind,
                    label: continuationFeedback.kind === "failed"
                      ? t("conversation.continuation.claimFailed")
                      : t("conversation.continuing"),
                  } : null}
                onContinue={() => void handleContinueConversation(null)}
              />
            ) : null}
          onCopyText={props.onCopyText}
          onExpandedStateChange={handleExpandedStateChange}
          onOpenSubagentPanel={handleOpenSubagentPanelFromTimeline}
          onRootInputGenerationAction={handleRootInputGenerationAction}
          optimisticUserInput={optimisticUserInput}
          onPlanArtifactCancel={handlePlanArtifactCancel}
          onPlanArtifactExecute={handlePlanArtifactExecute}
          onPlanArtifactSuggest={handlePlanArtifactSuggest}
          onPreserveHistoryPrepend={conversationScrollController.preserveHistoryPrepend}
          onRevealPromptAnchor={conversationScrollController.revealAnchor}
          onScrollStateChange={conversationScrollController.handleScroll}
          pendingPlanArtifact={pendingPlanArtifact}
          initialExpandedState={activeSurfaceLayoutState?.expandedState ?? null}
          reasoningDisplayMode={thinkingDisplayMode}
          scrollRef={workspaceRef}
          sessionId={selectedConversation.id}
          sessionRunning={conversationIsRunning(surfaceConversation)}
          snapshot={eventWindowSnapshot}
          suppressEmptyPlaceholder={isEmptyConversation || isMaterializingConversationSurface}
          transientReasoningStatus={providerRetryStatus ? (
            <ProviderRetryCountdownLine countdown={providerRetryStatus} />
          ) : transientReasoningStatus ? (
            <TransientReasoningStatusLine status={transientReasoningStatus} />
          ) : null}
          workspacePath={selectedProject?.path ?? null}
          onTrace={props.onTrace}
        />
      ) : null}
      {props.selectedDraftConversation?.pendingSubmission && pendingSubmissionVisible ? (
        <ConversationSubmissionStatus stage={props.selectedDraftConversation.pendingSubmission.stage} />
      ) : null}
      {!selectedConversation && props.selectedDraftConversation?.pendingSubmission ? (
        <DraftSubmissionTranscript
          scrollRef={workspaceRef}
          submission={props.selectedDraftConversation.pendingSubmission}
          surfaceId={props.selectedDraftConversation.id}
        />
      ) : null}
      {selectedConversation ? (
        <ConversationFindBar
          key={selectedConversation.id}
          onReveal={revealFindMatch}
          scrollRef={workspaceRef}
        />
      ) : null}
      {visibleStreamingThinkingOverlayBlock && activeSurfaceConversationId ? (
        <StreamingThinkingOverlay
          block={visibleStreamingThinkingOverlayBlock}
          conversationId={activeSurfaceConversationId}
          leaving={streamingThinkingOverlayLeaving}
        />
      ) : null}
      {composerDock}
      <ConversationGenerationDialogV1
        action={generationDialogAction}
        onCancel={() => {
          if (!generationSubmitting) setGenerationTarget(null);
        }}
        onConfirm={handleConfirmGenerationAction}
        submitting={generationSubmitting}
      />
    </>
  );
  return (
    <AnswerReplyContext.Provider value={answerReplyStore}>
      <div
        className="vk-chat"
        data-empty={isEmptyConversation ? "" : undefined}
        data-loading={showConversationLoadingOnly ? "" : undefined}
        style={{ "--composer-dock-height": `${composerDockHeight}px` } as React.CSSProperties}
      >
        {showConversationLoadingOnly ? (
          <ConversationLoadingCenter />
        ) : (
          conversationSurfaceContent
        )}
      </div>
    </AnswerReplyContext.Provider>
  );
}

/** Reports whether one V1 UserInput already owns the browser submission identity.
 * 判断一条 V1 UserInput 是否已经拥有浏览器提交身份。
 *
 * @param events canonical selected-session events.
 *               当前选中会话的 canonical 事件。
 * @param clientActionId exact browser idempotency identity.
 *                       精确的浏览器幂等身份。
 * @returns true only for one exact persisted UserInput match.
 *          仅在存在精确持久化 UserInput 匹配时返回 true。 */
function eventWindowHasAuthoritativeSubmissionCorrelation(
  events: readonly ConversationEventSummary[],
  clientActionId: string,
): boolean {
  return events.some((event) => (
    event.event.kind === "user_input"
    && event.event.client_action_id === clientActionId
  ));
}

/** Reports whether a canonical V1 window contains visible transcript content.
 * 判断一个 canonical V1 窗口是否包含可见转录内容。
 *
 * @param events canonical selected-session events.
 *               当前选中会话的 canonical 事件。
 * @returns true when at least one user, assistant, tool, or durable display fact exists.
 *          存在至少一条用户、assistant、工具或可靠显示事实时返回 true。 */
function eventWindowHasVisibleTranscript(events: readonly ConversationEventSummary[]): boolean {
  return events.some((summary) => {
    if (summary.visibility === "host_only") {
      return false;
    }
    switch (summary.event.kind) {
      case "user_input":
      case "assistant_reasoning":
      case "assistant_text":
      case "tool_call":
      case "display_note":
      case "request_config_changed":
      case "subagent_return":
        return true;
      default:
        return false;
    }
  });
}

/** Projects the latest actively streaming V1 reasoning event into the established popup shell.
 * 将最新活动流式 V1 思考事件投影到既有浮层外壳。
 *
 * @param events canonical selected-session events.
 *               当前选中会话的 canonical 事件。
 * @returns one presentation-only block, or null when no reasoning stream is active.
 *          一条仅用于展示的块；没有活动思考流时返回 null。 */
function latestStreamingReasoningBlock(
  events: readonly ConversationEventSummary[],
  t: Translate,
): ConversationBlock | null {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    // summary is inspected in reverse only to select the latest fact; no display order is changed.
    // summary 仅为选择最新事实而逆向检查；不会改变任何显示顺序。
    const summary = events[index];
    if (summary.event.kind !== "assistant_reasoning" || summary.event.status !== "streaming") {
      continue;
    }
    return {
      id: `v1-reasoning:${summary.seq}:${summary.event_revision}`,
      kind: "thinking",
      title: t("conversation.block.thinking"),
      summary: t("conversation.block.thinkingActive"),
      body: summary.event.text,
      status: summary.event.status,
    };
  }
  return null;
}

/** Builds a structure-only V1 stream signature without reading cumulative event bodies.
 * 在不读取累计事件正文的情况下构建仅结构性的 V1 流签名。
 *
 * @param snapshot atomic controller snapshot.
 *                 原子控制器快照。
 * @returns stable owner, frontier, and per-event revision identity.
 *          稳定的归属、前沿与逐事件修订身份。 */
function buildConversationEventWindowStreamSignature(
  snapshot: ConversationEventWindowControllerSnapshot,
): string {
  // eventRevisions captures same-seq streaming updates without hashing large text bodies.
  // eventRevisions 捕获同 seq 流式更新，同时不散列大段文本正文。
  const eventRevisions = snapshot.events
    .map((event) => `${event.seq}:${event.event_revision}`)
    .join(",");
  return [
    snapshot.owner?.session_id ?? "none",
    snapshot.owner?.window_generation ?? "none",
    snapshot.memory_revision ?? "none",
    snapshot.durable_revision ?? "none",
    snapshot.has_more ? "more" : "complete",
    eventRevisions,
  ].join("|");
}
