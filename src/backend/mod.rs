// Backend boundary: the only way the UI talks to data.
// 后端边界：界面访问数据的唯一通道。
//
// The UI thread never blocks on I/O. It sends `Command`s and drains `BackendEvent`s from a
// `BackendHandle` once per frame. `mock` provides a scripted in-process backend for the display
// demo; the service backend (HTTP RPC + `/events` WebSocket) will implement the same handle so the
// app state and views stay unchanged when wiring.
// 界面线程从不阻塞于 I/O。它通过 `BackendHandle` 发送 `Command`，并在每帧消费 `BackendEvent`。
// `mock` 为展示 demo 提供脚本化的进程内后端；服务后端（HTTP RPC + `/events` WebSocket）将实现同一
// 句柄，使对接时应用状态与视图无需改动。

pub mod mock;

use std::sync::mpsc::{Receiver, Sender};

use crate::model::*;

/// User intents sent to the backend; each maps onto one service RPC.
/// 发送给后端的用户意图；每个对应一个服务 RPC。
#[derive(Debug, Clone)]
pub enum Command {
    /// `service_select_conversation`
    SelectConversation(ConversationId),
    /// Creates a draft conversation in a project (`draftSlotKey = new:<projectId>`).
    /// 在项目中创建草稿会话。
    NewConversation {
        project_id: Option<String>,
    },
    /// `service_submit_conversation_prompt`
    Submit {
        conversation_id: ConversationId,
        prompt: String,
        mode: ExecutionMode,
        effort: ReasoningEffort,
        model_id: String,
        /// Attached file paths (`composer.attachment`).
        /// 附件文件路径。
        attachments: Vec<String>,
    },
    /// `service_stop_conversation_run`
    Stop {
        conversation_id: ConversationId,
    },
    /// `service_reply_pending_question`
    ReplyQuestion {
        request_id: String,
        answers: Vec<Vec<String>>,
    },
    /// `service_reject_pending_question`
    RejectQuestion {
        request_id: String,
    },
    /// `service_set_session_plan_mode`
    SetPlanMode {
        conversation_id: ConversationId,
        enabled: bool,
    },
    /// `service_workspace_diff_snapshot` / `workspace.git.catalog`
    GitRefresh,
    /// `service_workspace_git_file_action` (stage / unstage)
    GitStage {
        paths: Vec<String>,
        stage: bool,
    },
    /// `service_workspace_diff_file_detail`
    GitLoadDiff {
        path: String,
    },
    /// `workspace.git.branches`
    GitLoadBranches,
    /// `workspace.git.switchBranch`, `create` for a new branch from HEAD
    GitSwitchBranch {
        name: String,
        create: bool,
    },
    /// `service_generate_git_commit_message`
    GitGenerateCommitMessage,
    /// `service_run_git_commit_action`
    GitCommit {
        message: String,
        new_branch: Option<String>,
        include_unstaged: bool,
        push: bool,
    },
    GitPush,
    /// `service_list_directory`; `None` lists the user's home directory.
    /// 列出目录；`None` 列出用户主目录。
    ListDirectory {
        path: Option<String>,
        show_hidden: bool,
        /// Include files (attachment picker) or only directories (project picker).
        /// 是否包含文件（附件选择器），否则只列目录（项目选择器）。
        files: bool,
    },
    /// `service_add_project`
    AddProject {
        path: String,
    },
    /// `service_rename_project`
    RenameProject {
        project_id: String,
        name: String,
    },
    /// `service_set_project_pinned`
    SetProjectPinned {
        project_id: String,
        pinned: bool,
    },
    /// `service_archive_project_conversations`; `remove_project` also forgets the project.
    /// 归档项目下全部会话；`remove_project` 同时移除该项目。
    ArchiveProjectConversations {
        project_id: String,
        remove_project: bool,
    },
    /// `service_rename_conversation`
    RenameConversation {
        conversation_id: ConversationId,
        title: String,
    },
    /// `service_set_conversation_pinned`
    SetConversationPinned {
        conversation_id: ConversationId,
        pinned: bool,
    },
    /// `service_archive_conversation`
    ArchiveConversation {
        conversation_id: ConversationId,
    },
    /// `service_mark_conversation_unread`
    MarkUnread {
        conversation_id: ConversationId,
    },
    /// Stops or ends one subagent of a conversation.
    /// 停止或结束会话中的一个子代理。
    AgentControl {
        conversation_id: ConversationId,
        agent_id: String,
        op: AgentOp,
    },
    /// Sends a steering or follow-up message to a subagent (`conversation.subagent.detail.message`).
    /// 向子代理发送引导或后续消息。
    AgentMessage {
        conversation_id: ConversationId,
        agent_id: String,
        message: AgentMessage,
    },
    /// Stops injecting one dynamic memory into this conversation (`workspaceMemory.suppressForSession`).
    /// 停止向本会话注入某条动态记忆。
    SuppressMemory {
        conversation_id: ConversationId,
        memory_id: String,
    },
    /// Permanently deletes an invalid memory (`workspaceMemory.permanentDeleteInvalid`).
    /// 永久删除无效记忆。
    DeleteMemory {
        conversation_id: ConversationId,
        memory_id: String,
    },
    Goal {
        conversation_id: ConversationId,
        op: GoalOp,
    },
    /// Settings, system plugins and other system areas.
    /// 设置、系统插件与其他系统区域。
    System(SystemCommand),
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
    Connection(ConnectionState),
    /// `workbench` snapshot.
    Workbench(WorkbenchSnapshot),
    /// `conversationSummaryChanged` / `updateConversationRunPhase`.
    ConversationSummary(ConversationSummary),
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
    GitDiff(FileDiff),
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
    /// `service_list_directory` result.
    Directory(DirectoryListing),
    /// System areas snapshot, replaced as a whole.
    /// 系统区域快照，整体替换。
    System(Box<SystemSnapshot>),
    /// A finished action; the UI localizes the notice.
    /// 已完成的操作；由界面本地化提示。
    Notice(Notice),
    /// Free-form message from the backend (service errors arrive already worded).
    /// 后端的自由文本消息（服务错误到达时已带文案）。
    Toast {
        severity: Severity,
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
    Restored(String),
    Deleted,
    Renamed,
    SettingsSaved,
    CredentialAdded,
    CredentialTested {
        ok: bool,
    },
    NotificationSent,
    LogsCleaned,
    MemoryUpdated,
    JobStarted(String),
    JobFinished(String),
    MemorySuppressed,
    MemoryDeleted,
    /// The goal changed; `None` means it was removed.
    /// 目标已变化；`None` 表示已移除。
    Goal(Option<GoalStatus>),
    AgentStopped(String),
    AgentClosed(String),
    /// A plugin, skill or index action finished; carries the item name.
    /// 插件、技能或索引操作已完成；携带条目名称。
    Done(String),
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
