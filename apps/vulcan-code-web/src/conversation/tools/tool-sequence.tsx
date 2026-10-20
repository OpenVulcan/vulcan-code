// Head of a folded run of consecutive tool calls: one tick per call tinted by family and state, the call count,
// how many calls each family made, and how many did not succeed, so a closed group still says what happened.
// 连续工具调用折叠组的标题：每次调用一格、按家族与状态着色，并给出调用总数、各家族次数与未成功次数，
// 让收起的分组仍能说明发生了什么。
import { ChevronRight } from "lucide-react";
import { useI18n } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { TOOL_FAMILIES, type ToolFamily } from "./tool-card";
import { toolCallFamily } from "./tool-call-card";
import { readToolCall, type ToolCardState } from "./tool-call";

// ToolSequenceEntry is one call as the sequence head counts it.
// ToolSequenceEntry 是序列标题统计的一次调用。
export interface ToolSequenceEntry {
  // Purpose family.
  // 用途家族。
  family: ToolFamily;
  // Visual lifecycle.
  // 视觉生命周期。
  state: ToolCardState;
}

// toolSequenceEntry reads one call; a row without a durable event counts as an other-family call in the row state.
// toolSequenceEntry 读取一次调用；没有可靠事件的记录按行状态计为其他家族调用。
export function toolSequenceEntry(event: ToolCallEvent | undefined, running: boolean): ToolSequenceEntry {
  if (!event) return { family: "other", state: running ? "running" : "done" };
  return { family: toolCallFamily(event), state: readToolCall(event).state };
}

// ToolSequenceSummary is what a sequence head reports.
// ToolSequenceSummary 是序列标题报告的内容。
export interface ToolSequenceSummary {
  // Call counts per family, in the fixed family order, omitting families without calls.
  // 按固定家族顺序排列的各家族调用数，省略没有调用的家族。
  families: Array<[ToolFamily, number]>;
  // Calls that failed or were stopped.
  // 失败或被停止的调用数。
  unsuccessful: number;
  // Calls still running or waiting.
  // 仍在运行或等待的调用数。
  active: number;
}

// summarizeToolSequence counts a sequence for its head.
// summarizeToolSequence 为序列标题统计一个序列。
export function summarizeToolSequence(entries: readonly ToolSequenceEntry[]): ToolSequenceSummary {
  const counts = new Map<ToolFamily, number>();
  let unsuccessful = 0;
  let active = 0;
  for (const entry of entries) {
    counts.set(entry.family, (counts.get(entry.family) ?? 0) + 1);
    if (entry.state === "failed" || entry.state === "stopped") unsuccessful += 1;
    if (entry.state === "running" || entry.state === "waiting") active += 1;
  }
  return {
    active,
    families: TOOL_FAMILIES.flatMap((family) => (counts.has(family) ? [[family, counts.get(family)!] as [ToolFamily, number]] : [])),
    unsuccessful,
  };
}

/** Renders the toggle that opens or closes a folded tool sequence.
 * 渲染展开或收起工具序列折叠组的切换头。 */
export function ToolSequenceHead(props: {
  // Calls in order.
  // 按顺序排列的调用。
  entries: readonly ToolSequenceEntry[];
  // Whether the sequence body is open.
  // 序列正文是否展开。
  expanded: boolean;
  // Requests the opposite state.
  // 请求相反状态。
  onToggle: () => void;
}) {
  const { t } = useI18n();
  const summary = summarizeToolSequence(props.entries);
  const total = props.entries.length;
  const title = summary.active > 0
    ? t("conversation.toolSequence.runningSummary").replace("{completed}", String(total - summary.active)).replace("{running}", String(summary.active))
    : t("conversation.toolSequence.summary").replace("{count}", String(total));
  return (
    <button
      aria-expanded={props.expanded}
      className="vk-tools-head"
      data-active={summary.active > 0 ? "" : undefined}
      onClick={props.onToggle}
      type="button"
    >
      <ChevronRight aria-hidden="true" className="vk-tools-chev" />
      <span aria-hidden="true" className="vk-tools-strip">
        {props.entries.map((entry, index) => <i data-family={entry.family} data-state={entry.state} key={index} />)}
      </span>
      <b>{title}</b>
      {summary.families.length > 1 ? (
        <span className="vk-tools-families">
          {summary.families.map(([family, count]) => (
            <span data-family={family} key={family}>{t(`toolCard.family.${family}`)} {count}</span>
          ))}
        </span>
      ) : null}
      {summary.unsuccessful > 0 ? (
        <span className="vk-del">{t("conversation.toolSequence.failed").replace("{count}", String(summary.unsuccessful))}</span>
      ) : null}
    </button>
  );
}
