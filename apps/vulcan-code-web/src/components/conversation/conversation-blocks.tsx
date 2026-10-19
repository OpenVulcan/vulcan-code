import { ThinkingPreview } from "./thinking-preview";
import React from "react";
import {
  Bot,
  ChevronDown,
  ChevronRight,
  FileDiff,
  ScrollText,
} from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/ui-kit/primitives/tooltip";
import { useI18n, type TranslationKey } from "@/i18n";
import { cn } from "@/lib/utils";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import {
  ConversationMeasuredCollapse,
  useConversationMeasuredExpansion,
} from "@/components/conversation/conversation-measured-collapse";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
import { StreamingMarkdownDocument } from "@/components/conversation/streaming-markdown-document";
import {
  shouldUseStreamingThinkingMarkdown,
  streamingThinkingIdentity,
} from "@/components/conversation/thinking-render-path";
import {
  metadataString,
  metadataValue,
  numberMetadata,
} from "@/components/conversation/conversation-metadata";
import { formatWorkProcessDuration } from "@/components/conversation/conversation-timeline";
import { TurnStatsTooltipCard } from "@/components/conversation/conversation-turn-stats-card";
import { useOpenTurnChangeReview } from "@/components/conversation/turn-change-review-context";
import type { TurnChangeSummaryV1 } from "@/diff/turn-change-summary-v1";

// LazyTurnChangeFileList loads the per-turn file list and its stylesheet on first expansion.
// LazyTurnChangeFileList 在首次展开时加载每轮文件列表及其样式表。
const LazyTurnChangeFileList = React.lazy(async () => {
  const module = await import("@/components/conversation/turn-change-file-list");
  return { default: module.TurnChangeFileList };
});

// ConversationDisplayBlock contains the transcript fields needed by extracted display blocks.
// ConversationDisplayBlock 包含已拆分展示块所需的转录字段。
export interface ConversationDisplayBlock {
  // Stable block id used by React keys and section memory.
  // React key 与分段记忆使用的稳定块标识。
  id: string;
  // Optional authoritative backend message identity used by live stream binding.
  // 实时流绑定使用的可选权威后端消息身份。
  sourceMessageId?: string;
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
  // Conversation identity used to bind streamed thinking rows to the live Store.
  // 用于把流式思考行绑定到实时 Store 的会话身份。
  conversationId: string | null;
  // onOpenMedia opens generated media referenced from markdown content.
  // onOpenMedia 打开 Markdown 内容引用的生成媒体。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  const { t } = useI18n();
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
                conversationId={props.conversationId}
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
export function contextCompactionProgressKey(stage: string): TranslationKey {
  switch (stage) {
    case "prepare_input":
      return "contextCompaction.progress.prepareInput";
    case "prepare_remote_tooling":
      return "contextCompaction.progress.prepareRemoteTooling";
    case "remote_compaction_check":
      return "contextCompaction.progress.remoteCompactionCheck";
    case "remote_compaction_complete":
      return "contextCompaction.progress.remoteCompactionComplete";
    case "remote_compaction_local_fallback":
      return "contextCompaction.progress.remoteCompactionLocalFallback";
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
    case "provider_lowering_repair":
      return "contextCompaction.progress.providerLoweringRepair";
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
  if (trigger === "auto_budget") {
    return t("contextCompaction.trigger.auto");
  }
  return t("contextCompaction.trigger.manual");
}

// ThinkingBlock renders a gutter trigger whose popup retains the complete reasoning document.
// ThinkingBlock 渲染左侧按钮，并在弹窗中保留完整思考文档。
export function ThinkingBlock(props: {
  // autoExpanded keeps call-site compatibility with thinking overlay restoration.
  // autoExpanded 保持与思考浮层恢复调用点兼容。
  autoExpanded: boolean;
  // block contains the thinking transcript payload.
  // block 包含思考转录载荷。
  block: ConversationDisplayBlock;
  // Conversation identity used by the live stream Store.
  // 实时流 Store 使用的会话身份。
  conversationId: string | null;
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
  // content preserves the complete reasoning payload for the gutter preview.
  // content 为左侧预览保留完整思考载荷。
  const content = props.block.body.trim() || props.block.summary || t("contextCompaction.emptyThinking");
  // liveMarkdownIdentity binds one thinking block to its exact live Store stream and block mount.
  // liveMarkdownIdentity 将单个思考块绑定到精确的实时 Store 流和块挂载实例。
  const liveMarkdownIdentity = streamingThinkingIdentity(
    props.conversationId,
    props.block.sourceMessageId,
    props.block.id,
  );
  // streamingMarkdownIdentity keeps the segmented renderer through the active-to-sealed handoff.
  // streamingMarkdownIdentity 在 active 到 sealed 的交接期间保持分段渲染器。
  const [streamingMarkdownIdentity, setStreamingMarkdownIdentity] = React.useState<string | null>(
    () => props.streaming ? liveMarkdownIdentity : null,
  );
  React.useEffect(() => {
    if (props.streaming && liveMarkdownIdentity) {
      setStreamingMarkdownIdentity(liveMarkdownIdentity);
    }
  }, [liveMarkdownIdentity, props.streaming]);
  // useStreamingMarkdown is limited to blocks that actually entered the live rendering path.
  // useStreamingMarkdown 仅对确实进入实时渲染路径的块启用。
  const useStreamingMarkdown = shouldUseStreamingThinkingMarkdown(
    liveMarkdownIdentity,
    props.streaming,
    streamingMarkdownIdentity,
  );
  // durationLabel remains available inside the preview instead of occupying a separate transcript header.
  // durationLabel 保留在预览内部，不再占据独立转录标题。
  const durationLabel = props.streaming ? t("contextCompaction.thinkingActive") : thinkingDurationLabel(props.block, locale, t);
  return (
    <div className="vk-gutter-anchor">
      <ThinkingPreview compact streaming={props.streaming}>
        {durationLabel ? <p>{durationLabel}</p> : null}
        {useStreamingMarkdown && props.conversationId && props.block.sourceMessageId ? (
          <StreamingMarkdownDocument conversationId={props.conversationId} fallbackContent={content}
            fallbackStreaming={props.streaming} messageId={props.block.sourceMessageId} onOpenMedia={props.onOpenMedia} />
        ) : <MarkdownMessage content={content} eager streaming={props.streaming} onOpenMedia={props.onOpenMedia} />}
      </ThinkingPreview>
    </div>
  );
}

// thinkingDurationLabel reads a concrete thinking duration from transcript metadata.
// thinkingDurationLabel 从转录元数据中读取明确记录的思考耗时。
function thinkingDurationLabel(block: ConversationDisplayBlock, locale: import("@/i18n").LocaleCode, t: (key: import("@/i18n-messages").TranslationKey) => string): string | null {
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

// TurnStatsBlock renders the compact per-turn statistics row and its structured hover card.
// TurnStatsBlock 渲染紧凑轮次统计横条及其结构化悬浮卡片。
export function TurnStatsBlock(props: {
  // block contains the compact turn statistics payload.
  // block 包含紧凑轮次统计载荷。
  block: ConversationDisplayBlock;
}) {
  // locale and t keep both the compact row and its detailed card synchronized with the active language.
  // locale 与 t 使紧凑横条和详情卡片同步跟随当前语言。
  const { locale, t } = useI18n();
  // turnKind identifies display-only child-agent feedback rows without inferring from their body text.
  // turnKind 通过权威元数据识别仅供展示的子代理反馈行，不从正文猜测。
  const turnKind = metadataString(props.block.rawMetadata, [
    "turn_stats.turn_kind",
    "turnStats.turnKind",
  ]);
  // subagentStats controls the badge icon and prevents duplicating a child section inside a child-only row.
  // subagentStats 控制徽标图标，并避免在纯子代理统计行内重复展示子代理分区。
  const subagentStats = turnKind?.trim().toLowerCase() === "subagent_feedback";
  // summary keeps the existing compact transcript wording while the tooltip uses structured metadata.
  // summary 保留现有紧凑转录文案，而悬浮卡改用结构化元数据。
  const summary = subagentStats
    ? ((subagentTurnStatsSummary(props.block.rawMetadata, t) ?? props.block.summary) || props.block.title)
    : (props.block.summary || props.block.title);
  // statisticsRow is shared by structured and legacy blocks so legacy transcripts never open an empty tooltip.
  // statisticsRow 由结构化与旧版块共享，避免旧版转录打开空白悬浮卡。
  const statisticsRow = (
    <div
      aria-label={summary}
      className={cn("turn-stats-row", props.block.status ?? "completed", subagentStats && "subagent")}
      tabIndex={0}
    >
      {subagentStats ? <Bot aria-hidden="true" size={13} /> : <span aria-hidden="true">{turnStatsIcon(props.block.status)}</span>}
      <strong>{summary}</strong>
    </div>
  );
  // turnChanges is the Turn's applied file changes merged into this row; absent when the Turn changed no file.
  // turnChanges 是合并到本行的该 Turn 已应用文件变更；该 Turn 未改动文件时缺省。
  const turnChanges = turnChangeSummaryMetadata(props.block.rawMetadata);
  const turnId = metadataValue(props.block.rawMetadata, ["v1_turn_id"]);
  // openInGit exists only while the review panel's last-turn source shows exactly this Turn.
  // openInGit 仅在审核面板的上一轮来源恰好展示本 Turn 时存在。
  const openInGit = useOpenTurnChangeReview(typeof turnId === "number" ? turnId : null);
  const [changesOpen, setChangesOpen] = React.useState(false);
  const statistics = turnStatsMetadataObject(props.block.rawMetadata) ? (
    <Tooltip>
      <TooltipTrigger asChild>
        {statisticsRow}
      </TooltipTrigger>
      <TooltipContent
        align="start"
        className="context-usage-tooltip turn-stats-tooltip"
        showArrow={false}
        side="top"
        sideOffset={8}
      >
        <TurnStatsTooltipCard
          locale={locale}
          metadata={props.block.rawMetadata}
          subagentStats={subagentStats}
        />
      </TooltipContent>
    </Tooltip>
  ) : statisticsRow;
  if (!turnChanges) {
    return statistics;
  }
  return (
    <div className="turn-stats-line">
      {statistics}
      <button
        aria-expanded={changesOpen}
        aria-label={t("conversation.turnChanges.title")}
        className="turn-stats-row turn-changes-toggle"
        onClick={() => setChangesOpen((current) => !current)}
        type="button"
      >
        <FileDiff aria-hidden="true" size={13} />
        <strong>{formatStatsTemplate(t("conversation.turnChanges.files"), turnChanges.files.length)}</strong>
        <span className="file-change-added">+{turnChanges.additions.toLocaleString(locale)}</span>
        <span className="file-change-removed">-{turnChanges.deletions.toLocaleString(locale)}</span>
        {changesOpen ? <ChevronDown aria-hidden="true" size={13} /> : <ChevronRight aria-hidden="true" size={13} />}
      </button>
      {changesOpen ? (
        <React.Suspense fallback={null}>
          <LazyTurnChangeFileList summary={turnChanges} onOpenInGit={openInGit} />
        </React.Suspense>
      ) : null}
    </div>
  );
}

// turnChangeSummaryMetadata reads the Turn change summary the V1 presentation attached to a statistics block.
// turnChangeSummaryMetadata 读取 V1 展示附加到统计块上的 Turn 变更汇总。
function turnChangeSummaryMetadata(metadata: unknown): TurnChangeSummaryV1 | null {
  const value = metadataValue(metadata, ["turn_changes"]);
  return value && typeof value === "object" && Array.isArray((value as TurnChangeSummaryV1).files)
    ? value as TurnChangeSummaryV1
    : null;
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
