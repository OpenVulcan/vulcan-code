import type { ConversationDetailRef, ToolDetailPayload } from "@/components/conversation/conversation-detail";
import type { ConversationSessionCommitBusyState } from "@/components/conversation/session-todolist-panel";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type {
  ComposerAiCapabilityConfigurationSave,
  ComposerSlashCommandResult,
  ComposerSubmission,
} from "@/components/composer/composer-model";
import type { SkillActionProps, SkillSubAreaId } from "@/components/features/skills/SkillsArea";
import type { MemoryActionProps, MemorySubAreaId } from "@/components/features/memory/MemoryArea";
import type { ConversationMenuAction, ProjectMenuAction } from "@/components/project/ProjectSidebar";
import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import type {
  DiffReviewSelection,
  WorkspaceDiffFileDetailRequest,
  WorkspaceDiffFileDetailResponse,
  WorkspaceDiffSearchRequest,
  WorkspaceDiffSearchResponse,
  WorkspaceDiffSnapshotResponse,
  WorkspaceGitFileActionRequest,
  WorkspaceGitFileActionResult,
} from "@/diff/types";
import type {
  ComposerAttachmentSummary,
  ComposerSlashCommandSummary,
  ConversationHydrationProgress,
  ConversationSummary,
  MainAreaId,
  PendingPromptSummary,
  ProjectSummary,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  StartupSnapshot,
  UserPromptEditDraftSummary,
  WorkbenchSnapshot,
  WorkspaceFileDirectoryListing,
  WorkspaceFileFilterOptions,
  WorkspaceFileReadResponse,
} from "@/types";
import type { WorkspaceDiffFileDetailLoadOptions } from "@/service/workbench-snapshot-service";
import type { WorkspacePageDescriptor } from "@/components/workspace/workspace-page-descriptor";
// WorkspaceMainView identifies the active main surface inside the workbench.
// WorkspaceMainView 标识工作台内部当前激活的主内容界面。
export type WorkspaceMainView = "chat" | "skills" | "memory";

// WorkspaceShellProps contains the visible workspace layout state and injected service edges.
// WorkspaceShellProps 包含可见工作区布局状态和注入的服务边界。
export interface WorkspaceShellProps {
  // Startup snapshot shown by nested workspace panels.
  // 嵌套工作区面板展示的启动快照。
  startup: StartupSnapshot;
  // Workbench snapshot rendered by the project tree and conversation area.
  // 项目树与会话区渲染使用的工作台快照。
  workbench: WorkbenchSnapshot;
  // Active main surface inside the workbench.
  // 工作台内部当前激活的主内容界面。
  mainView: WorkspaceMainView;
  // Shared navigation sidebar open state.
  // 共享导航侧栏展开状态。
  sidebarOpen: boolean;
  // Updates the shared navigation sidebar open state.
  // 更新共享导航侧栏展开状态。
  onSidebarOpenChange: (open: boolean) => void;
  // Selected embedded skills sub page.
  // 当前选中的内嵌技能子页面。
  activeSkillSubArea: SkillSubAreaId;
  // Selected embedded memory sub page.
  // 当前选中的内嵌记忆子页面。
  activeMemorySubArea: MemorySubAreaId;
  // Draft conversation selected before a persisted session exists.
  // 持久会话创建前选中的草稿会话。
  selectedDraftConversation: DraftConversationSelection | null;
  // Selected conversation summary, when one exists.
  // 当前选中的会话摘要，如果存在。
  selectedConversation?: ConversationSummary;
  // Selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Conversation id currently being hydrated.
  // 当前正在水合的会话标识。
  loadingConversationId: string | null;
  // Last conversation loading error.
  // 最近一次会话加载错误。
  conversationLoadError: string | null;
  // Current lazy conversation body loading progress.
  // 当前懒加载会话正文进度。
  conversationHydration: ConversationHydrationProgress | null;
  // Whether older history exists before the loaded selected conversation window.
  // 当前已加载选中会话窗口之前是否存在更早历史。
  conversationHasMoreHistory: boolean;
  // Whether an older history page is currently being loaded.
  // 当前是否正在加载更早历史页。
  conversationHistoryLoading: boolean;
  // Pinned conversation ids used by the project tree.
  // 项目树使用的置顶会话标识集合。
  pinnedConversationIds: Set<string>;
  // Handles one conversation context-menu action.
  // 处理单个会话右键菜单动作。
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Creates a new conversation in one project.
  // 在单个项目中创建新会话。
  onCreateConversation: (project: ProjectSummary) => void;
  // Opens the create-project flow.
  // 打开创建项目流程。
  onCreateProject: () => void;
  // Opens the skills feature area from the workbench sidebar.
  // 从工作台侧栏打开技能功能区。
  onOpenSkills: () => void;
  // Opens the memory feature area from the workbench sidebar.
  // 从工作台侧栏打开记忆功能区。
  onOpenMemory: () => void;
  // Selects one top-level application area from the sidebar brand menu.
  // 从侧栏品牌菜单选择一个顶层应用区域。
  onSelectArea: (area: MainAreaId) => void;
  // Updates the selected embedded skills sub page.
  // 更新当前选中的内嵌技能子页面。
  onSelectSkillSubArea: (subArea: SkillSubAreaId) => void;
  // Updates the selected embedded memory sub page.
  // 更新当前选中的内嵌记忆子页面。
  onSelectMemorySubArea: (subArea: MemorySubAreaId) => void;
  // Persists the expanded project id list.
  // 持久化展开的项目标识列表。
  onExpandedProjectIdsChange: (projectIds: string[]) => void;
  // Handles one project context-menu action.
  // 处理单个项目右键菜单动作。
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  // Persists project order changes.
  // 持久化项目排序变化。
  onReorderProjects: (projectIds: string[]) => void;
  // Selects one persisted conversation.
  // 选择一个持久化会话。
  onSelectConversation: (conversationId: string) => void;
  // Loads one older history page for a conversation.
  // 为单个会话加载一页更早历史。
  onLoadConversationHistory: (conversationId: string) => Promise<boolean>;
  // Selects the active model for the workbench.
  // 选择工作台当前模型。
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
  // Selects the active reasoning effort for the workbench.
  // 选择工作台当前推理强度。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  skillActions: SkillActionProps;
  // Service-backed memory page actions.
  // 服务端支持的记忆页面动作。
  memoryActions: MemoryActionProps;
  // Fixed footer content pinned to the workspace bottom edge.
  // 固定在工作区底边的信息栏内容。
  footer?: React.ReactNode;
  // App-wide chrome actions rendered in the shared content header.
  // 渲染在共享内容标题栏中的应用级窗口控制区。
  chromeActions?: React.ReactNode;
  // Whether shared title bar empty space should be a native drag region.
  // 共享标题栏空白区域是否作为原生拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
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
  // Stops the selected conversation run.
  // 停止当前选中会话运行。
  onStopConversationRun: () => Promise<boolean>;
  // Opens a routed subagent transcript panel.
  // 打开已路由的子代理转录面板。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Closes the routed subagent transcript panel.
  // 关闭已路由的子代理转录面板。
  onCloseSubagentPanel: () => Promise<void>;
  // Stops one subagent task.
  // 停止一个子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Force-stops one live command session.
  // 强制停止单条实时命令会话。
  onStopCommandSession: (commandSessionId: string, ownerSessionId?: string | null) => Promise<void>;
  // Edits one queued prompt.
  // 编辑一个队列提示词。
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
  // Loads a fresh workspace diff snapshot.
  // 加载最新工作区 diff 快照。
  onLoadWorkspaceDiffSnapshot: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    selection: DiffReviewSelection,
  ) => Promise<WorkspaceDiffSnapshotResponse>;
  // Loads a fresh workspace diff detail for one file.
  // 加载单个文件的最新工作区 diff 详情。
  onLoadWorkspaceDiffFileDetail: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    selection: DiffReviewSelection,
    file: WorkspaceDiffFileDetailRequest,
    options?: WorkspaceDiffFileDetailLoadOptions,
  ) => Promise<WorkspaceDiffFileDetailResponse>;
  // Loads compact file-level search hits for the current workspace diff.
  // 加载当前工作区 diff 的紧凑文件级搜索命中。
  onLoadWorkspaceDiffSearch: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    selection: DiffReviewSelection,
    search: WorkspaceDiffSearchRequest,
  ) => Promise<WorkspaceDiffSearchResponse>;
  // Runs one file-level Git action for the workspace review panel.
  // 为工作区审核面板执行一次文件级 Git 操作。
  onRunWorkspaceGitFileAction: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    request: WorkspaceGitFileActionRequest,
  ) => Promise<WorkspaceGitFileActionResult>;
  // Loads one lazy workspace file directory.
  // 加载单个懒加载工作区文件目录。
  onLoadWorkspaceFileDirectory: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    directoryPath: string | null,
    filters: WorkspaceFileFilterOptions,
  ) => Promise<WorkspaceFileDirectoryListing>;
  // Loads one workspace file preview response.
  // 加载单个工作区文件预览响应。
  onLoadWorkspaceFilePreview: (
    workspacePath: string,
    projectId: string | null,
    conversationId: string | null,
    filePath: string,
    filters: WorkspaceFileFilterOptions,
  ) => Promise<WorkspaceFileReadResponse>;
  // Stages one browser-provided attachment through the service.
  // 通过服务暂存一个浏览器提供的附件。
  onStageAttachmentBytes: (
    projectId: string | null,
    conversationId: string | null,
    file: File,
  ) => Promise<ComposerAttachmentSummary | null>;
  // Copies text through the shell-provided clipboard path.
  // 通过外壳提供的剪贴板路径复制文本。
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
  // Loads hidden tool details for transcript rows.
  // 为转录行加载隐藏工具详情。
  onLoadToolDetail: (detailRef: ConversationDetailRef) => Promise<ToolDetailPayload | null>;
  // Resolves local media paths into renderable URLs.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Emits lightweight click diagnostics.
  // 输出轻量点击诊断信息。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
}

