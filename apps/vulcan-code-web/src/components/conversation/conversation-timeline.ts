import type { ConversationMessage, ConversationSummary } from "@/types";
import type {
  ConversationRenderItem,
  ConversationTimelineItem,
} from "@/components/conversation/conversation-model";
import { isContextCompactionBlock, isMergeableToolBlock } from "@/components/conversation/conversation-classifiers";
import { metadataString, metadataValue } from "@/components/conversation/conversation-metadata";
import { isSubagentFeedbackMetadata } from "@/components/conversation/conversation-tool-metadata";
import { formatMessage, readLocaleMessage, type LocaleCode } from "@/i18n";
import { integrateProcessThinking } from "@/components/conversation/thinking-display";

// ProjectedTurnState stores transient grouping data while projecting transcript rows.
// ProjectedTurnState 存储投影转录行时使用的临时分组数据。
interface ProjectedTurnState {
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
}

// projectConversationTimelineItems groups each user turn into a visible processing shell.
// projectConversationTimelineItems 将每个用户轮次分组成可见处理过程外壳。
export function projectConversationTimelineItems(
  renderItems: ConversationRenderItem[],
  conversation: ConversationSummary | undefined,
  conversationRunning: boolean,
): ConversationTimelineItem[] {
  const timeline: ConversationTimelineItem[] = [];
  const turnStartUserIds = conversationTurnStartUserIds(renderItems);
  const turnStartIdsWithLaterTurn = new Set(turnStartUserIds.slice(0, -1));
  let currentTurn: ProjectedTurnState | null = null;

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
    // Associate reasoning before extracting final output so the final answer keeps its own gutter entry.
    // 在提取最终输出之前关联思考，让最终回答保留属于自己的左侧入口。
    const processItems = integrateProcessThinking(
      currentTurn.bodyItems.filter((item) => !isPlanArtifactRenderItem(item)
        && (!isTurnStatsRenderItem(item) || isSubagentTurnStatsRenderItem(item))),
      isAssistantMarkdownRenderItem,
    );
    // Trailing host interactions must not steal the parent final answer or move before it.
    // 末尾宿主交互不能抢占父助手最终回复，也不能移动到它之前。
    const trailingInteractions: ConversationRenderItem[] = [];
    if (!running && !stopped) {
      while (processItems.at(-1)?.blocks.some((block) => block.kind === "agent-interaction")) {
        trailingInteractions.unshift(processItems.pop()!);
      }
    }
    const finalItem = !running && !stopped ? popTrailingFinalAssistantItem(processItems) : null;
    if (!finalItem) {
      processItems.push(...trailingInteractions.splice(0));
    }
    const turnUserItem = currentTurn.userItem;
    const shouldDefaultCollapse = Boolean(turnUserItem) && Boolean(finalItem) && !running && !stopped && turnStartIdsWithLaterTurn.has(turnUserItem?.id ?? "");
    const startedAtMs = conversationMessageUnixMillis(currentTurn.anchorItem.sourceMessage ?? currentTurn.anchorItem.userMessage);
    const statsDurationMs = turnStatsDurationMs(stats);
    const terminalMessageMs = conversationMessageUnixMillis(finalItem?.sourceMessage ?? currentTurn.statsItem?.sourceMessage);
    const projectedDurationMs = startedAtMs && terminalMessageMs
      ? Math.max(0, terminalMessageMs - startedAtMs)
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
          durationMs: statsDurationMs ?? (terminal ? null : projectedDurationMs),
        },
      });
    }
    if (finalItem) {
      timeline.push({ kind: "item", item: finalItem, finalAssistantOutput: true });
    }
    for (const item of trailingInteractions) {
      timeline.push({ kind: "item", item });
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
    // Canonical turn identity also covers agent-started work without a visible human prompt.
    // 规范回合身份同样覆盖没有可见人类提示词的代理发起工作。
    const itemTurnId = canonicalRenderTurnId(item);
    const currentTurnId = currentTurn ? canonicalRenderTurnId(currentTurn.anchorItem) : null;
    if (currentTurn && itemTurnId !== null && currentTurnId !== null && itemTurnId !== currentTurnId) {
      flushTurn(false);
    }
    if (item.role === "user") {
      if (!currentTurn || currentTurn.statsItem) {
        flushTurn(false);
        currentTurn = newProjectedTurn(item, item);
        continue;
      }
      if (isTurnStartUserRenderItem(item) || inferredTurnStartUserRenderItem(item, currentTurn.bodyItems)) {
        flushTurn(false);
        currentTurn = newProjectedTurn(item, item);
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
      // A claimed task starts the turn above its work divider; in-turn steering stays inside the process.
      // 已认领任务在工作分隔线上方开启回合；回合内插队仍保留在过程内部。
      // An inbox task message is a root input, so it always opens its own turn.
      // inbox 任务消息是根输入，因此总是开启自己的回合。
      const interaction = item.sourceMessage?.agentInteraction?.event;
      if (itemTurnId !== null && (interaction?.kind === "collaboration"
        && interaction.after_state.kind === "inbound_task_message"
        && interaction.after_state.acceptance.message.kind === "task_start"
        || interaction?.kind === "user_input" && interaction.input_source === "root")) {
        timeline.push({ kind: "item", item });
        currentTurn = newProjectedTurn(null, item);
        continue;
      }
      if ((itemTurnId !== null
        && (item.blocks.some((block) => block.kind === "thinking" || block.kind === "markdown" || isMergeableToolBlock(block))
          || item.sourceMessage?.agentInteraction?.event.kind === "collaboration"
            && item.sourceMessage.agentInteraction.event.after_state.kind === "inbound_task_message"))
        || timelineCanStartImplicitProcessTurn(timeline, item, conversationRunning)) {
        currentTurn = newProjectedTurn(null, item, [item]);
        continue;
      }
      timeline.push({ kind: "item", item });
      continue;
    }
    if (isSubagentTurnStatsRenderItem(item)) {
      currentTurn.bodyItems.push(item);
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
  return timeline;
}

/** Reads the adapter's authoritative V1 turn identity, or null for unbound host facts.
 * 读取适配器的权威 V1 回合身份；未绑定宿主事实返回空值。
 * The input is one display row; no user message or neighboring timestamp is inferred.
 * 输入为单条展示行；不推测用户消息或相邻时间戳。 */
function canonicalRenderTurnId(item: ConversationRenderItem): number | null {
  const value = metadataValue(item.sourceMessage?.metadata ?? item.userMessage?.metadata, ["v1_turn_id"]);
  return typeof value === "number" && Number.isSafeInteger(value) ? value : null;
}

// conversationTimelineItemKey returns the stable business key for one projected timeline entry.
// conversationTimelineItemKey 返回单条投影时间线记录的稳定业务键。
// Parameters: item is the projected timeline entry whose identity must survive content-only updates.
// 参数：item 是身份必须跨纯内容更新保持稳定的投影时间线记录。
// Returns: a deterministic key scoped to the entry kind and authoritative render identity.
// 返回值：由记录类型与权威渲染身份共同限定的确定性键。
export function conversationTimelineItemKey(item: ConversationTimelineItem): string {
  if (item.kind === "fork-source") {
    return "fork-source";
  }
  if (item.kind === "item") {
    return item.item.renderKey ?? `item:${item.item.id}`;
  }
  if (item.kind === "final-assistant-meta") {
    return `final-assistant-meta:${item.item.renderKey ?? item.item.id}`;
  }
  return item.process.renderKey ?? `process:${item.process.id}`;
}

// newProjectedTurn creates the mutable grouping state for one projected turn.
// newProjectedTurn 为单个投影回合创建可变分组状态。
function newProjectedTurn(
  // Optional user item that started the turn.
  // 开启该回合的可选用户条目。
  userItem: ConversationRenderItem | null,
  // Anchor item used for timing and stable ids.
  // 用于时间与稳定标识的锚点条目。
  anchorItem: ConversationRenderItem,
  // Initial process body items.
  // 初始过程正文条目。
  bodyItems: ConversationRenderItem[] = [],
): ProjectedTurnState {
  return {
    userItem,
    anchorItem,
    bodyItems,
    guidanceItems: [],
    statsItem: null,
  };
}

// timelineCanStartImplicitProcessTurn detects host-started turns after a prior terminal stats row.
// timelineCanStartImplicitProcessTurn 检测前一轮统计结束后由宿主发起的新回合。
function timelineCanStartImplicitProcessTurn(
  timeline: ConversationTimelineItem[],
  item: ConversationRenderItem,
  conversationRunning: boolean,
): boolean {
  if (
    item.role === "user"
    || isTurnStatsRenderItem(item)
    || isContextCompactionRenderItem(item)
    || isSubagentFeedbackDisplayRenderItem(item)
    // Configuration notices describe user settings, never the start of agent work.
    // 配置提示描述用户设置，绝不能作为代理工作的起点。
    || item.blocks.some((block) => block.kind === "notice"
      && ["model_switch", "reasoning_effort_switch"].includes(metadataString(block.rawMetadata, ["notice_kind"]) ?? ""))
    || item.blocks.some((block) => block.kind === "agent-interaction")
  ) {
    return false;
  }
  const previousItem = lastTimelineRenderItem(timeline);
  if (previousItem && isTurnStatsRenderItem(previousItem) && !isSubagentTurnStatsRenderItem(previousItem)) {
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
// Parameter item is one projected row; returns true for the canonical assistant content shape.
// 参数 item 为投影记录；规范助手正文形态返回 true。
export function isAssistantMarkdownRenderItem(item: ConversationRenderItem): boolean {
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
export function formatWorkProcessDuration(durationMs: number | null, locale: LocaleCode = "en-US"): string {
  if (durationMs === null || !Number.isFinite(durationMs)) {
    return "";
  }
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return formatMessage(readLocaleMessage(locale, "conversation.duration.hms"), { hours, minutes, seconds });
  }
  if (minutes > 0) {
    return formatMessage(readLocaleMessage(locale, "conversation.duration.ms"), { minutes, seconds });
  }
  return formatMessage(readLocaleMessage(locale, "conversation.duration.s"), { seconds });
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
    if (isTurnStatsRenderItem(item) && !isSubagentTurnStatsRenderItem(item)) {
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

// isSubagentTurnStatsRenderItem returns whether a stats row belongs to child-agent feedback.
// isSubagentTurnStatsRenderItem 返回统计行是否属于智能体反馈。
function isSubagentTurnStatsRenderItem(item: ConversationRenderItem): boolean {
  return item.blocks.some((block) => block.kind === "turn-stats" && turnStatsKind(block.rawMetadata) === "subagent_feedback");
}

// isSubagentFeedbackDisplayRenderItem returns whether a row is a display-only child-agent feedback card.
// isSubagentFeedbackDisplayRenderItem 返回条目是否为仅显示的智能体反馈卡。
function isSubagentFeedbackDisplayRenderItem(item: ConversationRenderItem): boolean {
  return item.blocks.some((block) => {
    return isSubagentFeedbackMetadata(block.rawMetadata);
  });
}

// turnStatsKind returns the serialized turn-stats kind when present.
// turnStatsKind 返回存在时的序列化回合统计类型。
function turnStatsKind(metadata: unknown): string | null {
  const stats = turnStatsObject(metadata);
  const value = stats?.turn_kind ?? stats?.turnKind;
  return typeof value === "string" && value.trim() ? value.trim().toLowerCase() : null;
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
// popTrailingFinalAssistantItem extracts an assistant Markdown row only when it is chronologically last.
// popTrailingFinalAssistantItem 仅在 Assistant Markdown 行按时间顺序确实位于末尾时提取它。
// Parameters: items is the mutable turn body after plan artifacts have been removed.
// 参数：items 是移除计划产物后的可变回合正文集合。
// Returns: the trailing final assistant row, or null when later process or failure events exist.
// 返回值：末尾的最终 Assistant 行；若其后存在处理或失败事件则返回 null。
function popTrailingFinalAssistantItem(items: ConversationRenderItem[]): ConversationRenderItem | null {
  const item = items.at(-1);
  if (!item || !isAssistantMarkdownRenderItem(item)) {
    return null;
  }
  items.pop();
  return item;
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
  const record = metadata as Record<string, unknown>;
  const value = record.turn_stats ?? record.turnStats;
  if (value && typeof value === "object") {
    return value as Record<string, unknown>;
  }
  return directTurnStatsObject(record);
}

// directTurnStatsObject accepts block-local metadata that is already the statistics object.
// directTurnStatsObject 接受已经是统计对象本体的块级元数据。
function directTurnStatsObject(metadata: Record<string, unknown>): Record<string, unknown> | null {
  const status = metadata.status;
  const turnKind = metadata.turn_kind ?? metadata.turnKind;
  const hasUsageField = "provider_call_count" in metadata
    || "providerCallCount" in metadata
    || "total_tokens" in metadata
    || "totalTokens" in metadata
    || "duration_ms" in metadata
    || "durationMs" in metadata;
  return (typeof status === "string" || typeof turnKind === "string") && hasUsageField
    ? metadata
    : null;
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
