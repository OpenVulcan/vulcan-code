import type { GeneratedMediaAttachment } from "@/components/media/generated-media";
import type { ConversationMessage } from "@/types";

// ConversationBlockKind describes one WebUI-renderable conversation block family.
// ConversationBlockKind 描述 WebUI 可渲染的单个会话块族。
export type ConversationBlockKind = "markdown" | "tool-call" | "tool-result" | "command" | "diff" | "thinking" | "notice" | "turn-stats" | "plan-artifact";

// ConversationPlanArtifact describes one submitted Plan artifact embedded in a turn.
// ConversationPlanArtifact 描述嵌入单个回合中的已提交计划产物。
export interface ConversationPlanArtifact {
  // Human-readable plan name supplied to submit_plan.
  // 提交给 submit_plan 的人类可读计划名称。
  name: string;
  // Absolute plan file path returned by submit_plan.
  // submit_plan 返回的绝对计划文件路径。
  path: string;
  // Full Markdown content captured from submit_plan.
  // 从 submit_plan 捕获的完整 Markdown 内容。
  content: string;
  // Stable updated-at marker returned by submit_plan.
  // submit_plan 返回的稳定更新时间标记。
  updatedAt: string;
}

// ConversationRenderItemKind describes the projected transcript item family.
// ConversationRenderItemKind 描述投影后的转录展示项族。
export type ConversationRenderItemKind = "user" | "assistant" | "system";

// ConversationBlock describes one parsed message block shown inside the transcript.
// ConversationBlock 描述转录区中展示的单个解析后消息块。
export interface ConversationBlock {
  // Stable block identifier within the owning message.
  // 所属消息内的稳定块标识。
  id: string;
  // Semantic block kind used for component selection.
  // 用于选择组件的语义块类型。
  kind: ConversationBlockKind;
  // Short block title shown in the compact row or modal.
  // 展示在紧凑行或弹窗中的简短块标题。
  title: string;
  // Full block body used by markdown renderers and detail views.
  // 供 Markdown 渲染器与详情视图使用的完整块正文。
  body: string;
  // Compact summary shown before opening the detail view.
  // 打开详情视图前展示的紧凑摘要。
  summary: string;
  // Optional status text detected from legacy transcript formats.
  // 从旧转录格式中识别出的可选状态文本。
  status?: string;
  // Optional tool name detected from tool-call or tool-result formats.
  // 从工具调用或工具返回格式中识别出的可选工具名称。
  toolName?: string;
  // Whether this block carries explicit tool execution metadata or a legacy tool marker.
  // 该块是否携带明确工具执行元数据或旧版工具标记。
  toolEvidence?: boolean;
  // Optional provider or host-visible tool call identifier.
  // 可选 Provider 或宿主可见工具调用标识。
  toolCallId?: string;
  // Optional host-visible tool result identifier.
  // 可选宿主可见工具返回标识。
  toolResultId?: string;
  // Optional command-session identifier for command-like tool rows.
  // 命令类工具行使用的可选命令会话标识。
  commandSessionId?: string;
  // Optional child task-session identifier for routed subagent review.
  // 用于路由子代理复看的可选子任务会话标识。
  taskSessionId?: string;
  // Optional raw metadata used for generated media and future plugin-specific renderers.
  // 可选原始元数据，用于生成媒体与未来插件专属渲染器。
  rawMetadata?: unknown;
  // Generated media outputs resolved from host tool metadata.
  // 从宿主工具元数据解析出的生成媒体输出。
  mediaAttachments?: GeneratedMediaAttachment[];
  // Optional context-compaction section id used to merge status rows.
  // 用于合并状态行的可选上下文压缩分段标识。
  compactionSectionId?: string;
  // Optional context-compaction role inside a merged section.
  // 合并分段内的可选上下文压缩角色。
  compactionRole?: string;
  // Optional context-compaction state label.
  // 可选上下文压缩状态标签。
  compactionState?: string;
  // Optional context-compaction trigger label.
  // 可选上下文压缩触发来源标签。
  compactionTrigger?: string;
  // Optional context-compaction budget note.
  // 可选上下文压缩预算摘要。
  compactionBudgetNote?: string;
  // Optional visible message id that this compaction boundary should render before.
  // 可选压缩边界应渲染在其前方的可见消息标识。
  compactionInsertBeforeMessageId?: string;
  // Optional submitted Plan artifact rendered as a transcript card.
  // 可选已提交计划产物，用作转录卡片渲染。
  planArtifact?: ConversationPlanArtifact;
}

// RuntimeCue describes the compact running-state badge shown above the composer.
// RuntimeCue 描述输入区上方展示的紧凑运行状态徽标。
export interface RuntimeCue {
  // Stable cue family used to pick the icon and visual treatment.
  // 用于选择图标与视觉表现的稳定提示类型。
  kind: "default";
  // Localized text shown in the badge.
  // 徽标中展示的本地化文本。
  text: string;
}

// StatusbarRuntimeActivity describes one centered footer runtime activity badge.
// StatusbarRuntimeActivity 描述底部栏居中的单个运行活动徽标。
export interface StatusbarRuntimeActivity {
  // Stable activity family used to pick the footer icon.
  // 用于选择底部栏图标的稳定活动类型。
  kind: "tool" | "subagent";
  // Localized status text.
  // 本地化状态文本。
  text: string;
}

// ConversationRenderItem describes one row rendered after transcript projection.
// ConversationRenderItem 描述转录投影后的单条渲染行。
export interface ConversationRenderItem {
  // Stable projected row identifier.
  // 稳定投影行标识。
  id: string;
  // Stable render-instance key used when historical rows reuse the same persisted id.
  // 当历史行复用同一个持久化标识时使用的稳定渲染实例键。
  renderKey?: string;
  // Projected row role.
  // 投影行角色。
  role: ConversationRenderItemKind;
  // Blocks rendered inside this projected row.
  // 在该投影行中渲染的块集合。
  blocks: ConversationBlock[];
  // Original transcript message used for timing and fallback projection.
  // 用于耗时与兜底投影的原始转录消息。
  sourceMessage?: ConversationMessage;
  // Original user message when this row is a user bubble.
  // 当该行为用户气泡时对应的原始用户消息。
  userMessage?: ConversationMessage;
}

// ConversationProcessRenderItem describes one collapsible per-turn processing group.
// ConversationProcessRenderItem 描述单个可折叠的轮次处理过程分组。
export interface ConversationProcessRenderItem {
  // Stable process row identifier.
  // 稳定过程行标识。
  id: string;
  // Stable render-instance key used by virtual measurement and React reconciliation.
  // 虚拟测量与 React 协调使用的稳定渲染实例键。
  renderKey?: string;
  // Process rows that belong to the current turn before final assistant output.
  // 属于当前轮次最终助手输出前的过程行。
  items: ConversationRenderItem[];
  // User guidance rows shown under the process header but outside the folded process body.
  // 显示在过程标题下且不进入折叠正文的用户引导行。
  guidanceItems: ConversationRenderItem[];
  // Whether the process body may be manually collapsed.
  // 过程正文是否允许手动折叠。
  collapsible: boolean;
  // Whether the process should start collapsed.
  // 过程是否应默认折叠。
  defaultCollapsed: boolean;
  // Whether the turn is still running.
  // 当前轮次是否仍在运行。
  running: boolean;
  // Whether the user stopped this turn.
  // 当前轮次是否由用户停止。
  stopped: boolean;
  // Turn start timestamp in Unix milliseconds.
  // 轮次开始的 Unix 毫秒时间戳。
  startedAtMs: number | null;
  // Final fixed duration in milliseconds when available.
  // 可用时的固定最终耗时毫秒数。
  durationMs: number | null;
}

// ConversationTimelineItem describes either one normal row or one grouped process row.
// ConversationTimelineItem 描述单条普通行或单条分组过程行。
export type ConversationTimelineItem =
  | {
    // Timeline row rendered as one concrete transcript item.
    // 渲染为单条具体转录项的时间线行。
    kind: "item";
    // Projected transcript row.
    // 投影后的转录行。
    item: ConversationRenderItem;
    // Whether this item is the final assistant answer for one turn.
    // 该条目是否为单个回合的最终 Assistant 回答。
    finalAssistantOutput?: boolean;
  }
  | {
    // Timeline row rendered as metadata actions for one final assistant answer.
    // 渲染为单条最终 Assistant 回答元信息操作的时间线行。
    kind: "final-assistant-meta";
    // Final assistant row used for raw copy content and timestamp.
    // 用于原文复制内容与时间戳的最终 Assistant 行。
    item: ConversationRenderItem;
  }
  | {
    // Timeline row rendered as a display-only fork origin separator.
    // 渲染为纯展示分叉来源分隔行的时间线行。
    kind: "fork-source";
  }
  | { kind: "process"; process: ConversationProcessRenderItem };
