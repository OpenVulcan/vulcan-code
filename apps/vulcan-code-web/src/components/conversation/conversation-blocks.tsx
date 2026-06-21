import React from "react";
import {
  BrainCircuit,
  ChevronDown,
  ChevronRight,
  ScrollText,
} from "lucide-react";
import { ShinyText } from "@/components/ui/shiny-text";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useI18n } from "@/i18n";
import { cn } from "@/lib/utils";
import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import { ConversationCollapsibleMotion } from "@/components/conversation/conversation-collapsible-motion";
import { dispatchConversationManualViewportInteraction } from "@/components/conversation/conversation-layout-events";
import { MarkdownMessage } from "@/components/conversation/conversation-markdown";
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
  const startBlock = props.blocks.find((block) => block.compactionRole === "start") ?? props.blocks[0];
  const thinkingBlocks = props.blocks.filter((block) => block.compactionRole === "thinking" && block.kind === "thinking");
  const bodyBlock = props.blocks.find((block) => block.compactionRole === "body");
  const endBlock = props.blocks.find((block) => block.compactionRole === "end");
  const state = endBlock?.compactionState ?? bodyBlock?.compactionState ?? startBlock?.compactionState ?? "running";
  const running = state === "running";
  const displayStatus = running ? "running" : state === "failed" ? "failed" : "completed";
  const [expanded, setExpanded] = React.useState(running);
  const sectionIdRef = React.useRef(startBlock?.compactionSectionId ?? startBlock?.id ?? "");

  React.useEffect(() => {
    const sectionId = startBlock?.compactionSectionId ?? startBlock?.id ?? "";
    if (sectionIdRef.current !== sectionId) {
      sectionIdRef.current = sectionId;
      setExpanded(running);
      return;
    }
    if (!running) {
      setExpanded(false);
    }
  }, [running, startBlock?.compactionSectionId, startBlock?.id]);

  const title = running ? (startBlock?.title ?? "上下文正在压缩") : (endBlock?.title ?? "上下文已压缩");
  const detailBody = bodyBlock?.body?.trim() || (displayStatus === "failed" ? endBlock?.body?.trim() ?? "" : "");
  const hasExpandedContent = thinkingBlocks.length > 0 || Boolean(detailBody);

  return (
    <section className={cn("context-compaction-block", displayStatus, expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="context-compaction-toggle"
        onClick={() => {
          dispatchConversationManualViewportInteraction();
          setExpanded((current) => !current);
        }}
        type="button"
      >
        <span className="context-compaction-title">
          <ScrollText size={15} />
          <strong>{title}</strong>
          {expanded ? <ChevronDown className="context-compaction-toggle-icon" size={13} /> : <ChevronRight className="context-compaction-toggle-icon" size={13} />}
        </span>
      </button>
      <ConversationCollapsibleMotion expanded={expanded}>
        <div className="context-compaction-body">
          {thinkingBlocks.map((block) => {
            const content = block.body.trim() || block.summary || "暂无思考内容";
            return (
              <div className="context-compaction-thinking thinking-block" key={block.id}>
                <div className="thinking-body">
                  <MarkdownMessage content={`**Thinking:** ${content}`} eager onOpenMedia={props.onOpenMedia} />
                </div>
              </div>
            );
          })}
          {detailBody ? (
            <MarkdownMessage content={detailBody} onOpenMedia={props.onOpenMedia} />
          ) : !hasExpandedContent ? (
            <span>正在生成上下文压缩总结。</span>
          ) : null}
          <div className="context-compaction-end-line" aria-hidden="true" />
        </div>
      </ConversationCollapsibleMotion>
    </section>
  );
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
  // onOpenMedia opens generated media referenced from markdown content.
  // onOpenMedia 打开 Markdown 内容引用的生成媒体。
  onOpenMedia: (attachment: GeneratedMediaAttachment) => void;
}) {
  const content = props.block.body.trim() || props.block.summary || "暂无思考内容";
  const shouldAutoExpand = props.streaming || props.autoExpanded;
  const [expanded, setExpanded] = React.useState(shouldAutoExpand);
  const wasStreamingRef = React.useRef(props.streaming);

  React.useEffect(() => {
    wasStreamingRef.current = props.streaming;
    setExpanded(shouldAutoExpand);
  }, [props.block.id]);

  React.useEffect(() => {
    if (shouldAutoExpand) {
      setExpanded(true);
    }
    if (wasStreamingRef.current && !props.streaming) {
      setExpanded(false);
    }
    wasStreamingRef.current = props.streaming;
  }, [props.streaming, shouldAutoExpand]);

  const durationLabel = props.streaming ? "正在思考" : thinkingDurationLabel(props.block);

  return (
    <section className={cn("thinking-block", expanded && "expanded")}>
      <button
        aria-expanded={expanded}
        className="thinking-toggle"
        onClick={() => {
          dispatchConversationManualViewportInteraction();
          setExpanded((current) => !current);
        }}
        type="button"
      >
        <span className="thinking-toggle-label">
          <BrainCircuit size={15} />
          <strong>思考过程</strong>
          {durationLabel ? <span>{durationLabel}</span> : null}
        </span>
        {expanded ? (
          <ChevronDown className="tool-presentation-toggle thinking-toggle-icon" size={15} />
        ) : (
          <ChevronRight className="tool-presentation-toggle thinking-toggle-icon" size={15} />
        )}
      </button>
      <ConversationCollapsibleMotion expanded={expanded}>
        <div className="thinking-body">
          <MarkdownMessage content={content} eager streaming={props.streaming} onOpenMedia={props.onOpenMedia} />
        </div>
      </ConversationCollapsibleMotion>
    </section>
  );
}

// thinkingDurationLabel reads a concrete thinking duration from transcript metadata.
// thinkingDurationLabel 从转录元数据中读取明确记录的思考耗时。
function thinkingDurationLabel(block: ConversationDisplayBlock): string | null {
  const durationMs = thinkingDurationMs(block.rawMetadata);
  if (durationMs === null) {
    return null;
  }
  const formatted = formatWorkProcessDuration(durationMs);
  return formatted ? `持续 ${formatted}` : null;
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
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <div className={`turn-stats-row ${props.block.status ?? "completed"}`}>
          <span aria-hidden="true">{turnStatsIcon(props.block.status)}</span>
          <strong>{props.block.summary || props.block.title}</strong>
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
          <ShinyText text={t("runtime.thinking")} />
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
