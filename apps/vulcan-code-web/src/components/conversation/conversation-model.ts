import type { GeneratedMediaAttachment } from "@/components/media/generated-media";

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

// ConversationBlock describes one parsed message block shown inside the transcript.
// ConversationBlock 描述转录区中展示的单个解析后消息块。
export interface ConversationBlock {
  // Stable block identifier within the owning message.
  // 所属消息内的稳定块标识。
  id: string;
  // Authoritative backend message identity that produced this parsed block.
  // 生成当前解析块的权威后端消息身份。
  sourceMessageId?: string;
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
  // Optional command-session identifier for command-like tool rows.
  // 命令类工具行使用的可选命令会话标识。
  commandSessionId?: string;
  // Optional stable subagent task identifier for display metadata.
  // 用于展示元数据的可选稳定子代理任务标识。
  subagentTaskId?: string;
  // Optional visible child-agent name resolved from subagent tool metadata.
  // 从子代理工具元数据解析出的可选可见子代理名称。
  subagentName?: string;
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
