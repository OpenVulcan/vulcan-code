import type { ConversationMessage, ConversationSummary } from "@/types";
import type {
  ConversationRenderItem,
  ConversationTimelineItem,
} from "@/components/conversation/conversation-model";
import { isContextCompactionBlock } from "@/components/conversation/conversation-classifiers";
import { metadataValue } from "@/components/conversation/conversation-metadata";

// projectConversationTimelineItems groups each user turn into a visible processing shell.
// projectConversationTimelineItems 将每个用户轮次分组成可见处理过程外壳。
export function projectConversationTimelineItems(
  renderItems: ConversationRenderItem[],
  conversation: ConversationSummary | undefined,
  nowMs: number,
  conversationRunning: boolean,
): ConversationTimelineItem[] {
  const timeline: ConversationTimelineItem[] = [];
  const turnStartUserIds = conversationTurnStartUserIds(renderItems);
  const turnStartIdsWithLaterTurn = new Set(turnStartUserIds.slice(0, -1));
  let currentTurn: {
    // User item that starts the current turn.
    // 当前轮次的起始用户项。
    userItem: ConversationRenderItem | null;
    // Item used as the timestamp and identity anchor for system-started turns.
    // 用作系统发起回合时间戳与标识锚点的条目。
    anchorItem: ConversationRenderItem;
    // Assistant or system process rows collected before the final answer.
    // 最终回答之前收集到的助手或系统过程行。
    bodyItems: ConversationRenderItem[];
    // Guidance user prompts that belong to the already open turn.
    // 属于已打开轮次的用户引导提示。
    guidanceItems: ConversationRenderItem[];
    // Optional terminal turn statistics row.
    // 可选的终态轮次统计行。
    statsItem: ConversationRenderItem | null;
  } | null = null;

  const flushTurn = (forceRunning: boolean) => {
    if (!currentTurn) {
      return;
    }
    if (currentTurn.userItem) {
      timeline.push({ kind: "item", item: currentTurn.userItem });
    }
    const stats = currentTurn.statsItem?.blocks[0]?.rawMetadata;
    const statsStatus = turnStatsStatus(stats);
    const stopped = statsStatus === "stopped";
    const terminal = Boolean(currentTurn.statsItem);
    const running = forceRunning && !terminal;
    const planArtifactItems = latestPlanArtifactRenderItems(currentTurn.bodyItems);
    const bodyItems = currentTurn.bodyItems.filter((item) => !isPlanArtifactRenderItem(item));
    const finalItem = !running && !stopped ? popFinalAssistantItem(bodyItems) : null;
    const processItems = bodyItems.filter((item) => !isTurnStatsRenderItem(item));
    const turnUserItem = currentTurn.userItem;
    const shouldDefaultCollapse = Boolean(turnUserItem) && Boolean(finalItem) && !running && !stopped && turnStartIdsWithLaterTurn.has(turnUserItem?.id ?? "");
    const startedAtMs = conversationMessageUnixMillis(currentTurn.anchorItem.sourceMessage ?? currentTurn.anchorItem.userMessage);
    const terminalMessageMs = conversationMessageUnixMillis(finalItem?.sourceMessage ?? currentTurn.statsItem?.sourceMessage);
    const projectedDurationMs = startedAtMs && terminalMessageMs
      ? Math.max(0, terminalMessageMs - startedAtMs)
      : null;
    const runningDurationMs = running
      ? (startedAtMs ? Math.max(0, nowMs - startedAtMs) : null)
      : null;
    if (processItems.length > 0 || running || stopped || currentTurn.guidanceItems.length > 0) {
      timeline.push({
        kind: "process",
        process: {
          id: `process-${currentTurn.anchorItem.id}`,
          renderKey: `process:${currentTurn.anchorItem.renderKey ?? currentTurn.anchorItem.id}`,
          items: processItems,
          guidanceItems: currentTurn.guidanceItems,
          collapsible: Boolean(finalItem) && !running && !stopped,
          defaultCollapsed: shouldDefaultCollapse,
          running,
          stopped,
          startedAtMs,
          durationMs: turnStatsDurationMs(stats) ?? runningDurationMs ?? projectedDurationMs,
        },
      });
    }
    if (finalItem) {
      timeline.push({ kind: "item", item: finalItem, finalAssistantOutput: true });
    }
    if (currentTurn.statsItem) {
      timeline.push({ kind: "item", item: currentTurn.statsItem });
    }
    if (finalItem) {
      timeline.push({ kind: "final-assistant-meta", item: finalItem });
    }
    if (!running) {
      for (const item of planArtifactItems) {
        timeline.push({ kind: "item", item });
      }
    }
    currentTurn = null;
  };

  for (const item of renderItems) {
    if (item.role === "user") {
      if (!currentTurn || currentTurn.statsItem) {
        flushTurn(false);
        currentTurn = { userItem: item, anchorItem: item, bodyItems: [], guidanceItems: [], statsItem: null };
        continue;
      }
      if (isTurnStartUserRenderItem(item) || inferredTurnStartUserRenderItem(item, currentTurn.bodyItems)) {
        flushTurn(false);
        currentTurn = { userItem: item, anchorItem: item, bodyItems: [], guidanceItems: [], statsItem: null };
        continue;
      }
      if (isTurnProcessUserRenderItem(item)) {
        currentTurn.bodyItems.push(item);
        continue;
      }
      currentTurn.guidanceItems.push(item);
      continue;
    }
    if (!currentTurn) {
      if (timelineCanStartImplicitProcessTurn(timeline, item, conversationRunning)) {
        currentTurn = {
          userItem: null,
          anchorItem: item,
          bodyItems: [item],
          guidanceItems: [],
          statsItem: null,
        };
        continue;
      }
      timeline.push({ kind: "item", item });
      continue;
    }
    if (isTurnStatsRenderItem(item)) {
      currentTurn.statsItem = item;
      flushTurn(false);
      continue;
    }
    currentTurn.bodyItems.push(item);
  }

  flushTurn(conversationRunning || Boolean(conversation && conversation.status.trim().toLowerCase() === "running"));
  return insertForkSourceTimelineItem(timeline, conversation);
}

// timelineCanStartImplicitProcessTurn detects host-started turns after a prior terminal stats row.
// timelineCanStartImplicitProcessTurn 检测前一轮统计结束后由宿主发起的新回合。
function timelineCanStartImplicitProcessTurn(
  timeline: ConversationTimelineItem[],
  item: ConversationRenderItem,
  conversationRunning: boolean,
): boolean {
  if (item.role === "user" || isTurnStatsRenderItem(item) || isContextCompactionRenderItem(item)) {
    return false;
  }
  const previousItem = lastTimelineRenderItem(timeline);
  if (previousItem && isTurnStatsRenderItem(previousItem)) {
    return true;
  }
  return conversationRunning && timeline.length === 0;
}

// isContextCompactionRenderItem returns whether one projected row is only a context-compaction status block.
// isContextCompactionRenderItem 判断单条投影行是否只是上下文压缩状态块。
function isContextCompactionRenderItem(item: ConversationRenderItem): boolean {
  return item.blocks.some(isContextCompactionBlock);
}

// isTurnProcessUserRenderItem returns whether one user row belongs inside the process chronology.
// isTurnProcessUserRenderItem 返回单条用户行是否属于过程时间线内部。
function isTurnProcessUserRenderItem(item: ConversationRenderItem): boolean {
  if (item.role !== "user") {
    return false;
  }
  return metadataValue(item.userMessage?.metadata ?? item.sourceMessage?.metadata, ["turn_process"]) === true;
}

// isTurnStartUserRenderItem returns whether one user row starts a normal user turn.
// isTurnStartUserRenderItem 返回单条用户行是否开启常规用户轮次。
function isTurnStartUserRenderItem(item: ConversationRenderItem): boolean {
  if (item.role !== "user") {
    return false;
  }
  const metadata = item.userMessage?.metadata ?? item.sourceMessage?.metadata;
  return metadataValue(metadata, ["starts_user_turn"]) === true
    && metadataValue(metadata, ["turn_process"]) !== true;
}

// inferredTurnStartUserRenderItem preserves legacy and incremental rows before turn metadata arrives.
// inferredTurnStartUserRenderItem 在轮次 metadata 到达前保留旧记录与增量行的常规分组。
function inferredTurnStartUserRenderItem(
  item: ConversationRenderItem,
  currentTurnBodyItems: ConversationRenderItem[],
): boolean {
  if (item.role !== "user" || isTurnProcessUserRenderItem(item) || hasExplicitTurnStartMetadata(item)) {
    return false;
  }
  return currentTurnBodyItems.some(isAssistantMarkdownRenderItem);
}

// hasExplicitTurnStartMetadata returns whether a row already carries authoritative turn-start metadata.
// hasExplicitTurnStartMetadata 返回单条记录是否已经携带权威的轮次起点 metadata。
function hasExplicitTurnStartMetadata(item: ConversationRenderItem): boolean {
  const value = metadataValue(item.userMessage?.metadata ?? item.sourceMessage?.metadata, ["starts_user_turn"]);
  return value === true || value === false;
}

// isAssistantMarkdownRenderItem returns whether a row is the visible assistant answer shape.
// isAssistantMarkdownRenderItem 返回单条记录是否为可见助手回答形态。
function isAssistantMarkdownRenderItem(item: ConversationRenderItem): boolean {
  return item.role === "assistant" && item.blocks.length === 1 && item.blocks[0]?.kind === "markdown";
}

// lastTimelineRenderItem returns the latest concrete transcript item already emitted.
// lastTimelineRenderItem 返回已经输出的最后一条具体转录项。
function lastTimelineRenderItem(timeline: ConversationTimelineItem[]): ConversationRenderItem | null {
  for (let index = timeline.length - 1; index >= 0; index -= 1) {
    const entry = timeline[index];
    if (entry?.kind === "item") {
      return entry.item;
    }
  }
  return null;
}

// insertForkSourceTimelineItem inserts a display-only fork separator after its retained anchor row.
// insertForkSourceTimelineItem 在保留锚点行之后插入纯展示分叉来源分隔线。
function insertForkSourceTimelineItem(
  timeline: ConversationTimelineItem[],
  conversation: ConversationSummary | undefined,
): ConversationTimelineItem[] {
  const anchorMessageId = conversation?.forkSource?.anchorMessageId?.trim();
  if (!conversation?.forkSource) {
    return timeline;
  }
  const next = [...timeline];
  if (!anchorMessageId) {
    next.push({ kind: "fork-source" });
    return next;
  }
  let insertIndex = -1;
  for (let index = 0; index < next.length; index += 1) {
    const item = next[index];
    if (timelineItemMatchesMessageId(item, anchorMessageId)) {
      insertIndex = index;
    }
  }
  if (insertIndex < 0) {
    return timeline;
  }
  while (next[insertIndex + 1]?.kind === "final-assistant-meta") {
    insertIndex += 1;
  }
  next.splice(insertIndex + 1, 0, { kind: "fork-source" });
  return next;
}

// timelineItemMatchesMessageId returns whether one timeline item is anchored to one transcript message id.
// timelineItemMatchesMessageId 判断单个时间线条目是否锚定到指定转录消息标识。
function timelineItemMatchesMessageId(item: ConversationTimelineItem, messageId: string): boolean {
  if (item.kind === "item") {
    return renderItemMatchesMessageId(item.item, messageId);
  }
  if (item.kind === "final-assistant-meta") {
    return renderItemMatchesMessageId(item.item, messageId);
  }
  if (item.kind === "process") {
    return item.process.items.some((processItem) => renderItemMatchesMessageId(processItem, messageId))
      || item.process.guidanceItems.some((processItem) => renderItemMatchesMessageId(processItem, messageId));
  }
  return false;
}

// renderItemMatchesMessageId returns whether a projected row originated from one transcript message id.
// renderItemMatchesMessageId 判断投影行是否来源于指定转录消息标识。
function renderItemMatchesMessageId(item: ConversationRenderItem, messageId: string): boolean {
  return item.id === messageId
    || item.sourceMessage?.id === messageId
    || item.userMessage?.id === messageId;
}

// conversationMessageUnixMillis parses the optional Unix-millis timestamp carried by one message.
// conversationMessageUnixMillis 解析单条消息携带的可选 Unix 毫秒时间戳。
export function conversationMessageUnixMillis(message?: ConversationMessage): number | null {
  const raw = message?.createdAt?.trim();
  if (!raw) {
    return null;
  }
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

// formatShortConversationTimestamp renders the compact local timestamp used by transcript metadata.
// formatShortConversationTimestamp 渲染转录元信息使用的紧凑本地时间戳。
export function formatShortConversationTimestamp(message?: ConversationMessage): string | null {
  const timestamp = conversationMessageUnixMillis(message);
  if (!timestamp) {
    return null;
  }
  const date = new Date(timestamp);
  const pad = (value: number) => value.toString().padStart(2, "0");
  return `${date.getMonth() + 1}/${date.getDate()} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

// formatWorkProcessDuration formats elapsed duration for the process header.
// formatWorkProcessDuration 格式化过程标题使用的处理耗时。
export function formatWorkProcessDuration(durationMs: number | null): string {
  if (durationMs === null || !Number.isFinite(durationMs)) {
    return "";
  }
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}小时${minutes}分钟${seconds}秒`;
  }
  if (minutes > 0) {
    return `${minutes}分钟${seconds}秒`;
  }
  return `${seconds}秒`;
}

// latestThinkingBlockIdAfterLastUser returns the newest thinking block that belongs to the active user turn.
// latestThinkingBlockIdAfterLastUser 返回属于当前用户 turn 的最新思考块。
export function latestThinkingBlockIdAfterLastUser(items: ConversationRenderItem[]): string | null {
  let latestThinkingBlockId: string | null = null;
  for (const item of items) {
    if (item.role === "user") {
      latestThinkingBlockId = null;
      continue;
    }
    for (const block of item.blocks) {
      if (block.kind === "thinking") {
        latestThinkingBlockId = block.id;
      }
    }
  }
  return latestThinkingBlockId;
}

// conversationTurnStartUserIds derives true user-turn starts, excluding guidance prompts inside a running turn.
// conversationTurnStartUserIds 推导真实用户轮次起点，并排除运行中追加的引导输入。
function conversationTurnStartUserIds(renderItems: ConversationRenderItem[]): string[] {
  const turnStartIds: string[] = [];
  let hasOpenTurn = false;
  let currentTurnHasStats = false;
  let currentTurnBodyItems: ConversationRenderItem[] = [];
  for (const item of renderItems) {
    if (item.role === "user") {
      if (isTurnProcessUserRenderItem(item)) {
        continue;
      }
      if (
        !hasOpenTurn
        || currentTurnHasStats
        || isTurnStartUserRenderItem(item)
        || inferredTurnStartUserRenderItem(item, currentTurnBodyItems)
      ) {
        turnStartIds.push(item.id);
        hasOpenTurn = true;
        currentTurnHasStats = false;
        currentTurnBodyItems = [];
      }
      continue;
    }
    if (!hasOpenTurn) {
      continue;
    }
    if (isTurnStatsRenderItem(item)) {
      currentTurnHasStats = true;
      currentTurnBodyItems = [];
      continue;
    }
    currentTurnBodyItems.push(item);
  }
  return turnStartIds;
}

// isTurnStatsRenderItem returns whether one projected row is the terminal turn-statistics row.
// isTurnStatsRenderItem 返回单条投影行是否为终态轮次统计行。
function isTurnStatsRenderItem(item: ConversationRenderItem): boolean {
  return item.blocks.some((block) => block.kind === "turn-stats");
}

// isPlanArtifactRenderItem returns whether one projected row is a submitted Plan artifact card.
// isPlanArtifactRenderItem 返回单条投影行是否为已提交计划产物卡。
function isPlanArtifactRenderItem(item: ConversationRenderItem): boolean {
  return item.blocks.some((block) => block.kind === "plan-artifact");
}

// latestPlanArtifactRenderItems keeps only the final submitted Plan card from one turn.
// latestPlanArtifactRenderItems 仅保留单个回合中最后提交的计划卡。
function latestPlanArtifactRenderItems(items: ConversationRenderItem[]): ConversationRenderItem[] {
  const planArtifactItems = items.filter(isPlanArtifactRenderItem);
  const latest = planArtifactItems.at(-1);
  return latest ? [latest] : [];
}

// popFinalAssistantItem removes and returns the final assistant markdown row when present.
// popFinalAssistantItem 在存在最终助手 Markdown 行时移除并返回它。
function popFinalAssistantItem(items: ConversationRenderItem[]): ConversationRenderItem | null {
  for (let index = items.length - 1; index >= 0; index -= 1) {
    const item = items[index];
    if (isAssistantMarkdownRenderItem(item)) {
      items.splice(index, 1);
      return item;
    }
  }
  return null;
}

// turnStatsDurationMs reads the total turn duration from structured turn statistics.
// turnStatsDurationMs 从结构化轮次统计中读取总轮次耗时。
function turnStatsDurationMs(metadata: unknown): number | null {
  const stats = turnStatsObject(metadata);
  return numericMetadata(stats?.duration_ms ?? stats?.durationMs);
}

// turnStatsStatus reads the compact turn-stats terminal status.
// turnStatsStatus 读取紧凑轮次统计终态。
function turnStatsStatus(metadata: unknown): string | null {
  const stats = turnStatsObject(metadata);
  const status = stats?.status;
  return typeof status === "string" && status.trim() ? status.trim() : null;
}

// turnStatsObject reads the structured turn-stats payload from transcript metadata.
// turnStatsObject 从转录元数据中读取结构化轮次统计载荷。
function turnStatsObject(metadata: unknown): Record<string, unknown> | null {
  if (!metadata || typeof metadata !== "object") {
    return null;
  }
  const value = (metadata as Record<string, unknown>).turn_stats ?? (metadata as Record<string, unknown>).turnStats;
  return value && typeof value === "object" ? value as Record<string, unknown> : null;
}

// numericMetadata reads a number from unknown JSON values.
// numericMetadata 从未知 JSON 值中读取数字。
function numericMetadata(value: unknown): number | null {
  if (typeof value === "number") {
    return value;
  }
  if (typeof value === "string" && value.trim()) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}
