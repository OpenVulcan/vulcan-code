// Client-side domain model.
// 客户端领域模型。
//
// These types intentionally mirror the service wire shapes (`vulcan-service-api` workbench DTOs and
// `vulcan-session-core` v1 conversation events) so the mock backend and the future service backend
// feed the same reducer. Only the fields the TUI renders are kept; wiring adds a mapping layer from
// the wire DTOs to these types instead of reshaping the UI.
// 这些类型刻意对应服务传输形状（`vulcan-service-api` 工作台 DTO 与 `vulcan-session-core` v1 会话事件），
// 使模拟后端与未来的服务后端驱动同一个 reducer。只保留 TUI 渲染所需字段；对接时增加从传输 DTO
// 到这些类型的映射层，而不是改动界面结构。

pub mod insight;
pub mod system;

use serde_json::Value;

pub use insight::*;
pub use system::*;

pub type ConversationId = String;

/// Conversation run phase (`idle|running|queued|completed|failed`).
/// 会话运行阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunPhase {
    Idle,
    /// Memory recall (PreCheck) before the model answers.
    /// 模型回答前的记忆召回（PreCheck）。
    MemoryRecall,
    Running,
    Queued,
    Completed,
    Failed,
}

impl RunPhase {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            RunPhase::MemoryRecall | RunPhase::Running | RunPhase::Queued
        )
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub pinned: bool,
}

#[derive(Debug, Clone)]
pub struct ConversationSummary {
    pub id: ConversationId,
    pub title: String,
    pub project_id: Option<String>,
    pub run_phase: RunPhase,
    pub last_updated_label: String,
    pub unread: bool,
    /// A pending question or failure needs the user.
    /// 有等待中的提问或失败需要用户处理。
    pub attention: bool,
}

/// One entry of a directory listing used by the folder and attachment pickers.
/// 文件夹与附件选择器使用的目录条目。
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    /// The directory is a Git repository root.
    /// 该目录是 Git 仓库根目录。
    pub is_repo: bool,
    pub hidden: bool,
    pub size: Option<u64>,
}

/// `service_list_directory` result: the listed path, its parent and children, plus filesystem roots.
/// `service_list_directory` 结果：所列路径、其父目录与子项，以及文件系统根。
#[derive(Debug, Clone)]
pub struct DirectoryListing {
    pub path: String,
    pub parent: Option<String>,
    pub roots: Vec<String>,
    pub entries: Vec<DirEntry>,
    pub error: Option<String>,
}

impl DirectoryListing {
    /// Joins a child name onto the listed path with the path's own separator.
    /// 用所列路径自身的分隔符拼接子项名称。
    pub fn child(&self, name: &str) -> String {
        let separator = if self.path.contains('\\') { '\\' } else { '/' };
        if self.path.ends_with(separator) {
            format!("{}{name}", self.path)
        } else {
            format!("{}{separator}{name}", self.path)
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    pub provider: String,
    pub context_window: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Build,
    Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningEffort {
    Off,
    Low,
    Medium,
    High,
    Max,
}

impl ReasoningEffort {
    pub const ALL: [ReasoningEffort; 5] = [
        ReasoningEffort::Off,
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::Max,
    ];
}

/// Initial state delivered once the backend is connected (`service_workbench_snapshot`).
/// 后端连接后下发的初始状态（`service_workbench_snapshot`）。
#[derive(Debug, Clone)]
pub struct WorkbenchSnapshot {
    pub projects: Vec<Project>,
    pub conversations: Vec<ConversationSummary>,
    pub selected_conversation_id: Option<ConversationId>,
    pub models: Vec<ModelInfo>,
    pub selected_model_id: String,
    pub ui_locale: Option<String>,
    /// Service base URL; `/web` opens the WebUI here.
    /// 服务基础 URL；`/web` 在此打开 WebUI。
    pub base_url: String,
}

// ---- Conversation events (vulcan-session-core v1) ----

// Event states are the shared v1 types themselves, so the TUI cannot drift from the service.
// 事件状态直接使用共享 v1 类型，使 TUI 不会与服务产生偏差。
pub use vulcan_session_core::v1::{
    AssistantSemanticStatus, NoticeSeverity, ToolCallStatus, TurnOutcome, UserInputSource,
};

/// Lifecycle questions the TUI asks of a shared tool status.
/// TUI 对共享工具状态提出的生命周期判断。
pub trait ToolCallStatusExt {
    /// Whether the call has not reached a terminal state yet.
    /// 调用是否尚未到达终态。
    fn is_open(&self) -> bool;
}

impl ToolCallStatusExt for ToolCallStatus {
    fn is_open(&self) -> bool {
        matches!(
            *self,
            ToolCallStatus::Prepared | ToolCallStatus::Running | ToolCallStatus::WaitingUser
        )
    }
}

/// File-level summary of a tool changeset.
/// 工具变更集的文件级摘要。
#[derive(Debug, Clone)]
pub struct ChangesetFile {
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone)]
pub enum EventBody {
    UserInput {
        text: String,
        input_source: UserInputSource,
        /// File names attached to the prompt.
        /// 随提示附带的文件名。
        attachments: Vec<String>,
    },
    AssistantReasoning {
        text: String,
        status: AssistantSemanticStatus,
    },
    AssistantText {
        text: String,
        status: AssistantSemanticStatus,
    },
    ToolCall {
        tool_name: String,
        input: Value,
        output: Option<String>,
        changeset: Vec<ChangesetFile>,
        status: ToolCallStatus,
        started_at_ms: Option<u64>,
        finished_at_ms: Option<u64>,
    },
    /// Rendered from `display_note`; the service sends a reason code that the client localizes.
    /// 来自 `display_note`；服务发送原因代码，由客户端本地化。
    DisplayNote {
        severity: NoticeSeverity,
        message: String,
    },
    TurnStats {
        outcome: TurnOutcome,
        duration_ms: u64,
        total_tokens: u64,
        requests: u32,
        tool_calls: u32,
    },
}

/// One `ConversationEventSummary`; `seq` is the stable identity within a conversation.
/// 一条 `ConversationEventSummary`；`seq` 是会话内的稳定标识。
#[derive(Debug, Clone)]
pub struct ConversationEvent {
    pub seq: u64,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub body: EventBody,
}

// ---- Workbench side state ----

#[derive(Debug, Clone)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct Question {
    pub header: String,
    pub question: String,
    pub options: Vec<QuestionOption>,
    pub multiple: bool,
}

/// A pending `ask` request (`pending_question`); answers are `string[][]`, one list per question.
/// 等待中的 `ask` 请求；回答为 `string[][]`，每个问题一组。
#[derive(Debug, Clone)]
pub struct PendingQuestion {
    pub id: String,
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone)]
pub struct PendingPrompt {
    pub preview: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ContextUsage {
    pub used_tokens: u64,
    pub max_tokens: u64,
}

impl ContextUsage {
    pub fn percent(&self) -> u8 {
        if self.max_tokens == 0 {
            return 0;
        }
        ((self.used_tokens * 100) / self.max_tokens).min(100) as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone)]
pub struct TodoItem {
    pub content: String,
    pub status: TodoStatus,
}

#[derive(Debug, Clone)]
pub struct CommandSession {
    pub id: String,
    pub title: String,
    pub command: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub screen: Vec<String>,
}

/// Subagent lifecycle (`conversation.subagent.state.*`).
/// 子代理生命周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Running,
    /// Finished its work and waits for more messages.
    /// 已完成工作，等待更多消息。
    Standby,
    Stopped,
    Failed,
    /// Ended; it cannot receive messages any more.
    /// 已结束，不能再接收消息。
    Closed,
}

impl AgentState {
    pub const ALL: [AgentState; 5] = [
        AgentState::Running,
        AgentState::Standby,
        AgentState::Stopped,
        AgentState::Failed,
        AgentState::Closed,
    ];
}

/// A message waiting for a subagent: steering interrupts, follow-ups wait for the current step.
/// 等待子代理的消息：引导会中断当前步骤，后续消息等待当前步骤结束。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMessage {
    pub steering: bool,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    /// Agent definition (profile) the subagent was created from.
    /// 子代理创建时使用的代理定义（档案）。
    pub profile: String,
    pub task: String,
    pub state: AgentState,
    /// Effective prompt the agent received.
    /// 代理收到的实际提示。
    pub prompt: String,
    pub pending: Vec<AgentMessage>,
    /// Recent activity, newest last.
    /// 最近的活动，最新的在最后。
    pub activity: Vec<String>,
    pub result: Option<String>,
    pub usage: UsageBucket,
}

// ---- Git ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

impl GitFileStatus {
    pub fn letter(self) -> &'static str {
        match self {
            GitFileStatus::Modified => "M",
            GitFileStatus::Added => "A",
            GitFileStatus::Deleted => "D",
            GitFileStatus::Renamed => "R",
            GitFileStatus::Untracked => "?",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GitFile {
    pub path: String,
    pub status: GitFileStatus,
    pub staged: bool,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone)]
pub struct GitCommit {
    pub short_sha: String,
    pub subject: String,
}

#[derive(Debug, Clone)]
pub struct GitSnapshot {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<GitFile>,
    pub unpushed: Vec<GitCommit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Add,
    Delete,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub old_no: Option<u32>,
    pub new_no: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone)]
pub struct GitBranch {
    pub name: String,
    pub current: bool,
    pub remote: bool,
    pub last_commit: String,
}
