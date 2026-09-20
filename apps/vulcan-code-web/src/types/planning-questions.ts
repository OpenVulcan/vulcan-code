export interface ComposerSlashCommandSummary {
  // Stable command identifier used by WebUI routing.
  // WebUI 路由使用的稳定指令标识。
  id: string;
  // Canonical English slash trigger shown in the composer.
  // 输入区展示的规范英文 slash 触发词。
  command: string;
  // Chinese command description.
  // 中文指令说明。
  descriptionZh: string;
  // English command description.
  // 英文指令说明。
  descriptionEn: string;
}

// SubagentNavigationSnapshot describes the backend-routed subagent surface state.
// SubagentNavigationSnapshot 描述由后端路由的子代理界面状态。
export interface SubagentNavigationSnapshot {
  // Parent conversation that owns the subagent task cards.
  // 拥有子代理任务卡片的父会话。
  parentConversationId: string;
  // Conversation id currently rendered by the transcript surface.
  // 当前由转录界面渲染的会话标识。
  activeConversationId: string;
  // Active child task id when a subagent surface is open.
  // 子代理界面打开时的活动子任务标识。
  activeTaskId?: string | null;
  // Ordered subagent tasks projected by the backend.
  // 由后端投影的有序子代理任务。
  tasks?: SubagentTaskSummary[];
}

// SubagentTaskSummary describes one subagent navigation row.
// SubagentTaskSummary 描述单条子代理导航行。
export interface SubagentTaskSummary {
  // Pending messages from the authoritative child inbox, ordered by delivery.
  // 来自权威子代理收件箱的待处理消息，按投递顺序排列。
  pendingMessages?: { deliveryId: string; message: string; steering: boolean }[];
  // Stable child task conversation id.
  // 稳定子任务会话标识。
  taskId: string;
  // Caller-facing task id retained in task-session metadata.
  // 保留在 task-session 元数据中的调用方面向任务标识。
  publicTaskId: string;
  // Human-facing label built by the shared backend projection.
  // 由共享后端投影构建的人类可读标签。
  label: string;
  // Optional task description.
  // 可选任务描述。
  description?: string | null;
  // Optional catalog agent identifier used by agent-mode child agents.
  // 智能体模式子代理使用的可选目录智能体标识。
  agentProfileId?: string | null;
  // Canonical run state key.
  // 规范运行状态键。
  runState: string;
  // Whether the shared task runtime reports this task as running.
  // 共享任务运行时是否报告该任务正在运行。
  isRunning: boolean;
  // Whether this task is currently opened.
  // 该任务当前是否已打开。
  isActive: boolean;
}

// PendingQuestionSnapshot describes one ask-tool request waiting for user input.
// PendingQuestionSnapshot 描述一个正在等待用户输入的 ask 工具请求。
export interface PendingQuestionSnapshot {
  // Stable request id used by reply and reject commands.
  // 回复和拒绝命令使用的稳定请求标识。
  id: string;
  // Controller session that owns the blocked ask tool call.
  // 拥有阻塞 ask 工具调用的控制器会话。
  sessionId: string;
  // Runtime sequence used to order concurrent ask requests.
  // 用于排序并发 ask 请求的运行时序号。
  sequence: number;
  // Ordered prompts shown in the ask modal.
  // ask 弹窗中按顺序展示的问题提示。
  questions: PendingQuestionPromptSummary[];
}

// PendingQuestionPromptSummary describes one prompt inside a pending ask request.
// PendingQuestionPromptSummary 描述待处理 ask 请求中的单条问题提示。
export interface PendingQuestionPromptSummary {
  // Full question text shown to the user.
  // 展示给用户的完整问题文本。
  question: string;
  // Short field header shown above the question.
  // 展示在问题上方的简短字段标题。
  header: string;
  // Selectable choices offered by this prompt.
  // 该问题提供的可选答案。
  options: PendingQuestionOptionSummary[];
  // Whether multiple structured choices may be selected together.
  // 是否允许同时选择多个结构化选项。
  multiple: boolean;
}

// PendingQuestionOptionSummary describes one selectable ask choice.
// PendingQuestionOptionSummary 描述 ask 问题中的单个可选答案。
export interface PendingQuestionOptionSummary {
  // Choice label returned to the model when selected.
  // 被选中时回传给模型的选项标签。
  label: string;
  // Helper text explaining the choice.
  // 解释该选项的辅助说明。
  description: string;
}

// SessionTodolistSummary describes the current model-authored session todolist.
// SessionTodolistSummary 描述当前模型编写的会话 todolist。
export interface SessionTodolistSummary {
  // Optional explanation attached to the latest plan update.
  // 最近一次计划更新附带的可选说明。
  explanation?: string | null;
  // Ordered todolist items shown in the session tool panel.
  // 会话工具面板中展示的有序 todolist 条目。
  items: SessionTodolistItemSummary[];
  // Completed item count.
  // 已完成条目数量。
  completedCount: number;
  // Total item count.
  // 条目总数量。
  totalCount: number;
  // Unix-millis update timestamp text.
  // Unix 毫秒格式的更新时间文本。
  updatedAt?: string | null;
}

// SessionTodolistItemSummary describes one todolist item for WebUI rendering.
// SessionTodolistItemSummary 描述供 WebUI 渲染的单个 todolist 条目。
export interface SessionTodolistItemSummary {
  // Human-readable step text supplied by the model.
  // 模型提供的人类可读步骤文本。
  step: string;
  // Stable status key: pending, in_progress, or completed.
  // 稳定状态键：pending、in_progress 或 completed。
  status: "pending" | "in_progress" | "completed";
}

// SessionPlanModeSummary describes the current session Plan mode state.
// SessionPlanModeSummary 描述当前会话的计划模式状态。
export interface SessionPlanModeSummary {
  // Stable execution mode key such as build or plan.
  // 稳定执行模式键，例如 build 或 plan。
  executionMode: string;
  // Whether Plan mode steering has already been injected for this activation.
  // 当前计划模式激活期间是否已经注入过计划模式引导。
  promptInjected: boolean;
  // Latest generated plan artifact, if one exists.
  // 最近生成的计划产物，如果存在。
  artifact?: SessionPlanArtifactSummary | null;
}

// SessionPlanArtifactSummary describes one generated Plan artifact.
// SessionPlanArtifactSummary 描述单个已生成的计划产物。
export interface SessionPlanArtifactSummary {
  // Human-readable plan name supplied by the model.
  // 模型提供的人类可读计划名称。
  name: string;
  // Absolute filesystem path of the generated Markdown plan file.
  // 生成的 Markdown 计划文件绝对路径。
  path: string;
  // Full Markdown plan content captured at submit time.
  // 提交时捕获的完整 Markdown 计划内容。
  content?: string | null;
  // Unix-millis update timestamp text.
  // Unix 毫秒格式的更新时间文本。
  updatedAt: string;
}

// SessionGoalStatus is the complete persisted lifecycle state exposed by the backend.
// SessionGoalStatus 是后端暴露的完整持久化生命周期状态。
export type SessionGoalStatus =
  | "active"
  | "paused"
  | "blocked"
  | "usage_limited"
  | "budget_limited"
  | "complete";

// SessionGoalUserStatusUpdate contains only lifecycle targets authorized for direct user toggles.
// SessionGoalUserStatusUpdate 仅包含允许用户直接切换的生命周期目标。
export type SessionGoalUserStatusUpdate = Extract<SessionGoalStatus, "active" | "paused">;

// SessionGoalSummary describes the current persisted session goal.
// SessionGoalSummary 描述当前持久化的会话目标。
export interface SessionGoalSummary {
  /** Whether the baseline difference proves the budget usage.
   * 基线差值能否证明预算用量。 */
  usageKnown?: boolean;
  // Session identifier that owns the goal.
  // 拥有该目标的会话标识。
  sessionId: string;
  // Stable goal version identifier.
  // 稳定目标版本标识。
  goalId: string;
  // Human-authored objective text.
  // 用户编写的目标文本。
  objective: string;
  // Stable status key copied from the backend goal state.
  // 从后端目标状态复制的稳定状态键。
  status: SessionGoalStatus;
  // Optional explicit token budget.
  // 可选显式 token 预算。
  tokenBudget?: number | null;
  // Accounted token usage for this goal.
  // 当前目标已记账的 token 用量。
  tokensUsed: number;
  // Live usage total shown in the composer strip.
  // 输入区目标条展示的实时用量总数。
  usageTokens: number;
  // Provider API call count observed for the current goal turn.
  // 当前目标轮次观测到的 Provider API 调用次数。
  apiCallCount: number;
  // Tool call count observed for the current goal turn.
  // 当前目标轮次观测到的工具调用次数。
  toolCallCount: number;
  // Cumulative uncached input tokens observed for the current goal.
  // 当前目标累计观测到的新增输入 Token 数。
  inputTokens: number;
  // Cumulative cache-hit input tokens observed for the current goal.
  // 当前目标累计观测到的缓存命中输入 Token 数。
  cachedInputTokens: number;
  // Cumulative output tokens observed for the current goal.
  // 当前目标累计观测到的输出 Token 数。
  outputTokens: number;
  // Cumulative reasoning tokens observed inside the current goal output.
  // 当前目标输出中累计观测到的思考 Token 数。
  reasoningTokens: number;
  // Cumulative provider tokens observed for the current goal.
  // 当前目标累计观测到的 Provider 总 Token 数。
  totalTokens: number;
  // Accounted wall-clock time in seconds.
  // 当前目标已记账的墙钟时间秒数。
  timeUsedSeconds: number;
  // Exact event-derived clock; absent in local drafts and older snapshots, which must not tick.
  // 精确事件派生时钟；本地草稿及旧快照缺失时不得自行走时。
  workTime?: {
    // Active elapsed milliseconds through the durable observation.
    // 截至持久化观察点的活动累计毫秒数。
    elapsedMs: number;
    // Server observation timestamp, independent of client receipt latency.
    // 服务端观察时间戳，独立于客户端收包延迟。
    observedAtMs: number;
  } | null;
  // Creation timestamp in Unix epoch milliseconds.
  // 创建时间的 Unix 纪元毫秒值。
  createdAtMs: number;
  // Last update timestamp in Unix epoch milliseconds.
  // 最近更新时间的 Unix 纪元毫秒值。
  updatedAtMs: number;
}
