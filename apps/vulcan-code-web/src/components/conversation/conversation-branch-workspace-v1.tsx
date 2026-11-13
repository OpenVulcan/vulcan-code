import * as React from "react";
import { useConversationToolRequestMembership } from "@/state/conversation-live-stream-context";
import { ConversationAgentInteractionActionsContext } from "./conversation-collaboration-card";
import { conversationV1SubagentNames, createConversationV1CollaborationNavigation, type ConversationV1CollaborationNavigation } from "./conversation-collaboration-navigation";
import { ConversationCollaborationStateContext, projectConversationCollaborationState } from "./conversation-collaboration-state";
import { ThinkingPopupContext } from "./thinking-preview";
import { TurnChangeReviewTargetContext, type TurnChangeReviewTarget } from "./turn-change-review-context";
import { resolveLastReviewTurnIdV1 } from "@/diff/turn-change-summary-v1";

import type { GeneratedMediaAttachment, GeneratedMediaKind } from "@/components/media/generated-media";
import {
  completedTurnEndSeqForPresentation,
  createConversationBranchPresentationProjectorV1,
  NO_REQUEST_DRAFTS,
  rootInputPresentationMetadata,
} from "@/components/conversation/conversation-branch-presentation-v1";
import { createConversationRenderItemsProjector } from "@/components/conversation/conversation-transcript";
import { ConversationWorkspace } from "@/components/conversation/conversation-workspace";
import { ConversationHistoryCleanup } from "@/components/conversation/conversation-history-cleanup";
import { useConversationCleanedHistory } from "@/components/conversation/conversation-cleaned-history";
import { DedicatedTranscriptMemoryProvider } from "@/components/conversation/dedicated-transcript-memory";
import type { ConversationSurfaceExpandedState } from "@/components/conversation/conversation-layout-restore";
import type { ConversationPlanArtifact } from "@/components/conversation/conversation-model";
import type { PlanArtifactActionPayload } from "@/components/conversation/conversation-plan-artifact";
import {
  hiddenThinkingBlockIdsForDisplayMode,
  normalizeThinkingDisplayMode,
  type ThinkingDisplayMode,
} from "@/components/conversation/thinking-display";
import type { ToolDetailPayload } from "@/components/conversation/conversation-detail";
import { useI18n } from "@/i18n";
import type { ConversationEventWindowControllerSnapshot } from "@/service/conversation-event-window-controller";
import { selectDedicatedMemory } from "@/service/dedicated-memory";
import { conversationSelectionEqual, useEqualValue, type ConversationEventWindowSnapshotReader } from "@/state/conversation-event-window-react";
import { ConversationTranscriptStreamContext } from "./conversation-transcript-stream";
import type { SelectedConversationEventWindowCommands } from "@/service/selected-conversation-event-window-registry";
import type {
  ConversationEventSummary,
  ResourceId,
} from "@/service/conversation-event-window-store";
import { useConversationSessionResourcesV1 } from "@/state/conversation-session-resources-v1";
import { withStoppedTurnStatsV1 } from "@/service/conversation-stopped-turn-stats-v1";
import type {
  CommandSessionSummary,
  ComposerAttachmentSummary,
  ConversationMessage,
  ConversationSummary,
  ProviderSummary,
} from "@/types";

// ConversationBranchGenerationActionV1 keeps edit and fork anchors structurally distinct.
// ConversationBranchGenerationActionV1 在结构上严格区分编辑锚点与分叉锚点。
export type ConversationBranchGenerationActionV1 =
  | {
    // Edit replaces one canonical root input and restarts its Turn.
    // 编辑替换一条规范根输入并重新启动其回合。
    kind: "edit";
    // Canonical root-input sequence.
    // 规范根输入序号。
    targetSeq: number;
    // Persisted root-input text shown in the generation dialog.
    // 在 generation 对话框中显示的持久化根输入正文。
    text: string;
  }
  | {
    // Fork retains one completed Turn and starts no successor Turn.
    // 分叉保留一个完整回合，且不启动后继回合。
    kind: "fork";
    // Canonical terminal TurnStats seq of the retained Turn.
    // 被保留回合的规范终态 TurnStats seq。
    sourceTurnEndSeq: number;
  };

// ConversationBranchOptimisticUserInputV1 describes one process-local prompt before its canonical V1 event arrives.
// ConversationBranchOptimisticUserInputV1 描述规范 V1 事件到达前的一条进程内提示词。
export interface ConversationBranchOptimisticUserInputV1 {
  // Browser idempotency identity used for exact reconciliation.
  // 用于精确对账的浏览器幂等身份。
  clientActionId: string;
  // User-visible prompt text.
  // 用户可见提示词正文。
  text: string;
  // Ordered display-only attachment names copied from the composer.
  // 从输入区复制的有序纯展示附件名称。
  attachmentNames: readonly string[];
}

// ConversationBranchWorkspaceV1Props binds the restored branch presentation to one authoritative V1 owner.
// ConversationBranchWorkspaceV1Props 把恢复后的分支展示绑定到一个权威 V1 所有者。
export interface ConversationBranchWorkspaceV1Props {
  // Exact direct and embedded routes supplied by the mounted host, never inferred from event text.
  // 已挂载宿主提供的精确直接及嵌入路由，绝不从事件正文推测。
  collaborationNavigation?: ConversationV1CollaborationNavigation;
  // Live provider/model catalog; absence represents an isolated host without configuration.
  // 实时供应商和模型目录；缺失表示不带配置的隔离宿主。
  providers?: readonly ProviderSummary[];
  // Exact selected persisted session identity.
  // 精确选中的持久化会话身份。
  sessionId: string;
  // Current conversation summary used only by established visual chrome.
  // 仅供既有视觉外壳使用的当前会话摘要。
  conversation: ConversationSummary;
  // Atomic selected-session V1 window.
  // 当前选中会话的原子 V1 窗口。
  snapshot: ConversationEventWindowControllerSnapshot;
  // Optional live reader for a surface that subscribes to structural snapshots.
  // 供订阅结构快照的界面使用的可选实时读取器。
  snapshotReader?: ConversationEventWindowSnapshotReader;
  // Commands permanently bound to the same selected owner generation.
  // 永久绑定到同一选中所有者 generation 的命令。
  commands: SelectedConversationEventWindowCommands;
  // Project workspace path used by Markdown and file cards.
  // Markdown 与文件卡使用的项目工作区路径。
  workspacePath?: string | null;
  // Current reasoning display preference.
  // 当前思考展示偏好。
  reasoningDisplayMode: ThinkingDisplayMode;
  // Whether the owning session is executing.
  // 所属会话当前是否正在执行。
  sessionRunning: boolean;
  // Process-local optimistic submission awaiting its exact V1 event.
  // 等待精确 V1 事件的进程内乐观提交。
  optimisticUserInput?: ConversationBranchOptimisticUserInputV1 | null;
  // Current pending Plan artifact.
  // 当前待处理的 Plan 产物。
  pendingPlanArtifact?: ConversationPlanArtifact | null;
  // Executes the pending Plan artifact.
  // 执行待处理 Plan 产物。
  onPlanArtifactExecute?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Sends a revision suggestion for the pending Plan artifact.
  // 为待处理 Plan 产物发送修改建议。
  onPlanArtifactSuggest?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Cancels the pending Plan artifact.
  // 取消待处理 Plan 产物。
  onPlanArtifactCancel?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Live command sessions used by restored terminal cards.
  // 恢复后的终端卡使用的实时命令会话。
  commandSessions: CommandSessionSummary[];
  // Previously captured expansion state for this surface.
  // 当前界面此前捕获的展开状态。
  initialExpandedState: ConversationSurfaceExpandedState | null;
  // Stores process, prompt, and tool-card expansion choices.
  // 保存处理过程、提示词与工具卡的展开选择。
  onExpandedStateChange: (surfaceId: string, state: ConversationSurfaceExpandedState) => void;
  // Preserves the semantic viewport while older events are prepended.
  // 前置更早事件时保留语义视口。
  onPreserveHistoryPrepend: (load: () => Promise<boolean>) => Promise<boolean>;
  // Receives scroll events from the exact transcript element.
  // 接收精确转录元素的滚动事件。
  onScrollStateChange: () => void;
  // Scroll element reference owned by ChatArea.
  // 由 ChatArea 持有的滚动元素引用。
  scrollRef: React.RefObject<HTMLElement | null>;
  // Reveals one rendered turn shell; enables the edge-pinned prompt navigator when present.
  // 定位一个已渲染 Turn 外壳；提供时启用贴边提示词导航。
  onRevealPromptAnchor?: (anchor: HTMLElement) => void;
  // Opens one routed child-agent transcript.
  // 打开一个已路由的子代理转录。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Copies visible transcript text.
  // 复制可见转录文本。
  onCopyText: (text: string) => Promise<void> | void;
  // Receives an edit or fork action anchored by a canonical root input.
  // 接收由规范根输入锚定的编辑或分叉操作。
  onRootInputGenerationAction?: (action: ConversationBranchGenerationActionV1) => void;
  // Request-local reasoning status shown after durable events.
  // 显示在可靠事件之后的请求局部思考状态。
  transientReasoningStatus?: React.ReactNode;
  // Sticky footer rendered inside the transcript surface.
  // 渲染在转录界面内的粘性底部区域。
  footer?: React.ReactNode;
  // Whether the established empty placeholder must stay hidden.
  // 是否隐藏既有空占位。
  suppressEmptyPlaceholder?: boolean;
  // Optional local UI trace callback.
  // 可选本地界面诊断回调。
  onTrace?: (action: string, detail?: Record<string, unknown>) => void;
}

// ConversationBranchWorkspaceV1 renders the complete branch UI from canonical V1 events only.
// ConversationBranchWorkspaceV1 仅从规范 V1 事件渲染完整分支界面。
export function ConversationBranchWorkspaceV1(props: ConversationBranchWorkspaceV1Props) {
  const { t } = useI18n();
  // Preserve the stream context across structural updates that keep the same exact owner.
  // 保持同一精确归属下的结构更新前后流式上下文稳定。
  const streamOwner = props.snapshot.owner;
  const serviceEpoch = props.snapshot.service_epoch;
  const stream = React.useMemo(() => props.snapshotReader ? {
    reader: props.snapshotReader,
    owner: streamOwner,
    serviceEpoch,
  } : null, [props.snapshotReader, streamOwner, serviceEpoch]);
  // Membership creates cards once; individual cards subscribe to argument revisions.
  // 成员创建一次卡片；各卡片分别订阅参数修订。
  const requestDrafts = useConversationToolRequestMembership(props.sessionId);
  // historyLoading is process-local; the canonical cursor remains owned by the controller.
  // historyLoading 属于进程内状态；规范游标仍由控制器持有。
  const [historyLoading, setHistoryLoading] = React.useState(false);
  // historyRequest separates synchronous duplicate clicks and stale owner completions.
  // historyRequest 区分同步重复点击与过期归属请求的完成。
  const historyRequest = React.useRef<object | null>(null);
  // ownerReady prevents a stale previous-session window from entering the restored renderer.
  // ownerReady 防止上一会话的过期窗口进入恢复后的渲染器。
  const ownerReady = props.snapshot.owner?.session_id === props.sessionId;
  // Load the replacement rows alongside their coverage on every ordinary session open.
  // 每次打开普通会话时，同时读取替换条目与其覆盖范围。
  const historySessionId = ownerReady && props.conversation.dedicated !== true ? props.sessionId : null;
  // One snapshot controls both hiding original events and showing retained rows.
  // 同一快照控制原始事件的隐藏及保留条目的显示。
  const cleanedHistory = useConversationCleanedHistory(historySessionId);
  // events preserves the exact controller order and never falls back to legacy messages.
  // events 保留控制器精确顺序，并且绝不回退到旧消息。
  const events = ownerReady ? props.snapshot.events : [];
  // dedicatedMemory keeps its identity while streaming deltas leave the memory facts unchanged.
  // dedicatedMemory 在流式增量未改变记忆事实时保持身份。
  // reviewTurnId is the Turn the review panel's last-turn source shows for this transcript.
  // reviewTurnId 是审核面板上一轮来源为该转录展示的 Turn。
  const reviewTurnId = ownerReady ? resolveLastReviewTurnIdV1(events, props.snapshot.open_turn_id) : null;
  const reviewTarget = React.useMemo<TurnChangeReviewTarget>(
    () => ({ sessionId: props.sessionId, turnId: reviewTurnId }),
    [props.sessionId, reviewTurnId],
  );
  const dedicatedMemory = useEqualValue(ownerReady ? selectDedicatedMemory(props.snapshot) : null, conversationSelectionEqual);
  // collaborationRoutes only enables participants present in the current host navigation snapshot.
  // collaborationRoutes 仅启用当前宿主导航快照中存在的参与会话。
  const collaborationRoutes = React.useMemo(
    () => props.collaborationNavigation ? createConversationV1CollaborationNavigation(props.collaborationNavigation) : null,
    [props.collaborationNavigation],
  );
  // Navigation remains host-owned; the UI never decides whether an agent question should be cancelled.
  // 导航仍归宿主所有；界面不决定是否撤销代理问题。
  // subagentNames keeps its identity while streaming deltas leave every name unchanged.
  // subagentNames 在流式增量未改变任何名称时保持身份。
  const subagentNames = useEqualValue(
    React.useMemo(() => conversationV1SubagentNames(events, collaborationRoutes?.subagentNames), [collaborationRoutes, events]),
    sameMapEntries,
  );
  const collaborationActions = React.useMemo(() => ({
    onOpenSession: collaborationRoutes?.onOpenSession,
    availableSessionIds: collaborationRoutes?.availableSessionIds,
    subagentNames,
  }), [collaborationRoutes, subagentNames]);
  // collaborationEvents is the only input of the collaboration state; it keeps its identity while deltas touch
  // other events, so the context consumers do not re-render per streaming delta.
  // collaborationEvents 是协作状态的唯一输入；增量只触及其他事件时它保持身份，使上下文消费者不会随每个流式增量
  // 重新渲染。
  const collaborationEvents = useEqualValue(
    React.useMemo(() => events.filter((summary) => summary.event.kind === "collaboration"), [events]),
    sameReferences,
  );
  // Latest facts update the original cards without creating independent delivery rows.
  // 最新事实更新原卡片，不创建独立投递行。
  const collaborationState = React.useMemo(() => projectConversationCollaborationState(collaborationEvents), [collaborationEvents]);
  // resourceIds preserves first occurrence order across typed user and generated-resource references.
  // resourceIds 保留强类型用户资源与生成资源引用的首次出现顺序。
  const resourceIds = React.useMemo(() => collectConversationResourceIdsV1(events), [events]);
  // resources resolves only project-level ResourceIds owned by this exact session.
  // resources 仅解析由当前精确会话拥有的项目级 ResourceId。
  const resources = useConversationSessionResourcesV1(ownerReady ? props.sessionId : null, resourceIds);
  // presentedEvents adds the statistics a stopped Turn shows until Continue consumes it.
  // presentedEvents 追加已停止 Turn 在被继续执行消费前展示的统计。
  const presentedEvents = React.useMemo(
    () => {
      // Preserve canonical cursor events while omitting the ranges replaced by independent history.
      // 保留规范游标事件，同时省略已由独立历史替换的范围。
      const source = ownerReady && !props.sessionRunning ? withStoppedTurnStatsV1(props.snapshot) : events;
      const ranges = cleanedHistory.publication?.cleaned_ranges;
      return ranges?.length
        ? source.filter((event) => !ranges.some((range) => event.seq >= range.from && event.seq <= range.through))
        : source;
    },
    [events, cleanedHistory.publication, ownerReady, props.sessionRunning, props.snapshot],
  );
  // projectBranchPresentation keeps per-event projections across snapshots of this surface: it projects everything
  // on first open or after the session changes, then re-projects only the events each streaming update replaced.
  // projectBranchPresentation 在本界面的各次快照之间保留逐事件投影：首次打开或会话变化后全量投影，之后每次流式
  // 更新只重新投影被替换的事件。
  const [projectBranchPresentation] = React.useState(createConversationBranchPresentationProjectorV1);
  // canonicalPresentation is a pure display projection and never becomes storage or RPC authority.
  // canonicalPresentation 是纯展示投影，绝不成为存储或 RPC 权威。
  const canonicalPresentation = React.useMemo(
    () => projectBranchPresentation(props.sessionId, presentedEvents, resources, t, props.providers,
      props.sessionRunning ? requestDrafts : NO_REQUEST_DRAFTS),
    [presentedEvents, projectBranchPresentation, props.sessionId, props.providers, props.sessionRunning, requestDrafts, resources, t],
  );
  // messages appends only the process-local optimistic bubble after canonical V1 facts.
  // messages 仅在规范 V1 事实之后追加进程内乐观气泡。
  const messages = React.useMemo(() => {
    if (!props.optimisticUserInput) return canonicalPresentation.messages;
    return [
      ...canonicalPresentation.messages,
      optimisticConversationMessageV1(props.sessionId, props.optimisticUserInput),
    ];
  }, [canonicalPresentation.messages, props.optimisticUserInput, props.sessionId]);
  // renderItems reuses the exact branch parser only when the optimistic overlay changes the display list.
  // renderItems 仅在乐观层改变展示列表时复用精确的分支解析器。
  // projectOptimisticRenderItems reuses parsed canonical messages while the optimistic bubble is visible.
  // projectOptimisticRenderItems 在乐观气泡可见期间复用已解析的规范消息。
  const projectOptimisticRenderItems = React.useMemo(() => createConversationRenderItemsProjector(t), [t]);
  const renderItems = React.useMemo(
    () => props.optimisticUserInput
      ? projectOptimisticRenderItems(messages)
      : canonicalPresentation.renderItems,
    [canonicalPresentation.renderItems, messages, projectOptimisticRenderItems, props.optimisticUserInput],
  );
  // hiddenThinkingBlockIds restores the branch inline/popup filtering contract.
  // hiddenThinkingBlockIds 恢复分支的内联与弹窗过滤契约。
  // It keeps its identity while the hidden set is unchanged, because every transcript row re-renders when it changes.
  // 隐藏集合未变化时保持身份，因为它一变化每个转录行都会重新渲染。
  const hiddenThinkingBlockIds = useEqualValue(
    React.useMemo(
      () => hiddenThinkingBlockIdsForDisplayMode(
        props.reasoningDisplayMode,
        renderItems,
      ),
      [props.reasoningDisplayMode, renderItems],
    ),
    sameSetMembers,
  );

  React.useEffect(() => {
    historyRequest.current = null;
    setHistoryLoading(false);
  }, [props.sessionId, props.snapshot.owner?.window_generation]);

  // handleLoadMoreHistory prepends one owner-bound V1 page without invoking the old history RPC.
  // handleLoadMoreHistory 前置一页绑定所有者的 V1 事件，不调用旧历史 RPC。
  const handleLoadMoreHistory = React.useCallback(async (): Promise<boolean> => {
    if (!ownerReady || !props.snapshot.has_more || historyRequest.current !== null) return false;
    // request unlocks only the attempt that created it, never a subsequent owner's page request.
    // request 仅解锁创建它的尝试，绝不解锁后续归属的分页请求。
    const request = {};
    historyRequest.current = request;
    setHistoryLoading(true);
    try {
      return await props.commands.prependOlder();
    } finally {
      if (historyRequest.current === request) {
        historyRequest.current = null;
        setHistoryLoading(false);
      }
    }
  }, [ownerReady, props.commands, props.snapshot.has_more]);

  // generationAction is withheld from dedicated sessions, whose edit-regenerate and fork are frozen after creation.
  // generationAction 不提供给专职会话，其编辑重生成与分叉在创建后即冻结。
  const generationAction = props.conversation.dedicated === true || dedicatedMemory !== null
    ? undefined
    : props.onRootInputGenerationAction;

  // handleEditUserPrompt maps the restored icon action back to the exact V1 root-input anchor.
  // handleEditUserPrompt 把恢复后的图标操作映射回精确 V1 根输入锚点。
  const handleEditUserPrompt = React.useCallback((message: ConversationMessage): void => {
    const rootInput = rootInputPresentationMetadata(message);
    if (!rootInput || props.sessionRunning) return;
    props.onRootInputGenerationAction?.({
      kind: "edit",
      targetSeq: rootInput.seq,
      text: rootInput.text,
    });
  }, [props.onRootInputGenerationAction, props.sessionRunning]);

  // canonicalMessages lets the fork action read the committed messages without changing identity per delta.
  // canonicalMessages 让分叉操作读取已提交消息，且不随每个增量改变身份。
  const canonicalMessages = React.useRef(canonicalPresentation.messages);
  React.useLayoutEffect(() => {
    canonicalMessages.current = canonicalPresentation.messages;
  }, [canonicalPresentation.messages]);

  // handleForkConversation maps the Turn-end fork icon to the same Turn's terminal statistics seq.
  // handleForkConversation 把 Turn 末尾分叉图标映射到同一回合的终态统计 seq。
  const handleForkConversation = React.useCallback((
    _conversationId: string,
    anchorMessageId?: string,
  ): boolean => {
    if (!anchorMessageId) return false;
    const messages = canonicalMessages.current;
    const sourceMessage = messages.find((message) => message.id === anchorMessageId);
    const sourceTurnEndSeq = completedTurnEndSeqForPresentation(
      messages,
      sourceMessage,
    );
    if (sourceTurnEndSeq === null || !props.onRootInputGenerationAction) return false;
    props.onRootInputGenerationAction({
      kind: "fork",
      sourceTurnEndSeq,
    });
    return true;
  }, [props.onRootInputGenerationAction]);

  return (
    <ConversationTranscriptStreamContext.Provider value={stream}>
    <ThinkingPopupContext.Provider value={normalizeThinkingDisplayMode(props.reasoningDisplayMode) === "popup_only"}>
    <ConversationAgentInteractionActionsContext.Provider value={collaborationActions}>
    <ConversationCollaborationStateContext.Provider value={collaborationState}>
    <DedicatedTranscriptMemoryProvider memory={dedicatedMemory}>
    <TurnChangeReviewTargetContext.Provider value={reviewTarget}>
    {historySessionId !== null ? (
      <ConversationHistoryCleanup key={historySessionId} sessionId={historySessionId}
        running={props.sessionRunning} onCleaned={cleanedHistory.accept} />
    ) : null}
    <ConversationWorkspace
      autoExpandStreamingThinking={normalizeThinkingDisplayMode(props.reasoningDisplayMode) === "inline_output"}
      commandSessions={props.commandSessions}
      conversation={props.conversation}
      error={null}
      footer={props.footer}
      cleanedHistory={cleanedHistory.publication && historySessionId !== null
        && (cleanedHistory.publication.history.turns.length > 0 || cleanedHistory.publication.history.resources.length > 0)
        ? { sessionId: historySessionId, history: cleanedHistory.publication.history, onRemove: cleanedHistory.remove }
        : null}
      hasMoreHistory={ownerReady && props.snapshot.has_more}
      hiddenThinkingBlockIds={hiddenThinkingBlockIds}
      historyLoading={historyLoading}
      initialExpandedState={props.initialExpandedState}
      messages={messages}
      onCopyText={props.onCopyText}
      onEditUserPrompt={generationAction ? handleEditUserPrompt : undefined}
      onExpandedStateChange={props.onExpandedStateChange}
      onExtractGeneratedMediaAttachments={emptyGeneratedMediaAttachmentsV1}
      onForkConversation={generationAction ? handleForkConversation : undefined}
      onLoadMoreHistory={handleLoadMoreHistory}
      onLoadToolDetail={unavailableToolDetailV1}
      onOpenSubagentPanel={props.onOpenSubagentPanel}
      onPlanArtifactCancel={props.onPlanArtifactCancel}
      onPlanArtifactExecute={props.onPlanArtifactExecute}
      onPlanArtifactSuggest={props.onPlanArtifactSuggest}
      onPreserveHistoryPrepend={props.onPreserveHistoryPrepend}
      onScrollStateChange={props.onScrollStateChange}
      onTrace={props.onTrace}
      parentConversationId={props.sessionId}
      pendingPlanArtifact={props.pendingPlanArtifact}
      renderItems={renderItems}
      resolveComposerAttachmentMedia={composerAttachmentMediaV1}
      resolveMediaSourceUrl={identityMediaSourceUrlV1}
      scrollRef={props.scrollRef}
      onRevealPromptAnchor={props.onRevealPromptAnchor}
      sessionRunning={props.sessionRunning}
      suppressEmptyPlaceholder={!ownerReady || props.suppressEmptyPlaceholder}
      transientReasoningStatus={props.transientReasoningStatus}
      workspacePath={props.workspacePath}
    />
    </TurnChangeReviewTargetContext.Provider>
    </DedicatedTranscriptMemoryProvider>
    </ConversationCollaborationStateContext.Provider>
    </ConversationAgentInteractionActionsContext.Provider>
    </ThinkingPopupContext.Provider>
    </ConversationTranscriptStreamContext.Provider>
  );
}

// collectConversationResourceIdsV1 collects typed references in canonical event order without sorting.
// collectConversationResourceIdsV1 按规范事件顺序收集强类型引用且不排序。
function collectConversationResourceIdsV1(events: readonly ConversationEventSummary[]): ResourceId[] {
  const resourceIds: ResourceId[] = [];
  for (const summary of events) {
    if (summary.event.kind === "user_input") {
      resourceIds.push(...(summary.event.resource_ids ?? []));
      continue;
    }
    if (summary.event.kind !== "tool_call") continue;
    for (const generatedResource of summary.event.generated_resources ?? []) {
      resourceIds.push(generatedResource.resource_id);
      if (generatedResource.preview_resource_id !== undefined) {
        resourceIds.push(generatedResource.preview_resource_id);
      }
    }
  }
  return resourceIds;
}

// optimisticConversationMessageV1 creates one display-only bubble without assigning a V1 sequence.
// optimisticConversationMessageV1 创建一条不分配 V1 序号的纯展示气泡。
function optimisticConversationMessageV1(
  sessionId: string,
  optimistic: ConversationBranchOptimisticUserInputV1,
): ConversationMessage {
  return {
    id: `optimistic:${optimistic.clientActionId}`,
    conversationId: sessionId,
    role: "user",
    body: optimistic.text,
    editableUserPrompt: false,
    attachments: optimistic.attachmentNames.map((fileName) => ({
      kind: "file",
      fileName,
      localPath: "",
      byteLen: null,
      mimeType: "application/octet-stream",
    })),
    metadata: {
      client_action_id: optimistic.clientActionId,
      starts_user_turn: true,
      optimistic: true,
    },
  };
}

// composerAttachmentMediaV1 maps only browser-safe opaque resource URLs into the restored preview type.
// composerAttachmentMediaV1 仅把浏览器安全的不透明资源 URL 映射为恢复后的预览类型。
function composerAttachmentMediaV1(attachment: ComposerAttachmentSummary): GeneratedMediaAttachment | null {
  if (!attachment.localPath) return null;
  return {
    kind: generatedMediaKindV1(attachment),
    fileName: attachment.fileName,
    localPath: attachment.localPath,
    mimeType: attachment.mimeType ?? undefined,
  };
}

// generatedMediaKindV1 selects a display family from the adapter's explicit attachment kind.
// generatedMediaKindV1 从适配器明确的附件类型中选择展示族。
function generatedMediaKindV1(attachment: ComposerAttachmentSummary): GeneratedMediaKind {
  if (attachment.kind === "image") return "image";
  if (attachment.kind === "audio") return "audio";
  if (attachment.kind === "video") return "video";
  return "document";
}

// identityMediaSourceUrlV1 preserves the authenticated opaque URL emitted by the resource client.
// identityMediaSourceUrlV1 保留资源客户端发出的鉴权不透明 URL。
function identityMediaSourceUrlV1(path: string): string {
  return path;
}

// unavailableToolDetailV1 prevents the restored card shell from reviving the removed detail RPC.
// unavailableToolDetailV1 防止恢复后的卡片外壳复活已删除的详情 RPC。
async function unavailableToolDetailV1(): Promise<ToolDetailPayload | null> {
  return null;
}

// emptyGeneratedMediaAttachmentsV1 rejects hidden-detail media fallback; V1 generated resources are inline facts.
// emptyGeneratedMediaAttachmentsV1 拒绝隐藏详情媒体回退；V1 生成资源是内联事实。
function emptyGeneratedMediaAttachmentsV1(): GeneratedMediaAttachment[] {
  return [];
}

// sameReferences reports whether two lists hold the same objects in the same order.
// sameReferences 判断两个列表是否按相同顺序持有相同对象。
function sameReferences<T>(left: readonly T[], right: readonly T[]): boolean {
  return left.length === right.length && left.every((item, index) => Object.is(item, right[index]));
}

// sameSetMembers reports whether two sets hold the same members.
// sameSetMembers 判断两个集合是否持有相同成员。
function sameSetMembers<T>(left: ReadonlySet<T>, right: ReadonlySet<T>): boolean {
  if (left.size !== right.size) return false;
  for (const member of left) if (!right.has(member)) return false;
  return true;
}

// sameMapEntries reports whether two maps hold the same keys with identical values.
// sameMapEntries 判断两个映射是否持有相同键及同一值。
function sameMapEntries<K, V>(left: ReadonlyMap<K, V>, right: ReadonlyMap<K, V>): boolean {
  if (left.size !== right.size) return false;
  for (const [key, value] of left) if (!right.has(key) || !Object.is(right.get(key), value)) return false;
  return true;
}
