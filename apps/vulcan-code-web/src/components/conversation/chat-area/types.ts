import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type {
  ComposerAiCapabilityConfigurationSave,
  ComposerSlashCommandResult,
  ComposerSubmission,
} from "@/components/composer/composer-model";
import type { ThinkingDisplayMode } from "@/components/conversation/thinking-display";
import type { ConversationSessionReviewSummary } from "@/components/conversation/session-todolist-panel";
import type { PlanArtifactActionPayload } from "@/components/conversation/conversation-plan-artifact";
import type {
  ConversationDetailRef,
  ToolDetailPayload,
} from "@/components/conversation/conversation-detail";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import type {
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ConversationHydrationProgress,
  PendingPromptSummary,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  StartupSnapshot,
  UserPromptEditDraftSummary,
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
  // Conversation id currently being hydrated from the service.
  // 当前正在从服务水合的会话标识。
  loadingConversationId: string | null;
  // Last conversation body loading error.
  // 最近一次会话正文加载错误。
  conversationLoadError: string | null;
  // Current lazy conversation body loading progress.
  // 当前懒加载会话正文进度。
  conversationHydration: ConversationHydrationProgress | null;
  // Whether the selected conversation has older history before the loaded window.
  // 当前选中会话已加载窗口之前是否还有更早历史。
  conversationHasMoreHistory: boolean;
  // Whether an older history page is currently being loaded.
  // 当前是否正在加载更早历史页。
  conversationHistoryLoading: boolean;
  // Current Git review summary shown beside compact session progress.
  // 当前用于紧凑会话进程旁展示的 Git 审查摘要。
  sessionReviewSummary: ConversationSessionReviewSummary | null;
  // Selects the active workbench model.
  // 选择当前工作台模型。
  onSelectWorkbenchModel: (choice: ModelPickerChoice) => Promise<boolean>;
  // Loads backend-managed secondary routes for one model.
  // 加载单个模型的后端二级路由。
  onListWorkbenchModelSecondaryRoutes: (
    providerId: string,
    modelId: string,
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
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // Persists the active thinking display mode.
  // 持久化当前思考展示模式。
  onSelectWorkbenchThinkingDisplayMode: (mode: ThinkingDisplayMode) => Promise<boolean>;
  // Routes one composer slash command.
  // 路由一个输入区斜杠命令。
  onSlashCommand: (command: ComposerSlashCommandSummary) => ComposerSlashCommandResult;
  // Submits one composer prompt.
  // 提交一个输入区提示词。
  onSubmitPrompt: (submission: ComposerSubmission) => Promise<boolean>;
  // Persists one session-scoped AI capability draft.
  // 持久化单个会话级 AI 能力草稿。
  onSaveAiCapabilityConfiguration: (request: ComposerAiCapabilityConfigurationSave) => Promise<boolean>;
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
  onUpdateSessionGoalStatus: (status: string) => Promise<boolean>;
  // Persists the current session Plan-mode toggle.
  // 持久化当前会话计划模式开关。
  onSetSessionPlanMode: (enabled: boolean) => Promise<boolean>;
  // Persists one current session VMM memory policy mode.
  // 持久化当前会话的单个 VMM 记忆策略模式。
  onSetSessionVmmPolicy: (policy: string, mode: string) => Promise<boolean>;
  // Stops the currently running conversation.
  // 停止当前运行中的会话。
  onStopConversationRun: () => Promise<boolean>;
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
  onGetUserPromptEditDraft: (
    conversationId: string,
    messageId: string,
  ) => Promise<UserPromptEditDraftSummary | null>;
  // Submits one user prompt edit and replays it.
  // 提交单条用户提示词编辑并重新执行。
  onSubmitUserPromptEdit: (request: {
    conversationId: string;
    messageId: string;
    prompt: string;
    attachments: ComposerAttachmentSummary[];
  }) => Promise<boolean>;
  // Creates a forked copy of one conversation.
  // 创建单个会话的分叉副本。
  onForkConversation: (conversationId: string, beforeMessageId?: string | null) => Promise<boolean>;
  // Selects one persisted conversation from a transcript navigation affordance.
  // 从转录导航入口选择一个持久化会话。
  onSelectConversation: (conversationId: string) => void;
  // Loads one older history page for a conversation.
  // 为单个会话加载一页更早历史。
  onLoadConversationHistory: (conversationId: string) => Promise<boolean>;
  // Loads hidden tool details for expanded transcript rows.
  // 为展开的转录行加载隐藏工具详情。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Resolves local media paths into renderable URLs.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Emits lightweight click diagnostics.
  // 输出轻量点击诊断信息。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
  // Stable layout key that changes when outer shared shell geometry changes.
  // 外层共享外壳几何布局变化时更新的稳定布局键。
  layoutStabilityKey?: string;
}

