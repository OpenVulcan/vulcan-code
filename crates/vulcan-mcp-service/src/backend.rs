//! Service operations the MCP endpoint needs, implemented by the service kernel.
//! MCP 端点所需的服务操作，由服务内核实现。

use std::path::PathBuf;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use vulcan_provider_config::provider_config::{
    McpMediaTargetGrant, PersistedMcpServiceConfiguration,
};

/// One project as listed to MCP clients.
/// 列给 MCP 客户端的一个项目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpProject {
    /// Stable project identifier.
    /// 稳定项目标识。
    pub id: String,
    /// Display name.
    /// 显示名称。
    pub name: String,
    /// Workspace directory.
    /// 工作区目录。
    pub path: String,
}

/// One active conversation as listed to MCP clients.
/// 列给 MCP 客户端的一个活动会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpSessionRow {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub id: String,
    /// Display title.
    /// 显示标题。
    pub title: String,
    /// Owning project.
    /// 所属项目。
    pub project_id: Option<String>,
    /// Live run phase such as `Idle`, `Thinking`, `Ready`, `Failed`, or `Stopped`.
    /// 实时运行阶段，如 `Idle`、`Thinking`、`Ready`、`Failed` 或 `Stopped`。
    pub run_phase: String,
    /// Whether the session is working right now.
    /// 会话当前是否正在工作。
    pub running: bool,
    /// What needs the user's attention, such as `question` or `approval`.
    /// 需要用户关注的事项，如 `question` 或 `approval`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attention: Option<String>,
    /// Last update time as reported by the workbench.
    /// 工作台报告的最近更新时间。
    pub updated_at: String,
    /// Whether the user has unread output.
    /// 用户是否有未读输出。
    pub unread: bool,
    /// Whether this is a dedicated assistant session.
    /// 是否为专职助手会话。
    pub dedicated: bool,
}

/// Live status of one session, used both for display and for waiting.
/// 一个会话的实时状态，同时用于展示与等待。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSessionStatus {
    /// Sidebar summary.
    /// 侧栏摘要。
    pub row: McpSessionRow,
    /// Questions currently waiting on the user, including permission approvals.
    /// 当前等待用户的问题，包括权限审批。
    pub questions: Vec<McpPendingQuestion>,
    /// Prompts queued behind the running turn.
    /// 排在运行中回合之后的提示词。
    pub queued_prompts: usize,
    /// Exclusive event frontier: the next event will receive this sequence number.
    /// 排他事件前沿：下一条事件将获得该序号。
    pub next_seq: u64,
}

/// One question waiting on the user.
/// 一个等待用户的问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpPendingQuestion {
    /// Identifier passed back when answering.
    /// 回答时传回的标识。
    pub request_id: String,
    /// Present when this is a permission approval, which only the user may decide.
    /// 为权限审批时存在，只有用户可以决定。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_tool: Option<String>,
    /// Questions in display order.
    /// 按显示顺序排列的问题。
    pub prompts: Vec<McpQuestionPrompt>,
}

/// One question inside a pending request.
/// 待处理请求中的一个问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpQuestionPrompt {
    /// Short header.
    /// 简短标题。
    pub header: String,
    /// Full question text.
    /// 完整问题文本。
    pub question: String,
    /// Offered choices; free text is accepted as well.
    /// 提供的选项；同样接受自由文本。
    pub options: Vec<McpQuestionOption>,
    /// Whether several options may be chosen together.
    /// 是否可同时选择多个选项。
    pub multiple: bool,
}

/// One offered answer.
/// 一个提供的答案。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpQuestionOption {
    /// Answer label to send back.
    /// 要回传的答案标签。
    pub label: String,
    /// Explanation of the choice.
    /// 选项说明。
    pub description: String,
}

/// Kinds of transcript entries exposed to MCP clients.
/// 向 MCP 客户端暴露的转录条目类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTranscriptKind {
    /// A user or agent message entering the session.
    /// 进入会话的用户或代理消息。
    User,
    /// Assistant reply text.
    /// 助手回复文本。
    Assistant,
    /// One tool call.
    /// 一次工具调用。
    Tool,
    /// A turn ended.
    /// 一个回合结束。
    TurnEnd,
    /// A host notice such as an error.
    /// 宿主通知，如错误。
    Notice,
    /// A child agent returned its result.
    /// 子代理返回结果。
    ChildResult,
}

/// One projected transcript entry.
/// 一条投影后的转录条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpTranscriptEntry {
    /// Event sequence number.
    /// 事件序号。
    pub seq: u64,
    /// Owning turn, when the event belongs to one.
    /// 所属回合（若有）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<u64>,
    /// Entry kind.
    /// 条目类型。
    pub kind: McpTranscriptKind,
    /// Event status such as `completed`, `streaming`, or a turn outcome.
    /// 事件状态，如 `completed`、`streaming` 或回合结果。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Tool name for tool entries.
    /// 工具条目的工具名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Text body: message text, notice detail, or a bounded tool-output preview.
    /// 正文：消息文本、通知详情或有界的工具输出预览。
    pub text: String,
    /// Last update in Unix milliseconds.
    /// 最近更新时间（Unix 毫秒）。
    pub updated_at_ms: i64,
}

/// One bounded page of transcript entries.
/// 一页有界的转录条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpTranscriptPage {
    /// Entries in sequence order.
    /// 按序号排列的条目。
    pub entries: Vec<McpTranscriptEntry>,
    /// Last event sequence scanned, which may exceed the last returned entry.
    /// 已扫描的最后事件序号，可能大于最后返回的条目。
    pub last_scanned_seq: Option<u64>,
    /// Exclusive event frontier at read time.
    /// 读取时的排他事件前沿。
    pub next_seq: u64,
}

/// Exact chat model selection used by a submission.
/// 提交使用的精确对话模型选择。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpModelSelection {
    /// Provider identifier.
    /// 供应商标识。
    pub provider_id: String,
    /// Model identifier.
    /// 模型标识。
    pub model_id: String,
    /// Credential connection.
    /// 凭据连接。
    pub connection_id: String,
}

/// One prompt submission.
/// 一次提示词提交。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSubmission {
    /// Existing session; absent creates a session in `project_id`.
    /// 已有会话；缺省时在 `project_id` 中新建会话。
    pub session_id: Option<String>,
    /// Project for a new session.
    /// 新会话所属项目。
    pub project_id: Option<String>,
    /// Prompt text.
    /// 提示词文本。
    pub prompt: String,
    /// Explicit model; absent keeps the session's own or the default model.
    /// 显式模型；缺省时保留会话自身或默认模型。
    pub model: Option<McpModelSelection>,
    /// Explicit reasoning effort.
    /// 显式推理档位。
    pub reasoning_effort: Option<String>,
    /// Whether the turn runs in plan mode.
    /// 回合是否以计划模式运行。
    pub plan_mode: bool,
    /// Idempotency identity of this submission.
    /// 本次提交的幂等身份。
    pub submission_id: String,
}

/// Acknowledgement of an accepted submission.
/// 已接纳提交的确认。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpSubmissionAck {
    /// Session that received the prompt.
    /// 接收提示词的会话。
    pub session_id: String,
    /// Whether the prompt was queued behind a running turn.
    /// 提示词是否排在运行中回合之后。
    pub queued: bool,
}

/// One chat model a session can run.
/// 会话可运行的一个对话模型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpChatModel {
    /// Provider identifier.
    /// 供应商标识。
    pub provider_id: String,
    /// Model identifier.
    /// 模型标识。
    pub model_id: String,
    /// Credential connection.
    /// 凭据连接。
    pub connection_id: String,
    /// Model display name.
    /// 模型显示名称。
    pub model_name: String,
    /// Provider display name.
    /// 供应商显示名称。
    pub provider_name: String,
    /// Account display name.
    /// 账号显示名称。
    pub account_name: String,
    /// Whether this account is the preferred one for the model.
    /// 该账号是否为此模型的首选账号。
    pub preferred_account: bool,
}

/// One media target an MCP client may run, resolved against live accounts.
/// MCP 客户端可运行的一个媒体目标，已对照实时账号解析。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpMediaModel {
    /// Public `provider/model` identifier.
    /// 公开 `provider/model` 标识。
    pub model: String,
    /// Stable capability identifier.
    /// 稳定能力标识。
    pub capability: String,
    /// Target display name.
    /// 目标显示名称。
    pub display_name: String,
    /// Credential connection.
    /// 凭据连接。
    pub connection_id: String,
    /// Manifest target identifier.
    /// Manifest 目标标识。
    pub target_id: String,
    /// Reviewed profile and exact argument schema rendered as Markdown.
    /// 渲染为 Markdown 的已审核说明与精确参数 Schema。
    pub parameters_markdown: String,
}

/// One media run request.
/// 一次媒体运行请求。
#[derive(Debug, Clone, PartialEq)]
pub struct McpMediaRun {
    /// Requesting client.
    /// 发起请求的客户端。
    pub client_id: String,
    /// Job identifier.
    /// 任务标识。
    pub job_id: String,
    /// Provider-task owner used for cancellation.
    /// 用于取消的供应商任务所有者。
    pub owner_id: String,
    /// Stable capability identifier.
    /// 稳定能力标识。
    pub capability: String,
    /// Credential connection.
    /// 凭据连接。
    pub connection_id: String,
    /// Manifest target identifier.
    /// Manifest 目标标识。
    pub target_id: String,
    /// Model-native arguments.
    /// 模型原生参数。
    pub arguments: Value,
}

/// One file a media run produced.
/// 媒体运行产生的一个文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpMediaFile {
    /// Absolute local path.
    /// 绝对本地路径。
    pub path: PathBuf,
    /// Resource kind such as `image` or `video`.
    /// 资源类型，如 `image` 或 `video`。
    pub kind: String,
    /// MIME type.
    /// MIME 类型。
    pub mime_type: String,
    /// Size in bytes.
    /// 字节大小。
    pub byte_len: u64,
}

/// Result of one media run.
/// 一次媒体运行的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpMediaOutcome {
    /// Produced files in provider order.
    /// 按供应商顺序排列的产出文件。
    pub files: Vec<McpMediaFile>,
    /// Provider text returned beside or instead of files.
    /// 供应商随文件或代替文件返回的文本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Service operations behind the MCP endpoint.
/// MCP 端点背后的服务操作。
#[async_trait]
pub trait McpServiceBackend: Send + Sync + 'static {
    /// Returns the persisted MCP settings, read fresh for every request.
    /// 返回持久化 MCP 设置，每次请求都重新读取。
    fn configuration(&self) -> Result<PersistedMcpServiceConfiguration, String>;

    /// Lists every open project.
    /// 列出每个打开的项目。
    async fn projects(&self) -> Result<Vec<McpProject>, String>;

    /// Lists every active top-level session.
    /// 列出每个活动的顶层会话。
    async fn sessions(&self) -> Result<Vec<McpSessionRow>, String>;

    /// Reads the live status of one active session; `None` when it does not exist.
    /// 读取一个活动会话的实时状态；不存在时返回 `None`。
    async fn session_status(&self, session_id: &str) -> Result<Option<McpSessionStatus>, String>;

    /// Reads up to `limit` events after `after_seq` and projects the dialogue-relevant ones.
    /// 读取 `after_seq` 之后最多 `limit` 条事件，并投影其中与对话相关的部分。
    async fn read_transcript(
        &self,
        session_id: &str,
        after_seq: u64,
        limit: usize,
        include_tool_output: bool,
    ) -> Result<McpTranscriptPage, String>;

    /// Submits one prompt without moving the user's selected conversation.
    /// 提交一条提示词，且不移动用户当前选中的会话。
    async fn submit(&self, submission: McpSubmission) -> Result<McpSubmissionAck, String>;

    /// Stops the running turn of one session.
    /// 停止一个会话运行中的回合。
    async fn stop(&self, session_id: &str) -> Result<(), String>;

    /// Answers (`Some`) or dismisses (`None`) one non-approval question.
    /// 回答（`Some`）或驳回（`None`）一个非审批问题。
    async fn answer_question(
        &self,
        request_id: &str,
        answers: Option<Vec<Vec<String>>>,
    ) -> Result<bool, String>;

    /// Lists every chat model sessions can run.
    /// 列出会话可运行的每个对话模型。
    async fn chat_models(&self) -> Result<Vec<McpChatModel>, String>;

    /// Resolves granted media targets against live accounts; unavailable grants are skipped.
    /// 对照实时账号解析已授权媒体目标；不可用的授权被跳过。
    async fn media_models(
        &self,
        grants: &[McpMediaTargetGrant],
    ) -> Result<Vec<McpMediaModel>, String>;

    /// Runs one media target and stores its outputs as local files.
    /// 运行一个媒体目标并把输出保存为本地文件。
    async fn run_media(&self, run: McpMediaRun) -> Result<McpMediaOutcome, String>;

    /// Records cancellation for asynchronous provider work owned by `owner_id`.
    /// 为 `owner_id` 拥有的异步供应商工作记录取消。
    fn cancel_media(&self, owner_id: &str) -> Result<(), String>;
}
