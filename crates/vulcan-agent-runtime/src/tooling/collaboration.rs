//! Native agent-collaboration commands and host runtime tool candidates.
//! 原生代理协作命令与宿主运行时工具候选。

use crate::agent_runtime::task_runtime::collaboration_lifecycle::{
    ChildTaskActivationRequest, ChildTaskLifecycleReceipt, ChildTaskReservationRequest,
    ChildTaskTerminalRequest,
};
use serde::{Deserialize, Serialize};

/// Derives the shared operation fingerprint from exact host invocation coordinates and tool name.
/// 根据精确宿主调用坐标及工具名称派生共享操作指纹。
/// Returns a stable identity for retries; callers must authenticate the invocation before writing.
/// 返回重试使用的稳定身份；调用方必须在写入前认证调用。
pub fn stable_collaboration_operation_id(
    context: &VulcanToolInvocationContext,
    tool_id: &str,
) -> String {
    let provider_tool_call_id = context.provider_tool_call_id.as_deref().unwrap_or("none");
    format!(
        "v1:{}:{}:{}:{}:{}:{}:{}:{}",
        context.session_id,
        tool_id,
        context.request_stream_id.as_str(),
        context.api_call_id.value(),
        context.tool_call_id.value(),
        context.tool_seq.value(),
        context.controller_tool_call_id,
        provider_tool_call_id
    )
}
use serde_json::{Value, json};
use std::fmt;
use std::sync::Arc;
use vulcan_tools_manager::VulcanToolInvocationContext;
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolInvocationResult,
    VulcanToolOrigin, VulcanToolSourceKind,
};

/// Maximum number of sessions or session events returned by one collaboration read.
/// 单次协作读取允许返回的会话或会话事件最大数量。
pub const MAX_AGENT_COLLABORATION_LIMIT: u32 = 100;

/// Maximum cursor length accepted by session listing.
/// 会话列表接受的游标最大长度。
pub const MAX_AGENT_COLLABORATION_CURSOR_CHARS: usize = 512;

/// Maximum text length accepted by one collaboration message or question.
/// 单条协作消息或问题接受的最大文本长度。
pub const MAX_AGENT_COLLABORATION_TEXT_CHARS: usize = 16 * 1024;

/// Minimum wait timeout accepted by collaboration wait commands.
/// 协作等待命令接受的最小等待超时。
pub const MIN_AGENT_COLLABORATION_TIMEOUT_MS: u64 = 1;

/// Maximum wait timeout accepted by collaboration wait commands.
/// 协作等待命令接受的最大等待超时。
pub const MAX_AGENT_COLLABORATION_TIMEOUT_MS: u64 = 3_600_000;

/// Stable public id for the general collaboration question tool.
/// 通用协作提问工具的稳定公开标识。
pub const ASK_QUESTION_TOOL_ID: &str = "ask_question";

/// Stable public id for the legacy child-agent wait tool's collaboration wake binding.
/// 旧子代理等待工具协作唤醒绑定的稳定公开标识。
///
/// The identity is owned by the subagent core tool surface; this binding must not redeclare it.
/// 该身份由子代理核心工具面持有；此处绑定不得重复声明它。
pub const WAIT_AGENT_TOOL_ID: &str = vulcan_subagent_core::WAIT_AGENT_TOOL;

/// Stable public id for the direct parent question tool.
/// 直接向父会话提问工具的稳定公开标识。
pub const ASK_PARENT_TOOL_ID: &str = "ask_parent";

/// Stable public id for the question reply tool.
/// 问题回答工具的稳定公开标识。
pub const REPLY_QUESTION_TOOL_ID: &str = "reply_question";

/// Stable public id for waiting on a question.
/// 等待问题答案工具的稳定公开标识。
pub const WAIT_QUESTION_TOOL_ID: &str = "wait_question";

/// Stable public id for cancelling a question.
/// 撤销问题工具的稳定公开标识。
pub const CANCEL_QUESTION_TOOL_ID: &str = "cancel_question";

/// Stable public id for listing existing sessions.
/// 列出现有会话工具的稳定公开标识。
pub const LIST_SESSIONS_TOOL_ID: &str = "list_sessions";

/// Stable public id for reading one existing session.
/// 读取现有会话工具的稳定公开标识。
pub const READ_SESSION_TOOL_ID: &str = "read_session";

/// Stable public id for sending a message to an existing session.
/// 向现有会话发送消息工具的稳定公开标识。
pub const SEND_SESSION_MESSAGE_TOOL_ID: &str = "send_session_message";

/// Stable public id for sending a fire-and-forget notification to another authorized session.
/// 向另一个获授权会话发送即发即忘通知工具的稳定公开标识。
pub const SEND_NOTIFICATION_TOOL_ID: &str = "send_notification";

/// Stable public id for waiting on an existing session.
/// 等待现有会话工具的稳定公开标识。
pub const WAIT_SESSION_TOOL_ID: &str = "wait_session";

/// Delivery mode for a message sent to an existing session.
/// 发送到现有会话的消息投递模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMessageDeliveryMode {
    /// Queue the message behind the target's current work.
    /// 将消息排在目标当前工作之后。
    Queue,
    /// Deliver the message at the target's next legal interruption boundary.
    /// 在目标下一个合法中断边界投递消息。
    Interrupt,
}

/// Arguments for asking one exact target session a question.
/// 向一个精确目标会话提问的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct AskQuestionArgs {
    /// Exact target session id authorized by the host.
    /// 由宿主授权的精确目标会话标识。
    pub target_session_id: String,
    /// Question text delivered to the target session.
    /// 投递给目标会话的问题正文。
    pub question: String,
}

/// Arguments for asking the current agent's real parent session.
/// 向当前代理真实父会话提问的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct AskParentArgs {
    /// Question text delivered to the resolved parent session.
    /// 投递给解析出的父会话的问题正文。
    pub question: String,
}

/// Arguments for replying to one exact question transaction.
/// 回答一个精确问题事务的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ReplyQuestionArgs {
    /// Exact question transaction id returned by a question tool.
    /// 问题工具返回的精确问题事务标识。
    pub question_id: String,
    /// Answer text submitted to the question owner.
    /// 提交给问题所有者的回答正文。
    pub answer: String,
    /// Explicitly states that the recipient cannot answer the question.
    /// 明确表示收件方无法回答该问题。
    #[serde(default)]
    pub unable_to_answer: bool,
}

/// Arguments for waiting on one exact question transaction.
/// 等待一个精确问题事务的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct WaitQuestionArgs {
    /// Exact question transaction id to observe.
    /// 要观察的精确问题事务标识。
    pub question_id: String,
    /// Last question revision already observed by the caller.
    /// 调用方已经观察到的问题修订号。
    pub after_revision: Option<u64>,
    /// Maximum time to wait for a new question revision.
    /// 等待新问题修订的最大时长。
    pub timeout_ms: u64,
}

/// Arguments for cancelling one exact question transaction.
/// 撤销一个精确问题事务的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CancelQuestionArgs {
    /// Exact question transaction id to cancel.
    /// 要撤销的精确问题事务标识。
    pub question_id: String,
    /// Human-readable cancellation reason retained by the service.
    /// 由服务保留的人类可读撤销原因。
    pub reason: String,
}

/// Arguments for listing sessions visible to the current authority.
/// 列出当前授权可见会话的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ListSessionsArgs {
    /// Maximum number of session summaries to return.
    /// 返回的会话摘要最大数量。
    pub limit: u32,
    /// Opaque continuation cursor from a previous listing.
    /// 上一次列表返回的不透明续页游标。
    pub cursor: Option<String>,
}

/// Arguments for reading one exact session's bounded event range.
/// 读取一个精确会话有界事件范围的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct ReadSessionArgs {
    /// Exact session id to read.
    /// 要读取的精确会话标识。
    pub session_id: String,
    /// Return events after this already observed sequence.
    /// 返回该已观察序号之后的事件。
    pub after_seq: Option<u64>,
    /// Maximum number of events to return.
    /// 返回的事件最大数量。
    pub limit: u32,
}

/// Arguments for sending a message to one existing session.
/// 向一个现有会话发送消息的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct SendSessionMessageArgs {
    /// Exact target session id authorized by the host.
    /// 由宿主授权的精确目标会话标识。
    pub session_id: String,
    /// Message text delivered to the target session.
    /// 投递给目标会话的消息正文。
    pub message: String,
    /// Delivery behavior requested for the message.
    /// 请求采用的消息投递行为。
    pub delivery_mode: SessionMessageDeliveryMode,
}

/// Arguments for sending one fire-and-forget notification to an authorized session.
/// 向一个获授权会话发送单条即发即忘通知的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct SendNotificationArgs {
    /// Exact target session id authorized by the host.
    /// 由宿主授权的精确目标会话标识。
    pub target_session_id: String,
    /// Notification text delivered without a reply contract.
    /// 不附带回复契约投递的通知正文。
    pub message: String,
}

/// Arguments for waiting on one existing session's revision.
/// 等待一个现有会话修订的参数。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct WaitSessionArgs {
    /// Exact session id to observe.
    /// 要观察的精确会话标识。
    pub session_id: String,
    /// Last durable revision observed by the caller; omission captures the current published revision.
    /// 调用方已观察的可靠修订号；省略时捕获当前已发布修订。
    pub after_revision: Option<u64>,
    /// Maximum time to wait for a new session revision.
    /// 等待新会话修订的最大时长。
    pub timeout_ms: u64,
}

/// Typed command sent across the native collaboration runtime boundary.
/// 发送到原生协作运行时边界的类型化命令。
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "command", content = "args")]
pub enum AgentCollaborationCommand {
    /// Ask an explicitly selected session.
    /// 向明确选择的会话提问。
    #[serde(rename = "ask_question")]
    AskQuestion(AskQuestionArgs),
    /// Ask the current agent's resolved parent.
    /// 向当前代理解析出的父会话提问。
    #[serde(rename = "ask_parent")]
    AskParent(AskParentArgs),
    /// Reply to a question received by the current session.
    /// 回答当前会话收到的问题。
    #[serde(rename = "reply_question")]
    ReplyQuestion(ReplyQuestionArgs),
    /// Wait for a question revision.
    /// 等待问题修订。
    #[serde(rename = "wait_question")]
    WaitQuestion(WaitQuestionArgs),
    /// Cancel a question owned by the current session.
    /// 撤销当前会话拥有的问题。
    #[serde(rename = "cancel_question")]
    CancelQuestion(CancelQuestionArgs),
    /// List sessions authorized for the current caller.
    /// 列出当前调用方获准访问的会话。
    #[serde(rename = "list_sessions")]
    ListSessions(ListSessionsArgs),
    /// Read one authorized session.
    /// 读取一个获准访问的会话。
    #[serde(rename = "read_session")]
    ReadSession(ReadSessionArgs),
    /// Send a queue or interrupt message to one authorized session.
    /// 向一个获准会话发送排队或中断消息。
    #[serde(rename = "send_session_message")]
    SendSessionMessage(SendSessionMessageArgs),
    /// Send a fire-and-forget notification to one authorized session.
    /// 向一个获授权会话发送即发即忘通知。
    #[serde(rename = "send_notification")]
    SendNotification(SendNotificationArgs),
    /// Wait for one authorized session revision.
    /// 等待一个获准会话修订。
    #[serde(rename = "wait_session")]
    WaitSession(WaitSessionArgs),
}

/// Host-injected runtime port for authoritative agent collaboration operations.
/// 宿主注入的权威代理协作操作运行时端口。
#[async_trait::async_trait]
pub trait NativeAgentCollaborationRuntime: fmt::Debug + Send + Sync {
    /// Closes a child through the host after durably sealing its exact V1 message target.
    /// 宿主持久封闭精确 V1 消息目标之后关闭子代理。
    /// `context` authenticates the original ToolCall; `task_runtime` owns runtime archival and `args` names its child.
    /// `context` 认证原始工具调用；`task_runtime` 拥有运行时归档，`args` 指定其子代理。
    /// Returns the ordinary close tool result or an explicit host admission error.
    /// 返回普通关闭工具结果或明确的宿主准入错误。
    async fn close_child_agent(
        &self,
        _context: VulcanToolInvocationContext,
        _task_runtime: Arc<crate::agent_runtime::task_runtime::TaskRuntime>,
        _args: Value,
    ) -> Result<vulcan_tools_manager::VulcanToolInvocationResult, String> {
        Err(String::from("The collaboration host does not support durable child closure."))
    }

    /// Compacts the exact child through its durable host, returning only a committed canonical snapshot.
    /// 通过可靠宿主压缩精确子任务，仅返回已提交规范快照。
    /// `request` carries trusted execution inputs; None means no source needs compaction, errors forbid continuation.
    /// `request` 携带可信执行输入；None 表示无来源需要压缩，错误禁止继续。
    async fn compact_child_task(
        &self,
        request: crate::agent_runtime::prompt_compaction::ChildTaskCompactionRequest,
    ) -> Result<Option<vulcan_core::types::ConversationSnapshot>, String>;
    /// Durably reserves initial or resumed execution before publishing child history and admitting its listener.
    /// 在发布子历史及准入监听器之前可靠预留初始或续跑执行。
    /// Trusted runtime preparation and invocation bind the original execution; returns after host handoff.
    /// 可信运行时准备与调用绑定原执行；宿主交接后返回。
    async fn create_child_task(
        &self,
        reservation: ChildTaskReservationRequest,
        preparation: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String>;
    /// Persists trusted child preparation configuration before any provider request begins.
    /// 在任何 Provider 请求开始前持久化可信子准备配置。
    /// The request comes from TaskRuntime; this operation is not exposed as a model tool.
    /// 请求来自 TaskRuntime；此操作不作为模型工具暴露。
    async fn capture_child_execution_profile(
        &self,
        request: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest,
    ) -> Result<(), String>;
    /// Executes one typed collaboration command with the complete trusted invocation identity.
    /// 使用完整可信调用身份执行一条类型化协作命令。
    async fn invoke(
        &self,
        context: VulcanToolInvocationContext,
        command: AgentCollaborationCommand,
    ) -> Result<Value, String>;

    /// Waits for an incoming question that can interrupt a legacy wait_agent call.
    /// 等待能够打断旧 wait_agent 调用的传入问题。
    async fn wait_for_incoming_question(
        &self,
        context: VulcanToolInvocationContext,
    ) -> Result<Value, String>;

    /// Reserves one child task before its provider launch can escape to the host.
    /// 在子任务 Provider 启动事件逸出到宿主前预留一条子任务。
    async fn reserve_child_task(
        &self,
        request: ChildTaskReservationRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String>;

    /// Activates one previously reserved child task after launch admission.
    /// 在启动准入后激活一条已经预留的子任务。
    async fn activate_child_task(
        &self,
        request: ChildTaskActivationRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String>;

    /// Records one explicit terminal outcome for a reserved or active child task.
    /// 为已预留或活动中的子任务记录一个明确终态结果。
    async fn terminal_child_task(
        &self,
        request: ChildTaskTerminalRequest,
    ) -> Result<ChildTaskLifecycleReceipt, String>;
}

/// Builds all model-visible native agent-collaboration tool candidates for one host port.
/// 根据一个宿主端口构建全部模型可见原生代理协作工具候选。
pub fn build_agent_collaboration_tool_candidates(
    runtime: Arc<dyn NativeAgentCollaborationRuntime>,
) -> Vec<VulcanToolCandidate> {
    vec![
        build_command_candidate(
            runtime.clone(),
            ASK_QUESTION_TOOL_ID,
            "Ask an authorized agent session a question. Return the durable question id and delivery acknowledgement immediately; do not wait for the answer.",
            ask_question_args_schema(),
            VulcanPlanToolPolicy::Visible,
            parse_ask_question,
        ),
        build_command_candidate(
            runtime.clone(),
            ASK_PARENT_TOOL_ID,
            "Ask the current agent's real parent session a question and return immediately with its durable question id; do not wait for the answer.",
            ask_parent_args_schema(),
            VulcanPlanToolPolicy::Visible,
            parse_ask_parent,
        ),
        build_command_candidate(
            runtime.clone(),
            REPLY_QUESTION_TOOL_ID,
            "Submit an answer or an explicit unable-to-answer result for one received agent question. Return only the durable submission acknowledgement; a final response is not an answer.",
            reply_question_args_schema(),
            VulcanPlanToolPolicy::Visible,
            parse_reply_question,
        ),
        build_command_candidate(
            runtime.clone(),
            WAIT_QUESTION_TOOL_ID,
            "Wait for an authorized agent question revision. Timeout ends this wait only and does not cancel the question.",
            wait_question_args_schema(),
            VulcanPlanToolPolicy::Visible,
            parse_wait_question,
        ),
        build_command_candidate(
            runtime.clone(),
            CANCEL_QUESTION_TOOL_ID,
            "Cancel one question transaction owned by the current agent and return its authoritative settlement.",
            cancel_question_args_schema(),
            VulcanPlanToolPolicy::Visible,
            parse_cancel_question,
        ),
        build_command_candidate(
            runtime.clone(),
            LIST_SESSIONS_TOOL_ID,
            "List existing agent sessions authorized for collaboration.",
            list_sessions_args_schema(),
            VulcanPlanToolPolicy::Visible,
            parse_list_sessions,
        ),
        build_command_candidate(
            runtime.clone(),
            READ_SESSION_TOOL_ID,
            "Read a bounded event range from one authorized agent session.",
            read_session_args_schema(),
            VulcanPlanToolPolicy::Visible,
            parse_read_session,
        ),
        build_command_candidate(
            runtime.clone(),
            SEND_SESSION_MESSAGE_TOOL_ID,
            "Send a queue or interrupt message to one authorized agent session. Queue requests a separate follow-up task; interrupt guides the exact active business turn without cancelling it. Returns stable operation and delivery IDs immediately. Accepted means the target recorded the intent, not that work started, guidance entered a request, or the task completed.",
            send_session_message_args_schema(),
            VulcanPlanToolPolicy::Hidden,
            parse_send_session_message,
        ),
        build_command_candidate(
            runtime.clone(),
            SEND_NOTIFICATION_TOOL_ID,
            "Send a fire-and-forget notification to another authorized session, including the current agent's parent. This returns immediately, does not start or steer a task, does not create a question, and expects no reply.",
            send_notification_args_schema(),
            VulcanPlanToolPolicy::Visible,
            parse_send_notification,
        ),
        build_command_candidate(
            runtime,
            WAIT_SESSION_TOOL_ID,
            "Wait for a published revision from one authorized agent session. A changed revision is not task completion; use read_session with your event cursor to inspect new facts. An incoming question returns waited_session_id and the original after_revision so you can resume waiting. Timeout ends this wait only and does not cancel the session task.",
            wait_session_args_schema(),
            VulcanPlanToolPolicy::Visible,
            parse_wait_session,
        ),
    ]
}

/// Validates command fields before they cross into the authoritative service port.
/// 在命令进入权威服务端口前校验命令字段。
impl AgentCollaborationCommand {
    /// Validates identity, text, cursor, count, and timeout bounds for this command.
    /// 校验此命令的身份、文本、游标、数量与超时边界。
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::AskQuestion(args) => {
                validate_session_id(&args.target_session_id, ASK_QUESTION_TOOL_ID)?;
                validate_text(&args.question, "question", ASK_QUESTION_TOOL_ID)
            }
            Self::AskParent(args) => validate_text(&args.question, "question", ASK_PARENT_TOOL_ID),
            Self::ReplyQuestion(args) => {
                validate_id(&args.question_id, "question_id", REPLY_QUESTION_TOOL_ID)?;
                validate_text(&args.answer, "answer", REPLY_QUESTION_TOOL_ID)
            }
            Self::WaitQuestion(args) => {
                validate_id(&args.question_id, "question_id", WAIT_QUESTION_TOOL_ID)?;
                validate_timeout(args.timeout_ms, WAIT_QUESTION_TOOL_ID)
            }
            Self::CancelQuestion(args) => {
                validate_id(&args.question_id, "question_id", CANCEL_QUESTION_TOOL_ID)?;
                validate_text(&args.reason, "reason", CANCEL_QUESTION_TOOL_ID)
            }
            Self::ListSessions(args) => {
                validate_limit(args.limit, LIST_SESSIONS_TOOL_ID)?;
                validate_cursor(args.cursor.as_deref(), LIST_SESSIONS_TOOL_ID)
            }
            Self::ReadSession(args) => {
                validate_session_id(&args.session_id, READ_SESSION_TOOL_ID)?;
                validate_limit(args.limit, READ_SESSION_TOOL_ID)
            }
            Self::SendSessionMessage(args) => {
                validate_session_id(&args.session_id, SEND_SESSION_MESSAGE_TOOL_ID)?;
                validate_text(&args.message, "message", SEND_SESSION_MESSAGE_TOOL_ID)
            }
            Self::SendNotification(args) => {
                validate_session_id(&args.target_session_id, SEND_NOTIFICATION_TOOL_ID)?;
                validate_text(&args.message, "message", SEND_NOTIFICATION_TOOL_ID)
            }
            Self::WaitSession(args) => {
                validate_session_id(&args.session_id, WAIT_SESSION_TOOL_ID)?;
                validate_timeout(args.timeout_ms, WAIT_SESSION_TOOL_ID)
            }
        }
    }
}

/// Builds one typed collaboration candidate with strict argument parsing.
/// 使用严格参数解析构建一个类型化协作工具候选。
fn build_command_candidate(
    runtime: Arc<dyn NativeAgentCollaborationRuntime>,
    tool_id: &'static str,
    description: &'static str,
    parameters_schema: Value,
    plan_policy: VulcanPlanToolPolicy,
    parser: fn(Value, &'static str) -> Result<AgentCollaborationCommand, String>,
) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor::new(
            tool_id,
            tool_id,
            description,
            None,
            VulcanToolOrigin::new(VulcanToolSourceKind::Native, "native", tool_id),
        ),
        parameters_schema,
        ai_visible: true,
        plan_policy,
        request_streaming: None,
        invoke: Arc::new(move |arguments, context| {
            let runtime = runtime.clone();
            Box::pin(async move {
                let command = parser(arguments, tool_id)?;
                let output = runtime.invoke(context, command).await?;
                Ok(VulcanToolInvocationResult::from_model_output(output))
            })
        }),
    }
}

/// Parses one strict JSON command argument object.
/// 解析一个严格的 JSON 命令参数对象。
fn parse_command<T>(
    arguments: Value,
    tool_id: &'static str,
    wrap: fn(T) -> AgentCollaborationCommand,
) -> Result<AgentCollaborationCommand, String>
where
    T: for<'de> Deserialize<'de>,
{
    let typed = serde_json::from_value(arguments)
        .map_err(|error| format!("{tool_id} received invalid arguments: {error}."))?;
    let command = wrap(typed);
    command.validate()?;
    Ok(command)
}

/// Parses ask_question arguments.
/// 解析 ask_question 参数。
fn parse_ask_question(
    arguments: Value,
    tool_id: &'static str,
) -> Result<AgentCollaborationCommand, String> {
    parse_command(arguments, tool_id, AgentCollaborationCommand::AskQuestion)
}

/// Parses ask_parent arguments.
/// 解析 ask_parent 参数。
fn parse_ask_parent(
    arguments: Value,
    tool_id: &'static str,
) -> Result<AgentCollaborationCommand, String> {
    parse_command(arguments, tool_id, AgentCollaborationCommand::AskParent)
}

/// Parses reply_question arguments.
/// 解析 reply_question 参数。
fn parse_reply_question(
    arguments: Value,
    tool_id: &'static str,
) -> Result<AgentCollaborationCommand, String> {
    parse_command(arguments, tool_id, AgentCollaborationCommand::ReplyQuestion)
}

/// Parses wait_question arguments.
/// 解析 wait_question 参数。
fn parse_wait_question(
    arguments: Value,
    tool_id: &'static str,
) -> Result<AgentCollaborationCommand, String> {
    parse_command(arguments, tool_id, AgentCollaborationCommand::WaitQuestion)
}

/// Parses cancel_question arguments.
/// 解析 cancel_question 参数。
fn parse_cancel_question(
    arguments: Value,
    tool_id: &'static str,
) -> Result<AgentCollaborationCommand, String> {
    parse_command(
        arguments,
        tool_id,
        AgentCollaborationCommand::CancelQuestion,
    )
}

/// Parses list_sessions arguments.
/// 解析 list_sessions 参数。
fn parse_list_sessions(
    arguments: Value,
    tool_id: &'static str,
) -> Result<AgentCollaborationCommand, String> {
    parse_command(arguments, tool_id, AgentCollaborationCommand::ListSessions)
}

/// Parses read_session arguments.
/// 解析 read_session 参数。
fn parse_read_session(
    arguments: Value,
    tool_id: &'static str,
) -> Result<AgentCollaborationCommand, String> {
    parse_command(arguments, tool_id, AgentCollaborationCommand::ReadSession)
}

/// Parses send_session_message arguments.
/// 解析 send_session_message 参数。
fn parse_send_session_message(
    arguments: Value,
    tool_id: &'static str,
) -> Result<AgentCollaborationCommand, String> {
    parse_command(
        arguments,
        tool_id,
        AgentCollaborationCommand::SendSessionMessage,
    )
}

/// Parses send_notification arguments.
/// 解析 send_notification 参数。
fn parse_send_notification(
    arguments: Value,
    tool_id: &'static str,
) -> Result<AgentCollaborationCommand, String> {
    parse_command(
        arguments,
        tool_id,
        AgentCollaborationCommand::SendNotification,
    )
}

/// Parses wait_session arguments.
/// 解析 wait_session 参数。
fn parse_wait_session(
    arguments: Value,
    tool_id: &'static str,
) -> Result<AgentCollaborationCommand, String> {
    parse_command(arguments, tool_id, AgentCollaborationCommand::WaitSession)
}

/// Validates a non-empty exact identifier.
/// 校验一个非空精确标识。
fn validate_id(value: &str, field: &str, tool_id: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{tool_id} requires a non-empty {field}."));
    }
    Ok(())
}

/// Validates a session identifier field.
/// 校验会话标识字段。
fn validate_session_id(value: &str, tool_id: &str) -> Result<(), String> {
    validate_id(value, "session_id", tool_id)
}

/// Validates bounded non-empty collaboration text.
/// 校验有界非空协作文本。
fn validate_text(value: &str, field: &str, tool_id: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{tool_id} requires a non-empty {field}."));
    }
    if value.chars().count() > MAX_AGENT_COLLABORATION_TEXT_CHARS {
        return Err(format!(
            "{tool_id} {field} exceeds the maximum length of {MAX_AGENT_COLLABORATION_TEXT_CHARS} characters."
        ));
    }
    Ok(())
}

/// Validates a bounded pagination cursor.
/// 校验有界分页游标。
fn validate_cursor(value: Option<&str>, tool_id: &str) -> Result<(), String> {
    if let Some(cursor) = value {
        if cursor.chars().count() > MAX_AGENT_COLLABORATION_CURSOR_CHARS {
            return Err(format!(
                "{tool_id} cursor exceeds the maximum length of {MAX_AGENT_COLLABORATION_CURSOR_CHARS} characters."
            ));
        }
    }
    Ok(())
}

/// Validates a bounded positive page size.
/// 校验有界正数分页大小。
fn validate_limit(value: u32, tool_id: &str) -> Result<(), String> {
    if value == 0 || value > MAX_AGENT_COLLABORATION_LIMIT {
        return Err(format!(
            "{tool_id} limit must be between 1 and {MAX_AGENT_COLLABORATION_LIMIT}."
        ));
    }
    Ok(())
}

/// Validates a bounded finite wait timeout.
/// 校验有界有限等待超时。
fn validate_timeout(value: u64, tool_id: &str) -> Result<(), String> {
    if !(MIN_AGENT_COLLABORATION_TIMEOUT_MS..=MAX_AGENT_COLLABORATION_TIMEOUT_MS).contains(&value) {
        return Err(format!(
            "{tool_id} timeout_ms must be between {MIN_AGENT_COLLABORATION_TIMEOUT_MS} and {MAX_AGENT_COLLABORATION_TIMEOUT_MS}."
        ));
    }
    Ok(())
}

/// Builds the ask_question JSON Schema.
/// 构建 ask_question JSON Schema。
fn ask_question_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "target_session_id": { "type": "string", "minLength": 1 },
            "question": { "type": "string", "minLength": 1, "maxLength": MAX_AGENT_COLLABORATION_TEXT_CHARS }
        },
        "required": ["target_session_id", "question"],
        "additionalProperties": false
    })
}

/// Builds the ask_parent JSON Schema.
/// 构建 ask_parent JSON Schema。
fn ask_parent_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "question": { "type": "string", "minLength": 1, "maxLength": MAX_AGENT_COLLABORATION_TEXT_CHARS }
        },
        "required": ["question"],
        "additionalProperties": false
    })
}

/// Builds the reply_question JSON Schema.
/// 构建 reply_question JSON Schema。
fn reply_question_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "question_id": { "type": "string", "minLength": 1 },
            "answer": { "type": "string", "minLength": 1, "maxLength": MAX_AGENT_COLLABORATION_TEXT_CHARS },
            "unable_to_answer": { "type": "boolean", "default": false }
        },
        "required": ["question_id", "answer"],
        "additionalProperties": false
    })
}

/// Builds the wait_question JSON Schema.
/// 构建 wait_question JSON Schema。
fn wait_question_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "question_id": { "type": "string", "minLength": 1 },
            "after_revision": { "type": "integer", "minimum": 0 },
            "timeout_ms": { "type": "integer", "minimum": MIN_AGENT_COLLABORATION_TIMEOUT_MS, "maximum": MAX_AGENT_COLLABORATION_TIMEOUT_MS }
        },
        "required": ["question_id", "timeout_ms"],
        "additionalProperties": false
    })
}

/// Builds the cancel_question JSON Schema.
/// 构建 cancel_question JSON Schema。
fn cancel_question_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "question_id": { "type": "string", "minLength": 1 },
            "reason": { "type": "string", "minLength": 1, "maxLength": MAX_AGENT_COLLABORATION_TEXT_CHARS }
        },
        "required": ["question_id", "reason"],
        "additionalProperties": false
    })
}

/// Builds the list_sessions JSON Schema.
/// 构建 list_sessions JSON Schema。
fn list_sessions_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "limit": { "type": "integer", "minimum": 1, "maximum": MAX_AGENT_COLLABORATION_LIMIT },
            "cursor": { "type": "string", "maxLength": MAX_AGENT_COLLABORATION_CURSOR_CHARS }
        },
        "required": ["limit"],
        "additionalProperties": false
    })
}

/// Builds the read_session JSON Schema.
/// 构建 read_session JSON Schema。
fn read_session_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "minLength": 1 },
            "after_seq": { "type": "integer", "minimum": 0 },
            "limit": { "type": "integer", "minimum": 1, "maximum": MAX_AGENT_COLLABORATION_LIMIT }
        },
        "required": ["session_id", "limit"],
        "additionalProperties": false
    })
}

/// Builds the send_session_message JSON Schema.
/// 构建 send_session_message JSON Schema。
fn send_session_message_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "minLength": 1 },
            "message": { "type": "string", "minLength": 1, "maxLength": MAX_AGENT_COLLABORATION_TEXT_CHARS },
            "delivery_mode": { "type": "string", "enum": ["queue", "interrupt"] }
        },
        "required": ["session_id", "message", "delivery_mode"],
        "additionalProperties": false
    })
}

/// Builds the send_notification JSON Schema.
/// 构建 send_notification JSON Schema。
fn send_notification_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "target_session_id": { "type": "string", "minLength": 1 },
            "message": { "type": "string", "minLength": 1, "maxLength": MAX_AGENT_COLLABORATION_TEXT_CHARS }
        },
        "required": ["target_session_id", "message"],
        "additionalProperties": false
    })
}

/// Builds the wait_session JSON Schema.
/// 构建 wait_session JSON Schema。
fn wait_session_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "minLength": 1 },
            "after_revision": { "type": "integer", "minimum": 0 },
            "timeout_ms": { "type": "integer", "minimum": MIN_AGENT_COLLABORATION_TIMEOUT_MS, "maximum": MAX_AGENT_COLLABORATION_TIMEOUT_MS }
        },
        "required": ["session_id", "timeout_ms"],
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FakeCollaborationRuntime;

    #[async_trait::async_trait]
    impl NativeAgentCollaborationRuntime for FakeCollaborationRuntime {
        /// Rejects unexpected compaction in a tool-surface fixture; no snapshot is returned.
        /// 在工具表面夹具中拒绝非预期压缩；不返回快照。
        async fn compact_child_task(
            &self,
            _: crate::agent_runtime::prompt_compaction::ChildTaskCompactionRequest,
        ) -> Result<Option<vulcan_core::types::ConversationSnapshot>, String> {
            Err(String::from("Unexpected child compaction in test host."))
        }
        /// Rejects unexpected initial creation in this narrowly scoped test host.
        /// 在此限定范围的测试宿主中拒绝非预期初始创建。
        async fn create_child_task(
            &self,
            _: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskReservationRequest,
            _: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest,
        ) -> Result<
            crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildTaskLifecycleReceipt,
            String,
        > {
            Err(String::from("Unexpected initial child creation."))
        }
        /// Accepts host-only capture for this tool-surface fixture.
        /// 为此工具表面夹具接纳仅宿主配置捕获。
        async fn capture_child_execution_profile(
            &self,
            _request: crate::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest,
        ) -> Result<(), String> {
            Ok(())
        }
        /// Returns an empty synthetic result for candidate construction tests.
        /// 为候选工具构建测试返回空的合成结果。
        async fn invoke(
            &self,
            _context: VulcanToolInvocationContext,
            _command: AgentCollaborationCommand,
        ) -> Result<Value, String> {
            Ok(Value::Null)
        }

        /// Returns an empty synthetic result for wait wrapper tests.
        /// 为等待包装测试返回空的合成结果。
        async fn wait_for_incoming_question(
            &self,
            _context: VulcanToolInvocationContext,
        ) -> Result<Value, String> {
            Ok(Value::Null)
        }

        /// Rejects child-task reservation because the fake runtime has no host owner.
        /// 由于合成运行时没有宿主所有者，拒绝子任务预留。
        async fn reserve_child_task(
            &self,
            _request: ChildTaskReservationRequest,
        ) -> Result<ChildTaskLifecycleReceipt, String> {
            Err(String::from(
                "fake collaboration runtime does not implement child-task reservation",
            ))
        }

        /// Rejects child-task activation because the fake runtime has no host owner.
        /// 由于合成运行时没有宿主所有者，拒绝子任务激活。
        async fn activate_child_task(
            &self,
            _request: ChildTaskActivationRequest,
        ) -> Result<ChildTaskLifecycleReceipt, String> {
            Err(String::from(
                "fake collaboration runtime does not implement child-task activation",
            ))
        }

        /// Rejects child-task terminalization because the fake runtime has no host owner.
        /// 由于合成运行时没有宿主所有者，拒绝子任务终态提交。
        async fn terminal_child_task(
            &self,
            _request: ChildTaskTerminalRequest,
        ) -> Result<ChildTaskLifecycleReceipt, String> {
            Err(String::from(
                "fake collaboration runtime does not implement child-task terminalization",
            ))
        }
    }

    /// Verifies each argument struct rejects unknown fields through serde.
    /// 验证每个参数结构体通过 serde 拒绝未知字段。
    #[test]
    fn collaboration_arguments_reject_unknown_fields() {
        let error = serde_json::from_value::<AskQuestionArgs>(json!({
            "target_session_id": "session",
            "question": "question",
            "source_session_id": "forged"
        }))
        .expect_err("unknown source identity must be rejected");
        assert!(error.to_string().contains("unknown field"));

        let error = serde_json::from_value::<SendNotificationArgs>(json!({
            "target_session_id": "session",
            "message": "notice",
            "source_session_id": "forged"
        }))
        .expect_err("unknown source identity must be rejected");
        assert!(error.to_string().contains("unknown field"));
    }

    /// Verifies typed command validation enforces bounded pagination and waiting.
    /// 验证类型化命令校验强制执行分页与等待边界。
    #[test]
    fn collaboration_commands_validate_bounded_fields() {
        let command = AgentCollaborationCommand::ListSessions(ListSessionsArgs {
            limit: MAX_AGENT_COLLABORATION_LIMIT + 1,
            cursor: None,
        });
        assert!(command.validate().is_err());

        let command = AgentCollaborationCommand::WaitSession(WaitSessionArgs {
            session_id: String::from("session"),
            after_revision: None,
            timeout_ms: MAX_AGENT_COLLABORATION_TIMEOUT_MS + 1,
        });
        assert!(command.validate().is_err());
    }

    /// Verifies the delivery enum exposes only queue and interrupt values.
    /// 验证投递枚举只暴露 queue 与 interrupt 值。
    #[test]
    fn delivery_mode_uses_stable_wire_values() {
        assert_eq!(
            serde_json::to_value(SessionMessageDeliveryMode::Queue).unwrap(),
            json!("queue")
        );
        assert_eq!(
            serde_json::to_value(SessionMessageDeliveryMode::Interrupt).unwrap(),
            json!("interrupt")
        );
    }

    /// Verifies the notification command uses its exact tagged wire name and fields.
    /// 验证通知命令使用精确的带标签线协议名称和字段。
    #[test]
    fn notification_command_uses_exact_wire_shape() {
        let command: AgentCollaborationCommand = serde_json::from_value(json!({
            "command": "send_notification",
            "args": {
                "target_session_id": "parent-session",
                "message": "status update"
            }
        }))
        .expect("send_notification wire shape must deserialize");
        assert!(matches!(
            command,
            AgentCollaborationCommand::SendNotification(SendNotificationArgs {
                target_session_id,
                message
            }) if target_session_id == "parent-session" && message == "status update"
        ));
    }

    /// Verifies question and read tools remain visible in Plan mode while session messages stay task-only.
    /// 验证问题与读取工具在 Plan 模式保持可见，而会话消息仍只用于任务执行。
    #[test]
    fn collaboration_plan_visibility_preserves_question_surface() {
        let candidates =
            build_agent_collaboration_tool_candidates(Arc::new(FakeCollaborationRuntime));
        assert_eq!(candidates.len(), 10);
        for candidate in candidates {
            if candidate.descriptor.id == SEND_SESSION_MESSAGE_TOOL_ID {
                assert!(matches!(
                    candidate.plan_policy,
                    VulcanPlanToolPolicy::Hidden
                ));
            } else {
                assert!(matches!(
                    candidate.plan_policy,
                    VulcanPlanToolPolicy::Visible
                ));
            }
        }
    }
}
