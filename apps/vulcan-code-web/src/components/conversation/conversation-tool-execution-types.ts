import type { GeneratedMediaAttachment } from "@/components/media/generated-media";

// ToolExecutionTraceCallback records local UI interactions without binding this module to the app logger.
// ToolExecutionTraceCallback 记录本地 UI 交互，避免本模块直接绑定应用埋点实现。
export type ToolExecutionTraceCallback = (action: string, detail?: Record<string, unknown>) => void;

// ToolExecutionConversationBlockKind names the block families needed by the compact tool row.
// ToolExecutionConversationBlockKind 命名紧凑工具行所需的块类型。
export type ToolExecutionConversationBlockKind = "tool-call" | "tool-result" | "command" | "diff" | "thinking" | "notice" | "turn-stats" | "markdown";

// ToolExecutionConversationBlock describes the minimal parsed block shape consumed by this module.
// ToolExecutionConversationBlock 描述本模块消费的最小解析块结构。
export interface ToolExecutionConversationBlock {
  // Stable block id from the transcript projection.
  // 来自转录投影的稳定块标识。
  id: string;
  // Semantic block family.
  // 语义块类型。
  kind: ToolExecutionConversationBlockKind;
  // User-visible block title.
  // 面向用户显示的块标题。
  title: string;
  // Full block body used by the detail modal.
  // 详情弹窗使用的完整块正文。
  body: string;
  // Compact block summary used in row fallbacks.
  // 行兜底展示使用的紧凑块摘要。
  summary: string;
  // Optional display status from host or provider metadata.
  // 来自宿主或 Provider 元数据的可选展示状态。
  status?: string;
  // Optional tool name resolved during transcript parsing.
  // 转录解析期间解析出的可选工具名。
  toolName?: string;
  // Optional tool-call id for hidden detail lookup.
  // 用于隐藏详情查询的可选工具调用标识。
  toolCallId?: string;
  // Optional tool-result id for status inference.
  // 用于状态推断的可选工具返回标识。
  toolResultId?: string;
  // Optional command-session id for display fallback.
  // 用于展示兜底的可选命令会话标识。
  commandSessionId?: string;
  // Optional stable subagent task-session id for display metadata.
  // 用于展示元数据的可选稳定子代理任务会话标识。
  taskSessionId?: string;
  // Optional visible child-agent name resolved from subagent tool metadata.
  // 从子代理工具元数据解析出的可选可见子代理名称。
  subagentName?: string;
  // Raw metadata retained from the service transcript record.
  // 从服务转录记录保留的原始元数据。
  rawMetadata?: unknown;
  // Generated media extracted while parsing visible records.
  // 解析可见记录时提取出的生成媒体。
  mediaAttachments?: GeneratedMediaAttachment[];
}
