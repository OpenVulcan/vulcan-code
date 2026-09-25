import React from "react";
import { useToolCollapsePreference } from "./tool-collapse-preferences";
import { ThinkingPreview } from "./thinking-preview";
import { MarkdownMessage } from "./conversation-markdown";
import { runConversationControlAction } from "./conversation-control-feedback";
import {
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Clock3,
  Copy,
  Pencil,
  Split,
  Workflow,
} from "lucide-react";

import {
  ComposerAttachmentStrip,
  type ComposerAttachmentPreviewMedia,
} from "@/components/composer/composer-controls";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import {
  type ConversationDetail,
  type ConversationDetailRef,
  type ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import { ConversationBlocksView } from "@/components/conversation/conversation-block-renderer";
import { isMergeableToolBlock } from "@/components/conversation/conversation-classifiers";
import { toolExecutionDisplayState } from "@/components/conversation/conversation-tool-execution-model";
import {
  ConversationMeasuredCollapse,
  useConversationMeasuredExpansion,
} from "@/components/conversation/conversation-measured-collapse";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import type { PlanArtifactActionPayload } from "@/components/conversation/conversation-plan-artifact";
import type { ConversationPlanArtifact } from "@/components/conversation/conversation-model";
import {
  conversationMessageUnixMillis,
  formatShortConversationTimestamp,
  formatWorkProcessDuration,
} from "@/components/conversation/conversation-timeline";
import type {
  ConversationProcessRenderItem,
  ConversationRenderItem,
} from "@/components/conversation/conversation-model";
import { metadataValue } from "@/components/conversation/conversation-metadata";
import type {
  CommandSessionSummary,
  ComposerAttachmentSummary,
  ConversationMessage,
} from "@/types";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";

import { USER_PROMPT_COLLAPSE_LINE_LIMIT } from "./conversation-bubble-limits";


// conversationPresentationMessageIsStreaming reads the explicit V1 adapter lifecycle marker.
// conversationPresentationMessageIsStreaming 读取 V1 适配层显式写入的生命周期标记。
// Parameters: message is one in-memory presentation record projected from a V1 event.
// 参数：message 是由 V1 事件投影得到的一条纯内存展示记录。
// Returns: true only when the adapter marked the canonical event as streaming.
// 返回值：仅当适配层将规范事件标记为流式时返回 true。
function conversationPresentationMessageIsStreaming(message?: ConversationMessage): boolean {
  if (!message?.metadata || typeof message.metadata !== "object" || Array.isArray(message.metadata)) {
    return false;
  }
  return (message.metadata as Record<string, unknown>).stream_state === "active";
}


// ConversationProcessBodyEntry describes one visible process row or one consecutive tool sequence.
// ConversationProcessBodyEntry 描述单条可见过程行或一组连续工具调用。
type ConversationProcessBodyEntry =
  | {
    // Single row that remains outside tool-sequence folding.
    // 保持在工具序列折叠之外的单条记录。
    kind: "item";
    // Visible process item.
    // 可见过程记录。
    item: ConversationRenderItem;
  }
  | {
    // Consecutive tool rows folded behind one summary toggle.
    // 折叠在单个摘要开关后的连续工具记录。
    kind: "tool-sequence";
    // Stable collapse identity derived from the first tool row.
    // 从首条工具记录派生的稳定折叠标识。
    collapseId: string;
    // The live tail or a still-running earlier group stays open by default.
    // 实时末组或仍在执行的前组默认保持展开。
    active: boolean;
    // Original consecutive tool order inside the single persistent group body.
    // 单一稳定分组正文内的原始连续工具顺序。
    items: ConversationRenderItem[];
    // Terminal tool rows, regardless of outcome, that belong inside the measured fold.
    // 不区分执行结果且属于可测量折叠区的终态工具记录。
    completedItems: ConversationRenderItem[];
    // Running tool rows used to keep the group active without splitting its body.
    // 用于保持分组活动的运行中记录，不拆分正文。
    runningItems: ConversationRenderItem[];
  };

// groupVisibleProcessBodyEntries folds only consecutive visible tool rows.
// groupVisibleProcessBodyEntries 仅折叠显示层面连续出现的工具记录。
// Parameters: processId identifies the owning turn, items contains already display-filtered rows,
// sessionRunning resolves tool calls without explicit results, and processRunning determines whether the tail can grow.
// 参数：processId 标识所属回合，items 包含显示过滤后的记录，sessionRunning 判定无明确结果的调用，processRunning 决定末组是否仍可增长。
// Returns: ordered single rows and tool-sequence groups without moving assistant content.
// 返回值：保持顺序的单条记录与工具序列组，且不会移动 Assistant 内容。
export function groupVisibleProcessBodyEntries(
  processId: string,
  items: ConversationRenderItem[],
  sessionRunning: boolean,
  // The owning process, unlike the session, may already be terminal.
  // 所属过程与会话不同，可能已经终结。
  processRunning = sessionRunning,
): ConversationProcessBodyEntry[] {
  // entries stores the final display order of single rows and folded tool sequences.
  // entries 保存单条记录与折叠工具序列的最终显示顺序。
  const entries: ConversationProcessBodyEntry[] = [];
  // pendingToolItems accumulates the current uninterrupted visible tool run.
  // pendingToolItems 累积当前未被打断的可见工具序列。
  let pendingToolItems: ConversationRenderItem[] = [];
  // Start with the first tool so appending another never moves a live card into a different container.
  // 从首个工具开始成组，避免追加工具时把实时卡片移到不同容器。
  const flushPendingTools = (isTail = false) => {
    if (pendingToolItems.length > 0) {
      // firstItem provides a stable identity even while a running sequence gains more tools.
      // firstItem 在运行中序列继续增加工具时仍提供稳定标识。
      const firstItem = pendingToolItems[0];
      // completedItems contains every terminal tool row without classifying its outcome.
      // completedItems 包含所有终态工具记录，不按执行结果分类。
      const completedItems: ConversationRenderItem[] = [];
      // runningItems contains every active tool row that must stay expanded.
      // runningItems 包含所有必须保持展开的活动工具记录。
      const runningItems: ConversationRenderItem[] = [];
      for (const item of pendingToolItems) {
        // toolBlocks is the already-confirmed execution block set for the current tool row.
        // toolBlocks 是当前工具行中已经确认的执行块集合。
        const toolBlocks = item.blocks.filter(isMergeableToolBlock);
        // displayState separates active rows from every terminal outcome.
        // displayState 仅区分活动记录与所有终态结果。
        const displayState = toolExecutionDisplayState(toolBlocks, sessionRunning);
        if (displayState === "running") {
          runningItems.push(item);
        } else {
          completedItems.push(item);
        }
      }
      entries.push({
        kind: "tool-sequence",
        collapseId: `tool-sequence:${processId}:${firstItem.renderKey ?? firstItem.id}`,
        active: processRunning && (isTail || runningItems.length > 0),
        completedItems,
        items: pendingToolItems,
        runningItems,
      });
    }
    pendingToolItems = [];
  };

  for (const item of items) {
    if (item.blocks.length > 0 && item.blocks.every(isMergeableToolBlock)) {
      pendingToolItems.push(item);
      continue;
    }
    flushPendingTools();
    entries.push({ kind: "item", item });
  }
  flushPendingTools(true);
  return entries;
}

// ConversationToolSequenceGroup renders one pure display-only fold for consecutive tool rows.
// ConversationToolSequenceGroup 渲染一个仅用于显示的连续工具记录折叠组。
// Parameters: collapseId preserves expansion memory, item arrays preserve execution state and order,
// renderItem renders the original row; active and collapseEnabled determine the lifecycle default.
// 参数：collapseId 保留展开记忆，各工具数组保留执行状态与顺序，renderItem 渲染原始记录；active 和 collapseEnabled 决定生命周期默认值。
// Returns one summary and one stable ordered body, without a separate retirement layer.
// 返回一个摘要及一个稳定有序正文，不再有独立退场层。
export function ConversationToolSequenceGroup(props: {
  // Whether the owning live process can still append tools to this group.
  // 所属实时过程是否仍可向此分组追加工具。
  active: boolean;
  // Whether automatic tool folding is enabled for this conversation.
  // 当前会话是否启用工具自动折叠。
  collapseEnabled: boolean;
  // Stable identity retained from the very first tool.
  // 从首个工具开始保持不变的标识。
  collapseId: string;
  // Terminal rows used only to calculate the summary.
  // 仅用于计算摘要的终态记录。
  completedItems: ConversationRenderItem[];
  // All tools remain in this one ordered body, including running tools.
  // 所有工具（包括运行中工具）始终留在此有序正文内。
  items: ConversationRenderItem[];
  // Original row renderer; returns one tool card without changing its identity.
  // 原始记录渲染器；返回工具卡且不改变其标识。
  renderItem: (item: ConversationRenderItem) => React.ReactNode;
  // Running rows used only to calculate the summary.
  // 仅用于计算摘要的运行中记录。
  runningItems: ConversationRenderItem[];
}) {
  // t resolves the count labels.
  // t 解析数量标签。
  const { t } = useI18n();
  // Only deliberate clicks enter expansion memory; lifecycle defaults remain derived.
  // 仅主动点击进入展开记忆；生命周期默认值始终由当前事实推导。
  const [rememberedExpanded, setExpanded] = useConversationMeasuredExpansion(props.collapseId, props.active);
  // Only multiple tools with folding enabled need a summary; keep their body stable as tools arrive.
  // 仅开启折叠的多工具序列需要汇总栏；工具陆续到达时保持正文容器稳定。
  const grouped = props.collapseEnabled && props.items.length > 1;
  // Single tools and disabled folding override any remembered collapsed state.
  // 单工具与关闭折叠均优先于任何已记忆的收起状态。
  const expanded = !grouped || rememberedExpanded;
  // summary counts terminal and running tools without splitting their rendering.
  // summary 统计终态与运行中工具，但不拆分它们的渲染位置。
  const summary = props.runningItems.length > 0
    ? t("conversation.toolSequence.runningSummary")
      .replace("{completed}", String(props.completedItems.length))
      .replace("{running}", String(props.runningItems.length))
    : t("conversation.toolSequence.summary").replace("{count}", String(props.completedItems.length));

  return (
    <section className="conversation-tool-sequence" data-expanded={expanded ? "true" : "false"} data-grouped={grouped ? "true" : "false"}>
      {grouped ? <button
        aria-expanded={expanded}
        className="conversation-tool-sequence-toggle"
        onClick={() => setExpanded(!expanded)}
        type="button"
      >
        <Workflow size={14} />
        <span className={cn(props.runningItems.length > 0 && "shimmer")}>{summary}</span>
        {expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
      </button> : null}
      <ConversationMeasuredCollapse collapseId={props.collapseId} expanded={expanded}>
        <div className="conversation-tool-sequence-body">
          {props.items.map(props.renderItem)}
        </div>
      </ConversationMeasuredCollapse>
    </section>
  );
}

// ConversationProcessGroup renders one per-turn process header and its foldable rows.
// ConversationProcessGroup 渲染单个轮次过程标题及其可折叠处理行。
export function ConversationProcessGroup(props: {
  // autoExpandThinkingBlockId keeps parity with normal block rendering.
  // autoExpandThinkingBlockId 保持与普通块渲染一致。
  autoExpandThinkingBlockId: string | null;
  // Optional user-controlled collapsed state remembered by the conversation surface.
  // 会话界面记住的可选用户控制折叠状态。
  collapsedOverride?: boolean;
  // hiddenThinkingBlockIds hides thinking blocks filtered by the display mode.
  // hiddenThinkingBlockIds 隐藏由展示模式过滤的思考块。
  hiddenThinkingBlockIds: ReadonlySet<string>;
  // Current clock tick used for live duration projection.
  // 用于实时耗时投影的当前时钟刻度。
  nowMs: number;
  // Parent conversation id used by subagent navigation.
  // 子代理导航使用的父会话标识。
  parentConversationId: string | null;
  // Process group payload.
  // 过程分组载荷。
  process: ConversationProcessRenderItem;
  // Whether the owning session is currently running.
  // 所属会话是否正在运行。
  sessionRunning: boolean;
  // WebUI-session expansion memory for long user prompt bubbles.
  // 长用户提示词气泡使用的当前 WebUI 会话展开记忆。
  expandedUserPromptIds: Set<string>;
  // WebUI-session expansion memory for custom tool presentation cards.
  // 自定义工具展示卡片使用的当前 WebUI 会话展开记忆。
  expandedToolPresentationIds: Set<string>;
  // Project workspace path used by file-oriented tool presentations.
  // 文件类工具展示使用的项目工作区路径。
  workspacePath?: string | null;
  // Live command-session summaries used by terminal presentations.
  // 终端展示使用的实时命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Records one user-triggered collapsed state change.
  // 记录一次用户触发的折叠状态变化。
  onCollapsedChange: (collapsed: boolean) => void;
  // Copies transcript text using the shell-provided clipboard path.
  // 使用外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void> | void;
  // Opens one backend-backed user prompt edit flow.
  // 打开单个后端支持的用户提示词编辑流程。
  onEditUserPrompt?: (message: ConversationMessage, visiblePrompt: string) => Promise<void> | void;
  // Whether user prompt editing is locked by the current runtime boundary.
  // 用户提示词编辑是否被当前运行时边界锁定。
  userPromptEditDisabled?: boolean;
  // Detail modal opener.
  // 详情弹窗打开器。
  onOpenDetail: (detail: ConversationDetail) => void;
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
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Subagent panel opener.
  // 子代理面板打开器。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Records one user prompt expansion state change.
  // 记录一次用户提示词展开状态变化。
  onUserPromptExpandedChange: (messageKey: string, expanded: boolean) => void;
  // Records one tool presentation expansion state change.
  // 记录一次工具展示展开状态变化。
  onToolPresentationExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Resolves composer attachments into generated-media compatible payloads.
  // 将输入区附件解析为兼容生成媒体的载荷。
  resolveComposerAttachmentMedia: (attachment: ComposerAttachmentSummary) => GeneratedMediaAttachment | null;
  // Resolves a local media file path into a renderable URL.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Loads one hidden tool-detail record.
  // 加载单条隐藏工具详情记录。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Extracts generated media attachments from hidden tool details.
  // 从隐藏工具详情中提取生成媒体附件。
  onExtractGeneratedMediaAttachments: (payload: ToolDetailPayload) => GeneratedMediaAttachment[];
  // Optional trace callback for local diagnostics.
  // 用于本地诊断的可选埋点回调。
  onTrace?: (action: string, detail?: Record<string, unknown>) => void;
}) {
  const { locale, t } = useI18n();
  const fallbackStartedAtMsRef = React.useRef(Date.now());
  React.useEffect(() => {
    fallbackStartedAtMsRef.current = Date.now();
  }, [props.process.id]);
  // Disabling tool folding also prevents the enclosing process from hiding its tools.
  // 关闭工具折叠时，也禁止外层过程隐藏工具。
  const { enabled: toolCollapseEnabled } = useToolCollapsePreference(props.parentConversationId);
  const collapsed = toolCollapseEnabled && (props.collapsedOverride ?? props.process.defaultCollapsed);
  const visibleGuidanceItems = props.process.guidanceItems.filter((item) => !renderItemHiddenByThinkingDisplay(item, props.hiddenThinkingBlockIds));
  const visibleProcessItems = props.process.items.filter((item) => !renderItemHiddenByThinkingDisplay(item, props.hiddenThinkingBlockIds));
  // visibleProcessBodyEntries groups only after thinking-display filtering establishes the visible sequence.
  // visibleProcessBodyEntries 仅在思考显示过滤确定可见序列后执行组合。
  const visibleProcessBodyEntries = groupVisibleProcessBodyEntries(
    props.process.id,
    visibleProcessItems,
    props.sessionRunning,
    props.process.running,
  );
  const visibleCollapsedProcessUserItems = collapsed ? visibleProcessItems.filter(isTurnProcessUserRenderItem) : [];
  // streamingProcessItemIds comes only from explicit backend lifecycle metadata, never row position or run phase.
  // streamingProcessItemIds 仅来自后端显式生命周期元数据，不依赖行位置或运行阶段。
  const streamingProcessItemIds = new Set(
    visibleProcessItems
      .filter((item) => conversationPresentationMessageIsStreaming(item.sourceMessage))
      .map((item) => item.id),
  );
  const liveDurationMs = props.process.running
    ? Math.max(0, props.nowMs - (props.process.startedAtMs ?? fallbackStartedAtMsRef.current))
    : null;
  const displayedDurationMs = props.process.durationMs ?? liveDurationMs;
  const durationLabel = formatWorkProcessDuration(displayedDurationMs, locale);
  // Use the owning process lifecycle so completed work stays terminal while another turn runs.
  // 使用所属过程的生命周期，避免其他回合运行时将已完成工作显示为进行中。
  const workLabel = t(props.process.running ? "conversation.process.working" : "conversation.process.worked");
  // Append the existing measured duration without changing timer or stopped-state semantics.
  // 附加现有实测耗时，不改变计时器或停止状态语义。
  const headerText = durationLabel ? `${workLabel} ${durationLabel}` : workLabel;
  const stoppedText = durationLabel
    ? t("conversation.process.stoppedAfter").replace("{duration}", durationLabel)
    : t("conversation.process.stopped");
  const bodyVisible = !props.process.collapsible || !collapsed;
  const processBodyCollapseId = `process-body:${props.process.id}`;
  // renderProcessBodyItem renders one original process row without changing its order or assistant semantics.
  // renderProcessBodyItem 渲染单条原始过程记录，不改变其顺序或 Assistant 语义。
  // Parameters: item is the original projected row.
  // 参数：item 是原始投影记录。
  // Returns: the standard process-row renderer with live-stream state preserved.
  // 返回值：保留实时流状态的标准过程记录渲染器。
  const renderProcessBodyItem = (item: ConversationRenderItem) => (
    <ConversationRenderItemView
      autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
      hiddenThinkingBlockIds={props.hiddenThinkingBlockIds}
      expandedToolPresentationIds={props.expandedToolPresentationIds}
      workspacePath={props.workspacePath}
      commandSessions={props.commandSessions}
      item={item}
      itemStreaming={streamingProcessItemIds.has(item.id)}
      key={item.id}
      parentConversationId={props.parentConversationId}
      sessionRunning={props.sessionRunning}
      expandedUserPromptIds={props.expandedUserPromptIds}
      onCopyText={props.onCopyText}
      onEditUserPrompt={props.onEditUserPrompt}
      userPromptEditDisabled={props.userPromptEditDisabled}
      onOpenDetail={props.onOpenDetail}
      onOpenMedia={props.onOpenMedia}
      onOpenSubagentPanel={props.onOpenSubagentPanel}
      pendingPlanArtifact={props.pendingPlanArtifact}
      onPlanArtifactExecute={props.onPlanArtifactExecute}
      onPlanArtifactSuggest={props.onPlanArtifactSuggest}
      onPlanArtifactCancel={props.onPlanArtifactCancel}
      onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
      onUserPromptExpandedChange={props.onUserPromptExpandedChange}
      resolveComposerAttachmentMedia={props.resolveComposerAttachmentMedia}
      resolveMediaSourceUrl={props.resolveMediaSourceUrl}
      onLoadToolDetail={props.onLoadToolDetail}
      onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
      onTrace={props.onTrace}
      forceEagerMarkdown
    />
  );
  return (
    <section
      className={cn("work-process-group", props.process.running && "running", props.process.stopped && "stopped")}
      data-process-body-visible={bodyVisible ? "true" : "false"}
      data-process-collapsed={collapsed ? "true" : "false"}
    >
      <button
        aria-expanded={bodyVisible}
        className="work-process-toggle"
        disabled={!props.process.collapsible || !toolCollapseEnabled}
        onClick={() => {
          if (!props.process.collapsible || !toolCollapseEnabled) {
            return;
          }
          dispatchConversationManualViewportInteraction();
          props.onCollapsedChange(!collapsed);
        }}
        type="button"
      >
        <Clock3 className="work-process-icon" size={14} />
        <span>{headerText}</span>
        {props.process.collapsible && toolCollapseEnabled ? (collapsed ? <ChevronRight className="work-process-toggle-icon" size={13} /> : <ChevronDown className="work-process-toggle-icon" size={13} />) : null}
      </button>
      <div className="work-process-guidance">
        {visibleGuidanceItems.map((item) => (
          <ConversationRenderItemView
            autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
            hiddenThinkingBlockIds={props.hiddenThinkingBlockIds}
            expandedToolPresentationIds={props.expandedToolPresentationIds}
            workspacePath={props.workspacePath}
            commandSessions={props.commandSessions}
            item={item}
            itemStreaming={false}
            key={item.id}
            parentConversationId={props.parentConversationId}
            sessionRunning={props.sessionRunning}
            expandedUserPromptIds={props.expandedUserPromptIds}
            onCopyText={props.onCopyText}
            onEditUserPrompt={props.onEditUserPrompt}
            userPromptEditDisabled={props.userPromptEditDisabled}
            onOpenDetail={props.onOpenDetail}
            onOpenMedia={props.onOpenMedia}
            onOpenSubagentPanel={props.onOpenSubagentPanel}
            pendingPlanArtifact={props.pendingPlanArtifact}
            onPlanArtifactExecute={props.onPlanArtifactExecute}
            onPlanArtifactSuggest={props.onPlanArtifactSuggest}
            onPlanArtifactCancel={props.onPlanArtifactCancel}
            onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
            onUserPromptExpandedChange={props.onUserPromptExpandedChange}
            resolveComposerAttachmentMedia={props.resolveComposerAttachmentMedia}
            resolveMediaSourceUrl={props.resolveMediaSourceUrl}
            onLoadToolDetail={props.onLoadToolDetail}
            onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
            onTrace={props.onTrace}
            forceEagerMarkdown
          />
        ))}
        {visibleCollapsedProcessUserItems.map((item) => (
          <ConversationRenderItemView
            autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
            hiddenThinkingBlockIds={props.hiddenThinkingBlockIds}
            expandedToolPresentationIds={props.expandedToolPresentationIds}
            workspacePath={props.workspacePath}
            commandSessions={props.commandSessions}
            item={item}
            itemStreaming={false}
            key={item.id}
            parentConversationId={props.parentConversationId}
            sessionRunning={props.sessionRunning}
            expandedUserPromptIds={props.expandedUserPromptIds}
            onCopyText={props.onCopyText}
            onEditUserPrompt={props.onEditUserPrompt}
            userPromptEditDisabled={props.userPromptEditDisabled}
            onOpenDetail={props.onOpenDetail}
            onOpenMedia={props.onOpenMedia}
            onOpenSubagentPanel={props.onOpenSubagentPanel}
            pendingPlanArtifact={props.pendingPlanArtifact}
            onPlanArtifactExecute={props.onPlanArtifactExecute}
            onPlanArtifactSuggest={props.onPlanArtifactSuggest}
            onPlanArtifactCancel={props.onPlanArtifactCancel}
            onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
            onUserPromptExpandedChange={props.onUserPromptExpandedChange}
            resolveComposerAttachmentMedia={props.resolveComposerAttachmentMedia}
            resolveMediaSourceUrl={props.resolveMediaSourceUrl}
            onLoadToolDetail={props.onLoadToolDetail}
            onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
            onTrace={props.onTrace}
            forceEagerMarkdown
          />
        ))}
      </div>
      <ConversationMeasuredCollapse collapseId={processBodyCollapseId} expanded={bodyVisible}>
        <div className="work-process-body">
          {visibleProcessBodyEntries.map((entry) => (
            entry.kind === "item"
              ? renderProcessBodyItem(entry.item)
              : (
                <ConversationToolSequenceGroup
                  active={entry.active}
                  collapseEnabled={toolCollapseEnabled}
                  collapseId={entry.collapseId}
                  completedItems={entry.completedItems}
                  items={entry.items}
                  key={entry.collapseId}
                  renderItem={renderProcessBodyItem}
                  runningItems={entry.runningItems}
                />
              )
          ))}
          {props.process.stopped ? (
            <div className="work-process-stopped-note">
              {stoppedText}
            </div>
          ) : null}
        </div>
      </ConversationMeasuredCollapse>
    </section>
  );
}

// isTurnProcessUserRenderItem returns whether one user row should stay visible while its process is collapsed.
// isTurnProcessUserRenderItem 返回单条用户行是否应在过程折叠时保持可见。
function isTurnProcessUserRenderItem(item: ConversationRenderItem): boolean {
  if (item.role !== "user") {
    return false;
  }
  return metadataValue(item.userMessage?.metadata ?? item.sourceMessage?.metadata, ["turn_process"]) === true;
}

// ConversationRenderItemView renders one projected transcript item.
// ConversationRenderItemView 渲染单条投影后的转录展示项。
export function ConversationRenderItemView(props: {
  // Projected transcript row.
  // 投影后的转录行。
  item: ConversationRenderItem;
  // Thinking block that should auto expand while running.
  // 运行中应自动展开的思考块。
  autoExpandThinkingBlockId: string | null;
  // Thinking blocks hidden by the display mode.
  // 被展示模式隐藏的思考块。
  hiddenThinkingBlockIds: ReadonlySet<string>;
  // Whether this row is the currently streaming process item.
  // 该行是否为当前流式过程项。
  itemStreaming?: boolean;
  // Whether this row is the final assistant answer for one completed turn.
  // 该行是否为已完成回合的最终 Assistant 回答。
  finalAssistantOutput?: boolean;
  // Force Markdown blocks in externally hidden panels to stay mounted when visible.
  // 强制外层隐藏面板中的 Markdown 块在可见时保持直接挂载。
  forceEagerMarkdown?: boolean;
  // Parent conversation id used by subagent routing and local expansion memory.
  // 子代理路由与本地展开记忆使用的父会话标识。
  parentConversationId: string | null;
  // Whether the owning session is currently running.
  // 所属会话是否正在运行。
  sessionRunning: boolean;
  // WebUI-session expansion memory for long user prompt bubbles.
  // 长用户提示词气泡使用的当前 WebUI 会话展开记忆。
  expandedUserPromptIds: Set<string>;
  // WebUI-session expansion memory for custom tool presentation cards.
  // 自定义工具展示卡片使用的当前 WebUI 会话展开记忆。
  expandedToolPresentationIds: Set<string>;
  // Project workspace path used by file-oriented tool presentations.
  // 文件类工具展示使用的项目工作区路径。
  workspacePath?: string | null;
  // Live command-session summaries used by terminal presentations.
  // 终端展示使用的实时命令会话摘要。
  commandSessions: CommandSessionSummary[];
  // Copies transcript text using the shell-provided clipboard path.
  // 使用外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void> | void;
  // Opens one backend-backed user prompt edit flow.
  // 打开单个后端支持的用户提示词编辑流程。
  onEditUserPrompt?: (message: ConversationMessage, visiblePrompt: string) => Promise<void> | void;
  // Whether user prompt editing is locked by the current runtime boundary.
  // 用户提示词编辑是否被当前运行时边界锁定。
  userPromptEditDisabled?: boolean;
  // Detail modal opener.
  // 详情弹窗打开器。
  onOpenDetail: (detail: ConversationDetail) => void;
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
  // Generated media opener.
  // 生成媒体打开器。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
  // Subagent panel opener.
  // 子代理面板打开器。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Records one user prompt expansion state change.
  // 记录一次用户提示词展开状态变化。
  onUserPromptExpandedChange: (messageKey: string, expanded: boolean) => void;
  // Records one tool presentation expansion state change.
  // 记录一次工具展示展开状态变化。
  onToolPresentationExpandedChange: (presentationId: string, expanded: boolean) => void;
  // Resolves composer attachments into generated-media compatible payloads.
  // 将输入区附件解析为兼容生成媒体的载荷。
  resolveComposerAttachmentMedia: (attachment: ComposerAttachmentSummary) => GeneratedMediaAttachment | null;
  // Resolves a local media file path into a renderable URL.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Loads one hidden tool-detail record.
  // 加载单条隐藏工具详情记录。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Extracts generated media attachments from hidden tool details.
  // 从隐藏工具详情中提取生成媒体附件。
  onExtractGeneratedMediaAttachments: (payload: ToolDetailPayload) => GeneratedMediaAttachment[];
  // Optional trace callback for local diagnostics.
  // 用于本地诊断的可选埋点回调。
  onTrace?: (action: string, detail?: Record<string, unknown>) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  const userBody = React.useMemo(
    () => stripUserPromptHeading(props.item.userMessage?.body ?? ""),
    [props.item.userMessage?.body],
  );
  const userPromptTextRef = React.useRef<HTMLParagraphElement | null>(null);
  const [userPromptOverflowing, setUserPromptOverflowing] = React.useState(false);
  const [userPromptCopied, setUserPromptCopied] = React.useState(false);
  // userPromptCollapsedHeight stores the stable preview height for long user prompts.
  // userPromptCollapsedHeight 保存长用户提示词的稳定预览高度。
  const [userPromptCollapsedHeight, setUserPromptCollapsedHeight] = React.useState(0);
  const userPromptKey = React.useMemo(() => {
    const message = props.item.userMessage;
    const messageId = message?.id ?? props.item.id;
    const surfaceId = props.parentConversationId ?? message?.conversationId ?? "conversation";
    return `${surfaceId}:${messageId}`;
  }, [props.item.id, props.item.userMessage, props.parentConversationId]);
  const [measuredUserPromptExpanded, setMeasuredUserPromptExpanded] = useConversationMeasuredExpansion(
    `user-prompt:${userPromptKey}`,
    props.expandedUserPromptIds.has(userPromptKey),
  );
  const userPromptExpanded = props.expandedUserPromptIds.has(userPromptKey) || measuredUserPromptExpanded;
  // userPromptCollapseExpanded keeps every user prompt on the measured-collapse path while only long prompts fold.
  // userPromptCollapseExpanded 让所有用户提示词都经过稳定测量折叠路径，仅长提示词实际收起。
  const userPromptCollapseExpanded = !userPromptOverflowing || userPromptExpanded;
  // userPromptMeasuredCollapsedHeight avoids hiding short prompts while preserving the long-prompt preview height.
  // userPromptMeasuredCollapsedHeight 避免隐藏短提示词，同时保留长提示词的预览高度。
  const userPromptMeasuredCollapsedHeight = userPromptOverflowing ? userPromptCollapsedHeight : 0;
  const userTimestampMs = conversationMessageUnixMillis(props.item.userMessage);
  const userTimestamp = formatShortConversationTimestamp(props.item.userMessage);
  const handleCopyUserPrompt = React.useCallback(async () => {
    if (!userBody) {
      return;
    }
    setUserPromptCopied(false);
    setUserPromptCopied(await runConversationControlAction(
      () => props.onCopyText(userBody), t("conversation.control.copyFailed"),
    ));
  }, [props, userBody, t]);
  React.useLayoutEffect(() => {
    if (props.item.role !== "user" || !userBody) {
      setUserPromptOverflowing(false);
      return;
    }
    const element = userPromptTextRef.current;
    if (!element) {
      return;
    }
    const measureOverflow = () => {
      const computedStyle = window.getComputedStyle(element);
      const lineHeight = Number.parseFloat(computedStyle.lineHeight);
      const effectiveLineHeight = Number.isFinite(lineHeight) && lineHeight > 0 ? lineHeight : 20;
      const collapsedHeight = effectiveLineHeight * USER_PROMPT_COLLAPSE_LINE_LIMIT;
      setUserPromptCollapsedHeight(collapsedHeight);
      setUserPromptOverflowing(element.scrollHeight > collapsedHeight + 1);
    };
    measureOverflow();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", measureOverflow);
      return () => window.removeEventListener("resize", measureOverflow);
    }
    const observer = new ResizeObserver(measureOverflow);
    observer.observe(element);
    return () => observer.disconnect();
  }, [props.item.role, userBody, userPromptExpanded]);
  if (props.item.role === "user") {
    const attachments = props.item.userMessage?.attachments ?? [];
    return (
      <article className="message-row user">
        {attachments.length ? (
          <ComposerAttachmentStrip
            attachments={attachments}
            resolvePreview={(attachment): ComposerAttachmentPreviewMedia | null => {
              const media = props.resolveComposerAttachmentMedia(attachment);
              if (!media) {
                return null;
              }
              return {
                kind: media.kind,
                sourceUrl: media.kind === "image" ? props.resolveMediaSourceUrl(media.localPath) : undefined,
              };
            }}
            onOpenMedia={(attachment) => {
              const media = props.resolveComposerAttachmentMedia(attachment);
              if (media) {
                props.onOpenMedia(media);
              }
            }}
          />
        ) : null}
        <div className="user-bubble">
          {userBody ? (
            <ConversationMeasuredCollapse
              collapseId={`user-prompt-body:${userPromptKey}`}
              collapsedHeight={userPromptMeasuredCollapsedHeight}
              expanded={userPromptCollapseExpanded}
            >
              <p className="user-prompt-text" ref={userPromptTextRef}>
                {userBody}
              </p>
            </ConversationMeasuredCollapse>
          ) : null}
          {userPromptOverflowing ? (
            <button
              aria-expanded={userPromptExpanded}
              className="user-prompt-expand-toggle"
              onClick={() => {
                setMeasuredUserPromptExpanded(!userPromptExpanded);
                props.onUserPromptExpandedChange(userPromptKey, !userPromptExpanded);
              }}
              type="button"
            >
              <span>{userPromptExpanded ? t("conversation.userPrompt.collapse") : t("conversation.userPrompt.expand")}</span>
              {userPromptExpanded ? (
                <ChevronUp className="user-prompt-expand-icon" size={14} />
              ) : (
                <ChevronDown className="user-prompt-expand-icon" size={14} />
              )}
            </button>
          ) : null}
        </div>
        <div className="user-message-meta">
          {userTimestamp && userTimestampMs ? (
            <time dateTime={new Date(userTimestampMs).toISOString()}>{userTimestamp}</time>
          ) : null}
          {userBody ? (
            <button
              aria-label={t("conversation.userPrompt.copy")}
              className="user-message-copy"
              onClick={() => void handleCopyUserPrompt()}
              onMouseLeave={() => setUserPromptCopied(false)}
              title={t("conversation.userPrompt.copy")}
              type="button"
            >
              {userPromptCopied ? <Check size={14} /> : <Copy size={14} />}
            </button>
          ) : null}
          {props.item.userMessage?.editableUserPrompt && props.onEditUserPrompt && !props.userPromptEditDisabled ? (
            <button
              aria-label={t("conversation.userPrompt.edit")}
              className="user-message-copy"
              onClick={() => void props.onEditUserPrompt?.(props.item.userMessage!, userBody)}
              title={t("conversation.userPrompt.edit")}
              type="button"
            >
              <Pencil size={14} />
            </button>
          ) : null}
        </div>
      </article>
    );
  }

  const visibleBlocks = props.hiddenThinkingBlockIds.size > 0
    ? props.item.blocks.filter((block) => !props.hiddenThinkingBlockIds.has(block.id))
    : props.item.blocks;
  if (visibleBlocks.length === 0) {
    return null;
  }

  return (
    <article className={cn("message-row", props.item.role, props.finalAssistantOutput && "assistant-final-output")}>
      <div className={cn("assistant-flow", props.item.reserveThinkingPrefix && "tool-thinking-aligned", props.item.leadingThinking && (props.item.blocks.some(isMergeableToolBlock) ? "tool-with-thinking" : "assistant-with-thinking"))}>
        {props.item.leadingThinking ? <ThinkingPreview compact>
          {props.item.leadingThinking.map((block) => <MarkdownMessage key={block.id} content={block.body} eager onOpenMedia={props.onOpenMedia} />)}
        </ThinkingPreview> : null}
        {props.item.role === "system" && !props.item.sourceMessage?.agentInteraction ? <span className="message-role-label">SYSTEM</span> : null}
        <div className="conversation-block-content">
        <ConversationBlocksView
          autoExpandThinkingBlockId={props.autoExpandThinkingBlockId}
          blocks={visibleBlocks}
          conversationId={props.item.sourceMessage?.conversationId ?? props.item.userMessage?.conversationId ?? null}
          expandedToolPresentationIds={props.expandedToolPresentationIds}
          workspacePath={props.workspacePath}
          commandSessions={props.commandSessions}
          forceEagerMarkdown={props.forceEagerMarkdown}
          itemStreaming={Boolean(props.itemStreaming)}
          parentConversationId={props.parentConversationId}
          sessionRunning={props.sessionRunning}
          onOpenDetail={props.onOpenDetail}
          onOpenMedia={props.onOpenMedia}
          onOpenSubagentPanel={props.onOpenSubagentPanel}
          pendingPlanArtifact={props.pendingPlanArtifact}
          onCopyText={props.onCopyText}
          onToolPresentationExpandedChange={props.onToolPresentationExpandedChange}
          onPlanArtifactExecute={props.onPlanArtifactExecute}
          onPlanArtifactSuggest={props.onPlanArtifactSuggest}
          onPlanArtifactCancel={props.onPlanArtifactCancel}
          onLoadToolDetail={props.onLoadToolDetail}
          onExtractGeneratedMediaAttachments={props.onExtractGeneratedMediaAttachments}
          onTrace={props.onTrace}
        />
        </div>
      </div>
    </article>
  );
}

// ConversationFinalAssistantMeta renders fixed actions below one final assistant turn.
// ConversationFinalAssistantMeta 渲染单个最终 Assistant 回合下方的固定操作信息。
export function ConversationFinalAssistantMeta(props: {
  // Final assistant row used for raw copy content and timestamp.
  // 用于原文复制内容与时间戳的最终 Assistant 行。
  item: ConversationRenderItem;
  // Copies transcript text using the shell-provided clipboard path.
  // 使用外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void> | void;
  // Creates a forked copy of the owning conversation.
  // 创建所属会话的分叉副本。
  onForkConversation?: (conversationId: string, anchorMessageId?: string) => Promise<boolean> | boolean;
}) {
  const { t } = useI18n();
  const finalAssistantRawText = finalAssistantOutputText(props.item);
  const finalAssistantTimestampMs = conversationMessageUnixMillis(props.item.sourceMessage);
  const finalAssistantTimestamp = formatShortConversationTimestamp(props.item.sourceMessage);
  const conversationId = props.item.sourceMessage?.conversationId ?? null;
  const forkAnchorMessageId = props.item.sourceMessage?.id ?? undefined;
  const [finalAssistantCopied, setFinalAssistantCopied] = React.useState(false);
  const handleCopyFinalAssistantOutput = React.useCallback(async () => {
    if (!finalAssistantRawText) {
      return;
    }
    setFinalAssistantCopied(false);
    setFinalAssistantCopied(await runConversationControlAction(
      () => props.onCopyText(finalAssistantRawText), t("conversation.control.copyFailed"),
    ));
  }, [finalAssistantRawText, props, t]);
  const handleForkConversation = React.useCallback(async () => {
    if (!conversationId) {
      return;
    }
    await props.onForkConversation?.(conversationId, forkAnchorMessageId);
  }, [conversationId, forkAnchorMessageId, props]);
  return (
    <div className="assistant-final-meta">
      {finalAssistantRawText ? (
        <button
          aria-label={finalAssistantCopied ? t("conversation.action.copiedFinal") : t("conversation.action.copyFinal")}
          className="assistant-final-meta-button"
          onClick={() => void handleCopyFinalAssistantOutput()}
          onMouseLeave={() => setFinalAssistantCopied(false)}
          title={finalAssistantCopied ? t("conversation.action.copiedFinal") : t("conversation.action.copyFinal")}
          type="button"
        >
          {finalAssistantCopied ? <Check size={13} /> : <Copy size={13} />}
        </button>
      ) : null}
      {conversationId && props.onForkConversation ? (
        <button
          aria-label={t("conversation.action.fork")}
          className="assistant-final-meta-button"
          onClick={() => void handleForkConversation()}
          title={t("conversation.action.fork")}
          type="button"
        >
          <Split size={13} />
        </button>
      ) : null}
      {finalAssistantTimestamp && finalAssistantTimestampMs ? (
        <time dateTime={new Date(finalAssistantTimestampMs).toISOString()}>{finalAssistantTimestamp}</time>
      ) : null}
    </div>
  );
}

// renderItemHiddenByThinkingDisplay hides a render row when all its thinking blocks are display-filtered.
// renderItemHiddenByThinkingDisplay 在一条渲染行的思考块全部被展示策略过滤时隐藏整行。
export function renderItemHiddenByThinkingDisplay(item: ConversationRenderItem, hiddenThinkingBlockIds: ReadonlySet<string>): boolean {
  return hiddenThinkingBlockIds.size > 0
    && item.blocks.length > 0
    && item.blocks.every((block) => block.kind === "thinking" && hiddenThinkingBlockIds.has(block.id));
}

// stripUserPromptHeading removes legacy transcript headings from visible user bubbles.
// stripUserPromptHeading 从可见用户气泡中移除旧版转录标题。
export function stripUserPromptHeading(body: string): string {
  return body
    .replace(/^\s*(?:#{1,6}\s*)?(?:\*\*)?(?:用户指令|User Prompt|USER)(?:\*\*)?\s*\n+/i, "")
    .trim();
}

// finalAssistantOutputText returns the raw Markdown source for one final assistant row.
// finalAssistantOutputText 返回单条最终 Assistant 行的 Markdown 原文。
export function finalAssistantOutputText(item: ConversationRenderItem): string {
  const sourceBody = item.sourceMessage?.body?.trim();
  if (sourceBody) {
    return sourceBody;
  }
  return item.blocks
    .filter((block) => block.kind === "markdown")
    .map((block) => block.body.trim())
    .filter(Boolean)
    .join("\n\n");
}

