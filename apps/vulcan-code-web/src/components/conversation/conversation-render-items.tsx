import React from "react";
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
  SessionProviderUsageSummary,
} from "@/types";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import { conversationMessageStreamState } from "@/state/conversation-live-stream-store";

// USER_PROMPT_COLLAPSE_LINE_LIMIT limits long user bubbles before the user expands them.
// USER_PROMPT_COLLAPSE_LINE_LIMIT 限制用户手动展开前长用户气泡的显示行数。
const USER_PROMPT_COLLAPSE_LINE_LIMIT = 20;

// TOOL_RETIREMENT_MIN_VISIBLE_MS guarantees that a newly grouped tool remains perceptible before automatic storage.
// TOOL_RETIREMENT_MIN_VISIBLE_MS 保证新进入工具组的工具在自动收纳前具有可感知的最低显示时间。
const TOOL_RETIREMENT_MIN_VISIBLE_MS = 1_000;

// TOOL_RETIREMENT_SETTLE_MS leaves a short completion acknowledgement before the exit motion starts.
// TOOL_RETIREMENT_SETTLE_MS 在退场动画开始前保留短暂的完成确认停留。
const TOOL_RETIREMENT_SETTLE_MS = 200;

// TOOL_RETIREMENT_EXIT_MS matches the visual row-flight and height-collapse duration.
// TOOL_RETIREMENT_EXIT_MS 对齐工具行飞出与高度收拢动画时长。
const TOOL_RETIREMENT_EXIT_MS = 800;

// TOOL_RETIREMENT_DOM_SETTLE_MS lets the browser paint the final zero-height frame before the row is removed from React output.
// TOOL_RETIREMENT_DOM_SETTLE_MS 允许浏览器在工具行退出 React 输出前绘制最终的零高度帧。
const TOOL_RETIREMENT_DOM_SETTLE_MS = 48;

// ToolRetirementPhase describes the pure frontend lifecycle of one successfully completed grouped tool.
// ToolRetirementPhase 描述单个成功完成且已成组工具的纯前端收纳生命周期。
type ToolRetirementPhase = "holding" | "exiting" | "folded";

// ToolRetirementEntry stores compositor-animation deadlines for one grouped tool row.
// ToolRetirementEntry 保存单个工具组行的合成动画截止时间。
interface ToolRetirementEntry {
  // Current frontend retirement phase.
  // 当前前端收纳阶段。
  phase: ToolRetirementPhase;
  // Monotonic time when the exit animation may start.
  // 允许开始退场动画的单调时间。
  exitAt: number;
  // Monotonic time when the row can move into the hidden fold.
  // 工具行可以进入隐藏折叠区的单调时间。
  foldAt: number;
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
    // Original consecutive tool order used by the visible transition layer.
    // 可见过渡层使用的原始连续工具顺序。
    items: ConversationRenderItem[];
    // Terminal tool rows that belong inside the measured fold.
    // 属于可测量折叠区的终态工具记录。
    completedItems: ConversationRenderItem[];
    // Running tool rows that must remain visible below the fold summary.
    // 必须保持显示在折叠摘要下方的运行中工具记录。
    runningItems: ConversationRenderItem[];
    // Failed or stopped tool rows that remain visible for diagnosis.
    // 为便于诊断而持续显示的失败或已停止工具记录。
    persistentItems: ConversationRenderItem[];
  };

// groupVisibleProcessBodyEntries folds only consecutive visible tool rows.
// groupVisibleProcessBodyEntries 仅折叠显示层面连续出现的工具记录。
// Parameters: processId identifies the owning turn, items contains already display-filtered rows,
// and sessionRunning resolves tool calls without an explicit terminal result.
// 参数：processId 标识所属回合，items 包含已经过显示策略过滤的记录，sessionRunning 用于判定尚无明确终态结果的工具调用。
// Returns: ordered single rows and tool-sequence groups without moving assistant content.
// 返回值：保持顺序的单条记录与工具序列组，且不会移动 Assistant 内容。
function groupVisibleProcessBodyEntries(
  processId: string,
  items: ConversationRenderItem[],
  sessionRunning: boolean,
): ConversationProcessBodyEntry[] {
  // entries stores the final display order of single rows and folded tool sequences.
  // entries 保存单条记录与折叠工具序列的最终显示顺序。
  const entries: ConversationProcessBodyEntry[] = [];
  // pendingToolItems accumulates the current uninterrupted visible tool run.
  // pendingToolItems 累积当前未被打断的可见工具序列。
  let pendingToolItems: ConversationRenderItem[] = [];
  // flushPendingTools emits one fold only when at least two visible tool rows are consecutive.
  // flushPendingTools 仅在至少两条可见工具记录连续时生成折叠组。
  const flushPendingTools = () => {
    if (pendingToolItems.length > 1) {
      // firstItem provides a stable identity even while a running sequence gains more tools.
      // firstItem 在运行中序列继续增加工具时仍提供稳定标识。
      const firstItem = pendingToolItems[0];
      // completedItems contains every terminal tool row that can be folded immediately.
      // completedItems 包含所有可立即折叠的终态工具记录。
      const completedItems: ConversationRenderItem[] = [];
      // runningItems contains every active tool row that must stay expanded.
      // runningItems 包含所有必须保持展开的活动工具记录。
      const runningItems: ConversationRenderItem[] = [];
      // persistentItems keeps failed or stopped tools visible instead of automatically hiding important diagnostics.
      // persistentItems 保持失败或已停止工具可见，避免自动隐藏重要诊断信息。
      const persistentItems: ConversationRenderItem[] = [];
      for (const item of pendingToolItems) {
        // toolBlocks is the already-confirmed execution block set for the current tool row.
        // toolBlocks 是当前工具行中已经确认的执行块集合。
        const toolBlocks = item.blocks.filter(isMergeableToolBlock);
        // displayState separates successful retirement from active and diagnostic rows.
        // displayState 将成功收纳记录与活动记录、诊断记录分离。
        const displayState = toolExecutionDisplayState(toolBlocks, sessionRunning);
        if (displayState === "running") {
          runningItems.push(item);
        } else if (displayState === "completed") {
          completedItems.push(item);
        } else {
          persistentItems.push(item);
        }
      }
      entries.push({
        kind: "tool-sequence",
        collapseId: `tool-sequence:${processId}:${firstItem.renderKey ?? firstItem.id}`,
        completedItems,
        items: pendingToolItems,
        persistentItems,
        runningItems,
      });
    } else if (pendingToolItems.length === 1) {
      entries.push({ kind: "item", item: pendingToolItems[0] });
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
  flushPendingTools();
  return entries;
}

// ConversationToolSequenceGroup renders one pure display-only fold for consecutive tool rows.
// ConversationToolSequenceGroup 渲染一个仅用于显示的连续工具记录折叠组。
// Parameters: collapseId preserves expansion memory, item arrays preserve execution state and order,
// renderItem renders the original row, and animateRetirement gates live-only automatic storage.
// 参数：collapseId 保留展开记忆，各工具数组保留执行状态与顺序，renderItem 渲染原始记录，animateRetirement 限制仅实时调用自动收纳。
// Returns: one compact summary, a measured completed-tool fold, and an ordered live retirement layer.
// 返回值：一个紧凑摘要、一个可测量已完成工具折叠区，以及保持顺序的实时收纳层。
function ConversationToolSequenceGroup(props: {
  // Whether newly grouped live tools may enter the automatic retirement lifecycle.
  // 新成组的实时工具是否允许进入自动收纳生命周期。
  animateRetirement: boolean;
  // Stable collapse identity for remount-safe expansion memory.
  // 用于可安全重新挂载展开记忆的稳定折叠标识。
  collapseId: string;
  // Successfully completed tool rows owned by the fold.
  // 折叠区包含的成功完成工具记录。
  completedItems: ConversationRenderItem[];
  // Original consecutive tool order used by the visible transition layer.
  // 可见过渡层使用的原始连续工具顺序。
  items: ConversationRenderItem[];
  // Failed or stopped tool rows that remain visible for diagnosis.
  // 为便于诊断而持续显示的失败或已停止工具记录。
  persistentItems: ConversationRenderItem[];
  // Renders one original tool row without changing its conversation semantics.
  // 渲染单条原始工具记录且不改变其会话语义。
  renderItem: (item: ConversationRenderItem) => React.ReactNode;
  // Active tool rows that remain visible until completion.
  // 完成前始终保持可见的活动工具记录。
  runningItems: ConversationRenderItem[];
}) {
  // t resolves the localized sequence summary.
  // t 解析本地化工具序列摘要。
  const { t } = useI18n();
  // expanded stores remount-safe manual expansion state for this tool sequence.
  // expanded 保存当前工具序列可安全重新挂载的手动展开状态。
  const [expanded, setExpanded] = useConversationMeasuredExpansion(props.collapseId, false);
  // firstVisibleAtRef stores client-observed visibility times without triggering renders.
  // firstVisibleAtRef 保存前端观察到的首次显示时间且不触发渲染。
  const firstVisibleAtRef = React.useRef<Map<string, number>>(new Map());
  // liveObservedIdsRef prevents a process-terminal snapshot from skipping retirement for tools previously seen live.
  // liveObservedIdsRef 防止过程终态快照跳过此前已实时显示工具的收纳动画。
  const liveObservedIdsRef = React.useRef<Set<string>>(new Set());
  // retirementById stores transient phases only for successfully completed grouped tools.
  // retirementById 仅保存成功完成且已成组工具的临时收纳阶段。
  const [retirementById, setRetirementById] = React.useState<Record<string, ToolRetirementEntry>>({});
  // retirementTimerRevision forces deadline recalculation even when a timer fires just before its target and changes no phase.
  // retirementTimerRevision 强制重新计算截止时间，即使计时器在目标前极短触发且没有改变任何阶段。
  const [retirementTimerRevision, setRetirementTimerRevision] = React.useState(0);

  React.useEffect(() => {
    // now is a monotonic frontend timestamp shared by this reconciliation pass.
    // now 是本次协调过程共享的前端单调时间戳。
    const now = performance.now();
    for (const item of props.items) {
      // itemId is the stable projected identity for one tool row.
      // itemId 是单条工具记录的稳定投影标识。
      const itemId = item.renderKey ?? item.id;
      if (!firstVisibleAtRef.current.has(itemId)) {
        firstVisibleAtRef.current.set(itemId, now);
      }
      if (props.animateRetirement) {
        liveObservedIdsRef.current.add(itemId);
      }
    }

    setRetirementById((current) => {
      // completedIds limits retained lifecycle entries to currently successful tools.
      // completedIds 将保留的生命周期记录限制为当前成功工具。
      const completedIds = new Set(props.completedItems.map((item) => item.renderKey ?? item.id));
      // next is a copy-on-write retirement map for this reconciliation.
      // next 是本次协调使用的写时复制收纳映射。
      const next = { ...current };
      // changed prevents no-op state updates from causing additional renders.
      // changed 防止无变化的状态更新触发额外渲染。
      let changed = false;

      for (const itemId of Object.keys(next)) {
        if (!completedIds.has(itemId)) {
          delete next[itemId];
          changed = true;
        }
      }

      for (const item of props.completedItems) {
        // itemId is the stable projected identity for the completed tool row.
        // itemId 是已完成工具记录的稳定投影标识。
        const itemId = item.renderKey ?? item.id;
        // existing preserves an already scheduled exit across process terminal updates.
        // existing 在过程终态更新期间保留已经调度的退场。
        const existing = next[itemId];
        if (expanded) {
          if (!existing || existing.phase !== "folded") {
            next[itemId] = {
              phase: "folded",
              exitAt: existing?.exitAt ?? now,
              foldAt: existing?.foldAt ?? now,
            };
            changed = true;
          }
          continue;
        }
        if (existing) {
          continue;
        }
        // shouldAnimate requires a live frontend observation and a currently collapsed group.
        // shouldAnimate 要求工具曾被前端实时观察且工具组当前处于折叠状态。
        const shouldAnimate = props.animateRetirement || liveObservedIdsRef.current.has(itemId);
        if (!shouldAnimate) {
          next[itemId] = { phase: "folded", exitAt: now, foldAt: now };
          changed = true;
          continue;
        }
        // firstVisibleAt anchors the one-second minimum to frontend visibility rather than backend execution time.
        // firstVisibleAt 将一秒最低显示时间锚定到前端可见时刻而非后端执行时间。
        const firstVisibleAt = firstVisibleAtRef.current.get(itemId) ?? now;
        // exitAt honors both minimum visibility and a short completion acknowledgement.
        // exitAt 同时满足最低显示时间与短暂完成确认停留。
        const exitAt = Math.max(
          firstVisibleAt + TOOL_RETIREMENT_MIN_VISIBLE_MS,
          now + TOOL_RETIREMENT_SETTLE_MS,
        );
        next[itemId] = {
          phase: "holding",
          exitAt,
          foldAt: exitAt + TOOL_RETIREMENT_EXIT_MS + TOOL_RETIREMENT_DOM_SETTLE_MS,
        };
        changed = true;
      }
      return changed ? next : current;
    });
  }, [expanded, props.animateRetirement, props.completedItems, props.items]);

  React.useEffect(() => {
    if (expanded) {
      return;
    }
    // now is the current monotonic time used to select the nearest lifecycle deadline.
    // now 是用于选择最近生命周期截止点的当前单调时间。
    const now = performance.now();
    // nextDeadline stores the nearest holding or exiting transition time.
    // nextDeadline 保存最近的等待或退场阶段转换时间。
    let nextDeadline: number | null = null;
    for (const entry of Object.values(retirementById)) {
      if (entry.phase === "folded") {
        continue;
      }
      // deadline selects the next boundary for the current entry phase.
      // deadline 选择当前记录阶段的下一边界。
      const deadline = entry.phase === "holding" ? entry.exitAt : entry.foldAt;
      nextDeadline = nextDeadline === null ? deadline : Math.min(nextDeadline, deadline);
    }
    if (nextDeadline === null) {
      return;
    }
    // timer advances every due entry using one timeout for the whole tool sequence.
    // timer 使用工具组共享的单个计时器推进所有到期记录。
    const timer = window.setTimeout(() => {
      // transitionNow is sampled once so simultaneous rows advance consistently.
      // transitionNow 仅采样一次，使同时到期的工具行一致推进。
      const transitionNow = performance.now();
      setRetirementById((current) => {
        // next is the updated retirement map for the due transition batch.
        // next 是本批到期转换使用的更新后收纳映射。
        const next = { ...current };
        // changed avoids replacing state when the timer fires before any deadline.
        // changed 避免计时器早于截止点触发时替换状态。
        let changed = false;
        for (const [itemId, entry] of Object.entries(next)) {
          if (entry.phase === "folded") {
            continue;
          }
          if (transitionNow >= entry.foldAt) {
            next[itemId] = { ...entry, phase: "folded" };
            changed = true;
          } else if (entry.phase === "holding" && transitionNow >= entry.exitAt) {
            next[itemId] = { ...entry, phase: "exiting" };
            changed = true;
          }
        }
        return changed ? next : current;
      });
      // Every consumed timer invalidates the schedule so an early callback cannot strand a holding or exiting row.
      // 每次计时器消费后都使调度失效，避免提前回调让等待或退场工具行永久滞留。
      setRetirementTimerRevision((current) => current + 1);
    }, Math.max(1, Math.ceil(nextDeadline - now)));
    return () => window.clearTimeout(timer);
  }, [expanded, retirementById, retirementTimerRevision]);

  // hasFoldableItems keeps every grouped sequence expandable while it owns visible tool rows.
  // hasFoldableItems 让包含可见工具记录的每个工具组都可以展开。
  const hasFoldableItems = props.items.length > 0;
  // terminalCount reports every non-running tool, including persistent diagnostics.
  // terminalCount 报告所有非运行中工具，包括持续显示的诊断记录。
  const terminalCount = props.completedItems.length + props.persistentItems.length;
  // transientCompletedIds identifies completed rows that have not entered the hidden fold yet.
  // transientCompletedIds 标识尚未进入隐藏折叠区的已完成工具行。
  const transientCompletedIds = new Set(
    expanded
      ? []
      : props.completedItems
        .filter((item) => {
          // itemId is the stable identity used to read the current retirement phase.
          // itemId 是读取当前收纳阶段使用的稳定标识。
          const itemId = item.renderKey ?? item.id;
          // phase defaults to holding during the first render before effect reconciliation.
          // phase 在 effect 首次协调前的初始渲染中默认处于等待阶段。
          const phase = retirementById[itemId]?.phase
            ?? (props.animateRetirement || liveObservedIdsRef.current.has(itemId) ? "holding" : "folded");
          return phase !== "folded";
        })
        .map((item) => item.renderKey ?? item.id),
  );
  // runningIds identifies active rows inside the original sequence order.
  // runningIds 标识原始序列顺序中的活动工具行。
  const runningIds = new Set(props.runningItems.map((item) => item.renderKey ?? item.id));
  // persistentIds identifies diagnostic rows that must not auto-retire.
  // persistentIds 标识不得自动收纳的诊断工具行。
  const persistentIds = new Set(props.persistentItems.map((item) => item.renderKey ?? item.id));
  // expandedItems preserves every completed, running, and diagnostic row in original sequence order.
  // expandedItems 按原始序列顺序保留全部完成、运行和诊断工具行。
  const expandedItems = expanded ? props.items : [];
  // visibleItems preserves collapsed live rows without duplicating the expanded body.
  // visibleItems 保留折叠状态的实时工具行，并避免与展开正文重复。
  const visibleItems = expanded
    ? []
    : props.items.filter((item) => {
      // itemId is the stable projected identity for sequence filtering.
      // itemId 是序列过滤使用的稳定投影标识。
      const itemId = item.renderKey ?? item.id;
      return transientCompletedIds.has(itemId) || runningIds.has(itemId) || persistentIds.has(itemId);
    });
  // summary reports terminal and active counts without exposing hidden thinking rows.
  // summary 报告终态与活动数量，不暴露被隐藏的思考记录。
  const summary = props.runningItems.length > 0
    ? t("conversation.toolSequence.runningSummary")
      .replace("{completed}", String(terminalCount))
      .replace("{running}", String(props.runningItems.length))
    : t("conversation.toolSequence.summary").replace("{count}", String(terminalCount));

  return (
    <section className="conversation-tool-sequence" data-expanded={expanded ? "true" : "false"}>
      <button
        aria-expanded={hasFoldableItems ? expanded : false}
        className="conversation-tool-sequence-toggle"
        disabled={!hasFoldableItems}
        onClick={() => {
          if (hasFoldableItems) {
            setExpanded(!expanded);
          }
        }}
        type="button"
      >
        <Workflow size={14} />
        <span className={cn(props.runningItems.length > 0 && "shimmer")}>{summary}</span>
        {hasFoldableItems ? (expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />) : null}
      </button>
      {hasFoldableItems ? (
        <ConversationMeasuredCollapse collapseId={props.collapseId} expanded={expanded}>
          <div className="conversation-tool-sequence-body">
            {expandedItems.map(props.renderItem)}
          </div>
        </ConversationMeasuredCollapse>
      ) : null}
      {visibleItems.length > 0 ? (
        <div className="conversation-tool-sequence-running">
          {visibleItems.map((item) => {
            // itemId is the stable identity used by React and retirement state.
            // itemId 是 React 与收纳状态共同使用的稳定标识。
            const itemId = item.renderKey ?? item.id;
            // retirementPhase is present only for a successfully completed transient row.
            // retirementPhase 仅存在于成功完成且正在过渡的工具行。
            const retirementPhase = transientCompletedIds.has(itemId)
              ? retirementById[itemId]?.phase ?? "holding"
              : null;
            return retirementPhase ? (
              <div
                className={cn("conversation-tool-retirement-slot", retirementPhase === "exiting" && "exiting")}
                data-retirement-phase={retirementPhase}
                key={itemId}
              >
                <div className="conversation-tool-retirement-viewport">
                  <div className="conversation-tool-retirement-row">
                    {props.renderItem(item)}
                  </div>
                </div>
              </div>
            ) : (
              <div className="conversation-tool-live-row" key={itemId}>
                {props.renderItem(item)}
              </div>
            );
          })}
        </div>
      ) : null}
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
  // Live physical Provider request accounting shown without waiting for turn completion.
  // 不等待整轮结束即可显示的实时物理 Provider 请求统计。
  sessionProviderUsage?: SessionProviderUsageSummary | null;
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
  onEditUserPrompt?: (conversationId: string, messageId: string) => Promise<void> | void;
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
  const collapsed = props.collapsedOverride ?? props.process.defaultCollapsed;
  const visibleGuidanceItems = props.process.guidanceItems.filter((item) => !renderItemHiddenByThinkingDisplay(item, props.hiddenThinkingBlockIds));
  const visibleProcessItems = props.process.items.filter((item) => !renderItemHiddenByThinkingDisplay(item, props.hiddenThinkingBlockIds));
  // previousProcessRunningRef provides one committed-render grace period for a final tool result arriving with the process terminal snapshot.
  // previousProcessRunningRef 为随过程终态快照一同到达的最终工具结果提供一次已提交渲染的宽限期。
  const previousProcessRunningRef = React.useRef(props.process.running);
  // animateToolRetirement enables pure frontend retirement only for the live process and its immediate terminal render.
  // animateToolRetirement 仅为实时过程及其紧邻的终态渲染启用纯前端收纳动画。
  const animateToolRetirement = props.process.running || previousProcessRunningRef.current;
  React.useEffect(() => {
    previousProcessRunningRef.current = props.process.running;
  }, [props.process.running]);
  // visibleProcessBodyEntries groups only after thinking-display filtering establishes the visible sequence.
  // visibleProcessBodyEntries 仅在思考显示过滤确定可见序列后执行组合。
  const visibleProcessBodyEntries = groupVisibleProcessBodyEntries(
    props.process.id,
    visibleProcessItems,
    props.sessionRunning,
  );
  const visibleCollapsedProcessUserItems = collapsed ? visibleProcessItems.filter(isTurnProcessUserRenderItem) : [];
  // streamingProcessItemIds comes only from explicit backend lifecycle metadata, never row position or run phase.
  // streamingProcessItemIds 仅来自后端显式生命周期元数据，不依赖行位置或运行阶段。
  const streamingProcessItemIds = new Set(
    visibleProcessItems
      .filter((item) => conversationMessageStreamState(item.sourceMessage) === "active")
      .map((item) => item.id),
  );
  const liveDurationMs = props.process.running
    ? Math.max(0, props.nowMs - (props.process.startedAtMs ?? fallbackStartedAtMsRef.current))
    : null;
  const displayedDurationMs = props.process.durationMs ?? liveDurationMs;
  const durationLabel = formatWorkProcessDuration(displayedDurationMs, locale);
  const headerText = durationLabel ? `${t("conversation.process.header")} ${durationLabel}` : t("conversation.process.header");
  const providerUsage = props.sessionProviderUsage;
  // activeProviderUsage keeps the legacy projection readable while new snapshots expose explicit scopes.
  // activeProviderUsage 在新快照暴露明确口径的同时保持旧投影可读。
  const activeProviderUsage = providerUsage?.activeBranch ?? (providerUsage
    ? {
        providerCallCount: providerUsage.providerCallCount,
        unknownUsageCallCount: providerUsage.unknownUsageCallCount,
        usage: providerUsage.cumulativeUsage,
      }
    : null);
  const providerUsageTotal = activeProviderUsage?.usage?.totalTokens ?? providerUsage?.lastUsage?.totalTokens;
  const providerUsageUnknown = t("conversation.process.providerUsageUnknown");
  const providerUsageLabel = providerUsage
    ? t("conversation.process.providerUsageBadge")
      .replace("{calls}", String(activeProviderUsage?.providerCallCount ?? 0))
      .replace("{inFlight}", String(providerUsage.inFlightCallCount))
      .replace("{tokens}", String(providerUsageTotal ?? providerUsageUnknown))
    : null;
  const providerUsageTitle = providerUsage
    ? [
        t("conversation.process.providerUsageActive")
          .replace("{calls}", String(activeProviderUsage?.providerCallCount ?? 0))
          .replace("{tokens}", String(providerUsageTotal ?? providerUsageUnknown))
          .replace("{unknown}", String(activeProviderUsage?.unknownUsageCallCount ?? 0)),
        t("conversation.process.providerUsageLifetime")
          .replace("{calls}", String(providerUsage.lifetime?.providerCallCount ?? activeProviderUsage?.providerCallCount ?? 0))
          .replace("{tokens}", String(providerUsage.lifetime?.usage?.totalTokens ?? providerUsageTotal ?? providerUsageUnknown)),
        t("conversation.process.providerUsageInherited")
          .replace("{calls}", String(providerUsage.inherited?.providerCallCount ?? 0))
          .replace("{tokens}", String(providerUsage.inherited?.usage?.totalTokens ?? 0)),
      ].join("\n")
    : undefined;
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
        disabled={!props.process.collapsible}
        onClick={() => {
          if (!props.process.collapsible) {
            return;
          }
          dispatchConversationManualViewportInteraction();
          props.onCollapsedChange(!collapsed);
        }}
        type="button"
      >
        <Clock3 className="work-process-icon" size={14} />
        <span>{headerText}</span>
        {providerUsageLabel ? (
          <span className="work-process-provider-usage" title={providerUsageTitle}>
            {providerUsageLabel}
          </span>
        ) : null}
        {props.process.collapsible ? (collapsed ? <ChevronRight className="work-process-toggle-icon" size={13} /> : <ChevronDown className="work-process-toggle-icon" size={13} />) : null}
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
                  animateRetirement={animateToolRetirement}
                  collapseId={entry.collapseId}
                  completedItems={entry.completedItems}
                  items={entry.items}
                  key={entry.collapseId}
                  persistentItems={entry.persistentItems}
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
  onEditUserPrompt?: (conversationId: string, messageId: string) => Promise<void> | void;
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
    await props.onCopyText(userBody);
    setUserPromptCopied(true);
  }, [props, userBody]);
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
              onClick={() => void props.onEditUserPrompt?.(props.item.userMessage!.conversationId, props.item.userMessage!.id)}
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
      <div className="assistant-flow">
        {props.item.role === "system" ? <span className="message-role-label">SYSTEM</span> : null}
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
    await props.onCopyText(finalAssistantRawText);
    setFinalAssistantCopied(true);
  }, [finalAssistantRawText, props]);
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
function stripUserPromptHeading(body: string): string {
  return body
    .replace(/^\s*(?:#{1,6}\s*)?(?:\*\*)?(?:用户指令|User Prompt|USER)(?:\*\*)?\s*\n+/i, "")
    .trim();
}

// finalAssistantOutputText returns the raw Markdown source for one final assistant row.
// finalAssistantOutputText 返回单条最终 Assistant 行的 Markdown 原文。
function finalAssistantOutputText(item: ConversationRenderItem): string {
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

