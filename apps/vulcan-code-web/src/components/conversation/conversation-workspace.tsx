import React from "react";
import {
  GeneratedMediaPlayerWindow,
  type GeneratedMediaAttachment,
} from "@/components/media/generated-media";
import { Split } from "lucide-react";
import {
  createEmptyConversationSurfaceExpandedState,
  type ConversationSurfaceExpandedState,
} from "@/components/conversation/conversation-layout-restore";
import {
  ConversationMeasuredCollapseProvider,
  conversationMeasuredCollapseMapEqual,
  type ConversationMeasuredCollapseMap,
} from "@/components/conversation/conversation-measured-collapse";
import { MarkdownImageWorkspacePathProvider } from "@/components/conversation/conversation-markdown";
import {
  ConversationDetailModal,
  type ConversationDetail,
  type ConversationDetailRef,
  type ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import {
  ConversationFinalAssistantMeta,
  ConversationProcessGroup,
  ConversationRenderItemView,
  renderItemHiddenByThinkingDisplay,
} from "@/components/conversation/conversation-render-items";
import { isContextCompactionBlock } from "@/components/conversation/conversation-classifiers";
import {
  PlanArtifactBlock,
  type PlanArtifactActionPayload,
} from "@/components/conversation/conversation-plan-artifact";
import type { ConversationPlanArtifact } from "@/components/conversation/conversation-model";
import {
  conversationTimelineItemKey,
  latestThinkingBlockIdAfterLastUser,
  projectConversationTimelineItems,
} from "@/components/conversation/conversation-timeline";
import type {
  ConversationProcessRenderItem,
  ConversationRenderItem,
  ConversationTimelineItem,
} from "@/components/conversation/conversation-model";
import type {
  CommandSessionSummary,
  ComposerAttachmentSummary,
  ConversationMessage,
  ConversationSummary,
} from "@/types";
import { useI18n } from "@/i18n";
import { ConversationTurnSeatList } from "@/components/conversation/conversation-turn-seat";
import {
  ConversationTimelineProjectionStore,
  type ConversationTurnProjection,
} from "@/components/conversation/conversation-timeline-projection-store";

// ConversationWorkspace renders the conversation transcript surface.
// ConversationWorkspace 渲染会话转录展示面。
export function ConversationWorkspace(props: {
  // Current conversation summary shown by this surface.
  // 当前展示面显示的会话摘要。
  conversation?: ConversationSummary;
  // Whether the owning conversation is currently running.
  // 所属会话当前是否正在运行。
  sessionRunning: boolean;
  // Whether the latest streaming thinking block may automatically expand in the inline transcript.
  // 最新流式思考块是否允许在内联会话中自动展开。
  autoExpandStreamingThinking?: boolean;
  // Thinking blocks filtered from the main transcript by the display mode.
  // 被展示模式从主会话面板过滤的思考块。
  hiddenThinkingBlockIds: ReadonlySet<string>;
  // Raw messages used only for empty-state parity.
  // 仅用于保持空状态一致性的原始消息。
  messages: ConversationMessage[];
  // Parsed rows for the same message window.
  // 同一消息窗口对应的已解析展示行。
  renderItems: ConversationRenderItem[];
  // Loading error for the visible transcript body.
  // 可见转录正文的加载错误。
  error: string | null;
  // Whether older history exists before the loaded transcript window.
  // 已加载转录窗口之前是否存在更早历史。
  hasMoreHistory: boolean;
  // Whether an older history page is being loaded.
  // 当前是否正在加载更早历史页。
  historyLoading: boolean;
  // Loads one older history page for this conversation.
  // 为当前会话加载一页更早历史。
  onLoadMoreHistory: (conversationId: string) => Promise<boolean>;
  // Preserves the semantic viewport while one older history page is prepended.
  // 在前置一页更早历史时保留语义视口。
  onPreserveHistoryPrepend: (load: () => Promise<boolean>) => Promise<boolean>;
  // Parent conversation id used by subagent navigation.
  // 子代理导航使用的父会话标识。
  parentConversationId: string | null;
  // Project workspace path used to resolve relative Markdown media references.
  // 用于解析 Markdown 相对媒体引用的项目工作区路径。
  workspacePath?: string | null;
  // Live command-session summaries used by terminal presentations.
  // 终端展示使用的实时命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Called when scroll position changes.
  // 滚动位置变化时调用。
  onScrollStateChange: () => void;
  // Opens a routed subagent panel.
  // 打开已路由的子代理面板。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Scroll container ref owned by the shell.
  // 由外壳持有的滚动容器引用。
  scrollRef: React.RefObject<HTMLElement | null>;
  // Previously captured runtime expansion state for this surface.
  // 当前界面此前捕获的运行期展开状态。
  initialExpandedState: ConversationSurfaceExpandedState | null;
  // Persists process, user-prompt, and tool-presentation expansion state for this surface.
  // 持久保存当前界面的处理过程、用户提示词与工具展示展开状态。
  onExpandedStateChange: (surfaceId: string, state: ConversationSurfaceExpandedState) => void;
  // Request-local reasoning status rendered at the end of the conversation timeline.
  // 渲染在会话时间线末尾的请求局部思考状态。
  transientReasoningStatus?: React.ReactNode;
  // Sticky footer rendered inside the transcript scroll container.
  // 渲染在转录滚动容器内部的粘性底部插槽。
  footer?: React.ReactNode;
  // Whether the default empty placeholder should be hidden.
  // 是否隐藏默认空占位内容。
  suppressEmptyPlaceholder?: boolean;
  // Copies transcript text using the shell-provided clipboard path.
  // 使用外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void> | void;
  // Opens one backend-backed user prompt edit flow.
  // 打开单个后端支持的用户提示词编辑流程。
  onEditUserPrompt?: (conversationId: string, messageId: string) => Promise<void> | void;
  // Creates a forked copy of one conversation.
  // 创建单个会话的分叉副本。
  onForkConversation?: (conversationId: string, anchorMessageId?: string) => Promise<boolean> | boolean;
  // Selects the original source conversation for a fork marker.
  // 选择分叉标记对应的原始来源会话。
  onSelectForkSourceConversation?: (conversationId: string) => void;
  // Currently pending Plan artifact, when user confirmation is required.
  // 需要用户确认时的当前待处理计划产物。
  pendingPlanArtifact?: ConversationPlanArtifact | null;
  // Executes one pending Plan artifact.
  // 执行单个待确认计划产物。
  onPlanArtifactExecute?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Sends a suggestion for one pending Plan artifact.
  // 为单个待确认计划产物发送补充建议。
  onPlanArtifactSuggest?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Cancels one pending Plan artifact.
  // 取消单个待确认计划产物。
  onPlanArtifactCancel?: (payload: PlanArtifactActionPayload) => Promise<void> | void;
  // Loads one hidden tool-detail record.
  // 加载单条隐藏工具详情记录。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Extracts generated media attachments from hidden tool details.
  // 从隐藏工具详情中提取生成媒体附件。
  onExtractGeneratedMediaAttachments: (payload: ToolDetailPayload) => GeneratedMediaAttachment[];
  // Resolves a local media file path into a renderable URL.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Resolves composer attachments into generated-media compatible payloads.
  // 将输入区附件解析为兼容生成媒体的载荷。
  resolveComposerAttachmentMedia: (attachment: ComposerAttachmentSummary) => GeneratedMediaAttachment | null;
  // Optional trace callback for local diagnostics.
  // 用于本地诊断的可选埋点回调。
  onTrace?: (action: string, detail?: Record<string, unknown>) => void;
}) {
  const { locale, t } = useI18n();
  // detail stores the currently opened transcript detail modal.
  // detail 保存当前打开的转录详情弹窗。
  const [detail, setDetail] = React.useState<ConversationDetail | null>(null);
  // mediaPlayer stores the generated media currently opened in the independent player window.
  // mediaPlayer 保存当前在独立播放窗口中打开的生成媒体。
  const [mediaPlayer, setMediaPlayer] = React.useState<GeneratedMediaAttachment | null>(null);
  // processCollapseOverrides stores user-toggled process fold states for the current UI session.
  // processCollapseOverrides 存储当前 UI 会话中用户手动切换的处理过程折叠状态。
  const [processCollapseOverrides, setProcessCollapseOverrides] = React.useState<Map<string, boolean>>(
    () => new Map(Object.entries(props.initialExpandedState?.processCollapsedByKey ?? {})),
  );
  // userPromptExpansionOverrides mirrors WebUI-session-only user prompt expansion memory for stable Turn Seats.
  // userPromptExpansionOverrides 镜像仅当前 WebUI 会话有效的用户提示词展开记忆，供稳定 Turn Seat 使用。
  const [userPromptExpansionOverrides, setUserPromptExpansionOverrides] = React.useState<Set<string>>(
    () => new Set(props.initialExpandedState?.expandedUserPromptIds ?? []),
  );
  // toolPresentationExpansionOverrides mirrors WebUI-session-only tool card expansion memory for stable Turn Seats.
  // toolPresentationExpansionOverrides 镜像仅当前 WebUI 会话有效的工具卡片展开记忆，供稳定 Turn Seat 使用。
  const [toolPresentationExpansionOverrides, setToolPresentationExpansionOverrides] = React.useState<Set<string>>(
    () => new Set(props.initialExpandedState?.expandedToolPresentationIds ?? []),
  );
  // measuredCollapseById mirrors stable expanded state for measured-collapse components on the current surface.
  // measuredCollapseById 镜像当前界面中实测折叠组件的稳定展开状态。
  const [measuredCollapseById, setMeasuredCollapseById] = React.useState<ConversationMeasuredCollapseMap>(
    () => ({ ...(props.initialExpandedState?.measuredCollapseById ?? {}) }),
  );
  // processCollapseOverridesRef exposes current process choices without changing the shared row renderer.
  // processCollapseOverridesRef 暴露当前过程选择，同时不改变共享行渲染器。
  const processCollapseOverridesRef = React.useRef(processCollapseOverrides);
  processCollapseOverridesRef.current = processCollapseOverrides;
  // userPromptExpansionOverridesRef exposes current prompt choices to exact row renders.
  // userPromptExpansionOverridesRef 向精确行渲染暴露当前提示词选择。
  const userPromptExpansionOverridesRef = React.useRef(userPromptExpansionOverrides);
  userPromptExpansionOverridesRef.current = userPromptExpansionOverrides;
  // toolPresentationExpansionOverridesRef exposes current tool-card choices to exact row renders.
  // toolPresentationExpansionOverridesRef 向精确行渲染暴露当前工具卡片选择。
  const toolPresentationExpansionOverridesRef = React.useRef(toolPresentationExpansionOverrides);
  toolPresentationExpansionOverridesRef.current = toolPresentationExpansionOverrides;
  // measuredCollapseByIdRef supplies the latest expanded-state memory to persistence callbacks.
  // measuredCollapseByIdRef 向持久化回调提供最新展开状态记忆。
  const measuredCollapseByIdRef = React.useRef(measuredCollapseById);
  measuredCollapseByIdRef.current = measuredCollapseById;
  const surfaceRestoreId = props.conversation?.id ?? props.parentConversationId ?? null;
  const expandedStateSurfaceRestoreIdRef = React.useRef(surfaceRestoreId);
  const timelineItems = React.useMemo(
    () => projectConversationTimelineItems(props.renderItems, props.conversation, props.sessionRunning),
    [props.renderItems, props.conversation, props.sessionRunning],
  );
  const visibleTimelineItems = React.useMemo(
    () => timelineItems.filter((item) => timelineItemVisibleInMainTranscript(item, props.hiddenThinkingBlockIds)),
    [props.hiddenThinkingBlockIds, timelineItems],
  );
  // pendingPlanArtifactAlreadyVisible prevents duplicated Plan cards after the fixed tool projection is present.
  // pendingPlanArtifactAlreadyVisible 在修复后的工具投影已存在时防止重复显示计划卡。
  const pendingPlanArtifactAlreadyVisible = React.useMemo(() => {
    const artifact = props.pendingPlanArtifact ?? null;
    if (!artifact) {
      return false;
    }
    return visibleTimelineItems.some((item) => timelineItemContainsPlanArtifact(item, artifact));
  }, [props.pendingPlanArtifact, visibleTimelineItems]);
  // lockedTimelineActionKeyList identifies rows before the latest completed compaction runtime boundary.
  // lockedTimelineActionKeyList 标识最后一次成功压缩运行时边界之前的只读行。
  const lockedTimelineActionKeyList = React.useMemo(() => {
    // latestCompletedCompactionIndex is the final durable read-only boundary in visible order.
    // latestCompletedCompactionIndex 是可见顺序中的最后一个持久只读边界。
    let latestCompletedCompactionIndex = -1;
    visibleTimelineItems.forEach((item, index) => {
      if (timelineItemHasCompletedContextCompaction(item)) {
        latestCompletedCompactionIndex = index;
      }
    });
    if (latestCompletedCompactionIndex < 0) {
      return [];
    }
    return visibleTimelineItems
      .slice(0, latestCompletedCompactionIndex + 1)
      .map(conversationTimelineItemKey);
  }, [visibleTimelineItems]);
  // lockedTimelineActionSignature changes only when the effective compaction boundary changes.
  // lockedTimelineActionSignature 仅在生效压缩边界变化时改变。
  const lockedTimelineActionSignature = JSON.stringify(lockedTimelineActionKeyList);
  // lockedTimelineActionKeys preserves Set identity across unrelated row projection updates.
  // lockedTimelineActionKeys 在无关行投影更新期间保留 Set 身份。
  const lockedTimelineActionKeys = React.useMemo(
    () => new Set<string>(JSON.parse(lockedTimelineActionSignature) as string[]),
    [lockedTimelineActionSignature],
  );
  // timelineCacheScopeKey is the exact owning surface identity; null represents an unowned empty transcript.
  // timelineCacheScopeKey 是精确所属界面身份；null 表示无所属者的空转录。
  const timelineCacheScopeKey = surfaceRestoreId;
  const turnGroups = React.useMemo(
    () => groupConversationTimelineItems(visibleTimelineItems),
    [visibleTimelineItems],
  );
  // timelineProjectionStore is the single exact row-subscription owner recreated only when surface ownership changes.
  // timelineProjectionStore 是唯一精确行订阅所有者，仅在界面所有权变化时重建。
  const timelineProjectionStore = React.useMemo(
    () => new ConversationTimelineProjectionStore(),
    [timelineCacheScopeKey],
  );
  // projectedTurnOrder is the freshly projected visual order before value stabilization.
  // projectedTurnOrder 是按值稳定前本次新投影的视觉顺序。
  const projectedTurnOrder = React.useMemo(
    () => turnGroups.map((group) => group.key),
    [turnGroups],
  );
  // turnOrderSignature changes only when stable business ordering changes.
  // turnOrderSignature 仅在稳定业务顺序变化时改变。
  const turnOrderSignature = JSON.stringify(projectedTurnOrder);
  // turnOrder preserves array identity across content-only updates so Seat-list reconciliation can be skipped.
  // turnOrder 在纯内容更新期间保留数组身份，使 Seat 列表可以跳过协调。
  const turnOrder = React.useMemo(
    () => JSON.parse(turnOrderSignature) as string[],
    [turnOrderSignature],
  );
  // turnGroupByKey supplies the explicit first-mount snapshot for rows not committed to the Store yet.
  // turnGroupByKey 为尚未提交到 Store 的行提供显式首次挂载快照。
  const turnGroupByKey = React.useMemo(
    () => new Map(turnGroups.map((group) => [group.key, group] as const)),
    [turnGroups],
  );
  // timelineProjectionLocaleRef forces complete row replacement only when localized projection content changes.
  // timelineProjectionLocaleRef 仅在本地化投影内容变化时强制完整替换行。
  const timelineProjectionLocaleRef = React.useRef(locale);
  // projectionLocaleChanged records the explicit non-message reason for replacing every projected row.
  // projectionLocaleChanged 记录替换全部投影行的显式非消息原因。
  const projectionLocaleChanged = timelineProjectionLocaleRef.current !== locale;
  React.useLayoutEffect(() => {
    timelineProjectionStore.reconcile(turnGroups, projectionLocaleChanged);
    timelineProjectionLocaleRef.current = locale;
    timelineProjectionStore.flushNotifications();
  }, [locale, projectionLocaleChanged, timelineProjectionStore, turnGroups]);
  // previousCommandSessionsRef identifies command-runtime changes that affect only terminal-owning Turns.
  // previousCommandSessionsRef 标识仅影响终端所属 Turn 的命令运行时变化。
  const previousCommandSessionsRef = React.useRef(props.commandSessions);
  React.useLayoutEffect(() => {
    if (previousCommandSessionsRef.current === props.commandSessions) {
      return;
    }
    previousCommandSessionsRef.current = props.commandSessions;
    timelineProjectionStore.invalidateRows(conversationTurnProjectionOwnsCommandSession);
    timelineProjectionStore.flushNotifications();
  }, [props.commandSessions, timelineProjectionStore]);
  const autoExpandThinkingBlockId = (props.autoExpandStreamingThinking ?? true)
    && props.sessionRunning
    && props.hiddenThinkingBlockIds.size === 0
    ? latestThinkingBlockIdAfterLastUser(props.renderItems)
    : null;
  // processCollapseKey binds one process choice to its authoritative surface identity.
  // processCollapseKey 将单个过程选择绑定到其权威界面身份。
  // Parameters: process is the exact projected process row.
  // 参数：process 是精确投影出的处理过程行。
  // Returns: a surface-scoped key, or null when no business surface owns the transcript.
  // 返回值：界面级键；当转录没有业务界面所有者时返回 null。
  const processCollapseKey = React.useCallback((process: ConversationProcessRenderItem): string | null => (
    surfaceRestoreId ? `${surfaceRestoreId}:${process.id}` : null
  ), [surfaceRestoreId]);
  const onExpandedStateChange = props.onExpandedStateChange;
  const persistExpandedState = React.useCallback((
    processCollapsedByKey: Map<string, boolean>,
    expandedUserPromptIds: Set<string>,
    expandedToolPresentationIds: Set<string>,
    nextMeasuredCollapseById: ConversationMeasuredCollapseMap,
  ) => {
    if (!surfaceRestoreId) {
      return;
    }
    onExpandedStateChange(surfaceRestoreId, {
      expandedToolPresentationIds: Array.from(expandedToolPresentationIds),
      expandedUserPromptIds: Array.from(expandedUserPromptIds),
      measuredCollapseById: nextMeasuredCollapseById,
      processCollapsedByKey: Object.fromEntries(processCollapsedByKey),
    });
  }, [onExpandedStateChange, surfaceRestoreId]);
  const handleMeasuredCollapseMemoryChange = React.useCallback((nextMeasuredCollapseById: ConversationMeasuredCollapseMap) => {
    measuredCollapseByIdRef.current = nextMeasuredCollapseById;
    setMeasuredCollapseById(nextMeasuredCollapseById);
    persistExpandedState(
      processCollapseOverridesRef.current,
      userPromptExpansionOverridesRef.current,
      toolPresentationExpansionOverridesRef.current,
      nextMeasuredCollapseById,
    );
  }, [persistExpandedState]);
  const handleProcessCollapsedChange = React.useCallback((process: ConversationProcessRenderItem, collapsed: boolean) => {
    const key = processCollapseKey(process);
    if (!key) {
      return;
    }
    // next is created synchronously so the exact invalidated Seat reads the new override immediately.
    // next 同步创建，使精确失效的 Seat 立即读取新的覆盖值。
    const next = new Map(processCollapseOverridesRef.current);
    next.set(key, collapsed);
    processCollapseOverridesRef.current = next;
    setProcessCollapseOverrides(next);
    persistExpandedState(
      next,
      userPromptExpansionOverridesRef.current,
      toolPresentationExpansionOverridesRef.current,
      measuredCollapseByIdRef.current,
    );
    timelineProjectionStore.invalidateRows((row) => row.items.some((item) => (
      item.kind === "process" && item.process.id === process.id
    )));
    timelineProjectionStore.flushNotifications();
  }, [persistExpandedState, processCollapseKey, timelineProjectionStore]);
  const handleUserPromptExpandedChange = React.useCallback((messageKey: string, expanded: boolean) => {
    // next mirrors the locally measured prompt expansion for later remount restoration.
    // next 镜像本地实测提示词展开状态，用于后续重新挂载恢复。
    const next = new Set(userPromptExpansionOverridesRef.current);
    if (expanded) {
      next.add(messageKey);
    } else {
      next.delete(messageKey);
    }
    userPromptExpansionOverridesRef.current = next;
    setUserPromptExpansionOverrides(next);
    persistExpandedState(
      processCollapseOverridesRef.current,
      next,
      toolPresentationExpansionOverridesRef.current,
      measuredCollapseByIdRef.current,
    );
  }, [persistExpandedState]);
  const handleToolPresentationExpandedChange = React.useCallback((presentationId: string, expanded: boolean) => {
    // next mirrors the card's local expansion for later remount restoration.
    // next 镜像卡片本地展开状态，用于后续重新挂载恢复。
    const next = new Set(toolPresentationExpansionOverridesRef.current);
    if (expanded) {
      next.add(presentationId);
    } else {
      next.delete(presentationId);
    }
    toolPresentationExpansionOverridesRef.current = next;
    setToolPresentationExpansionOverrides(next);
    persistExpandedState(
      processCollapseOverridesRef.current,
      userPromptExpansionOverridesRef.current,
      next,
      measuredCollapseByIdRef.current,
    );
  }, [persistExpandedState]);

  React.useLayoutEffect(() => {
    if (expandedStateSurfaceRestoreIdRef.current === surfaceRestoreId) {
      return;
    }
    expandedStateSurfaceRestoreIdRef.current = surfaceRestoreId;
    const nextExpandedState = props.initialExpandedState ?? createEmptyConversationSurfaceExpandedState();
    const nextProcessCollapseOverrides = new Map(Object.entries(nextExpandedState.processCollapsedByKey));
    const nextUserPromptExpansionOverrides = new Set(nextExpandedState.expandedUserPromptIds);
    const nextToolPresentationExpansionOverrides = new Set(nextExpandedState.expandedToolPresentationIds);
    setProcessCollapseOverrides((current) => stringBooleanMapEqual(current, nextProcessCollapseOverrides) ? current : nextProcessCollapseOverrides);
    setUserPromptExpansionOverrides((current) => stringSetEqual(current, nextUserPromptExpansionOverrides) ? current : nextUserPromptExpansionOverrides);
    setToolPresentationExpansionOverrides((current) => stringSetEqual(current, nextToolPresentationExpansionOverrides) ? current : nextToolPresentationExpansionOverrides);
    setMeasuredCollapseById((current) => (
      conversationMeasuredCollapseMapEqual(current, nextExpandedState.measuredCollapseById)
        ? current
        : { ...nextExpandedState.measuredCollapseById }
    ));
  }, [props.initialExpandedState, surfaceRestoreId]);

  const handleConversationScroll = React.useCallback(() => {
    props.onScrollStateChange();
  }, [props.onScrollStateChange]);
  // handleLoadMoreHistory preserves the first visible position while older turns are prepended.
  // handleLoadMoreHistory 在前置更早轮次时保持当前可见位置。
  const handleLoadMoreHistory = React.useCallback(async () => {
    const conversationId = props.conversation?.id ?? props.parentConversationId;
    const element = props.scrollRef.current;
    if (!conversationId || !element || props.historyLoading) {
      return;
    }
    await props.onPreserveHistoryPrepend(() => props.onLoadMoreHistory(conversationId));
  }, [
    props.conversation?.id,
    props.historyLoading,
    props.onLoadMoreHistory,
    props.onPreserveHistoryPrepend,
    props.parentConversationId,
    props.scrollRef,
  ]);

  // commandSessionsRef supplies current runtime truth without invalidating unrelated historical Seats.
  // commandSessionsRef 提供当前运行时真值，同时不使无关历史 Seat 失效。
  const commandSessionsRef = React.useRef(props.commandSessions);
  commandSessionsRef.current = props.commandSessions;

  const renderTimelineItem = React.useCallback((
    timelineItem: ConversationTimelineItem,
    seatNowMs: number,
  ): React.ReactNode => {
    if (timelineItem.kind === "fork-source") {
      return (
        <ConversationForkSourceSeparator
          label={t("conversation.forkSource.label")}
          onSelect={() => {
            const sourceId = props.conversation?.forkSource?.sourceConversationId;
            if (!sourceId) {
              return;
            }
            props.onSelectForkSourceConversation?.(sourceId);
          }}
        />
      );
    }
    if (timelineItem.kind === "final-assistant-meta") {
      const metaKey = conversationTimelineItemKey(timelineItem);
      const lockedByCompaction = lockedTimelineActionKeys.has(metaKey);
      return (
        <ConversationFinalAssistantMeta
          item={timelineItem.item}
          onCopyText={props.onCopyText}
          onForkConversation={!lockedByCompaction && timelineItem.item.sourceMessage ? props.onForkConversation : undefined}
        />
      );
    }
    if (timelineItem.kind === "item") {
      const lockedByCompaction = lockedTimelineActionKeys.has(conversationTimelineItemKey(timelineItem));
      return (
        <ConversationRenderItemView
          item={timelineItem.item}
          finalAssistantOutput={timelineItem.finalAssistantOutput}
          autoExpandThinkingBlockId={autoExpandThinkingBlockId}
          hiddenThinkingBlockIds={props.hiddenThinkingBlockIds}
          parentConversationId={props.parentConversationId}
          sessionRunning={props.sessionRunning}
          expandedUserPromptIds={userPromptExpansionOverridesRef.current}
          expandedToolPresentationIds={toolPresentationExpansionOverridesRef.current}
          workspacePath={props.workspacePath}
          commandSessions={commandSessionsRef.current}
          onCopyText={props.onCopyText}
          onEditUserPrompt={props.onEditUserPrompt}
          userPromptEditDisabled={lockedByCompaction}
          onOpenDetail={setDetail}
          onOpenMedia={setMediaPlayer}
          onOpenSubagentPanel={props.onOpenSubagentPanel}
          pendingPlanArtifact={props.pendingPlanArtifact}
          onPlanArtifactExecute={props.onPlanArtifactExecute}
          onPlanArtifactSuggest={props.onPlanArtifactSuggest}
          onPlanArtifactCancel={props.onPlanArtifactCancel}
          onUserPromptExpandedChange={handleUserPromptExpandedChange}
          onToolPresentationExpandedChange={handleToolPresentationExpandedChange}
          resolveComposerAttachmentMedia={props.resolveComposerAttachmentMedia}
          resolveMediaSourceUrl={props.resolveMediaSourceUrl}
          onLoadToolDetail={props.onLoadToolDetail}
          onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
          onTrace={props.onTrace}
        />
      );
    }
    // collapseKey is absent only when no authoritative business surface owns this transcript.
    // collapseKey 仅在没有权威业务界面拥有当前转录时缺失。
    const collapseKey = processCollapseKey(timelineItem.process);
    return (
      <ConversationProcessGroup
        autoExpandThinkingBlockId={autoExpandThinkingBlockId}
        collapsedOverride={collapseKey ? processCollapseOverridesRef.current.get(collapseKey) : undefined}
        hiddenThinkingBlockIds={props.hiddenThinkingBlockIds}
        nowMs={seatNowMs}
        parentConversationId={props.parentConversationId}
        process={timelineItem.process}
        sessionRunning={props.sessionRunning}
        expandedUserPromptIds={userPromptExpansionOverridesRef.current}
        expandedToolPresentationIds={toolPresentationExpansionOverridesRef.current}
        workspacePath={props.workspacePath}
        commandSessions={commandSessionsRef.current}
        onCollapsedChange={(collapsed) => handleProcessCollapsedChange(timelineItem.process, collapsed)}
        onCopyText={props.onCopyText}
        onEditUserPrompt={props.onEditUserPrompt}
        userPromptEditDisabled={lockedTimelineActionKeys.has(conversationTimelineItemKey(timelineItem))}
        onOpenDetail={setDetail}
        onOpenMedia={setMediaPlayer}
        onOpenSubagentPanel={props.onOpenSubagentPanel}
        pendingPlanArtifact={props.pendingPlanArtifact}
        onPlanArtifactExecute={props.onPlanArtifactExecute}
        onPlanArtifactSuggest={props.onPlanArtifactSuggest}
        onPlanArtifactCancel={props.onPlanArtifactCancel}
        onUserPromptExpandedChange={handleUserPromptExpandedChange}
        onToolPresentationExpandedChange={handleToolPresentationExpandedChange}
        resolveComposerAttachmentMedia={props.resolveComposerAttachmentMedia}
        resolveMediaSourceUrl={props.resolveMediaSourceUrl}
        onLoadToolDetail={props.onLoadToolDetail}
        onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
        onTrace={props.onTrace}
      />
    );
  }, [
    autoExpandThinkingBlockId,
    handleProcessCollapsedChange,
    handleToolPresentationExpandedChange,
    handleUserPromptExpandedChange,
    lockedTimelineActionKeys,
    processCollapseKey,
    props.conversation?.forkSource?.sourceConversationId,
    props.hiddenThinkingBlockIds,
    props.onCopyText,
    props.onEditUserPrompt,
    props.onExtractGeneratedMediaAttachments,
    props.onForkConversation,
    props.onLoadToolDetail,
    props.onOpenSubagentPanel,
    props.onPlanArtifactCancel,
    props.onPlanArtifactExecute,
    props.onPlanArtifactSuggest,
    props.onSelectForkSourceConversation,
    props.onTrace,
    props.parentConversationId,
    props.pendingPlanArtifact,
    props.resolveComposerAttachmentMedia,
    props.resolveMediaSourceUrl,
    props.sessionRunning,
    props.workspacePath,
    t,
  ]);
  return (
    <MarkdownImageWorkspacePathProvider value={props.workspacePath}>
      <ConversationMeasuredCollapseProvider
        initialMemory={measuredCollapseById}
        scopeId={surfaceRestoreId}
        onMemoryChange={handleMeasuredCollapseMemoryChange}
      >
        <section
          className="conversation-panel"
          data-conversation-surface-id={props.conversation?.id ?? props.parentConversationId ?? undefined}
          onScroll={handleConversationScroll}
          ref={props.scrollRef}
        >
          <div className="conversation-scroll-content">
            <div className="timeline">
              {props.error ? (
                <article className="message-card system error-message">
                  <span>system</span>
                  <p>{t("conversation.bodyLoadFailed").replace("{error}", props.error)}</p>
                </article>
              ) : null}
              {props.messages.length === 0 && !props.suppressEmptyPlaceholder ? (
                <article className="message-card system">
                  <span>system</span>
                  <p>{props.conversation ? t("conversation.emptyLoaded") : t("conversation.emptyWaiting")}</p>
                </article>
              ) : null}
              {props.hasMoreHistory ? (
                <div className="conversation-history-load-row">
                  <button
                    className="conversation-history-load-button"
                    disabled={props.historyLoading}
                    onClick={handleLoadMoreHistory}
                    type="button"
                  >
                    {props.historyLoading ? t("conversation.history.loading") : t("conversation.history.loadMore")}
                  </button>
                </div>
              ) : null}
              <ConversationTurnSeatList
                cacheScopeKey={timelineCacheScopeKey}
                renderItem={renderTimelineItem}
                rowKeys={turnOrder}
                rows={turnGroupByKey}
                store={timelineProjectionStore}
              />
              {props.pendingPlanArtifact && !pendingPlanArtifactAlreadyVisible ? (
                <PlanArtifactBlock
                  artifact={props.pendingPlanArtifact}
                  disabled={props.sessionRunning}
                  pending
                  onCancel={props.onPlanArtifactCancel ?? (() => undefined)}
                  onCopyText={props.onCopyText}
                  onExecute={props.onPlanArtifactExecute ?? (() => undefined)}
                  onSuggest={props.onPlanArtifactSuggest ?? (() => undefined)}
                />
              ) : null}
              {props.transientReasoningStatus ?? null}
            </div>
            {props.footer ? (
              <div className="conversation-footer" data-conversation-scroll-footer="true">
                {props.footer}
              </div>
            ) : null}
          </div>
          {detail ? (
            <ConversationDetailModal
              detail={detail}
              onCopyText={props.onCopyText}
              onClose={() => setDetail(null)}
              onLoadToolDetail={props.onLoadToolDetail}
              onOpenMedia={setMediaPlayer}
            />
          ) : null}
          {mediaPlayer ? (
            <GeneratedMediaPlayerWindow attachment={mediaPlayer} onClose={() => setMediaPlayer(null)} />
          ) : null}
        </section>
      </ConversationMeasuredCollapseProvider>
    </MarkdownImageWorkspacePathProvider>
  );
}

// timelineItemContainsPlanArtifact reports whether one timeline item already renders a matching Plan artifact.
// timelineItemContainsPlanArtifact 判断单个时间线项是否已经渲染匹配的计划产物。
function timelineItemContainsPlanArtifact(
  item: ConversationTimelineItem,
  artifact: ConversationPlanArtifact,
): boolean {
  if (item.kind === "item" || item.kind === "final-assistant-meta") {
    return renderItemContainsPlanArtifact(item.item, artifact);
  }
  if (item.kind !== "process") {
    return false;
  }
  return item.process.items.some((processItem) => renderItemContainsPlanArtifact(processItem, artifact))
    || item.process.guidanceItems.some((guidanceItem) => renderItemContainsPlanArtifact(guidanceItem, artifact));
}

// renderItemContainsPlanArtifact reports whether one render row carries a matching Plan artifact block.
// renderItemContainsPlanArtifact 判断单条渲染行是否携带匹配的计划产物块。
function renderItemContainsPlanArtifact(
  item: ConversationRenderItem,
  artifact: ConversationPlanArtifact,
): boolean {
  return item.blocks.some((block) => (
    block.planArtifact
      ? conversationPlanArtifactsMatch(block.planArtifact, artifact)
      : false
  ));
}

// timelineItemHasCompletedContextCompaction detects the durable runtime compaction boundary.
// timelineItemHasCompletedContextCompaction 检测持久运行时压缩边界。
function timelineItemHasCompletedContextCompaction(item: ConversationTimelineItem): boolean {
  if (item.kind === "item" || item.kind === "final-assistant-meta") {
    return renderItemHasCompletedContextCompaction(item.item);
  }
  if (item.kind === "process") {
    return item.process.items.some(renderItemHasCompletedContextCompaction)
      || item.process.guidanceItems.some(renderItemHasCompletedContextCompaction);
  }
  return false;
}

// renderItemHasCompletedContextCompaction checks one projected row for a completed compaction marker.
// renderItemHasCompletedContextCompaction 检查单条投影行是否包含已完成压缩标记。
function renderItemHasCompletedContextCompaction(item: ConversationRenderItem): boolean {
  return item.blocks.some((block) => (
    isContextCompactionBlock(block)
      && block.compactionState?.trim().toLowerCase() === "completed"
  ));
}

// conversationPlanArtifactsMatch compares stable file identity first and content identity second.
// conversationPlanArtifactsMatch 优先比较稳定文件身份，其次比较内容身份。
function conversationPlanArtifactsMatch(
  left: ConversationPlanArtifact,
  right: ConversationPlanArtifact,
): boolean {
  if (left.path && left.updatedAt && left.path === right.path && left.updatedAt === right.updatedAt) {
    return true;
  }
  return left.name === right.name && left.content.trim() === right.content.trim();
}

// ConversationForkSourceSeparator renders the display-only source marker for a forked conversation.
// ConversationForkSourceSeparator 渲染分叉会话的纯展示来源标记。
function ConversationForkSourceSeparator(props: {
  // Localized label shown in the separator center.
  // 展示在分隔线中央的本地化文本。
  label: string;
  // Opens the original source conversation when available.
  // 可用时打开原始来源会话。
  onSelect: () => void;
}) {
  return (
    <button className="conversation-fork-source" onClick={props.onSelect} type="button">
      <span className="conversation-fork-source__line" />
      <span className="conversation-fork-source__label">
        <Split size={14} />
        <span>{props.label}</span>
      </span>
      <span className="conversation-fork-source__line" />
    </button>
  );
}

// groupConversationTimelineItems groups projected rows into stable rendered turns.
// groupConversationTimelineItems 将已投影行组合为稳定的渲染轮次。
function groupConversationTimelineItems(items: ConversationTimelineItem[]): ConversationTurnProjection[] {
  const groups: ConversationTurnProjection[] = [];
  let current: ConversationTurnProjection | null = null;
  const flushCurrent = () => {
    if (!current || current.items.length === 0) {
      current = null;
      return;
    }
    groups.push(current);
    current = null;
  };
  for (const item of items) {
    if (item.kind === "fork-source") {
      flushCurrent();
      groups.push({ key: conversationTimelineItemKey(item), items: [item] });
      continue;
    }
    if (timelineItemStartsConversationTurn(item)) {
      flushCurrent();
      current = { key: conversationTimelineItemKey(item), items: [item] };
      continue;
    }
    if (!current) {
      current = { key: conversationTimelineItemKey(item), items: [item] };
      continue;
    }
    current.items.push(item);
  }
  flushCurrent();
  return groups;
}

// timelineItemStartsConversationTurn reports whether one projected row starts a new visible turn.
// timelineItemStartsConversationTurn 判断单条投影行是否开启新的可见轮次。
function timelineItemStartsConversationTurn(item: ConversationTimelineItem): boolean {
  return item.kind === "item" && item.item.role === "user";
}

// conversationTurnProjectionOwnsCommandSession reports whether one Turn renders live terminal state.
// conversationTurnProjectionOwnsCommandSession 判断单个 Turn 是否渲染实时终端状态。
// Parameters: group is one exact retained Turn projection.
// 参数：group 是一个精确保留的 Turn 投影。
// Returns: true when any direct, process, or guidance block carries a command-session id.
// 返回值：任意直接、过程或引导块携带命令会话标识时返回 true。
function conversationTurnProjectionOwnsCommandSession(group: ConversationTurnProjection): boolean {
  return group.items.some((item) => {
    if (item.kind === "fork-source") {
      return false;
    }
    if (item.kind === "item" || item.kind === "final-assistant-meta") {
      return conversationRenderItemOwnsCommandSession(item.item);
    }
    return item.process.items.some(conversationRenderItemOwnsCommandSession)
      || item.process.guidanceItems.some(conversationRenderItemOwnsCommandSession);
  });
}

// conversationRenderItemOwnsCommandSession checks one projected row for terminal runtime identity.
// conversationRenderItemOwnsCommandSession 检查单个投影行是否包含终端运行时身份。
// Parameters: item is one direct or grouped transcript render row.
// 参数：item 是一条直接或分组转录渲染行。
// Returns: true when at least one block is bound to a command session.
// 返回值：至少一个块绑定命令会话时返回 true。
function conversationRenderItemOwnsCommandSession(item: ConversationRenderItem): boolean {
  return item.blocks.some((block) => Boolean(block.commandSessionId));
}

// timelineItemVisibleInMainTranscript filters hidden business rows before stable Turn grouping.
// timelineItemVisibleInMainTranscript 在稳定 Turn 分组前过滤隐藏业务行。
function timelineItemVisibleInMainTranscript(item: ConversationTimelineItem, hiddenThinkingBlockIds: ReadonlySet<string>): boolean {
  if (item.kind === "fork-source") {
    return true;
  }
  if (item.kind === "item") {
    return !renderItemHiddenByThinkingDisplay(item.item, hiddenThinkingBlockIds);
  }
  if (item.kind === "final-assistant-meta") {
    return !renderItemHiddenByThinkingDisplay(item.item, hiddenThinkingBlockIds);
  }
  const hasVisibleGuidanceItem = item.process.guidanceItems.some((processItem) => !renderItemHiddenByThinkingDisplay(processItem, hiddenThinkingBlockIds));
  const hasVisibleProcessItem = item.process.items.some((processItem) => !renderItemHiddenByThinkingDisplay(processItem, hiddenThinkingBlockIds));
  return hasVisibleGuidanceItem
    || hasVisibleProcessItem
    || item.process.items.length > 0
    || item.process.running
    || item.process.stopped;
}

// stringBooleanMapEqual compares small expansion maps without rebuilding state for equal content.
// stringBooleanMapEqual 比较小型展开映射，避免内容相同仍重建状态。
function stringBooleanMapEqual(left: Map<string, boolean>, right: Map<string, boolean>): boolean {
  if (left.size !== right.size) {
    return false;
  }
  for (const [key, value] of left) {
    if (right.get(key) !== value) {
      return false;
    }
  }
  return true;
}

// stringSetEqual compares expansion id sets by value.
// stringSetEqual 按值比较展开标识集合。
function stringSetEqual(left: Set<string>, right: Set<string>): boolean {
  if (left.size !== right.size) {
    return false;
  }
  for (const value of left) {
    if (!right.has(value)) {
      return false;
    }
  }
  return true;
}
