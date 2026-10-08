// Backend boundary: the only way the UI talks to data.
// 后端边界：界面访问数据的唯一通道。
//
// The UI thread never blocks on I/O. It sends `Command`s and drains `BackendEvent`s from a
// `BackendHandle` once per frame. `service` talks to the running Vulcan Code service (HTTP RPC +
// `/events` WebSocket); `mock` is a scripted in-process backend for the display demo. Both
// implement the same handle, so app state and views do not know which one is attached.
// 界面线程从不阻塞于 I/O。它通过 `BackendHandle` 发送 `Command`，并在每帧消费 `BackendEvent`。
// `service` 连接运行中的 Vulcan Code 服务（HTTP RPC + `/events` WebSocket）；`mock` 为展示 demo
// 提供脚本化的进程内后端。两者实现同一句柄，应用状态与视图无需知道接入的是哪一个。

pub mod mock;
pub mod service;

use std::sync::mpsc::{Receiver, Sender};

use crate::model::*;

/// User intents sent to the backend. Each variant is named after the WebUI service function
/// (`apps/vulcan-code-web/src/service/*-service.ts`) it mirrors and sends the same RPC with the
/// same parameters; system areas use `Call` with a request built by `crate::service_api`.
/// 发送给后端的用户意图。每个变体以其对应的 WebUI 服务函数命名，并以相同参数发送相同 RPC；
/// 系统区域使用 `Call`，其请求由 `crate::service_api` 构建。
#[derive(Debug, Clone)]
pub enum Command {
    /// Persist a session answer mode with the same compare-and-set contract as WebUI.
    /// 按与 WebUI 相同的比较交换契约持久化会话回答模式。
    SetSessionAnswerMode(vulcan_service_api::SessionAnswerModeUpdateRequest),
    /// `selectConversation` (`service_select_conversation`), then `markConversationRead`.
    /// 选择会话，随后标记为已读。
    SelectConversation(ConversationId),
    /// Opens a local draft slot (the WebUI's `draftSlotKey = new:<projectId>`); the service only
    /// sees it with the first prompt.
    /// 打开本地草稿槽（对应 WebUI 的 `draftSlotKey`）；服务在首次提交时才看到它。
    NewConversation { project_id: Option<String> },
    /// `submitConversationPrompt`
    SubmitConversationPrompt {
        conversation_id: ConversationId,
        prompt: String,
        mode: ExecutionMode,
        effort: ReasoningEffort,
        model_id: String,
        /// Attached file paths, staged through `stageComposerAttachmentBytes`.
        /// 附件文件路径，经 `stageComposerAttachmentBytes` 暂存。
        attachments: Vec<String>,
    },
    /// `stopConversationRun`
    StopConversationRun { conversation_id: ConversationId },
    /// `replyPendingQuestion`
    ReplyPendingQuestion {
        request_id: String,
        answers: Vec<Vec<String>>,
    },
    /// `rejectPendingQuestion`
    RejectPendingQuestion { request_id: String },
    /// `setSessionPlanMode`
    SetSessionPlanMode {
        conversation_id: ConversationId,
        enabled: bool,
    },
    /// `selectPublicAiModel`; `model_id` is a `ModelInfo::id`.
    /// `model_id` 为 `ModelInfo::id`。
    SelectPublicAiModel {
        conversation_id: Option<ConversationId>,
        model_id: String,
    },
    /// `selectWorkbenchReasoningEffort`
    SelectWorkbenchReasoningEffort {
        conversation_id: Option<ConversationId>,
        effort: ReasoningEffort,
    },
    /// `loadWorkspaceGitCatalog` + `loadWorkspaceDiffSnapshot` (unstaged and staged).
    /// 加载 Git 目录与差异快照（未暂存与已暂存）。
    LoadWorkspaceDiffSnapshot,
    /// Targets the details, commit and push at one monitored repository of the project, as
    /// `handleSelectReviewRepository`.
    /// 将详情、提交与推送指向项目中一个受监听的仓库，对应 `handleSelectReviewRepository`。
    SelectGitRepository(String),
    /// `setWorkspaceGitMonitoring` (`workspace.git.setMonitoring`).
    SetGitMonitoring {
        repository_key: String,
        enabled: bool,
    },
    /// `runWorkspaceGitFileAction` (`stageFile` / `unstageFile`)
    RunWorkspaceGitFileAction { paths: Vec<String>, stage: bool },
    /// `loadWorkspaceDiffFileDetail`
    /// 读取由完整差异身份指定的文件详情。
    LoadWorkspaceDiffFileDetail { key: GitDiffKey },
    /// `loadWorkspaceGitBranches`
    LoadWorkspaceGitBranches,
    /// `switchWorkspaceGitBranch`; `create` makes a new branch from HEAD.
    /// `create` 表示从 HEAD 新建分支。
    SwitchWorkspaceGitBranch { name: String, create: bool },
    /// `generateGitCommitMessage`
    GenerateGitCommitMessage,
    /// `runGitCommitAction`; an empty message with `push` only pushes.
    /// 消息为空且 `push` 时只推送。
    RunGitCommitAction {
        message: String,
        new_branch: Option<String>,
        include_unstaged: bool,
        push: bool,
    },
    /// `browseProjectDirectories`; `None` starts at the service's default directory.
    /// `None` 从服务的默认目录开始。
    BrowseProjectDirectories { path: Option<String> },
    /// Lists local files for the attachment picker. This stands in for the browser's own file
    /// input, which the WebUI uses without any RPC; the bytes are then staged through
    /// `stageComposerAttachmentBytes`.
    /// 为附件选择器列出本地文件。它对应 WebUI 不经 RPC 使用的浏览器文件输入；字节随后经
    /// `stageComposerAttachmentBytes` 暂存。
    ListLocalFiles { path: Option<String> },
    /// `addProjectFromPath`
    AddProjectFromPath { path: String },
    /// `renameProject`
    RenameProject { project_id: String, name: String },
    /// `toggleProjectPinned`
    ToggleProjectPinned { project_id: String },
    /// `archiveProjectConversations`
    ArchiveProjectConversations { project_id: String },
    /// `removeProject`
    RemoveProject { project_id: String },
    /// `renameConversation`
    RenameConversation {
        conversation_id: ConversationId,
        title: String,
    },
    /// `archiveConversation`
    ArchiveConversation { conversation_id: ConversationId },
    /// `stopSubagentTask`
    StopSubagentTask {
        parent_conversation_id: ConversationId,
        task_id: String,
    },
    /// `closeSubagentTask`
    CloseSubagentTask {
        parent_conversation_id: ConversationId,
        task_id: String,
    },
    /// `runVmmMemoryAction` with `suppress_context_injection_item`, as the workspace memory
    /// panel's "suppress for this session".
    /// 以 `suppress_context_injection_item` 调用 `runVmmMemoryAction`，对应工作区记忆面板的
    /// “本会话不再注入”。
    SuppressContextInjectionItem {
        conversation_id: ConversationId,
        memory_id: String,
    },
    /// `runVmmMemoryAction` with `delete_memories`, as the panel's "permanently delete invalid".
    /// 以 `delete_memories` 调用 `runVmmMemoryAction`，对应面板的“永久删除无效记忆”。
    DeleteMemories {
        conversation_id: ConversationId,
        memory_id: String,
    },
    /// `saveSessionGoal`
    SaveSessionGoal {
        conversation_id: ConversationId,
        objective: String,
        token_budget: Option<u64>,
    },
    /// `resumeSessionGoal`: restarts a stopped goal as a fresh budget stage.
    /// 将已停止的目标恢复为新的预算阶段。
    ResumeSessionGoal {
        conversation_id: ConversationId,
        objective: String,
        token_budget: Option<u64>,
    },
    /// `clearSessionGoal`
    ClearSessionGoal { conversation_id: ConversationId },
    /// `updateSessionGoalStatus`; only `Active` and `Paused` are user updates.
    /// 只有 `Active` 与 `Paused` 属于用户可设置的状态。
    UpdateSessionGoalStatus {
        conversation_id: ConversationId,
        status: SessionGoalStatus,
    },
    /// Any other WebUI service function, built by the same-named function in
    /// `crate::service_api`.
    /// 其他任意 WebUI 服务函数，由 `crate::service_api` 中的同名函数构建。
    Call(crate::service_api::ServiceCall),
}

/// Connection state of the data source.
/// 数据源连接状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting,
}

/// Facts pushed by the backend; each maps onto one service event kind or RPC result.
/// 后端推送的事实；每个对应一种服务事件或 RPC 结果。
#[derive(Debug, Clone)]
pub enum BackendEvent {
    /// Replace the answer mode atomically for the exact owning session.
    /// 为精确所属会话原子替换回答模式。
    AnswerMode {
        conversation_id: ConversationId,
        state: AnswerModeSession,
    },
    Connection(ConnectionState),
    /// `workbench` snapshot.
    Workbench(WorkbenchSnapshot),
    /// `conversationSummaryChanged` / `updateConversationRunPhase`.
    ConversationSummary(ConversationSummary),
    /// The backend moved the selection, e.g. to a newly created conversation.
    /// 后端移动了选中项，例如移到新建的会话。
    Selected(ConversationId),
    /// Model and reasoning effort of the selected conversation; `efforts` lists what the model
    /// accepts (empty when unknown).
    /// 选中会话的模型与推理强度；`efforts` 列出模型接受的取值（未知时为空）。
    ModelSelection {
        model_id: String,
        effort: Option<ReasoningEffort>,
        efforts: Vec<ReasoningEffort>,
    },
    /// `service_session_event_hydration_v1` result: replaces the local event window.
    /// 替换本地事件窗口。
    ConversationHydrated {
        conversation_id: ConversationId,
        events: Vec<ConversationEvent>,
    },
    /// `conversationEventDeltaV1.events`: insert or replace by `seq`.
    /// 按 `seq` 插入或替换。
    EventUpsert {
        conversation_id: ConversationId,
        event: ConversationEvent,
    },
    /// `conversationEventDeltaV1.text_appends`: append streamed text to one event.
    /// 向单个事件追加流式文本。
    TextAppend {
        conversation_id: ConversationId,
        seq: u64,
        text: String,
    },
    #[expect(
        dead_code,
        reason = "neither backend streams run phases yet; mapped once the TUI shares service DTOs"
    )]
    RunPhase {
        conversation_id: ConversationId,
        phase: RunPhase,
    },
    PendingQuestion {
        conversation_id: ConversationId,
        question: Option<PendingQuestion>,
    },
    PendingPrompts {
        conversation_id: ConversationId,
        prompts: Vec<PendingPrompt>,
    },
    ContextUsage {
        conversation_id: ConversationId,
        usage: ContextUsage,
    },
    Todo {
        conversation_id: ConversationId,
        items: Vec<TodoItem>,
    },
    PlanMode {
        conversation_id: ConversationId,
        mode: ExecutionMode,
    },
    CommandSessions(Vec<CommandSession>),
    Agents {
        conversation_id: ConversationId,
        agents: Vec<AgentInfo>,
    },
    /// Memory context, lifetime usage, context breakdown and goal, replaced as a whole.
    /// 记忆上下文、生命周期用量、上下文构成与目标，整体替换。
    SessionInsight {
        conversation_id: ConversationId,
        insight: Box<SessionInsight>,
    },
    /// `workspaceGitChanged` followed by a diff snapshot.
    Git(GitSnapshot),
    /// The selected project changed; the previous project's Git state no longer applies.
    /// 所选项目已变化；上一项目的 Git 状态不再适用。
    GitCleared,
    /// Parsed diff content owned by the exact originating request.
    /// 由准确原始请求拥有的已解析差异内容。
    GitDiff {
        /// Immutable request and cache identity.
        /// 不可变请求及缓存身份。
        key: GitDiffKey,
        /// Parsed contents from the selected comparison mode.
        /// 选定比较模式的已解析内容。
        diff: FileDiff,
    },
    /// A file's diff could not be loaded; `message` is the service's sanitized reason, absent when
    /// the file is no longer in the current change list.
    /// 无法加载某文件的 diff；`message` 为服务的脱敏原因，文件已不在当前变更清单时为空。
    GitDiffFailed {
        key: GitDiffKey,
        message: Option<String>,
    },
    GitBranches(Vec<GitBranch>),
    /// Streaming commit message generation; `done` closes the stream.
    /// 流式生成提交信息；`done` 结束该流。
    CommitMessage {
        text: String,
        done: bool,
    },
    /// A Git action is in progress (`Some(label)`) or finished (`None`).
    /// Git 操作进行中（`Some(label)`）或已结束（`None`）。
    GitBusy(Option<GitBusy>),
    /// A finished Git action; the UI localizes the notice.
    /// 已完成的 Git 操作；由界面本地化提示。
    GitNotice(GitNotice),
    /// `projectsChanged`: the full project list.
    /// 完整项目列表。
    Projects(Vec<Project>),
    /// The conversation was archived or deleted and leaves the sidebar.
    /// 会话已归档或删除，从侧边栏移除。
    ConversationRemoved(ConversationId),
    /// `browseProjectDirectories` or local file listing result.
    /// 浏览项目目录或本地文件列表的结果。
    Directory(DirectoryListing),
    /// A loader result: the raw WebUI DTO (credentials blanked) replaces the slot.
    /// 加载器结果：原始 WebUI DTO（凭据已清空）替换该槽位。
    Slot {
        slot: crate::service_api::Slot,
        call: crate::service_api::ServiceCall,
        value: serde_json::Value,
    },
    /// A scoped loader failed; retains its exact request and already-sanitized cause for retry.
    /// 限定范围加载失败；保留准确请求及已脱敏原因以便重试。
    SlotFailed {
        /// Read projection that could not be loaded.
        /// 无法加载的读取投影。
        slot: crate::service_api::Slot,
        /// Exact loader envelope used for owner and pagination checks.
        /// 用于归属及分页检查的准确加载器结构。
        call: crate::service_api::ServiceCall,
        /// Sanitized transport/service message, without credential-bearing request text.
        /// 脱敏传输或服务消息，不包含持有凭据的请求文本。
        message: String,
    },
    /// The result of a preview call; the UI asks for confirmation before the second step.
    /// 预检调用的结果；界面在第二步前请求确认。
    Preview {
        call: crate::service_api::ServiceCall,
        result: serde_json::Value,
    },
    /// A system operation and its authoritative reloads have all settled.
    /// 系统操作与其权威刷新已全部结束。
    CallFinished,
    /// A finished action; the UI localizes the notice.
    /// 已完成的操作；由界面本地化提示。
    Notice(Notice),
    /// Free-form message from the backend (service errors arrive already worded).
    /// 后端的自由文本消息（服务错误到达时已带文案）。
    Toast {
        severity: NoticeSeverity,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitNotice {
    Committed,
    Pushed,
    Switched(String),
    Created(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    ProjectAdded(String),
    ProjectRemoved(String),
    Archived(String),
    ArchivedCount(usize),
    Renamed,
    MemorySuppressed,
    MemoryDeleted,
    /// The goal changed; `None` means it was removed.
    /// 目标已变化；`None` 表示已移除。
    Goal(Option<SessionGoalStatus>),
    AgentStopped(String),
    AgentClosed(String),
    /// A `service_api` mutation succeeded; carries its WebUI function name.
    /// `service_api` 变更调用成功；携带其 WebUI 函数名。
    Completed(&'static str),
    /// The service could not be reached; carries the technical cause.
    /// 无法连接服务；携带技术原因。
    ServiceUnavailable(String),
    /// An action needs the service but the connection is not up yet.
    /// 操作需要服务，但连接尚未建立。
    NotConnected,
    /// Service data did not match its shared type.
    /// 服务数据不符合其共享类型。
    Unreadable {
        /// Method or event kind that produced the data.
        /// 产生该数据的方法或事件类别。
        source: String,
        /// Value-free description of the mismatch.
        /// 不含取值的不匹配描述。
        detail: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitBusy {
    Committing,
    Pushing,
    Generating,
}

/// Two-way channel between the UI thread and a backend worker.
/// 界面线程与后端工作线程之间的双向通道。
pub struct BackendHandle {
    pub commands: Sender<Command>,
    pub events: Receiver<BackendEvent>,
    /// Short label for the status area, e.g. `mock` or the service base URL.
    /// 状态区的短标签，例如 `mock` 或服务基础 URL。
    pub label: String,
    pub is_mock: bool,
}

impl BackendHandle {
    /// Sends one command; a closed backend is reported by the connection state, not a panic.
    /// 发送一条命令；后端关闭由连接状态体现，而不是 panic。
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}
