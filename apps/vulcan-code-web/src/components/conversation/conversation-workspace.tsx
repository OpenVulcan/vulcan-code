import React from "react";
import {
  GeneratedMediaPlayerWindow,
  type GeneratedMediaAttachment,
} from "@/components/media/generated-media";
import {
  ConversationTimelineVirtualRow,
  useConversationTimelineVirtualWindow,
  type ConversationTimelineUserScrollDirection,
  type ConversationTimelineVirtualRestoreState,
} from "@/components/conversation/conversation-virtualization";
import {
  createEmptyConversationSurfaceExpandedState,
  type ConversationSurfaceExpandedState,
} from "@/components/conversation/conversation-layout-restore";
import { MarkdownImageWorkspacePathProvider } from "@/components/conversation/conversation-markdown";
import {
  ConversationDetailModal,
  type ConversationDetail,
  type ConversationDetailRef,
  type ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import {
  ConversationProcessGroup,
  ConversationRenderItemView,
  renderItemHiddenByThinkingOverlay,
} from "@/components/conversation/conversation-render-items";
import type { PlanArtifactActionPayload } from "@/components/conversation/conversation-plan-artifact";
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
  ComposerAttachmentSummary,
  ConversationMessage,
  ConversationSummary,
} from "@/types";

// CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX is the fallback row height before measurement.
// CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX 是测量前使用的兜底行高。
const CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX = 220;

// CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX is the fallback process-group height before measurement.
// CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX 是测量前处理过程分组使用的兜底高度。
const CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX = 320;

// ConversationWorkspace renders the conversation transcript surface.
// ConversationWorkspace 渲染会话转录展示面。
export function ConversationWorkspace(props: {
  // Current conversation summary shown by this surface.
  // 当前展示面显示的会话摘要。
  conversation?: ConversationSummary;
  // Whether the owning conversation is currently running.
  // 所属会话当前是否正在运行。
  sessionRunning: boolean;
  // Thinking block mirrored by the floating overlay and hidden from the transcript.
  // 被浮动思考面板镜像并从转录中隐藏的思考块。
  hiddenThinkingBlockId: string | null;
  // Raw messages used only for empty-state parity.
  // 仅用于保持空状态一致性的原始消息。
  messages: ConversationMessage[];
  // Parsed rows for the same message window.
  // 同一消息窗口对应的已解析展示行。
  renderItems: ConversationRenderItem[];
  // Loading error for the visible transcript body.
  // 可见转录正文的加载错误。
  error: string | null;
  // Parent conversation id used by subagent navigation.
  // 子代理导航使用的父会话标识。
  parentConversationId: string | null;
  // Project workspace path used to resolve relative Markdown media references.
  // 用于解析 Markdown 相对媒体引用的项目工作区路径。
  workspacePath?: string | null;
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
  // Initial bottom distance used before the virtual window has measured this surface.
  // 虚拟窗口测量当前界面前使用的初始底部距离。
  initialScrollDistanceFromBottom: number;
  // Previously measured virtualizer state for this transcript surface.
  // 当前转录界面此前测量过的虚拟列表状态。
  initialVirtualizerState: ConversationTimelineVirtualRestoreState | null;
  // Persists the latest measured virtualizer state for the active surface.
  // 持久保存当前界面最新测量到的虚拟列表状态。
  onVirtualizerStateChange: (surfaceId: string, state: ConversationTimelineVirtualRestoreState | null) => void;
  // Previously captured runtime expansion state for this surface.
  // 当前界面此前捕获的运行期展开状态。
  initialExpandedState: ConversationSurfaceExpandedState | null;
  // Persists process and user-prompt expansion state for this surface.
  // 持久保存当前界面的处理过程与用户提示词展开状态。
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
  const timelineItems = React.useMemo(
    () => projectConversationTimelineItems(props.renderItems, props.conversation, nowMs, props.sessionRunning),
    [props.renderItems, props.conversation, nowMs, props.sessionRunning],
  );
  const visibleTimelineItems = React.useMemo(
    () => timelineItems.filter((item) => timelineItemVisibleInMainTranscript(item, props.hiddenThinkingBlockId)),
    [props.hiddenThinkingBlockId, timelineItems],
  );
  const timelineCacheScopeKey = props.conversation?.id ?? props.parentConversationId ?? "conversation";
  const timelineVirtualizer = useConversationTimelineVirtualWindow(
    visibleTimelineItems,
    props.scrollRef,
    timelineCacheScopeKey,
    props.initialScrollDistanceFromBottom,
    props.initialVirtualizerState,
    props.onVirtualizerStateChange,
    conversationTimelineItemKey,
    conversationTimelineItemEstimatedHeight,
  );
  const autoExpandThinkingBlockId = props.sessionRunning && !props.hiddenThinkingBlockId
    ? latestThinkingBlockIdAfterLastUser(props.renderItems)
    : null;
  const processCollapseKey = React.useCallback((process: ConversationProcessRenderItem) => {
    const surfaceId = props.conversation?.id ?? props.parentConversationId ?? "unknown";
    return `${surfaceId}:${process.id}`;
  }, [props.conversation?.id, props.parentConversationId]);
  const surfaceRestoreId = props.conversation?.id ?? props.parentConversationId ?? null;
  const onExpandedStateChange = props.onExpandedStateChange;
  const persistExpandedState = React.useCallback((
    processCollapsedByKey: Map<string, boolean>,
    expandedUserPromptIds: Set<string>,
  ) => {
    if (!surfaceRestoreId) {
      return;
    }
    onExpandedStateChange(surfaceRestoreId, {
      expandedUserPromptIds: Array.from(expandedUserPromptIds),
      processCollapsedByKey: Object.fromEntries(processCollapsedByKey),
    });
  }, [onExpandedStateChange, surfaceRestoreId]);
  const handleProcessCollapsedChange = React.useCallback((process: ConversationProcessRenderItem, collapsed: boolean) => {
    const key = processCollapseKey(process);
    setProcessCollapseOverrides((current) => {
      const next = new Map(current);
      next.set(key, collapsed);
      persistExpandedState(next, userPromptExpansionOverrides);
      return next;
    });
  }, [persistExpandedState, processCollapseKey, userPromptExpansionOverrides]);
  const handleUserPromptExpandedChange = React.useCallback((messageKey: string, expanded: boolean) => {
    setUserPromptExpansionOverrides((current) => {
      const next = new Set(current);
      if (expanded) {
        next.add(messageKey);
      } else {
        next.delete(messageKey);
      }
      persistExpandedState(processCollapseOverrides, next);
      return next;
    });
  }, [persistExpandedState, processCollapseOverrides]);

  React.useEffect(() => {
    const nextExpandedState = props.initialExpandedState ?? createEmptyConversationSurfaceExpandedState();
    setProcessCollapseOverrides(new Map(Object.entries(nextExpandedState.processCollapsedByKey)));
    setUserPromptExpansionOverrides(new Set(nextExpandedState.expandedUserPromptIds));
  }, [props.initialExpandedState, surfaceRestoreId]);

  React.useEffect(() => {
    if (!props.sessionRunning) {
      return;
    }
    const interval = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, [props.sessionRunning]);

  const handleConversationScroll = React.useCallback(() => {
    timelineVirtualizer.onScroll();
    props.onScrollStateChange();
  }, [props, timelineVirtualizer]);
  // handleConversationUserScrollStart cancels automatic viewport correction before real user scroll input.
  // handleConversationUserScrollStart 在真实用户滚动输入前取消自动视口校正。
  const handleConversationUserScrollStart = React.useCallback((direction: ConversationTimelineUserScrollDirection = "unknown") => {
    timelineVirtualizer.cancelPendingScrollRestore(direction);
    props.onUserScrollStart(direction);
  }, [props, timelineVirtualizer]);
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

  const renderTimelineItem = (timelineItem: ConversationTimelineItem): React.ReactNode => timelineItem.kind === "item" ? (
    <ConversationRenderItemView
      item={timelineItem.item}
      autoExpandThinkingBlockId={autoExpandThinkingBlockId}
      hiddenThinkingBlockId={null}
      parentConversationId={props.parentConversationId}
      sessionRunning={props.sessionRunning}
      expandedUserPromptIds={userPromptExpansionOverrides}
      onCopyText={props.onCopyText}
      onEditUserPrompt={props.onEditUserPrompt}
      onOpenDetail={setDetail}
      onOpenMedia={setMediaPlayer}
      onOpenSubagentPanel={props.onOpenSubagentPanel}
      pendingPlanArtifact={props.pendingPlanArtifact}
      onPlanArtifactExecute={props.onPlanArtifactExecute}
      onPlanArtifactSuggest={props.onPlanArtifactSuggest}
      onPlanArtifactCancel={props.onPlanArtifactCancel}
      onUserPromptExpandedChange={handleUserPromptExpandedChange}
      resolveComposerAttachmentMedia={props.resolveComposerAttachmentMedia}
      resolveMediaSourceUrl={props.resolveMediaSourceUrl}
      onLoadToolDetail={props.onLoadToolDetail}
      onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
      onTrace={props.onTrace}
    />
  ) : (
    <ConversationProcessGroup
      autoExpandThinkingBlockId={autoExpandThinkingBlockId}
      collapsedOverride={processCollapseOverrides.get(processCollapseKey(timelineItem.process))}
      hiddenThinkingBlockId={props.hiddenThinkingBlockId}
      nowMs={nowMs}
      parentConversationId={props.parentConversationId}
      process={timelineItem.process}
      sessionRunning={props.sessionRunning}
      expandedUserPromptIds={userPromptExpansionOverrides}
      onCollapsedChange={(collapsed) => handleProcessCollapsedChange(timelineItem.process, collapsed)}
      onCopyText={props.onCopyText}
      onEditUserPrompt={props.onEditUserPrompt}
      onOpenDetail={setDetail}
      onOpenMedia={setMediaPlayer}
      onOpenSubagentPanel={props.onOpenSubagentPanel}
      pendingPlanArtifact={props.pendingPlanArtifact}
      onPlanArtifactExecute={props.onPlanArtifactExecute}
      onPlanArtifactSuggest={props.onPlanArtifactSuggest}
      onPlanArtifactCancel={props.onPlanArtifactCancel}
      onUserPromptExpandedChange={handleUserPromptExpandedChange}
      resolveComposerAttachmentMedia={props.resolveComposerAttachmentMedia}
      resolveMediaSourceUrl={props.resolveMediaSourceUrl}
      onLoadToolDetail={props.onLoadToolDetail}
      onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
      onTrace={props.onTrace}
    />
  );

  return (
    <MarkdownImageWorkspacePathProvider value={props.workspacePath}>
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
                <p>会话正文加载失败：{props.error}</p>
              </article>
            ) : null}
            {props.messages.length === 0 && !props.suppressEmptyPlaceholder ? (
              <article className="message-card system">
                <span>system</span>
                <p>{props.conversation ? "当前会话正文尚未载入或没有可见消息。" : "尚未选择会话，等待工作区数据载入。"}</p>
              </article>
            ) : null}
            {timelineVirtualizer.window.paddingTop > 0 ? (
              <div aria-hidden="true" className="timeline-virtual-spacer" style={{ height: timelineVirtualizer.window.paddingTop }} />
            ) : null}
            {timelineVirtualizer.window.items.map(({ item: timelineItem }) => {
              const rowKey = conversationTimelineItemKey(timelineItem);
              return timelineVirtualizer.window.measuring ? (
                <ConversationTimelineVirtualRow
                  key={rowKey}
                  rowKey={rowKey}
                  onHeight={timelineVirtualizer.onRowHeight}
                >
                  {renderTimelineItem(timelineItem)}
                </ConversationTimelineVirtualRow>
              ) : (
                <React.Fragment key={rowKey}>
                  {renderTimelineItem(timelineItem)}
                </React.Fragment>
              );
            })}
            {timelineVirtualizer.window.paddingBottom > 0 ? (
              <div aria-hidden="true" className="timeline-virtual-spacer" style={{ height: timelineVirtualizer.window.paddingBottom }} />
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
    </MarkdownImageWorkspacePathProvider>
  );
}

// conversationTimelineItemKey returns a stable measurement key for one timeline entry.
// conversationTimelineItemKey 返回单条时间线记录的稳定测量键。
function conversationTimelineItemKey(item: ConversationTimelineItem): string {
  return item.kind === "item" ? `item:${item.item.id}` : `process:${item.process.id}`;
}

// conversationTimelineItemEstimatedHeight returns a conservative initial height by entry kind.
// conversationTimelineItemEstimatedHeight 按记录类型返回保守初始高度。
function conversationTimelineItemEstimatedHeight(item: ConversationTimelineItem): number {
  return item.kind === "process"
    ? CONVERSATION_VIRTUAL_PROCESS_ESTIMATE_PX
    : CONVERSATION_VIRTUAL_ITEM_ESTIMATE_PX;
}

// timelineItemVisibleInMainTranscript filters virtualized rows before height estimation.
// timelineItemVisibleInMainTranscript 在高度估算前过滤主转录中不可见的虚拟行。
function timelineItemVisibleInMainTranscript(item: ConversationTimelineItem, hiddenThinkingBlockId: string | null): boolean {
  return item.kind !== "item" || !renderItemHiddenByThinkingOverlay(item.item, hiddenThinkingBlockId);
}
