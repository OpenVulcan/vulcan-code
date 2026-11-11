import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type {
  ComposerSlashCommandResult,
  ComposerSubmission,
} from "@/components/composer/composer-model";
import type { ConversationSessionReviewSummary } from "@/components/conversation/session-todolist-panel";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import type {
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ContextWindowSelection,
  PendingPromptSummary,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  SessionGoalUserStatusUpdate,
  StartupSnapshot,
  WorkbenchSnapshot,
} from "@/types";

// ChatAreaProps contains the state and service edges needed by the conversation surface.
// ChatAreaProps 包含会话展示面所需的状态与服务边界。
export interface ChatAreaProps {
  // Current startup state used for parity with the shell contract.
  // 当前启动状态，用于保持与外壳契约一致。
  startup: StartupSnapshot;
  // Current workbench snapshot that owns conversations, messages, providers, and composer data.
  // 当前工作台快照，包含会话、消息、供应商与输入区数据。
  workbench: WorkbenchSnapshot;
  // Draft conversation selected before a persisted session exists.
  // 持久会话创建前选中的草稿会话。
  selectedDraftConversation: DraftConversationSelection | null;
  // Selected parent conversation id.
  // 当前选中的父会话标识。
  selectedConversationId: string | null;
  // Retires the exact optimistic first prompt after canonical V1 correlation.
  // 在规范 V1 精确关联后移除对应乐观首条提示词。
  onFirstSubmissionReconciled?: (conversationId: string, clientSubmissionId: string) => void;
  // Current Git review summary shown beside compact session progress.
  // 当前用于紧凑会话进程旁展示的 Git 审查摘要。
  sessionReviewSummary: ConversationSessionReviewSummary | null;
  // Opens the review panel on the project's uncommitted changes.
  // 在审查面板中打开项目的未提交变更。
  onOpenReviewChanges?: () => void;
  // Optional DOM host that receives the session todolist status through a React portal.
  // 通过 React Portal 接收会话 Todo 状态的可选 DOM 宿主。
  todolistStatusHost?: HTMLElement | null;
  // Selects the active workbench model.
  // 选择当前工作台模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Loads backend-managed secondary routes for one model.
  // 加载单个模型的后端二级路由。
  onListWorkbenchModelSecondaryRoutes: (
    providerId: string,
    modelId: string,
    forceRefresh: boolean,
  ) => Promise<ProviderModelSecondaryRouteSummary[]>;
  // Persists a backend-managed secondary route for one model.
  // 持久化单个模型的后端二级路由。
  onSelectWorkbenchModelSecondaryRoute: (
    providerId: string,
    modelId: string,
    route: ProviderModelSecondaryRouteSelection | null,
  ) => Promise<boolean>;
  // Selects the active reasoning effort.
  // 选择当前推理强度。
  onSelectWorkbenchReasoningEffort: (effort: string, choice: ModelPickerChoice) => Promise<boolean>;
  // Opens the project knowledge-library management surface.
  // 打开项目知识库管理界面。
  onOpenKnowledge: () => void;
  // Routes one composer slash command.
  // 路由一个输入区斜杠命令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Submits one composer prompt.
  // 提交一个输入区提示词。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // Persists one user-authored session goal.
  // 持久化单个用户编写的会话目标。
  onSaveSessionGoal: (objective: string, tokenBudget?: number | null) => Promise<boolean>;
  // Resumes one stopped session goal with a fresh budget stage.
  // 使用新的预算阶段继续单个已停止会话目标。
  onResumeSessionGoal: (objective: string, tokenBudget?: number | null) => Promise<boolean>;
  // Clears the current user-authored session goal.
  // 清理当前用户编写的会话目标。
  onClearSessionGoal: () => Promise<boolean>;
  // Updates the current session goal lifecycle status.
  // 更新当前会话目标生命周期状态。
  onUpdateSessionGoalStatus: (status: SessionGoalUserStatusUpdate) => Promise<boolean>;
  // Persists the current session Plan-mode toggle.
  // 持久化当前会话计划模式开关。
  onSetSessionPlanMode: (enabled: boolean) => Promise<boolean>;
  // Persists one current session VMM memory policy mode.
  // 持久化当前会话的单个 VMM 记忆策略模式。
  onSetSessionVmmPolicy: (policy: string, mode: string) => Promise<boolean>;
  // Opens the memory settings destination; navigation never changes persisted memory policy.
  // 打开记忆设置目标；导航绝不改变持久记忆策略。
  onOpenMemorySettings?: () => void;
  // Persists one selected conversation output-style override.
  // 持久化当前会话的输出风格覆盖。
  onSetSessionOutputStyle: (styleId: string | null) => Promise<boolean>;
  // Persists or clears the selected session context-compaction window node.
  // 持久化或清除当前会话的上下文压缩窗口节点。
  onSetSessionContextCompactionWindow: (selection: ContextWindowSelection | null) => Promise<boolean>;
  // Stops the currently running conversation.
  // 停止当前运行中的会话。
  onStopConversationRun: () => Promise<boolean>;
  // Continues a terminated conversation from one exact retained ledger generation.
  // 从精确保留账本代际继续已终止会话。
  onContinueConversation?: (
    conversationId: string,
    clientContinuationId: string,
    supplement: ComposerSubmission | null,
    queuedInputActionId?: string,
  ) => Promise<boolean>;
  // Opens a subagent transcript in the right context panel.
  // 在右侧上下文面板中打开子代理转录。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Stops one subagent task.
  // 停止一个子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Edits one queued prompt.
  // 编辑一个队列中的提示词。
  onEditPendingPrompt: (entryId: string) => Promise<PendingPromptSummary | null>;
  // Inserts one queued prompt into the composer.
  // 将一个队列提示词插入输入区。
  onInsertPendingPrompt: (entryId: string) => Promise<void>;
  // Removes one queued prompt.
  // 移除一个队列提示词。
  onRemovePendingPrompt: (entryId: string) => Promise<void>;
  // Reorders queued prompts.
  // 重排队列提示词。
  onReorderPendingPrompts: (entryIds: string[], draggedEntryId: string, targetEntryId: string | null) => Promise<void>;
  // Stages a browser-provided file through the service.
  // 通过服务暂存浏览器提供的文件。
  onStageAttachmentBytes: (
    projectId: string | null,
    conversationId: string | null,
    file: File,
  ) => Promise<ComposerAttachmentSummary | null>;
  // Copies transcript text through the shell-provided clipboard path.
  // 通过外壳提供的剪贴板路径复制转录文本。
  onCopyText: (text: string) => Promise<void>;
  // Loads one backend-validated user prompt edit draft.
  // 加载单条后端校验过的用户提示词编辑草稿。
  onEditConversationGeneration: (
    sessionId: string,
    expectedWindowGeneration: number,
    targetSeq: number,
    replacementText: string,
  ) => Promise<boolean>;
  // Creates or exactly retries one immutable V1 conversation fork.
  // 创建或精确重试一个不可变 V1 会话分叉。
  onForkConversationGeneration: (
    sourceSessionId: string,
    expectedWindowGeneration: number,
    sourceTurnEndSeq: number,
    childSessionId: string,
    title: string,
  ) => Promise<string | null>;
  // Resolves one composer attachment path into a browser-readable URL.
  // 将一个输入区附件路径解析为浏览器可读 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Selects one persisted conversation from a transcript navigation affordance.
  // 从转录导航入口选择一个持久化会话。
  onSelectConversation: (conversationId: string) => void;
  // Emits lightweight click diagnostics.
  // 输出轻量点击诊断信息。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
  // Stable layout key that changes when outer shared shell geometry changes.
  // 外层共享外壳几何布局变化时更新的稳定布局键。
  layoutStabilityKey?: string;
}

