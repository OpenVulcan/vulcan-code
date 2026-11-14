import React from "react";
import { ConversationPromptFileReferences } from "./conversation-prompt-file-references";
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
import { ToolSequenceGroupedContext } from "@/components/conversation/conversation-tool-execution";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { fixedV1ToolEvent } from "@/components/conversation/tool-presentations/presentation-value-readers";
import { ToolSequenceHead, toolSequenceEntry } from "@/conversation/tools/tool-sequence";
import { toolExecutionDisplayState } from "@/components/conversation/conversation-tool-execution-model";
import {
  ConversationMeasuredCollapse,
  useConversationMeasuredExpansion,
  usePrefersReducedMotion,
} from "@/components/conversation/conversation-measured-collapse";
import {
  dispatchConversationAutomaticFold,
  dispatchConversationManualViewportInteraction,
} from "@/components/conversation/conversation-layout-events";
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
import {
  conversationProcessRenderItemEqual,
  conversationRenderItemEqual,
} from "@/components/conversation/conversation-timeline-projection-store";
import type {
  CommandSessionSummary,
  ComposerAttachmentSummary,
  ConversationMessage,
} from "@/types";
import { useI18n } from "@/i18n";
import { useWorkspaceFileOpener } from "@/components/workspace/workspace-file-open-request";
import { TerminalPathText } from "@/conversation/tools/views/terminal-view";
import { RenderErrorBoundary } from "@/app/render-error-boundary";
import { PageNotice } from "@/components/page/page-feedback";

import { USER_PROMPT_COLLAPSE_LINE_LIMIT } from "./conversation-bubble-limits";
import { DedicatedUserMessageMarks } from "./dedicated-transcript-memory";

// propsEqualExcept compares memoized row props: the projected row by its sources, everything else by identity.
// propsEqualExcept 比较缓存行的属性：投影行按来源比较，其余属性按引用比较。
// Parameters: previous and next are the props of two renders; key names the projected row; equal compares it.
// 参数：previous 与 next 是两次渲染的属性；key 指定投影行；equal 比较该行。
// Returns: true when re-rendering would produce the same row, so React may keep the previous output.
// 返回值：重新渲染会得到相同的行时返回 true，React 可保留上次输出。
function propsEqualExcept<P extends object, K extends keyof P>(
  previous: P,
  next: P,
  key: K,
  equal: (left: P[K], right: P[K]) => boolean,
): boolean {
  const previousKeys = Object.keys(previous) as (keyof P)[];
  if (previousKeys.length !== Object.keys(next).length) {
    return false;
  }
  return previousKeys.every((name) => (
    name === key ? equal(previous[key], next[key]) : Object.is(previous[name], next[name])
  ));
}

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
    // Original consecutive tool order inside the single persistent group body.
    // 单一稳定分组正文内的原始连续工具顺序。
    items: ConversationRenderItem[];
    // Running tool rows that stay visible while the group is folded.
    // 分组折叠时仍保持可见的运行中工具记录。
    runningItems: ConversationRenderItem[];
    // Whether the run can gain no more tools: another row followed it or its process stopped running.
    // 序列是否不会再增加工具：其后已有其他记录，或所属过程已不在运行。
    closed: boolean;
  };

// groupVisibleProcessBodyEntries keeps reasoning inside tool runs until any other visible row closes them.
// groupVisibleProcessBodyEntries 将思考保留在工具序列内，直到任何其他可见行结束序列。
// Parameters: processId identifies the owning turn, items contains already display-filtered rows,
// and sessionRunning resolves unfinished calls.
// 参数：processId 标识所属回合，items 包含显示过滤后的记录，sessionRunning 判定未完成调用。
// Returns: ordered single rows and tool-sequence groups without moving assistant content.
// 返回值：保持顺序的单条记录与工具序列组，且不会移动 Assistant 内容。
export function groupVisibleProcessBodyEntries(
  processId: string,
  items: ConversationRenderItem[],
  sessionRunning: boolean,
): ConversationProcessBodyEntry[] {
  // entries stores the final display order of single rows and folded tool sequences.
  // entries 保存单条记录与折叠工具序列的最终显示顺序。
  const entries: ConversationProcessBodyEntry[] = [];
  // pendingToolItems retains tools and intervening reasoning in their original order.
  // pendingToolItems 按原始顺序保留工具及其间的思考。
  let pendingToolItems: ConversationRenderItem[] = [];
  // Start with the first tool so appending another never moves a live card into a different container.
  // 从首个工具开始成组，避免追加工具时把实时卡片移到不同容器。
  // flushPendingTools closes the pending run into one entry; closed says whether it can still grow.
  // Returns no value.
  // flushPendingTools 将待定序列收为一个条目；closed 表示序列是否已不会再增长。无返回值。
  const flushPendingTools = (closed: boolean) => {
    if (pendingToolItems.length > 0) {
      // firstItem provides a stable identity even while a running sequence gains more tools.
      // firstItem 在运行中序列继续增加工具时仍提供稳定标识。
      const firstItem = pendingToolItems[0];
      // runningItems contains every active tool row that stays visible inside a folded group.
      // runningItems 包含折叠分组内仍保持可见的所有活动工具记录。
      const runningItems: ConversationRenderItem[] = [];
      for (const item of pendingToolItems) {
        // toolBlocks is the already-confirmed execution block set for the current tool row.
        // toolBlocks 是当前工具行中已经确认的执行块集合。
        const toolBlocks = item.blocks.filter(isMergeableToolBlock);
        // Reasoning stays in order but is never a running tool.
        // 思考保持原位，但从不算作运行中工具。
        if (toolBlocks.length > 0 && toolExecutionDisplayState(toolBlocks, sessionRunning) === "running") {
          runningItems.push(item);
        }
      }
      entries.push({
        kind: "tool-sequence",
        collapseId: `tool-sequence:${processId}:${toolSequenceItemKey(firstItem)}`,
        items: pendingToolItems,
        runningItems,
        closed,
      });
    }
    pendingToolItems = [];
  };

  for (const item of items) {
    if (item.blocks.length > 0 && item.blocks.every(isMergeableToolBlock)) {
      pendingToolItems.push(item);
      continue;
    }
    // Live reasoning must not repeatedly close and reopen the preceding tools.
    // 实时思考不能反复关闭再重新打开前面的工具组。
    if (pendingToolItems.length > 0 && item.role === "assistant"
      && item.blocks.every((block) => block.kind === "thinking")) {
      pendingToolItems.push(item);
      continue;
    }
    // Any other row (assistant text, context compaction, prompt injection, model or reasoning-effort switch,
    // in-turn guidance) closes the run: later tools can never rejoin rows already separated by it, so the
    // preceding run folds and the next tool opens a new one. Running tools still keep their run expanded.
    // 其他任何行（助手正文、上下文压缩、提示词注入、模型或思考级别切换、回合内引导）都结束序列：之后的工具
    // 不可能再并入已被它隔开的记录，因此前面的序列折叠，下一个工具开启新折叠区；仍在运行的工具保持展开。
    flushPendingTools(true);
    entries.push({ kind: "item", item });
  }
  // The trailing run stays open while the process runs, because the next tool may still join it.
  // 过程仍在运行时末尾序列保持开放，因为下一个工具仍可能并入。
  flushPendingTools(!sessionRunning);
  return entries;
}

// TOOL_SEQUENCE_SWAP_MS is the duration of the push-up animation when one card replaces another in a
// folded run; the stylesheet reads it through --vk-tool-swap.
// TOOL_SEQUENCE_SWAP_MS 是折叠序列中一张卡片替换另一张时上推动画的时长；样式表通过 --vk-tool-swap 读取。
const TOOL_SEQUENCE_SWAP_MS = 420;

// useToolSequenceHighWaterFloor measures the active run from its first card. The folded body then uses
// that height as its floor, capped by the same CSS limit as its maximum height.
// useToolSequenceHighWaterFloor 从首张卡片起测量活动序列。折叠正文使用该高度作为下限，并由同一个 CSS 高度上限约束。
// Parameters: body is the mounted body, active enables measurement, resetKey restarts it after a manual toggle.
// Returns nothing.
// 参数：body 是已挂载的正文，active 开启测量，resetKey 在手动切换后重新计量。无返回值。
function useToolSequenceHighWaterFloor(
  body: HTMLDivElement | null,
  active: boolean,
  resetKey: number,
): void {
  React.useLayoutEffect(() => {
    if (!body || !active) return;
    // Keep the measurement across live card replacements, including the first switch to grouped layout.
    // 保留实时卡片替换前的测量，包括首次切换到分组布局。
    let highWater = 0;
    // Record a larger body height so shorter successors can reuse the space. Takes no parameters and returns nothing.
    // 记录更高的正文高度，以便较矮的后续卡片复用空间。不接收参数，无返回值。
    const raise = () => {
      // Read the layout height, excluding scrollable overflow and transformed cards.
      // 读取布局高度，排除可滚动溢出及位移中的卡片。
      const height = body.offsetHeight;
      if (height > highWater) {
        highWater = height;
        body.style.setProperty("--vk-tool-seq-min-h", `${highWater}px`);
      }
    };
    raise();
    // Observe the committed body even when the collapse shell mounted it in a later commit.
    // 即使折叠外壳在后续提交才挂载正文，也观察已提交的正文。
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(raise);
    observer?.observe(body);
    return () => {
      observer?.disconnect();
      body.style.removeProperty("--vk-tool-seq-min-h");
    };
  }, [active, body, resetKey]);
}

// Return the row's render-instance key, using its ID when the projection supplies no separate key.
// 返回记录的渲染实例键，投影未提供独立键时使用其 ID。
function toolSequenceItemKey(item: ConversationRenderItem): string {
  return item.renderKey ?? item.id;
}

// ConversationToolSequenceGroup renders one pure display-only fold for consecutive tool rows.
// ConversationToolSequenceGroup 渲染一个仅用于显示的连续工具记录折叠组。
// Parameters: collapseId preserves expansion memory, item arrays preserve execution state and order,
// renderItem renders the original row; collapseEnabled decides whether the run folds at all.
// 参数：collapseId 保留展开记忆，各工具数组保留执行状态与顺序，renderItem 渲染原始记录；collapseEnabled 决定序列是否折叠。
// Returns one summary and one stable ordered body, without a separate retirement layer.
// 返回一个摘要及一个稳定有序正文，不再有独立退场层。
export function ConversationToolSequenceGroup(props: {
  // Whether the run can gain no more tools; a folded body is released only then.
  // 序列是否不会再增加工具；折叠正文只在此时收回。
  closed: boolean;
  // Whether automatic tool folding is enabled for this conversation.
  // 当前会话是否启用工具自动折叠。
  collapseEnabled: boolean;
  // Stable identity retained from the very first tool.
  // 从首个工具开始保持不变的标识。
  collapseId: string;
  // All tools remain in this one ordered body, including running tools.
  // 所有工具（包括运行中工具）始终留在此有序正文内。
  items: ConversationRenderItem[];
  // Original row renderer; returns one tool card without changing its identity.
  // 原始记录渲染器；返回工具卡且不改变其标识。
  renderItem: (item: ConversationRenderItem) => React.ReactNode;
  // Running rows, shown as full cards inside a folded group until they finish.
  // 运行中记录，在折叠组中以完整卡片显示，直到执行结束。
  runningItems: ConversationRenderItem[];
}) {
  // Groups start folded; only an explicit user expansion is remembered as open.
  // 分组默认收起；只有用户明确展开才被记忆为打开。
  const [rememberedExpanded, setExpanded] = useConversationMeasuredExpansion(props.collapseId, false);
  // entries counts each tool row by family and state for the sequence head; reasoning rows are skipped.
  // entries 按家族与状态统计每个工具行供序列标题使用；跳过思考行。
  const entries = React.useMemo(() => props.items.flatMap((item) => {
    const toolBlocks = item.blocks.filter(isMergeableToolBlock);
    if (toolBlocks.length === 0) return [];
    return [toolSequenceEntry(fixedV1ToolEvent(toolBlocks as ToolExecutionConversationBlock[]), props.runningItems.includes(item))];
  }), [props.items, props.runningItems]);
  // A run becomes one folded group from its second tool on, while it is still running; a single tool stays a plain card.
  // 运行从第二个工具起即成为一个折叠组（运行期间同样如此）；单个工具保持普通卡片。
  const grouped = props.collapseEnabled && entries.length > 1;
  // open shows every row: no group, folding disabled, or the user expanded it.
  // open 表示显示全部记录：未成组、关闭折叠或用户已展开。
  const open = !grouped || rememberedExpanded;
  const folded = grouped && !open;
  // liveItems are the rows the body must show now: all rows when open, otherwise only running tools.
  // liveItems 是正文此刻必须显示的记录：打开时为全部，否则只有运行中的工具。
  const liveItems = open ? props.items : props.runningItems;
  // holdArmedRef is set once a folded body shows a running card and cleared by a user toggle, so only an
  // automatically shown body is held between calls; a body the user just folded closes at once.
  // holdArmedRef 在折叠正文显示过运行卡片后置位，用户切换时清除；因此只有自动显示的正文会在调用之间保留，
  // 用户刚折叠的正文立即收起。
  const holdArmedRef = React.useRef(false);
  if (folded && props.runningItems.length > 0) holdArmedRef.current = true;
  // holding keeps the last card on screen between two calls of a run that can still grow, so the body
  // never closes only to reopen for the next tool.
  // holding 在仍可能增长的序列两次调用之间保留最后一张卡片，使正文不会收起后又为下一个工具重新打开。
  const holding = folded && !props.closed && liveItems.length === 0 && holdArmedRef.current;
  // Keep the visible keys only. The latest projection still owns each held or closing card's status.
  // 只保留可见记录的键。保留中或收起中的卡片状态仍取自最新投影。
  const lastShownKeys = React.useRef<ReadonlySet<string>>(new Set());
  const shownItems = liveItems.length > 0 || lastShownKeys.current.size === 0
    ? liveItems
    : props.items.filter((item) => lastShownKeys.current.has(toolSequenceItemKey(item)));
  const bodyVisible = liveItems.length > 0 || (holding && shownItems.length > 0);
  // sectionRef locates this group's transcript when announcing an automatic fold.
  // sectionRef 用于在通告自动折叠时定位本分组所属的转录。
  const sectionRef = React.useRef<HTMLElement | null>(null);
  // toolBody tracks late mounts by the collapse shell so height observation starts with the first visible card.
  // toolBody 跟踪折叠外壳的延后挂载，从首张可见卡片开始观察高度。
  const [toolBody, setToolBody] = React.useState<HTMLDivElement | null>(null);
  // toggledRef marks a commit caused by the user's own toggle, which is not an automatic fold.
  // toggledRef 标记由用户自己切换引起的提交，它不属于自动折叠。
  const toggledRef = React.useRef(false);
  // toggleCountRef counts user toggles: the high-water floor starts over and the cards remount closed after each.
  // toggleCountRef 记录用户切换次数：每次切换后高水位下限重新计算，卡片以收起状态重新挂载。
  const toggleCountRef = React.useRef(0);
  const handleToggle = () => {
    holdArmedRef.current = false;
    toggledRef.current = true;
    toggleCountRef.current += 1;
    setExpanded(!open);
  };
  // The first plain card contributes its height to the same run, before the second call makes it a folded group.
  // 首张普通卡片的高度也计入同一序列，在第二次调用将它变为折叠分组之前就已记录。
  useToolSequenceHighWaterFloor(toolBody, props.collapseEnabled && bodyVisible && (folded || (!grouped && !props.closed)), toggleCountRef.current);
  // Resolve the outgoing card from the latest rows even if it completes in the same update as its successor.
  // 即使旧卡结束和新卡出现发生在同一次更新，也从最新记录读取离场卡片。
  const [leavingKey, setLeavingKey] = React.useState<string | null>(null);
  const leaving = leavingKey === null ? undefined : props.items.find((item) => toolSequenceItemKey(item) === leavingKey);
  // reducedMotion swaps cards in place with no leaving copy, for the browser or the app motion setting.
  // reducedMotion 在浏览器或应用动效设置要求减少动效时原地替换卡片，不保留离场副本。
  const reducedMotion = usePrefersReducedMotion();
  // Track the previous single visible card for a one-for-one swap.
  // 记录上一次唯一可见卡片，用于一对一切换。
  const singleShownKey = shownItems.length === 1 ? toolSequenceItemKey(shownItems[0]) : null;
  const previousShownKey = React.useRef(singleShownKey);
  const previousBodyVisibleRef = React.useRef(bodyVisible);
  React.useLayoutEffect(() => {
    lastShownKeys.current = new Set(shownItems.map(toolSequenceItemKey));
    const previous = previousShownKey.current;
    previousShownKey.current = singleShownKey;
    const wasVisible = previousBodyVisibleRef.current;
    previousBodyVisibleRef.current = bodyVisible;
    const toggled = toggledRef.current;
    toggledRef.current = false;
    const motionReduced = reducedMotion || document.documentElement.dataset.vkMotion === "reduce";
    const replaced = folded && bodyVisible && !toggled && !motionReduced
      && previous !== null && singleShownKey !== null && previous !== singleShownKey;
    if (replaced) setLeavingKey(previous);
    // Only closing the folded body shrinks the transcript without user action; announce it in the same
    // commit, before observers see the smaller height, so the followed tail keeps its place.
    // 只有折叠正文收起会在无用户操作下让转录变矮；在同一提交内、观察器看到新高度之前通告，使锁底尾部保持原位。
    if (wasVisible && !bodyVisible && folded && !toggled && sectionRef.current) {
      dispatchConversationAutomaticFold(sectionRef.current);
    }
  }, [bodyVisible, folded, reducedMotion, shownItems, singleShownKey]);
  React.useEffect(() => {
    if (leavingKey === null) return;
    const timer = window.setTimeout(() => setLeavingKey(null), TOOL_SEQUENCE_SWAP_MS);
    return () => window.clearTimeout(timer);
  }, [leavingKey]);
  // enteringKey names the card that just replaced the leaving one.
  // enteringKey 指明刚替换掉离场卡片的新卡片。
  const enteringKey = leaving ? singleShownKey : null;

  return (
    <section
      className="vk-tool-seq"
      data-expanded={open ? "true" : "false"}
      data-grouped={grouped ? "true" : "false"}
      data-running={folded && props.runningItems.length > 0 ? "true" : "false"}
      ref={sectionRef}
      style={{ "--vk-tool-swap": `${TOOL_SEQUENCE_SWAP_MS}ms` } as React.CSSProperties}
    >
      {grouped ? <ToolSequenceHead entries={entries} expanded={open} onToggle={handleToggle} /> : null}
      <ConversationMeasuredCollapse collapseId={props.collapseId} expanded={bodyVisible}>
        <ToolSequenceGroupedContext.Provider value={grouped}>
        <div className="vk-tool-seq-body" ref={setToolBody}>
          {leaving && enteringKey !== null ? (
            <div
              aria-hidden="true"
              className="vk-tool-seq-slot"
              data-leaving=""
              key={`leaving:${leavingKey}`}
            >
              {props.renderItem(leaving)}
            </div>
          ) : null}
          {shownItems.map((item) => (
            <div
              className="vk-tool-seq-slot"
              data-entering={toolSequenceItemKey(item) === enteringKey ? "" : undefined}
              key={`${toggleCountRef.current}:${toolSequenceItemKey(item)}`}
            >
              {props.renderItem(item)}
            </div>
          ))}
        </div>
        </ToolSequenceGroupedContext.Provider>
      </ConversationMeasuredCollapse>
    </section>
  );
}

// ConversationProcessGroup renders one per-turn process header and its foldable rows.
// ConversationProcessGroup 渲染单个轮次过程标题及其可折叠处理行。
// PROCESS_DURATION_TICK_MS is the refresh interval of a running process's elapsed time.
// PROCESS_DURATION_TICK_MS 是运行中过程已用时长的刷新间隔。
const PROCESS_DURATION_TICK_MS = 1000;

// ProcessHeaderLabel renders the process header text; a running process ticks here, so only this text redraws.
// ProcessHeaderLabel 渲染过程标题文字；运行中的过程在此计时，因此只有这段文字重绘。
// Parameters: running selects the live clock, startedAtMs anchors it, durationMs is the settled duration.
// 参数：running 选择实时计时，startedAtMs 为计时起点，durationMs 为已结算时长。
function ProcessHeaderLabel(props: { running: boolean; startedAtMs: number; durationMs: number | null }) {
  const { locale, t } = useI18n();
  const [nowMs, setNowMs] = React.useState(() => Date.now());
  React.useEffect(() => {
    if (!props.running) return undefined;
    setNowMs(Date.now());
    const timer = window.setInterval(() => setNowMs(Date.now()), PROCESS_DURATION_TICK_MS);
    return () => window.clearInterval(timer);
  }, [props.running]);
  const durationMs = props.durationMs ?? (props.running ? Math.max(0, nowMs - props.startedAtMs) : null);
  const durationLabel = formatWorkProcessDuration(durationMs, locale);
  // Use the owning process lifecycle so completed work stays terminal while another turn runs.
  // 使用所属过程的生命周期，避免其他回合运行时将已完成工作显示为进行中。
  const workLabel = t(props.running ? "conversation.process.working" : "conversation.process.worked");
  return <span>{durationLabel ? `${workLabel} ${durationLabel}` : workLabel}</span>;
}

export const ConversationProcessGroup = React.memo(function ConversationProcessGroup(props: {
  // autoExpandThinkingBlockId keeps parity with normal block rendering.
  // autoExpandThinkingBlockId 保持与普通块渲染一致。
  autoExpandThinkingBlockId: string | null;
  // Optional user-controlled collapsed state remembered by the conversation surface.
  // 会话界面记住的可选用户控制折叠状态。
  collapsedOverride?: boolean;
  // hiddenThinkingBlockIds hides thinking blocks filtered by the display mode.
  // hiddenThinkingBlockIds 隐藏由展示模式过滤的思考块。
  hiddenThinkingBlockIds: ReadonlySet<string>;
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
  // Records one user-triggered collapsed state change for the given process; a stable callback keeps the memo valid.
  // 为指定过程记录一次用户触发的折叠状态变化；稳定的回调可保持缓存有效。
  onCollapsedChange: (process: ConversationProcessRenderItem, collapsed: boolean) => void;
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
    // An earlier terminal process cannot inherit a later turn's running status.
    // 已终止的历史过程不能继承后续回合的运行状态。
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
  // A stopped process is never running, so its label uses only the settled duration.
  // 已停止的过程不会处于运行中，因此其文案只使用已结算时长。
  const durationLabel = formatWorkProcessDuration(props.process.running ? null : props.process.durationMs, locale);
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
      className="vk-process"
      data-running={props.process.running ? "" : undefined}
      data-stopped={props.process.stopped ? "" : undefined}
      data-process-body-visible={bodyVisible ? "true" : "false"}
      data-process-collapsed={collapsed ? "true" : "false"}
    >
      <button
        aria-expanded={bodyVisible}
        className="vk-process-head"
        disabled={!props.process.collapsible || !toolCollapseEnabled}
        onClick={() => {
          if (!props.process.collapsible || !toolCollapseEnabled) {
            return;
          }
          dispatchConversationManualViewportInteraction();
          props.onCollapsedChange(props.process, !collapsed);
        }}
        type="button"
      >
        <Clock3 aria-hidden="true" />
        <ProcessHeaderLabel
          durationMs={props.process.durationMs}
          running={props.process.running}
          startedAtMs={props.process.startedAtMs ?? fallbackStartedAtMsRef.current}
        />
        {props.process.collapsible && toolCollapseEnabled ? (collapsed ? <ChevronRight aria-hidden="true" className="vk-process-chev" /> : <ChevronDown aria-hidden="true" className="vk-process-chev" />) : null}
      </button>
      <div className="vk-process-guidance">
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
        <div className="vk-process-body">
          {visibleProcessBodyEntries.map((entry) => (
            entry.kind === "item"
              ? renderProcessBodyItem(entry.item)
              : (
                <ConversationToolSequenceGroup
                  closed={entry.closed}
                  collapseEnabled={toolCollapseEnabled}
                  collapseId={entry.collapseId}
                  items={entry.items}
                  key={entry.collapseId}
                  renderItem={renderProcessBodyItem}
                  runningItems={entry.runningItems}
                />
              )
          ))}
          {props.process.stopped ? (
            <div className="vk-process-stopped">
              {stoppedText}
            </div>
          ) : null}
        </div>
      </ConversationMeasuredCollapse>
    </section>
  );
}, (previous, next) => propsEqualExcept(previous, next, "process", conversationProcessRenderItemEqual));

// isTurnProcessUserRenderItem returns whether one user row should stay visible while its process is collapsed.
// isTurnProcessUserRenderItem 返回单条用户行是否应在过程折叠时保持可见。
function isTurnProcessUserRenderItem(item: ConversationRenderItem): boolean {
  if (item.role !== "user") {
    return false;
  }
  return metadataValue(item.userMessage?.metadata ?? item.sourceMessage?.metadata, ["turn_process"]) === true;
}

// ConversationRenderItemViewProps carries one projected transcript item and the callbacks its row uses.
// ConversationRenderItemViewProps 携带单条投影后的转录展示项及其行使用的回调。
type ConversationRenderItemViewProps = {
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
};

// itemRevisions numbers each projected item object; projection replaces an item's object whenever its content
// changes, so a new number means new content to try rendering again.
// itemRevisions 为每个投影项对象编号；投影在内容变化时替换项对象，因此新编号意味着可以重新尝试渲染的新内容。
const itemRevisions = new WeakMap<ConversationRenderItem, string>();
let nextItemRevision = 0;
function itemRevision(item: ConversationRenderItem): string {
  let revision = itemRevisions.get(item);
  if (revision === undefined) {
    revision = String(nextItemRevision++);
    itemRevisions.set(item, revision);
  }
  return revision;
}

// ConversationRenderItemView renders one projected transcript item. A row that fails to render shows a notice in
// its place and leaves the rest of the conversation working; it tries again when the item's content changes.
// ConversationRenderItemView 渲染单条投影后的转录展示项。渲染失败的行在原位显示提示，会话其余部分照常工作；
// 该项内容变化时会再次尝试渲染。
// Memoized by row sources, so a row re-renders only when its own content or a shared input changes.
// 按行来源缓存，因此只有自身内容或共享输入变化时才重新渲染该行。
export const ConversationRenderItemView = React.memo(function ConversationRenderItemView(props: ConversationRenderItemViewProps) {
  return (
    <RenderErrorBoundary fallback={(error) => <ConversationItemFault error={error} />} resetKey={itemRevision(props.item)}>
      <ConversationRenderItemContent {...props} />
    </RenderErrorBoundary>
  );
}, (previous, next) => propsEqualExcept(previous, next, "item", conversationRenderItemEqual));

// ConversationItemFault stands in for a row that failed to render, with the error behind a disclosure.
// ConversationItemFault 代替渲染失败的行，错误信息收在展开区中。
function ConversationItemFault(props: { error: Error }) {
  const { t } = useI18n();
  return (
    <PageNotice
      className="vk-msg-fault"
      details={props.error.message}
      detailsLabel={t("features.shell.errorDetails")}
      title={t("conversation.itemFault")}
      tone="warning"
    />
  );
}

// ConversationRenderItemContent renders one projected transcript item inside its error boundary.
// ConversationRenderItemContent 在错误边界内渲染单条投影后的转录展示项。
function ConversationRenderItemContent(props: ConversationRenderItemViewProps) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // openFile shows a file the prompt mentions ("@path:12-20") in the files panel, where the workbench has one.
  // openFile 在工作台提供文件面板时，于其中显示提示词提及的文件（"@path:12-20"）。
  const openFile = useWorkspaceFileOpener();
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
      <article className="vk-prompt-row">
        <ConversationPromptFileReferences references={props.item.userMessage?.fileReferences ?? []} />
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
        {/* A resource-only input has no prompt text, so no empty bubble is drawn. */}
        {/* 仅资源的输入没有提示词文字，因此不绘制空气泡。 */}
        {userBody ? <div className="vk-prompt">
          {userBody ? (
            <ConversationMeasuredCollapse
              collapseId={`user-prompt-body:${userPromptKey}`}
              collapsedHeight={userPromptMeasuredCollapsedHeight}
              expanded={userPromptCollapseExpanded}
            >
              <p className="vk-prompt-text" ref={userPromptTextRef}>
                {openFile ? <TerminalPathText mentions openFile={openFile} text={userBody} title={t("workspaceFiles.showInFilesPanel")} /> : userBody}
              </p>
            </ConversationMeasuredCollapse>
          ) : null}
          {userPromptOverflowing ? (
            <button
              aria-expanded={userPromptExpanded}
              className="vk-prompt-more"
              onClick={() => {
                setMeasuredUserPromptExpanded(!userPromptExpanded);
                props.onUserPromptExpandedChange(userPromptKey, !userPromptExpanded);
              }}
              type="button"
            >
              <span>{userPromptExpanded ? t("conversation.userPrompt.collapse") : t("conversation.userPrompt.expand")}</span>
              {userPromptExpanded ? (
                <ChevronUp aria-hidden="true" />
              ) : (
                <ChevronDown aria-hidden="true" />
              )}
            </button>
          ) : null}
        </div> : null}
        <div className="vk-prompt-foot">
          <DedicatedUserMessageMarks body={userBody} metadata={props.item.userMessage?.metadata} />
          {userTimestamp && userTimestampMs ? (
            <time dateTime={new Date(userTimestampMs).toISOString()}>{userTimestamp}</time>
          ) : null}
          {userBody ? (
            <button
              aria-label={t("conversation.userPrompt.copy")}
              className="vk-icon-btn" data-size="sm"
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
              className="vk-icon-btn" data-size="sm"
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
    <article className="vk-answer-row" data-role={props.item.role} data-final={props.finalAssistantOutput ? "" : undefined}>
      <div className="vk-flow" data-think={props.item.leadingThinking ? (props.item.blocks.some(isMergeableToolBlock) ? "tool" : "answer") : undefined}>
        {props.item.leadingThinking ? <ThinkingPreview compact>
          {props.item.leadingThinking.map((block) => <MarkdownMessage key={block.id} content={block.body} eager onOpenMedia={props.onOpenMedia} />)}
        </ThinkingPreview> : null}
        <div className="vk-flow-body">
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
    <div className="vk-answer-meta">
      {finalAssistantRawText ? (
        <button
          aria-label={finalAssistantCopied ? t("conversation.action.copiedFinal") : t("conversation.action.copyFinal")}
          className="vk-icon-btn"
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
          className="vk-icon-btn"
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
