import React from "react";
import {
  Bot,
  BrainCircuit,
  ChevronDown,
  ChevronRight,
  ScrollText,
} from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import {
  ConversationMeasuredCollapse,
  useConversationMeasuredExpansion,
} from "@/components/conversation/conversation-measured-collapse";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import {
  metadataString,
  metadataValue,
  numberMetadata,
} from "@/components/conversation/conversation-metadata";
import { formatWorkProcessDuration } from "@/components/conversation/conversation-timeline";

// ConversationDisplayBlock contains the transcript fields needed by extracted display blocks.
// ConversationDisplayBlock 包含已拆分展示块所需的转录字段。
export interface ConversationDisplayBlock {
  // Stable block id used by React keys and section memory.
  // React key 与分段记忆使用的稳定块标识。
  id: string;
  // Block kind used by thinking and compaction filtering.
  // 思考与压缩过滤使用的块类型。
  kind: string;
  // User-facing block title.
  // 面向用户的块标题。
  title: string;
  // Compact block summary.
  // 紧凑块摘要。
  summary: string;
  // Full block body rendered as Markdown.
  // 以 Markdown 渲染的完整块正文。
  body: string;
  // Optional runtime status for compact status rows.
  // 紧凑状态行使用的可选运行状态。
  status?: string;
  // Optional context-compaction section id.
  // 可选上下文压缩分段标识。
  compactionSectionId?: string;
  // Optional context-compaction role.
  // 可选上下文压缩角色。
  compactionRole?: string;
  // Optional context-compaction state.
  // 可选上下文压缩状态。
  compactionState?: string;
  // Optional context-compaction trigger label.
  // 可选上下文压缩触发来源标签。
  compactionTrigger?: string;
  // Optional context-compaction budget note.
  // 可选上下文压缩预算摘要。
  compactionBudgetNote?: string;
  // Optional raw transcript metadata used by specialized display rows.
  // 专用展示行使用的可选原始转录元数据。
  rawMetadata?: unknown;
}

// ContextCompactionBlock renders the original compacted context status section as one collapsible row.
// ContextCompactionBlock 将原本的上下文压缩状态分段渲染为单个可折叠行。
export function ContextCompactionBlock(props: {
  // Section blocks contain start, optional body, and optional end rows.
  // 分段块包含开始、可选正文和可选结束行。
  blocks: ConversationDisplayBlock[];
  // onOpenMedia opens generated media referenced from markdown content.
  // onOpenMedia 打开 Markdown 内容引用的生成媒体。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  const { locale, t } = useI18n();
  const startBlock = props.blocks.find((block) => block.compactionRole === "start") ?? props.blocks[0];
  // progressBlocks contains diagnostic progress lines emitted before final summary text is available.
  // progressBlocks 包含最终总结文本可用前发出的诊断进度行。
  const progressBlocks = props.blocks.filter((block) => block.compactionRole === "progress");
  const thinkingBlocks = props.blocks.filter((block) => block.compactionRole === "thinking" && block.kind === "thinking");
  const bodyBlock = props.blocks.find((block) => block.compactionRole === "body");
  const endBlock = props.blocks.find((block) => block.compactionRole === "end");
  const state = endBlock?.compactionState ?? bodyBlock?.compactionState ?? startBlock?.compactionState ?? "running";
  const running = state === "running";
  const displayStatus = running ? "running" : state === "failed" ? "failed" : "completed";
  const sectionIdRef = React.useRef(startBlock?.compactionSectionId ?? startBlock?.id ?? "");
  const collapseId = `context-compaction:${startBlock?.compactionSectionId ?? startBlock?.id ?? "unknown"}`;
  const [expanded, setExpanded] = useConversationMeasuredExpansion(collapseId, running);

  React.useEffect(() => {
    const sectionId = startBlock?.compactionSectionId ?? startBlock?.id ?? "";
    if (sectionIdRef.current !== sectionId) {
      sectionIdRef.current = sectionId;
      setExpanded(running, { manual: false });
      return;
    }
    if (!running) {
      setExpanded(false, { manual: false });
    }
  }, [running, setExpanded, startBlock?.compactionSectionId, startBlock?.id]);

  const title = displayStatus === "failed"
    ? t("contextCompaction.title.failed")
    : running
      ? t("contextCompaction.title.running")
      : t("contextCompaction.title.completed");
  const detailBody = bodyBlock?.body?.trim() || (displayStatus === "failed" ? contextCompactionFailedDetail(endBlock, t) : "");
  const shouldShowProgress = progressBlocks.length > 0 && thinkingBlocks.length === 0 && !detailBody;
  const hasExpandedContent = shouldShowProgress || thinkingBlocks.length > 0 || Boolean(detailBody);

  return (
    <section className={cn("context-compaction-block", displayStatus, expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="context-compaction-toggle"
        onClick={() => {
          setExpanded(!expanded);
        }}
        type="button"
      >
        <span className="context-compaction-title">
          <ScrollText size={15} />
          <strong>{title}</strong>
          {expanded ? <ChevronDown className="context-compaction-toggle-icon" size={13} /> : <ChevronRight className="context-compaction-toggle-icon" size={13} />}
        </span>
      </button>
      <ConversationMeasuredCollapse collapseId={collapseId} expanded={expanded}>
        <div className="context-compaction-body">
          {shouldShowProgress ? progressBlocks.map((block) => {
            // content keeps progress rendering stable when the record is still being appended.
            // content 在记录仍持续追加时保持进度渲染稳定。
            const content = localizedContextCompactionProgress(block, t);
            return content ? (
              <div className="context-compaction-progress" key={block.id}>
                <MarkdownMessage content={content} eager onOpenMedia={props.onOpenMedia} />
              </div>
            ) : null;
          }) : null}
          {thinkingBlocks.map((block) => {
            return (
              <ThinkingBlock
                autoExpanded={false}
                autoExpandWhileStreaming={false}
                block={block}
                key={block.id}
                sessionRunning={running}
                streaming={running}
                onOpenMedia={props.onOpenMedia}
              />
            );
          })}
          {detailBody ? (
            <MarkdownMessage content={detailBody} onOpenMedia={props.onOpenMedia} />
          ) : !hasExpandedContent ? (
            <span>{t("contextCompaction.generating")}</span>
          ) : null}
          <div className="context-compaction-end-line" aria-hidden="true" />
        </div>
      </ConversationMeasuredCollapse>
    </section>
  );
}

// localizedContextCompactionProgress maps host progress stages to frontend locale text.
// localizedContextCompactionProgress 将宿主进度阶段映射为前端当前语言文本。
function localizedContextCompactionProgress(
  block: ConversationDisplayBlock,
  t: ReturnType<typeof useI18n>["t"],
): string {
  const stage = metadataString(block.rawMetadata, ["context_compaction_progress_stage"]) ?? block.body.trim();
  const key = contextCompactionProgressKey(stage);
  return `- ${t(key)}`;
}

// contextCompactionProgressKey returns the localization key for one compaction progress stage.
// contextCompactionProgressKey 返回单个压缩进度阶段对应的本地化键。
function contextCompactionProgressKey(stage: string): TranslationKey {
  switch (stage) {
    case "prepare_input":
      return "contextCompaction.progress.prepareInput";
    case "build_request":
      return "contextCompaction.progress.buildRequest";
    case "prepare_runtime":
      return "contextCompaction.progress.prepareRuntime";
    case "runtime_ready":
      return "contextCompaction.progress.runtimeReady";
    case "prepared":
      return "contextCompaction.progress.prepared";
    case "request_ready":
      return "contextCompaction.progress.requestReady";
    case "provider_checkpoint":
      return "contextCompaction.progress.providerCheckpoint";
    case "provider_lowering_repair":
      return "contextCompaction.progress.providerLoweringRepair";
    case "wire_request_ready":
      return "contextCompaction.progress.wireRequestReady";
    case "request_started":
      return "contextCompaction.progress.requestStarted";
    case "thinking_delta":
      return "contextCompaction.progress.thinkingDelta";
    case "output_delta":
      return "contextCompaction.progress.outputDelta";
    case "prepare_execution":
      return "contextCompaction.progress.prepareExecution";
    case "execution_ready":
      return "contextCompaction.progress.executionReady";
    case "bootstrap_runtime":
      return "contextCompaction.progress.bootstrapRuntime";
    default:
      return "contextCompaction.progress.unknown";
  }
}

// contextCompactionFailedDetail builds localized failure copy from structured metadata.
// contextCompactionFailedDetail 根据结构化元数据构建本地化失败说明。
function contextCompactionFailedDetail(
  block: ConversationDisplayBlock | undefined,
  t: ReturnType<typeof useI18n>["t"],
): string {
  if (!block) {
    return t("contextCompaction.failed.note");
  }
  const trigger = localizedContextCompactionTrigger(
    metadataString(block.rawMetadata, ["context_compaction_trigger"]) ?? block.compactionTrigger,
    t,
  );
  const error = metadataString(block.rawMetadata, ["context_compaction_error"]) ?? block.body.trim();
  const lines = [`**${t("contextCompaction.failed.trigger")}:** ${trigger}`];
  if (error) {
    lines.push(`**${t("contextCompaction.failed.error")}:** ${error}`);
  }
  lines.push(t("contextCompaction.failed.note"));
  return lines.join("\n");
}

// localizedContextCompactionTrigger returns the frontend-localized trigger label.
// localizedContextCompactionTrigger 返回前端本地化后的触发方式标签。
function localizedContextCompactionTrigger(
  trigger: string | undefined,
  t: ReturnType<typeof useI18n>["t"],
): string {
  if (trigger === "auto") {
    return t("contextCompaction.trigger.auto");
  }
  return t("contextCompaction.trigger.manual");
}

// ThinkingBlock renders thinking content directly inside the transcript.
// ThinkingBlock 在会话转录中直接渲染思考内容。
export function ThinkingBlock(props: {
  // autoExpanded keeps call-site compatibility with thinking overlay restoration.
  // autoExpanded 保持与思考浮层恢复调用点兼容。
  autoExpanded: boolean;
  // block contains the thinking transcript payload.
  // block 包含思考转录载荷。
  block: ConversationDisplayBlock;
  // sessionRunning keeps call-site compatibility with completion collapse behavior.
  // sessionRunning 保持与完成后折叠行为调用点兼容。
  sessionRunning: boolean;
  // streaming marks the currently growing thinking row.
  // streaming 标记当前仍在增长的思考行。
  streaming: boolean;
  // autoExpandWhileStreaming controls whether streaming thinking opens itself.
  // autoExpandWhileStreaming 控制流式思考是否自动展开自身。
  autoExpandWhileStreaming?: boolean;
  // onOpenMedia opens generated media referenced from markdown content.
  // onOpenMedia 打开 Markdown 内容引用的生成媒体。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  const { locale, t } = useI18n();
  const content = props.block.body.trim() || props.block.summary || t("contextCompaction.emptyThinking");
  const shouldAutoExpand = props.autoExpanded || (props.streaming && (props.autoExpandWhileStreaming ?? true));
  const collapseId = `thinking:${props.block.id}`;
  const [expanded, setExpanded] = useConversationMeasuredExpansion(collapseId, shouldAutoExpand);
  const wasStreamingRef = React.useRef(props.streaming);

  React.useEffect(() => {
    wasStreamingRef.current = props.streaming;
    setExpanded(shouldAutoExpand, { manual: false });
  }, [props.block.id, setExpanded, shouldAutoExpand]);

  React.useEffect(() => {
    if (shouldAutoExpand) {
      setExpanded(true, { manual: false });
    }
    if (wasStreamingRef.current && !props.streaming) {
      setExpanded(false, { manual: false });
    }
    wasStreamingRef.current = props.streaming;
  }, [props.streaming, setExpanded, shouldAutoExpand]);

  const durationLabel = props.streaming ? t("contextCompaction.thinkingActive") : thinkingDurationLabel(props.block, locale, t);

  return (
    <section className={cn("thinking-block", expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="thinking-toggle"
        onClick={() => {
          setExpanded(!expanded);
        }}
        type="button"
      >
        <span className="thinking-toggle-label">
          <BrainCircuit size={15} />
          <strong>{t("contextCompaction.thinkingLabel")}</strong>
          {durationLabel ? <span>{durationLabel}</span> : null}
        </span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle thinking-toggle-icon" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle thinking-toggle-icon" size={15} />
        )}
      </button>
      <ConversationMeasuredCollapse collapseId={collapseId} expanded={expanded}>
        <div className="thinking-body">
          <MarkdownMessage content={content} eager streaming={props.streaming} onOpenMedia={props.onOpenMedia} />
        </div>
      </ConversationMeasuredCollapse>
    </section>
  );
}

// thinkingDurationLabel reads a concrete thinking duration from transcript metadata.
// thinkingDurationLabel 从转录元数据中读取明确记录的思考耗时。
function thinkingDurationLabel(block: ConversationDisplayBlock, locale: "zh-CN" | "en-US", t: (key: import("@/i18n-messages").TranslationKey) => string): string | null {
  const durationMs = thinkingDurationMs(block.rawMetadata);
  if (durationMs === null) {
    return null;
  }
  const formatted = formatWorkProcessDuration(durationMs, locale);
  return formatted ? t("conversation.process.duration").replace("{duration}", formatted) : null;
}

// thinkingDurationMs extracts known duration or elapsed fields without fabricating fallback values.
// thinkingDurationMs 提取已知 duration 或 elapsed 字段，且不会伪造兜底耗时。
function thinkingDurationMs(metadata: unknown): number | null {
  const direct = thinkingDurationFromRecord(metadata, 0);
  if (direct !== null) {
    return direct;
  }
  return null;
}

// thinkingDurationFromRecord recursively scans shallow metadata records for concrete duration values.
// thinkingDurationFromRecord 递归扫描浅层元数据记录中的明确耗时值。
function thinkingDurationFromRecord(value: unknown, depth: number): number | null {
  if (!value || typeof value !== "object" || Array.isArray(value) || depth > 2) {
    return null;
  }
  const record = value as Record<string, unknown>;
  const candidateKeys = [
    "thinking_duration_ms",
    "thinkingDurationMs",
    "duration_ms",
    "durationMs",
    "elapsed_ms",
    "elapsedMs",
    "duration",
    "elapsed",
  ];
  for (const key of candidateKeys) {
    const parsed = thinkingDurationValueToMs(record[key], key);
    if (parsed !== null) {
      return parsed;
    }
  }
  for (const key of ["metadata", "display", "timing", "stats"]) {
    const nested = thinkingDurationFromRecord(record[key], depth + 1);
    if (nested !== null) {
      return nested;
    }
  }
  return null;
}

// thinkingDurationValueToMs normalizes metadata duration values into milliseconds.
// thinkingDurationValueToMs 将元数据耗时值规范化为毫秒。
function thinkingDurationValueToMs(value: unknown, key: string): number | null {
  const lowerKey = key.toLowerCase();
  if (typeof value === "number" && Number.isFinite(value) && value >= 0) {
    return lowerKey.includes("ms") ? value : value * 1000;
  }
  if (typeof value !== "string") {
    return null;
  }
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  const millisecondMatch = /^(\d+(?:\.\d+)?)\s*ms$/i.exec(trimmed);
  if (millisecondMatch) {
    return Number.parseFloat(millisecondMatch[1]);
  }
  const secondMatch = /^(\d+(?:\.\d+)?)\s*(s|秒)$/i.exec(trimmed);
  if (secondMatch) {
    return Number.parseFloat(secondMatch[1]) * 1000;
  }
  const numericValue = Number.parseFloat(trimmed);
  if (Number.isFinite(numericValue) && /^\d+(?:\.\d+)?$/.test(trimmed)) {
    return lowerKey.includes("ms") ? numericValue : numericValue * 1000;
  }
  return null;
}

// TurnStatsBlock renders the compact non-clickable per-turn statistics row.
// TurnStatsBlock 渲染不可点击的紧凑按轮次统计横条。
export function TurnStatsBlock(props: {
  // block contains the compact turn statistics payload.
  // block 包含紧凑轮次统计载荷。
  block: ConversationDisplayBlock;
}) {
  const { t } = useI18n();
  const turnKind = metadataString(props.block.rawMetadata, [
    "turn_stats.turn_kind",
    "turnStats.turnKind",
  ]);
  const subagentStats = turnKind?.trim().toLowerCase() === "subagent_feedback";
  const summary = subagentStats
    ? ((subagentTurnStatsSummary(props.block.rawMetadata, t) ?? props.block.summary) || props.block.title)
    : (props.block.summary || props.block.title);
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className={cn("turn-stats-row", props.block.status ?? "completed", subagentStats && "subagent")}>
          {subagentStats ? <Bot aria-hidden="true" size={13} /> : <span aria-hidden="true">{turnStatsIcon(props.block.status)}</span>}
          <strong>{summary}</strong>
        </div>
      </TooltipTrigger>
      <TooltipContent className="turn-stats-tooltip" side="right" sideOffset={8}>
        <pre>{props.block.body}</pre>
      </TooltipContent>
    </Tooltip>
  );
}

// StreamingThinkingOverlay renders the active thinking stream as a floating session-local surface.
// StreamingThinkingOverlay 将活动思考流渲染为当前会话内的浮动面板。
export function StreamingThinkingOverlay(props: {
  // block contains the currently streaming thinking transcript.
  // block 包含当前正在流式输出的思考转录。
  block: ConversationDisplayBlock;
  // leaving marks the popup fade-out phase after the hold delay.
  // leaving 标记保留等待后的弹窗淡出阶段。
  leaving: boolean;
}) {
  // t resolves localized labels for the floating thinking surface.
  // t 解析浮动思考面板使用的本地化文案。
  const { t } = useI18n();
  // bodyRef stores the scrollable thinking body for tail following.
  // bodyRef 保存可滚动思考正文，用于追随最新尾部。
  const bodyRef = React.useRef<HTMLDivElement | null>(null);

  React.useLayoutEffect(() => {
    const element = bodyRef.current;
    if (!element) {
      return;
    }
    element.scrollTop = element.scrollHeight;
  }, [props.block.body]);

  return (
    <section className={cn("streaming-thinking-overlay", props.leaving && "leaving")} aria-live="polite" aria-busy={!props.leaving}>
      <header className="streaming-thinking-header">
        <span className="streaming-thinking-title">
          <BrainCircuit size={16} />
          <span className="shimmer">{t("runtime.thinking")}</span>
        </span>
        <span className="streaming-thinking-caption">{t("runtime.thinkingOverlayHint")}</span>
      </header>
      <div className="streaming-thinking-body" ref={bodyRef}>
        <MarkdownMessage content={props.block.body || t("runtime.thinking")} eager streaming={!props.leaving} />
      </div>
    </section>
  );
}

// turnStatsIcon returns the compact status mark used by turn statistics.
// turnStatsIcon 返回轮次统计使用的紧凑状态标记。
function turnStatsIcon(status?: string): string {
  if (status === "failed") return "!";
  if (status === "stopped") return "■";
  return "◔";
}

// subagentTurnStatsSummary formats child-agent stats as a display-only usage chip.
// subagentTurnStatsSummary 将子智能体统计格式化为仅显示的消耗胶囊。
function subagentTurnStatsSummary(
  // Raw metadata attached to the turn-stats display block.
  // turn-stats 展示块携带的原始元数据。
  metadata: unknown,
  // Translation helper.
  // 翻译辅助函数。
  t: (key: TranslationKey) => string,
): string | null {
  const stats = turnStatsMetadataObject(metadata);
  if (!stats) {
    return null;
  }
  const providerCallCount = numberMetadata(stats.provider_call_count ?? stats.providerCallCount);
  const toolCallCount = numberMetadata(stats.tool_call_count ?? stats.toolCallCount);
  const totalTokens = numberMetadata(stats.total_tokens ?? stats.totalTokens);
  const cachedInputTokens = numberMetadata(stats.cached_input_tokens ?? stats.cachedInputTokens);
  const parts = [
    formatStatsTemplate(t("conversation.stats.providerCalls"), providerCallCount),
    formatStatsTemplate(t("conversation.stats.toolCalls"), toolCallCount),
  ];
  if (totalTokens > 0) {
    parts.push(formatStatsTemplate(t("conversation.stats.totalTokens"), totalTokens));
  }
  if (cachedInputTokens > 0) {
    parts.push(formatStatsTemplate(t("conversation.stats.cachedTokensCompact"), cachedInputTokens));
  }
  return parts.join(" · ");
}

// turnStatsMetadataObject resolves structured turn statistics from block metadata.
// turnStatsMetadataObject 从块元数据中解析结构化轮次统计。
function turnStatsMetadataObject(
  // Raw metadata attached to the display block.
  // 展示块携带的原始元数据。
  metadata: unknown,
): Record<string, unknown> | null {
  const value = metadataValue(metadata, ["turn_stats", "turnStats"]);
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

// formatStatsTemplate injects a count into a simple statistics template.
// formatStatsTemplate 向简单统计模板注入数量。
function formatStatsTemplate(
  // Template containing a count placeholder.
  // 包含数量占位符的模板。
  template: string,
  // Count value to render.
  // 需要渲染的数量。
  count: number,
): string {
  return template.replaceAll("{count}", String(count));
}
