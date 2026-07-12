import React from "react";
import {
  GeneratedMediaPlayerWindow,
  type GeneratedMediaAttachment,
} from "@/components/media/generated-media";
import {
  conversationScrollToDistanceFromBottom,
  type ConversationTimelineUserScrollDirection,
} from "@/components/conversation/chat-area-scroll";
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
import { CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT } from "@/components/conversation/conversation-layout-events";
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

// CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX is the fallback row height before measurement.
// CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX 是测量前使用的兜底行高。
const CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX = 220;

// CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX is the fallback process-group height before measurement.
// CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX 是测量前处理过程分组使用的兜底高度。
const CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX = 320;

// CONVERSATION_VIRTUAL_FINAL_META_ESTIMATE_PX is the fallback final-answer metadata row height.
// CONVERSATION_VIRTUAL_FINAL_META_ESTIMATE_PX 是最终回答元信息行测量前使用的兜底高度。
const CONVERSATION_VIRTUAL_FINAL_META_ESTIMATE_PX = 28;
// CONVERSATION_VIRTUAL_FORK_SOURCE_ESTIMATE_PX is the fallback fork-source separator row height.
// CONVERSATION_VIRTUAL_FORK_SOURCE_ESTIMATE_PX 是分叉来源分隔行测量前使用的兜底行高。
const CONVERSATION_VIRTUAL_FORK_SOURCE_ESTIMATE_PX = 40;

// CONVERSATION_TURN_RESTORE_FRAME_COUNT limits repeated restore frames after a surface switch.
// CONVERSATION_TURN_RESTORE_FRAME_COUNT 限制界面切换后的重复恢复帧数。
const CONVERSATION_TURN_RESTORE_FRAME_COUNT = 4;

// ConversationTurnGroup represents one stable rendered conversation turn shell.
// ConversationTurnGroup 表示单个稳定渲染的会话轮次外壳。
interface ConversationTurnGroup {
  // Stable group key used for shell identity and height memory.
  // 用于外壳身份与高度记忆的稳定分组键。
  key: string;
  // Timeline items that belong to this rendered turn.
  // 归属于该渲染轮次的时间线条目。
  items: ConversationTimelineItem[];
}

// ConversationWorkspaceInitialScrollRestore assigns initial viewport restoration to exactly one owner.
// ConversationWorkspaceInitialScrollRestore 将初始视口恢复分配给唯一所有者。
type ConversationWorkspaceInitialScrollRestore =
  | {
      // Parent owns initial restoration, so workspace distance input is forbidden.
      // 父级拥有初始恢复，因此禁止工作区距离输入。
      restoreInitialScrollDistance: false;
      initialScrollDistanceFromBottom?: never;
    }
  | {
      // Workspace owns initial restoration and requires an exact bottom distance.
      // 工作区拥有初始恢复，并要求精确底部距离。
      restoreInitialScrollDistance: true;
      initialScrollDistanceFromBottom: number;
    };

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
  // Called before user-driven scrolling mutates the transcript viewport.
  // 用户驱动滚动改变转录视口前调用。
  onUserScrollStart: (direction?: ConversationTimelineUserScrollDirection) => void;
  // Opens a routed subagent panel.
  // 打开已路由的子代理面板。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Scroll container ref owned by the shell.
  // 由外壳持有的滚动容器引用。
  scrollRef: React.RefObject<HTMLElement | null>;
  // Previously measured turn-shell heights for the active surface.
  // 当前界面此前测量的轮次外壳高度。
  initialTurnHeightsByKey?: Readonly<Record<string, number>> | null;
  // Persists one measured turn-shell height for an owning surface.
  // 为所属界面保存一条轮次外壳实测高度。
  onTurnHeightChange?: (surfaceId: string, turnKey: string, height: number) => void;
  // Removes stored turn heights that no longer belong to the active surface projection.
  // 删除已不属于当前界面投影的轮次高度。
  onRetainTurnHeights?: (surfaceId: string, liveTurnKeys: ReadonlySet<string>) => void;
  // Previously captured runtime expansion state for this surface.
  // 当前界面此前捕获的运行期展开状态。
  initialExpandedState: ConversationSurfaceExpandedState | null;
  // Persists process, user-prompt, and tool-presentation expansion state for this surface.
  // 持久保存当前界面的处理过程、用户提示词与工具展示展开状态。
  onExpandedStateChange: (surfaceId: string, state: ConversationSurfaceExpandedState) => void;
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
} & ConversationWorkspaceInitialScrollRestore) {
  const { t } = useI18n();
  // detail stores the currently opened transcript detail modal.
  // detail 保存当前打开的转录详情弹窗。
  const [detail, setDetail] = React.useState<ConversationDetail | null>(null);
  // mediaPlayer stores the generated media currently opened in the independent player window.
  // mediaPlayer 保存当前在独立播放窗口中打开的生成媒体。
  const [mediaPlayer, setMediaPlayer] = React.useState<GeneratedMediaAttachment | null>(null);
  const [nowMs, setNowMs] = React.useState(() => Date.now());
  // processCollapseOverrides stores user-toggled process fold states for the current UI session.
  // processCollapseOverrides 存储当前 UI 会话中用户手动切换的处理过程折叠状态。
  const [processCollapseOverrides, setProcessCollapseOverrides] = React.useState<Map<string, boolean>>(
    () => new Map(Object.entries(props.initialExpandedState?.processCollapsedByKey ?? {})),
  );
  // userPromptExpansionOverrides mirrors WebUI-session-only user prompt expansion memory for virtualized rows.
  // userPromptExpansionOverrides 镜像仅当前 WebUI 会话有效的用户提示词展开记忆，供虚拟行复挂载使用。
  const [userPromptExpansionOverrides, setUserPromptExpansionOverrides] = React.useState<Set<string>>(
    () => new Set(props.initialExpandedState?.expandedUserPromptIds ?? []),
  );
  // toolPresentationExpansionOverrides mirrors WebUI-session-only tool card expansion memory for virtualized rows.
  // toolPresentationExpansionOverrides 镜像仅当前 WebUI 会话有效的工具卡片展开记忆，供虚拟行复挂载使用。
  const [toolPresentationExpansionOverrides, setToolPresentationExpansionOverrides] = React.useState<Set<string>>(
    () => new Set(props.initialExpandedState?.expandedToolPresentationIds ?? []),
  );
  // measuredCollapseById mirrors stable collapse state and height memory for the current surface.
  // measuredCollapseById 镜像当前界面的稳定折叠状态与高度记忆。
  const [measuredCollapseById, setMeasuredCollapseById] = React.useState<ConversationMeasuredCollapseMap>(
    () => ({ ...(props.initialExpandedState?.measuredCollapseById ?? {}) }),
  );
  const surfaceRestoreId = props.conversation?.id ?? props.parentConversationId ?? null;
  const expandedStateSurfaceRestoreIdRef = React.useRef(surfaceRestoreId);
  const timelineItems = React.useMemo(
    () => projectConversationTimelineItems(props.renderItems, props.conversation, nowMs, props.sessionRunning),
    [props.renderItems, props.conversation, nowMs, props.sessionRunning],
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
  // lockedTimelineActionKeys identifies rows before the latest completed compaction runtime boundary.
  // lockedTimelineActionKeys 标识最后一次成功压缩运行时边界之前的只读行。
  const lockedTimelineActionKeys = React.useMemo(() => {
    let latestCompletedCompactionIndex = -1;
    visibleTimelineItems.forEach((item, index) => {
      if (timelineItemHasCompletedContextCompaction(item)) {
        latestCompletedCompactionIndex = index;
      }
    });
    if (latestCompletedCompactionIndex < 0) {
      return new Set<string>();
    }
    return new Set(
      visibleTimelineItems
        .slice(0, latestCompletedCompactionIndex + 1)
        .map(conversationTimelineItemKey),
    );
  }, [visibleTimelineItems]);
  const timelineCacheScopeKey = props.conversation?.id ?? props.parentConversationId ?? "conversation";
  // ownedInitialScrollDistance is exact only when the workspace is the declared restoration owner.
  // ownedInitialScrollDistance 仅在工作区是已声明恢复所有者时保持精确。
  const ownedInitialScrollDistance = props.restoreInitialScrollDistance
    ? props.initialScrollDistanceFromBottom
    : 0;
  const turnGroups = React.useMemo(
    () => groupConversationTimelineItems(visibleTimelineItems),
    [visibleTimelineItems],
  );
  // liveTurnKeys is the authoritative set of turn shells in the current projection.
  // liveTurnKeys 是当前投影中轮次外壳的权威键集合。
  const liveTurnKeys = React.useMemo(
    () => new Set(turnGroups.map((group) => group.key)),
    [turnGroups],
  );
  // Retention removes measurements only after the current projection establishes its authoritative keys.
  // 仅当当前投影建立权威键后，保留流程才删除旧测量值。
  React.useEffect(() => {
    props.onRetainTurnHeights?.(timelineCacheScopeKey, liveTurnKeys);
  }, [liveTurnKeys, props.onRetainTurnHeights, timelineCacheScopeKey]);
  const turnRestoreFrameIdsRef = React.useRef<number[]>([]);
  // restoredTurnSurfaceKeyRef tracks the last surface actually restored by this workspace.
  // restoredTurnSurfaceKeyRef 跟踪最近一个由当前工作区实际恢复的界面。
  const restoredTurnSurfaceKeyRef = React.useRef<string | null>(null);
  // clearTurnSurfaceRestoreFrames cancels pending surface-scoped scroll restore frames.
  // clearTurnSurfaceRestoreFrames 取消待执行的界面级滚动恢复帧。
  const clearTurnSurfaceRestoreFrames = React.useCallback(() => {
    for (const frameId of turnRestoreFrameIdsRef.current) {
      window.cancelAnimationFrame(frameId);
    }
    turnRestoreFrameIdsRef.current = [];
  }, []);
  // restoreTurnSurfaceBottomDistance restores the saved bottom distance for the active surface.
  // restoreTurnSurfaceBottomDistance 为当前界面恢复已保存的底部距离。
  const restoreTurnSurfaceBottomDistance = React.useCallback(() => {
    const element = props.scrollRef.current;
    if (!element || element.dataset.conversationSurfaceId !== timelineCacheScopeKey) {
      return;
    }
    conversationScrollToDistanceFromBottom(element, ownedInitialScrollDistance);
  }, [ownedInitialScrollDistance, props.scrollRef, timelineCacheScopeKey]);
  React.useLayoutEffect(() => {
    if (!props.restoreInitialScrollDistance) {
      restoredTurnSurfaceKeyRef.current = null;
      clearTurnSurfaceRestoreFrames();
      return;
    }
    if (restoredTurnSurfaceKeyRef.current === timelineCacheScopeKey) {
      return;
    }
    restoredTurnSurfaceKeyRef.current = timelineCacheScopeKey;
    clearTurnSurfaceRestoreFrames();
    restoreTurnSurfaceBottomDistance();
    // runRestoreFrame schedules exactly the remaining number of workspace-owned correction frames.
    // runRestoreFrame 精确调度剩余数量的工作区所有校正帧。
    // Parameters: remainingFrames is the positive number of callbacks still required.
    // 参数：remainingFrames 是仍需执行的正回调数量。
    // Returns: nothing; zero remaining frames terminate recursion without scheduling.
    // 返回值：无；剩余帧为零时终止递归且不再调度。
    const runRestoreFrame = (remainingFrames: number) => {
      if (remainingFrames <= 0) {
        return;
      }
      const frameId = window.requestAnimationFrame(() => {
        turnRestoreFrameIdsRef.current = turnRestoreFrameIdsRef.current.filter((candidate) => candidate !== frameId);
        restoreTurnSurfaceBottomDistance();
        runRestoreFrame(remainingFrames - 1);
      });
      turnRestoreFrameIdsRef.current.push(frameId);
    };
    runRestoreFrame(CONVERSATION_TURN_RESTORE_FRAME_COUNT);
    return clearTurnSurfaceRestoreFrames;
  }, [clearTurnSurfaceRestoreFrames, props.restoreInitialScrollDistance, restoreTurnSurfaceBottomDistance, timelineCacheScopeKey]);
  const autoExpandThinkingBlockId = (props.autoExpandStreamingThinking ?? true)
    && props.sessionRunning
    && props.hiddenThinkingBlockIds.size === 0
    ? latestThinkingBlockIdAfterLastUser(props.renderItems)
    : null;
  const processCollapseKey = React.useCallback((process: ConversationProcessRenderItem) => {
    const surfaceId = props.conversation?.id ?? props.parentConversationId ?? "unknown";
    return `${surfaceId}:${process.id}`;
  }, [props.conversation?.id, props.parentConversationId]);
  const onExpandedStateChange = props.onExpandedStateChange;
  const persistExpandedState = React.useCallback((
    processCollapsedByKey: Map<string, boolean>,
    expandedUserPromptIds: Set<string>,
    expandedToolPresentationIds: Set<string>,
    nextMeasuredCollapseById: ConversationMeasuredCollapseMap = measuredCollapseById,
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
  }, [measuredCollapseById, onExpandedStateChange, surfaceRestoreId]);
  const handleMeasuredCollapseMemoryChange = React.useCallback((nextMeasuredCollapseById: ConversationMeasuredCollapseMap) => {
    setMeasuredCollapseById(nextMeasuredCollapseById);
    persistExpandedState(processCollapseOverrides, userPromptExpansionOverrides, toolPresentationExpansionOverrides, nextMeasuredCollapseById);
  }, [persistExpandedState, processCollapseOverrides, toolPresentationExpansionOverrides, userPromptExpansionOverrides]);
  const handleProcessCollapsedChange = React.useCallback((process: ConversationProcessRenderItem, collapsed: boolean) => {
    const key = processCollapseKey(process);
    setProcessCollapseOverrides((current) => {
      const next = new Map(current);
      next.set(key, collapsed);
      persistExpandedState(next, userPromptExpansionOverrides, toolPresentationExpansionOverrides);
      return next;
    });
  }, [persistExpandedState, processCollapseKey, toolPresentationExpansionOverrides, userPromptExpansionOverrides]);
  const handleUserPromptExpandedChange = React.useCallback((messageKey: string, expanded: boolean) => {
    setUserPromptExpansionOverrides((current) => {
      const next = new Set(current);
      if (expanded) {
        next.add(messageKey);
      } else {
        next.delete(messageKey);
      }
      persistExpandedState(processCollapseOverrides, next, toolPresentationExpansionOverrides);
      return next;
    });
  }, [persistExpandedState, processCollapseOverrides, toolPresentationExpansionOverrides]);
  const handleToolPresentationExpandedChange = React.useCallback((presentationId: string, expanded: boolean) => {
    setToolPresentationExpansionOverrides((current) => {
      const next = new Set(current);
      if (expanded) {
        next.add(presentationId);
      } else {
        next.delete(presentationId);
      }
      persistExpandedState(processCollapseOverrides, userPromptExpansionOverrides, next);
      return next;
    });
  }, [persistExpandedState, processCollapseOverrides, userPromptExpansionOverrides]);

  React.useEffect(() => {
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

  React.useEffect(() => {
    if (!props.sessionRunning) {
      return;
    }
    const interval = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, [props.sessionRunning]);

  const handleConversationScroll = React.useCallback(() => {
    props.onScrollStateChange();
  }, [props.onScrollStateChange]);
  // handleConversationUserScrollStart cancels automatic viewport correction before real user scroll input.
  // handleConversationUserScrollStart 在真实用户滚动输入前取消自动视口校正。
  const handleConversationUserScrollStart = React.useCallback((direction: ConversationTimelineUserScrollDirection = "unknown") => {
    clearTurnSurfaceRestoreFrames();
    props.onUserScrollStart(direction);
  }, [clearTurnSurfaceRestoreFrames, props.onUserScrollStart]);
  // handleConversationWheel records the wheel direction before the scroll event changes viewport metrics.
  // handleConversationWheel 在滚动事件改变视口指标前记录滚轮方向。
  const handleConversationWheel = React.useCallback((event: React.WheelEvent<HTMLElement>) => {
    handleConversationUserScrollStart(event.deltaY < 0 ? "up" : "down");
  }, [handleConversationUserScrollStart]);
  // handleConversationPointerDown only treats scrollbar-gutter presses as scroll intent.
  // handleConversationPointerDown 只将滚动条边缘按下视为滚动意图。
  const handleConversationPointerDown = React.useCallback((event: React.PointerEvent<HTMLElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (event.clientX < rect.right - 20) {
      return;
    }
    handleConversationUserScrollStart("unknown");
  }, [handleConversationUserScrollStart]);

  React.useEffect(() => {
    const handleManualViewportInteraction = () => {
      clearTurnSurfaceRestoreFrames();
      props.onUserScrollStart("unknown");
    };
    window.addEventListener(CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT, handleManualViewportInteraction);
    return () => window.removeEventListener(CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT, handleManualViewportInteraction);
  }, [clearTurnSurfaceRestoreFrames, props.onUserScrollStart]);

  // handleLoadMoreHistory preserves the first visible position while older turns are prepended.
  // handleLoadMoreHistory 在前置更早轮次时保持当前可见位置。
  const handleLoadMoreHistory = React.useCallback(async () => {
    const conversationId = props.conversation?.id ?? props.parentConversationId;
    const element = props.scrollRef.current;
    if (!conversationId || !element || props.historyLoading) {
      return;
    }
    const previousScrollHeight = element.scrollHeight;
    const previousScrollTop = element.scrollTop;
    const loaded = await props.onLoadMoreHistory(conversationId);
    if (!loaded) {
      return;
    }
    window.requestAnimationFrame(() => {
      const nextElement = props.scrollRef.current;
      if (!nextElement) {
        return;
      }
      const heightDelta = nextElement.scrollHeight - previousScrollHeight;
      nextElement.scrollTop = previousScrollTop + Math.max(0, heightDelta);
      props.onScrollStateChange();
    });
  }, [
    props.conversation?.id,
    props.historyLoading,
    props.onLoadMoreHistory,
    props.onScrollStateChange,
    props.parentConversationId,
    props.scrollRef,
  ]);

  const renderTimelineItem = (timelineItem: ConversationTimelineItem): React.ReactNode => {
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
          expandedUserPromptIds={userPromptExpansionOverrides}
          expandedToolPresentationIds={toolPresentationExpansionOverrides}
          workspacePath={props.workspacePath}
          commandSessions={props.commandSessions}
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
    return (
      <ConversationProcessGroup
        autoExpandThinkingBlockId={autoExpandThinkingBlockId}
        collapsedOverride={processCollapseOverrides.get(processCollapseKey(timelineItem.process))}
        hiddenThinkingBlockIds={props.hiddenThinkingBlockIds}
        nowMs={nowMs}
        parentConversationId={props.parentConversationId}
        process={timelineItem.process}
        sessionRunning={props.sessionRunning}
        expandedUserPromptIds={userPromptExpansionOverrides}
        expandedToolPresentationIds={toolPresentationExpansionOverrides}
        workspacePath={props.workspacePath}
        commandSessions={props.commandSessions}
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
  };

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
          onPointerDown={handleConversationPointerDown}
          onScroll={handleConversationScroll}
          onTouchStart={() => handleConversationUserScrollStart("unknown")}
          onWheel={handleConversationWheel}
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
              {turnGroups.map((group) => (
                <ConversationTurnShell
                  estimatedHeight={conversationTurnGroupEstimatedHeight(group)}
                  initialHeight={props.initialTurnHeightsByKey?.[group.key] ?? null}
                  key={`${timelineCacheScopeKey}:${group.key}`}
                  onHeightChange={props.onTurnHeightChange}
                  shellKey={group.key}
                  surfaceKey={timelineCacheScopeKey}
                >
                  {group.items.map((timelineItem) => (
                    <React.Fragment key={conversationTimelineItemKey(timelineItem)}>
                      {renderTimelineItem(timelineItem)}
                    </React.Fragment>
                  ))}
                </ConversationTurnShell>
              ))}
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

// ConversationTurnShell keeps one loaded turn's outer height stable across remounts.
// ConversationTurnShell 在重新挂载之间保持单个已加载轮次的外壳高度稳定。
// Parameters: props identifies the surface and turn, supplies initial height guidance, and reports measurements.
// 参数：props 标识界面与轮次，提供初始高度指引并上报测量值。
// Returns: one measured section that releases its temporary minimum height after the first measurement.
// 返回值：一个实测 section，在首次测量后释放临时最小高度。
function ConversationTurnShell(props: {
  // Stable surface key used to isolate height memory between conversations.
  // 用于在不同会话之间隔离高度记忆的稳定界面键。
  surfaceKey: string;
  // Stable turn shell key inside the surface.
  // 界面内部稳定的轮次外壳键。
  shellKey: string;
  // Conservative height used before the first real measurement.
  // 首次真实测量之前使用的保守高度。
  estimatedHeight: number;
  // Previously measured height restored for this exact turn shell.
  // 当前精确轮次外壳此前测量的恢复高度。
  initialHeight: number | null;
  // Persists a changed measured height in the owning surface memory.
  // 将变化后的实测高度保存到所属界面记忆中。
  onHeightChange?: (surfaceId: string, turnKey: string, height: number) => void;
  // Rendered turn content.
  // 渲染出的轮次内容。
  children: React.ReactNode;
}) {
  // minimumHeight stabilizes the first paint with a measured height or the conservative estimate.
  // minimumHeight 用实测高度或保守估算值稳定首帧绘制。
  const [minimumHeight, setMinimumHeight] = React.useState<number | null>(
    props.initialHeight ?? props.estimatedHeight,
  );
  // measuredHeightRef lets the observer reject identical measurements without reconnecting.
  // measuredHeightRef 让观察器在不重连的前提下拒绝相同测量值。
  const measuredHeightRef = React.useRef<number | null>(props.initialHeight);
  // shellRef points to the measured turn-shell element.
  // shellRef 指向被测量的轮次外壳元素。
  const shellRef = React.useRef<HTMLDivElement | null>(null);

  React.useEffect(() => {
    const element = shellRef.current;
    if (!element) {
      return;
    }
    // updateHeight publishes changed measurements and releases the first-paint stabilizer.
    // updateHeight 发布变化后的测量值，并释放首帧稳定高度。
    // Parameters: none; the callback measures shellRef's current element.
    // 参数：无；回调测量 shellRef 当前元素。
    // Returns: nothing; local state and optional owner memory receive the result.
    // 返回值：无；结果写入本地状态与可选的所有者记忆。
    const updateHeight = () => {
      const nextHeight = Math.ceil(element.getBoundingClientRect().height);
      if (!Number.isFinite(nextHeight) || nextHeight <= 0) {
        return;
      }
      if (measuredHeightRef.current === nextHeight) {
        setMinimumHeight(null);
        return;
      }
      measuredHeightRef.current = nextHeight;
      props.onHeightChange?.(props.surfaceKey, props.shellKey, nextHeight);
      setMinimumHeight(null);
    };
    updateHeight();
    const observer = new ResizeObserver(updateHeight);
    observer.observe(element);
    return () => observer.disconnect();
  }, [props.onHeightChange, props.shellKey, props.surfaceKey]);

  return (
    <section
      className="conversation-turn-shell"
      data-conversation-row-key={props.shellKey}
      data-conversation-turn-shell={props.shellKey}
      ref={shellRef}
      style={minimumHeight === null ? undefined : { minHeight: minimumHeight }}
    >
      {props.children}
    </section>
  );
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

// conversationTimelineItemKey returns a stable measurement key for one timeline entry.
// conversationTimelineItemKey 返回单条时间线记录的稳定测量键。
function conversationTimelineItemKey(item: ConversationTimelineItem): string {
  if (item.kind === "fork-source") {
    return "fork-source";
  }
  if (item.kind === "item") {
    return item.item.renderKey ?? `item:${item.item.id}`;
  }
  if (item.kind === "final-assistant-meta") {
    return `final-assistant-meta:${item.item.renderKey ?? item.item.id}`;
  }
  return item.process.renderKey ?? `process:${item.process.id}`;
}

// groupConversationTimelineItems groups projected rows into stable rendered turns.
// groupConversationTimelineItems 将已投影行组合为稳定的渲染轮次。
function groupConversationTimelineItems(items: ConversationTimelineItem[]): ConversationTurnGroup[] {
  const groups: ConversationTurnGroup[] = [];
  let current: ConversationTurnGroup | null = null;
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

// conversationTurnGroupEstimatedHeight returns a conservative initial shell height.
// conversationTurnGroupEstimatedHeight 返回保守的轮次外壳初始高度。
function conversationTurnGroupEstimatedHeight(group: ConversationTurnGroup): number {
  return group.items.reduce(
    (total, item) => total + conversationTimelineItemEstimatedHeight(item),
    0,
  );
}

// conversationTimelineItemEstimatedHeight returns a conservative initial height by entry kind.
// conversationTimelineItemEstimatedHeight 按记录类型返回保守初始高度。
function conversationTimelineItemEstimatedHeight(item: ConversationTimelineItem): number {
  if (item.kind === "process") {
    return CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX;
  }
  if (item.kind === "fork-source") {
    return CONVERSATION_VIRTUAL_FORK_SOURCE_ESTIMATE_PX;
  }
  if (item.kind === "final-assistant-meta") {
    return CONVERSATION_VIRTUAL_FINAL_META_ESTIMATE_PX;
  }
  return CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX;
}

// timelineItemVisibleInMainTranscript filters virtualized rows before height estimation.
// timelineItemVisibleInMainTranscript 在高度估算前过滤主转录中不可见的虚拟行。
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
