import type { ConversationSessionCommitBusyState } from "@/components/conversation/session-todolist-panel";
import type { ModelPickerChoice } from "@/components/composer/model-picker-control";
import type {
  ComposerSlashCommandResult,
  ComposerSubmission,
} from "@/components/composer/composer-model";
import type { AgentActionProps } from "@/components/features/agents/AgentsArea";
import type { SkillActionProps, SkillSubAreaId } from "@/components/features/skills/SkillsArea";
import type { AgentSkillActionProps } from "@/components/features/agent-skills/agent-skills-types";
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
  ContextCompactionWindowNodeId,
  ConversationSummary,
  MainAreaId,
  PendingPromptSummary,
  ProjectSummary,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  SessionGoalUserStatusUpdate,
  StartupSnapshot,
  WorkbenchSnapshot,
  WorkspaceFileDirectoryListing,
  WorkspaceFileFilterOptions,
  WorkspaceFileReadResponse,
} from "@/types";
import type { NotificationPresentationContext } from "@/notifications/types";
import type { WorkspaceDiffFileDetailLoadOptions } from "@/service/workbench-snapshot-service";
import type { WorkspacePageDescriptor } from "@/components/workspace/workspace-page-descriptor";
// WorkspaceMainView identifies the active main surface inside the workbench.
// WorkspaceMainView 标识工作台内部当前激活的主内容界面。
export type WorkspaceMainView = "chat" | "agent-skills" | "skills" | "agents" | "memory" | "knowledge";

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
  // Visual navigation target and phase are separate from the authoritative selected session.
  // 视觉导航目标与阶段独立于权威选中会话。
  conversationNavigation?: { targetId: string; phase: "exiting" | "loading" | "waiting-owner" | "entering"; showLoading: boolean } | null;
  // Releases the loading surface after the exact selected event-window owner is ready.
  // 精确选中事件窗口归属就绪后解除加载界面。
  onConversationNavigationReady?: (conversationId: string) => void;
  // Retires the optimistic first prompt only after its canonical user input is visible.
  // 仅在对应规范用户输入可见后移除乐观首条提示词。
  onFirstSubmissionReconciled?: (conversationId: string, clientSubmissionId: string) => void;
  // Explicit project that owns project-level floating tools.
  // 持有项目级浮动工具的明确项目。
  activeProjectId: string | null;
  // Ref attached to the top-bar button used as the terminal Genie animation anchor.
  // 附加到顶部栏按钮的引用，用作终端精灵动画锚点。
  projectTerminalButtonRef: React.Ref<HTMLButtonElement>;
  // Count of running background terminals owned by the active project.
  // 当前活动项目持有的运行中后台终端数量。
  projectTerminalRunningCount: number;
  // Toggles the terminal container owned by the active project.
  // 切换当前活动项目持有的终端容器。
  onToggleProjectTerminal: () => void;
  // Publishes the exact persisted transcript currently visible in this workspace shell.
  // 发布当前工作区壳中真实可见的持久化转录。
  onNotificationPresentationContextChange: (context: NotificationPresentationContext) => void;
  // Whether a conversation fork request is currently blocking workspace interaction.
  // 当前是否有会话分叉请求正在阻塞工作台交互。
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
  // Opens the prompt skills feature area from the workbench sidebar.
  // 从工作台侧栏打开提示词技能功能区。
  onOpenAgentSkills: () => void;
  // Opens the agents feature area from the workbench sidebar.
  // 从工作台侧栏打开智能体功能区。
  onOpenAgents: () => void;
  // Opens the memory feature area from the workbench sidebar.
  // 从工作台侧栏打开记忆功能区。
  onOpenMemory: () => void;
  // Opens credential management from the VMM first-run prerequisite flow.
  // 从 VMM 首次配置前置流程打开凭据管理。
  onOpenCredentialManagement: () => void;
  // Opens the knowledge library management area from the workbench sidebar.
  // 从工作台侧栏打开知识库管理区域。
  onOpenKnowledge: () => void;
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
  // Selects one project without changing the current conversation.
  // 选择一个项目且不改变当前会话。
  onSelectProject: (projectId: string) => void;
  // Selects the active model for the workbench.
  // 选择工作台当前模型。
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
  // Selects the active reasoning effort for the workbench.
  // 选择工作台当前推理强度。
  onSelectWorkbenchReasoningEffort: (effort: string) => Promise<boolean>;
  // Service-backed skill page actions.
  // 服务端支持的技能页面动作。
  // Independent LuaSkills lifecycle actions.
  // 独立 LuaSkills 生命周期动作。
  luaSkillActions: SkillActionProps;
  // Independent prompt-based Agent Skills actions.
  // 独立提示词 Agent Skills 动作。
  agentSkillActions: AgentSkillActionProps;
  // Service-backed independent agent page actions.
  // 服务端支持的独立智能体页面动作。
  agentActions: AgentActionProps;
  // Service-backed memory page actions.
  // 服务端支持的记忆页面动作。
  memoryActions: MemoryActionProps;
  // Fixed footer content pinned to the workspace bottom edge.
  // 固定在工作区底边的信息栏内容。
  footer?: React.ReactNode;
  // DOM host for the centered session runtime status.
  // 会话运行状态居中显示使用的 DOM 宿主。
  runtimeStatusHost: HTMLElement | null;
  // DOM host for the todolist status placed beside the version label.
  // 放在版本号旁边的 Todo 状态所使用的 DOM 宿主。
  todolistStatusHost: HTMLElement | null;
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
  // Persists one selected conversation output-style override.
  // 持久化当前会话的输出风格覆盖。
  onSetSessionOutputStyle: (styleId: string | null) => Promise<boolean>;
  // Persists or clears the selected session context-compaction window node.
  // 持久化或清除当前会话的上下文压缩窗口节点。
  onSetSessionContextCompactionWindow: (nodeId: ContextCompactionWindowNodeId | null) => Promise<boolean>;
  // Stops the selected conversation run.
  // 停止当前选中会话运行。
  onStopConversationRun: () => Promise<boolean>;
  // Continues a terminated conversation from one exact retained ledger generation.
  // 从精确保留账本代际继续已终止会话。
  onContinueConversation?: (
    conversationId: string,
    clientContinuationId: string,
    supplement: ComposerSubmission | null,
  ) => Promise<boolean>;
  // Replaces one exact V1 root input and physically starts the rewritten Turn.
  // 替换一条精确 V1 根输入并物理启动重写回合。
  onEditConversationGeneration: (
    sessionId: string,
    expectedWindowGeneration: number,
    targetSeq: number,
    replacementText: string,
  ) => Promise<boolean>;
  // Creates or exactly retries one immutable V1 child generation.
  // 创建或精确重试一个不可变 V1 子 generation。
  onForkConversationGeneration: (
    sourceSessionId: string,
    expectedWindowGeneration: number,
    sourceTurnEndSeq: number,
    childSessionId: string,
    title: string,
  ) => Promise<string | null>;
  // Opens a subagent transcript in the right context panel.
  // 在右侧上下文面板中打开子代理转录。
  onOpenSubagentPanel: (parentConversationId: string, taskId: string) => Promise<boolean>;
  // Stops one subagent task.
  // 停止一个子代理任务。
  onStopSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
  // Closes one subagent lifecycle.
  // 关闭一个子智能体生命周期。
  onCloseSubagentTask: (parentConversationId: string, taskId: string) => Promise<void>;
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
  // Submits one user prompt edit and replays it.
  // 提交单条用户提示词编辑并重新执行。
  // Creates a forked copy of one conversation.
  // 创建单个会话的分叉副本。
  // Resolves local media paths into renderable URLs.
  // 将本地媒体路径解析为可渲染 URL。
  resolveMediaSourceUrl: (path: string) => string;
  // Emits lightweight click diagnostics.
  // 输出轻量点击诊断信息。
  onTrace: (action: string, detail?: Record<string, unknown>) => void;
}

