import type { ConversationTimelineItem } from "@/components/conversation/conversation-model";
import { finalAssistantOutputText, stripUserPromptHeading } from "@/components/conversation/conversation-render-items";
import type { ConversationTurnProjection } from "@/components/conversation/conversation-timeline-projection-store";

// CONVERSATION_PROMPT_NAVIGATOR_TITLE_MAX_CHARS bounds the prompt text carried by one navigator entry.
// CONVERSATION_PROMPT_NAVIGATOR_TITLE_MAX_CHARS 限制单个导航条目携带的提示词文本长度。
export const CONVERSATION_PROMPT_NAVIGATOR_TITLE_MAX_CHARS = 160;

// CONVERSATION_PROMPT_NAVIGATOR_PREVIEW_MAX_CHARS bounds the reply preview carried by one navigator entry.
// CONVERSATION_PROMPT_NAVIGATOR_PREVIEW_MAX_CHARS 限制单个导航条目携带的回复预览长度。
export const CONVERSATION_PROMPT_NAVIGATOR_PREVIEW_MAX_CHARS = 320;

// CONVERSATION_PROMPT_NAVIGATOR_ACTIVE_LINE_RATIO is the viewport fraction a prompt must pass to become current.
// CONVERSATION_PROMPT_NAVIGATOR_ACTIVE_LINE_RATIO 是提示词成为当前项前必须越过的视口比例线。
export const CONVERSATION_PROMPT_NAVIGATOR_ACTIVE_LINE_RATIO = 0.3;

// CONVERSATION_PROMPT_NAVIGATOR_MIN_ENTRIES is the smallest prompt count worth showing a navigator for.
// CONVERSATION_PROMPT_NAVIGATOR_MIN_ENTRIES 是值得显示导航的最小提示词数量。
export const CONVERSATION_PROMPT_NAVIGATOR_MIN_ENTRIES = 2;

// ConversationPromptNavigatorEntry describes one user prompt reachable from the navigator rail.
// ConversationPromptNavigatorEntry 描述导航轨道可定位的单个用户提示词。
export interface ConversationPromptNavigatorEntry {
  // Stable Turn key that also identifies the rendered turn shell.
  // 同时标识已渲染 Turn 外壳的稳定 Turn 键。
  rowKey: string;
  // Visible prompt text with collapsed whitespace; empty when the prompt carries no text.
  // 折叠空白后的可见提示词文本；提示词没有文本时为空。
  prompt: string;
  // Plain-text preview of the final assistant reply; empty when no final reply exists.
  // 最终助手回复的纯文本预览；没有最终回复时为空。
  replyPreview: string;
  // Whether the Turn is still processing.
  // 当前 Turn 是否仍在处理中。
  running: boolean;
}

// projectConversationPromptNavigatorEntries derives navigator entries from the ordered rendered Turns.
// projectConversationPromptNavigatorEntries 从有序已渲染 Turn 派生导航条目。
// Parameters: turns is the exact ordered Turn projection rendered by the transcript.
// 参数：turns 是转录渲染的精确有序 Turn 投影。
// Returns: one entry per Turn that starts with a user prompt, in transcript order.
// 返回值：按转录顺序为每个以用户提示词开头的 Turn 返回一个条目。
export function projectConversationPromptNavigatorEntries(
  turns: readonly ConversationTurnProjection[],
): readonly ConversationPromptNavigatorEntry[] {
  return createConversationPromptNavigatorProjector()(turns);
}

// ConversationPromptNavigatorProjector projects entries while reusing every entry whose source text is unchanged.
// ConversationPromptNavigatorProjector 投影条目，并复用源文本未变化的每个条目。
export type ConversationPromptNavigatorProjector = (
  turns: readonly ConversationTurnProjection[],
) => readonly ConversationPromptNavigatorEntry[];

// PromptNavigatorEntrySource records the exact inputs one cached entry was derived from.
// PromptNavigatorEntrySource 记录单个缓存条目所依据的精确输入。
interface PromptNavigatorEntrySource {
  // Raw user prompt body.
  // 原始用户提示词正文。
  prompt: string;
  // Raw final reply text, or null before a final reply exists.
  // 原始最终回复文本；最终回复出现前为 null。
  reply: string | null;
  // Whether the Turn was processing.
  // Turn 是否仍在处理中。
  running: boolean;
  // Entry derived from these inputs.
  // 由这些输入派生的条目。
  entry: ConversationPromptNavigatorEntry;
}

// createConversationPromptNavigatorProjector creates a projector for one transcript surface.
// createConversationPromptNavigatorProjector 为单个转录界面创建投影器。
// Returns: a projector that rebuilds only changed entries and returns the previous array when nothing changed,
// so streaming updates of one Turn neither re-run text cleanup for every Turn nor re-trigger rail measurement.
// 返回值：只重建变化条目、且无变化时返回上一数组的投影器，使单个 Turn 的流式更新既不对每个 Turn 重跑文本
// 清理，也不重新触发轨道测量。
export function createConversationPromptNavigatorProjector(): ConversationPromptNavigatorProjector {
  let sources = new Map<string, PromptNavigatorEntrySource>();
  let previous: readonly ConversationPromptNavigatorEntry[] = [];
  return (turns) => {
    const nextSources = new Map<string, PromptNavigatorEntrySource>();
    const entries: ConversationPromptNavigatorEntry[] = [];
    let changed = false;
    for (const turn of turns) {
      // first is the Turn's leading row; only a user row makes the Turn a prompt target.
      // first 是 Turn 的首行；只有用户行才使该 Turn 成为提示词目标。
      const first = turn.items[0];
      if (!first || first.kind !== "item" || first.item.role !== "user") {
        continue;
      }
      // finalReply is the Turn's final assistant answer when one has been produced.
      // finalReply 是当前 Turn 已产生的最终助手回答。
      const finalReply = turn.items.find(
        (item): item is Extract<ConversationTimelineItem, { kind: "item" }> => item.kind === "item" && item.finalAssistantOutput === true,
      );
      const prompt = first.item.userMessage?.body ?? "";
      const reply = finalReply ? finalAssistantOutputText(finalReply.item) : null;
      const running = turn.items.some((item) => item.kind === "process" && item.process.running);
      const cached = sources.get(turn.key);
      const entry = cached && cached.prompt === prompt && cached.reply === reply && cached.running === running
        ? cached.entry
        : {
            rowKey: turn.key,
            prompt: truncateNavigatorText(
              collapseNavigatorWhitespace(stripUserPromptHeading(prompt)),
              CONVERSATION_PROMPT_NAVIGATOR_TITLE_MAX_CHARS,
            ),
            replyPreview: reply === null
              ? ""
              : truncateNavigatorText(markdownToNavigatorPreview(reply), CONVERSATION_PROMPT_NAVIGATOR_PREVIEW_MAX_CHARS),
            running,
          };
      nextSources.set(turn.key, { prompt, reply, running, entry });
      changed ||= previous[entries.length] !== entry;
      entries.push(entry);
    }
    sources = nextSources;
    if (changed || entries.length !== previous.length) {
      previous = entries;
    }
    return previous;
  };
}

// resolveActivePromptNavigatorIndex finds the last prompt whose top edge has passed the active line.
// resolveActivePromptNavigatorIndex 查找顶部边缘已越过当前线的最后一个提示词。
// Parameters: count is the entry count, topAt reads one entry's viewport-relative top in transcript order, activeLine is the threshold.
// 参数：count 是条目数；topAt 按转录顺序读取单个条目相对视口的顶部；activeLine 是阈值。
// Returns: the active index, 0 when no prompt has passed the line, or -1 for an empty list.
// 返回值：当前索引；尚无提示词越过阈值时返回 0；列表为空时返回 -1。
export function resolveActivePromptNavigatorIndex(
  count: number,
  topAt: (index: number) => number,
  activeLine: number,
): number {
  if (count <= 0) {
    return -1;
  }
  // Tops increase monotonically in transcript order, so a binary search reads only O(log n) rectangles.
  // 顶部位置按转录顺序单调递增，因此二分查找只读取 O(log n) 个矩形。
  let low = 0;
  let high = count - 1;
  let found = 0;
  while (low <= high) {
    const middle = (low + high) >> 1;
    if (topAt(middle) <= activeLine) {
      found = middle;
      low = middle + 1;
    } else {
      high = middle - 1;
    }
  }
  return found;
}

// markdownToNavigatorPreview reduces Markdown syntax to a readable plain-text preview.
// markdownToNavigatorPreview 将 Markdown 语法简化为可读的纯文本预览。
// Parameters: markdown is the raw final reply source.
// 参数：markdown 是最终回复的原始 Markdown。
// Returns: plain text preserving line breaks, with list markers rendered as bullets.
// 返回值：保留换行的纯文本，列表标记渲染为圆点。
export function markdownToNavigatorPreview(markdown: string): string {
  return markdown
    .split(/\r?\n/)
    .filter((line) => !/^\s*(```|~~~)/.test(line))
    .map((line) => line
      .replace(/^\s*#{1,6}\s+/, "")
      .replace(/^\s*>\s?/, "")
      .replace(/^\s*[-*+]\s+(\[[ xX]\]\s+)?/, "• ")
      .replace(/!\[([^\]]*)\]\([^)]*\)/g, "$1")
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
      .replace(/(\*\*|__|`)/g, "")
      .trimEnd())
    .join("\n")
    .replace(/\n{2,}/g, "\n")
    .trim();
}

// collapseNavigatorWhitespace flattens one prompt into a single display line.
// collapseNavigatorWhitespace 将单个提示词压平为一行展示文本。
function collapseNavigatorWhitespace(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

// truncateNavigatorText bounds display text by code points and marks the cut with an ellipsis.
// truncateNavigatorText 按码点限制展示文本长度，并用省略号标记截断。
function truncateNavigatorText(text: string, maxChars: number): string {
  const codePoints = Array.from(text);
  if (codePoints.length <= maxChars) {
    return text;
  }
  return `${codePoints.slice(0, maxChars).join("").trimEnd()}…`;
}
