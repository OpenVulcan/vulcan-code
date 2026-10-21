//! MCP tool catalog: names, descriptions, input schemas, and the grant each tool needs.
//! MCP 工具目录：名称、说明、输入 Schema 以及每个工具所需的授权。

use serde_json::{Value, json};
use vulcan_provider_config::provider_config::McpServiceClient;

use crate::{
    MCP_DEFAULT_READ_LIMIT, MCP_DEFAULT_WAIT_SECONDS, MCP_MAX_READ_LIMIT, MCP_MAX_WAIT_SECONDS,
};

/// Model-extension capabilities MCP clients may be granted: image and video generation and editing.
/// MCP 客户端可被授予的模型扩展能力：图片与视频的生成和编辑。
pub const MCP_MEDIA_CAPABILITIES: [&str; 5] = [
    "image_generate",
    "image_edit",
    "video_generate",
    "video_edit",
    "video_extend",
];

/// Grant a tool requires.
/// 工具所需的授权。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolGrant {
    /// Session read access.
    /// 会话读取权限。
    Read,
    /// Sending to existing sessions.
    /// 向已有会话发送。
    Send,
    /// Creating sessions.
    /// 新建会话。
    Create,
    /// Stopping sessions.
    /// 停止会话。
    Stop,
    /// Answering questions.
    /// 回答提问。
    Answer,
    /// At least one media target.
    /// 至少一个媒体目标。
    Media,
}

impl ToolGrant {
    /// Returns whether `client` holds this grant.
    /// 判断 `client` 是否持有该授权。
    pub(crate) fn allows(self, client: &McpServiceClient) -> bool {
        let sessions = &client.sessions;
        match self {
            Self::Read => sessions.read,
            Self::Send => sessions.read && sessions.send,
            Self::Create => sessions.read && sessions.create,
            Self::Stop => sessions.read && sessions.stop,
            Self::Answer => sessions.read && sessions.answer_questions,
            Self::Media => !client.media_targets.is_empty(),
        }
    }
}

/// Stable tool identifiers.
/// 稳定工具标识。
pub(crate) mod names {
    pub(crate) const LIST_PROJECTS: &str = "list_projects";
    pub(crate) const LIST_SESSIONS: &str = "list_sessions";
    pub(crate) const GET_SESSION: &str = "get_session";
    pub(crate) const READ_MESSAGES: &str = "read_messages";
    pub(crate) const WAIT_FOR_REPLY: &str = "wait_for_reply";
    pub(crate) const LIST_CHAT_MODELS: &str = "list_chat_models";
    pub(crate) const SEND_MESSAGE: &str = "send_message";
    pub(crate) const CREATE_SESSION: &str = "create_session";
    pub(crate) const STOP_SESSION: &str = "stop_session";
    pub(crate) const ANSWER_QUESTION: &str = "answer_question";
    pub(crate) const LIST_MEDIA_MODELS: &str = "list_media_models";
    pub(crate) const GENERATE_MEDIA: &str = "generate_media";
    pub(crate) const GET_MEDIA_JOB: &str = "get_media_job";
    pub(crate) const CANCEL_MEDIA_JOB: &str = "cancel_media_job";
}

/// One catalog entry.
/// 一个目录条目。
pub(crate) struct ToolSpec {
    /// Stable name.
    /// 稳定名称。
    pub(crate) name: &'static str,
    /// Short human title.
    /// 简短人类标题。
    pub(crate) title: &'static str,
    /// Required grant.
    /// 所需授权。
    pub(crate) grant: ToolGrant,
    /// Whether the tool only reads state.
    /// 工具是否只读取状态。
    pub(crate) read_only: bool,
    /// Whether the tool can discard work.
    /// 工具是否可能丢弃工作。
    pub(crate) destructive: bool,
    /// Whether the tool reaches paid external providers.
    /// 工具是否访问付费外部供应商。
    pub(crate) open_world: bool,
}

/// Every tool in display order.
/// 按显示顺序排列的全部工具。
pub(crate) const TOOL_SPECS: [ToolSpec; 14] = [
    spec(
        names::LIST_PROJECTS,
        "List projects",
        ToolGrant::Read,
        true,
        false,
        false,
    ),
    spec(
        names::LIST_SESSIONS,
        "List sessions",
        ToolGrant::Read,
        true,
        false,
        false,
    ),
    spec(
        names::GET_SESSION,
        "Get session status",
        ToolGrant::Read,
        true,
        false,
        false,
    ),
    spec(
        names::READ_MESSAGES,
        "Read session messages",
        ToolGrant::Read,
        true,
        false,
        false,
    ),
    spec(
        names::WAIT_FOR_REPLY,
        "Wait for a reply",
        ToolGrant::Read,
        true,
        false,
        false,
    ),
    spec(
        names::LIST_CHAT_MODELS,
        "List chat models",
        ToolGrant::Read,
        true,
        false,
        false,
    ),
    spec(
        names::SEND_MESSAGE,
        "Send a message",
        ToolGrant::Send,
        false,
        false,
        true,
    ),
    spec(
        names::CREATE_SESSION,
        "Create a session",
        ToolGrant::Create,
        false,
        false,
        true,
    ),
    spec(
        names::STOP_SESSION,
        "Stop a session",
        ToolGrant::Stop,
        false,
        true,
        false,
    ),
    spec(
        names::ANSWER_QUESTION,
        "Answer a question",
        ToolGrant::Answer,
        false,
        false,
        false,
    ),
    spec(
        names::LIST_MEDIA_MODELS,
        "List media models",
        ToolGrant::Media,
        true,
        false,
        false,
    ),
    spec(
        names::GENERATE_MEDIA,
        "Generate media",
        ToolGrant::Media,
        false,
        false,
        true,
    ),
    spec(
        names::GET_MEDIA_JOB,
        "Get a media job",
        ToolGrant::Media,
        true,
        false,
        false,
    ),
    spec(
        names::CANCEL_MEDIA_JOB,
        "Cancel a media job",
        ToolGrant::Media,
        false,
        true,
        false,
    ),
];

/// Builds one catalog entry in a constant context.
/// 在常量上下文中构建一个目录条目。
const fn spec(
    name: &'static str,
    title: &'static str,
    grant: ToolGrant,
    read_only: bool,
    destructive: bool,
    open_world: bool,
) -> ToolSpec {
    ToolSpec {
        name,
        title,
        grant,
        read_only,
        destructive,
        open_world,
    }
}

/// Finds a tool by name.
/// 按名称查找工具。
pub(crate) fn tool_spec(name: &str) -> Option<&'static ToolSpec> {
    TOOL_SPECS.iter().find(|spec| spec.name == name)
}

/// Returns the `tools/list` entries `client` may call.
/// 返回 `client` 可调用的 `tools/list` 条目。
pub(crate) fn tool_definitions(client: &McpServiceClient) -> Vec<Value> {
    TOOL_SPECS
        .iter()
        .filter(|spec| spec.grant.allows(client))
        .map(|spec| {
            json!({
                "name": spec.name,
                "title": spec.title,
                "description": tool_description(spec.name),
                "inputSchema": tool_input_schema(spec.name),
                "annotations": {
                    "title": spec.title,
                    "readOnlyHint": spec.read_only,
                    "destructiveHint": spec.destructive,
                    "idempotentHint": spec.read_only,
                    "openWorldHint": spec.open_world,
                },
            })
        })
        .collect()
}

/// Shared description of the waiting behaviour.
/// 等待行为的共享说明。
fn wait_note() -> String {
    format!(
        "`wait_seconds` (default {MCP_DEFAULT_WAIT_SECONDS}, max {MCP_MAX_WAIT_SECONDS}, 0 = do not wait) bounds how long this call blocks. Keep it below your own tool-call timeout; when the result says `running`, call `wait_for_reply` again with the returned `next_seq`."
    )
}

/// Returns the English description of one tool.
/// 返回一个工具的英文说明。
fn tool_description(name: &str) -> String {
    match name {
        names::LIST_PROJECTS => String::from(
            "List the Vulcan Code projects this client may use, with their ids and workspace directories. Use a project id to create a session.",
        ),
        names::LIST_SESSIONS => String::from(
            "List active Vulcan Code conversations (sessions) newest first, with run phase and whether one waits for an answer. Filter by project or by running state.",
        ),
        names::GET_SESSION => String::from(
            "Get one session's live status: run phase, pending questions (including permission approvals that only the user can decide), queued prompts, and the current `next_seq` event cursor.",
        ),
        names::READ_MESSAGES => format!(
            "Read a session's transcript after an event cursor: user messages, assistant replies, tool calls, turn ends, and notices. Omit `after_seq` to read the most recent {MCP_DEFAULT_READ_LIMIT} events; pass the returned `next_seq` to continue."
        ),
        names::WAIT_FOR_REPLY => format!(
            "Block until a session finishes its current work, asks a question, or the wait ends, then return the reply and new messages after `after_seq`. {}",
            wait_note()
        ),
        names::LIST_CHAT_MODELS => String::from(
            "List the chat models Vulcan Code sessions can run, with provider, model, and account ids to pass as `model` and `account` when creating a session or sending a message.",
        ),
        names::SEND_MESSAGE => format!(
            "Send a message to an existing session as the user would. A running session queues it. Returns the reply once the session finishes, asks a question, or the wait ends. Permission approvals stay with the user in Vulcan Code. {}",
            wait_note()
        ),
        names::CREATE_SESSION => format!(
            "Create a new session in a project with its first message, optionally choosing a model from `list_chat_models` and plan mode. Returns the new session id and its reply. {}",
            wait_note()
        ),
        names::STOP_SESSION => String::from(
            "Stop a session's running turn. Work already done stays in the transcript; the unfinished part of the turn is discarded.",
        ),
        names::ANSWER_QUESTION => String::from(
            "Answer a question a session asked (from `get_session` or a wait result), one list of answers per question, or dismiss it with `dismiss: true`. Permission approvals cannot be answered here; they belong to the user.",
        ),
        names::LIST_MEDIA_MODELS => String::from(
            "List the image and video models this client may run. Pass `model` to get that model's reviewed constraints and exact `arguments` schema; always do this before `generate_media`.",
        ),
        names::GENERATE_MEDIA => format!(
            "Run an image or video model with its exact model-native `arguments` from `list_media_models`. Media-source fields accept absolute local file paths. Outputs are saved as local files whose paths are returned; images are also returned inline unless `include_images` is false. Generation is paid and may take minutes: if it is not done within `wait_seconds`, a `job_id` is returned for `get_media_job`. `wait_seconds` defaults to {MCP_DEFAULT_WAIT_SECONDS} (max {MCP_MAX_WAIT_SECONDS})."
        ),
        names::GET_MEDIA_JOB => String::from(
            "Get or wait for a media job started by `generate_media`, returning its files when it succeeded.",
        ),
        names::CANCEL_MEDIA_JOB => String::from(
            "Cancel a running media job. Providers may still bill work that already started.",
        ),
        _ => String::new(),
    }
}

/// Schema of the shared `wait_seconds` field.
/// 共享 `wait_seconds` 字段的 Schema。
fn wait_seconds_schema() -> Value {
    json!({
        "type": "integer",
        "minimum": 0,
        "maximum": MCP_MAX_WAIT_SECONDS,
        "description": "Seconds to wait for completion; 0 returns immediately."
    })
}

/// Schema of the shared `session_id` field.
/// 共享 `session_id` 字段的 Schema。
fn session_id_schema() -> Value {
    json!({ "type": "string", "minLength": 1, "description": "Session id from list_sessions or create_session." })
}

/// Schema of the shared model selection fields.
/// 共享模型选择字段的 Schema。
fn model_properties() -> (Value, Value, Value) {
    (
        json!({ "type": "string", "minLength": 3, "description": "Optional `provider_id/model_id` from list_chat_models; omitted keeps the session's model or the default." }),
        json!({ "type": "string", "minLength": 1, "description": "Optional account (connection id) from list_chat_models when the model is offered by several accounts." }),
        json!({ "type": "string", "minLength": 1, "description": "Optional reasoning effort supported by the chosen model, such as `low`, `medium`, or `high`." }),
    )
}

/// Returns the strict input schema of one tool.
/// 返回一个工具的严格输入 Schema。
fn tool_input_schema(name: &str) -> Value {
    let (model, account, reasoning) = model_properties();
    match name {
        names::LIST_PROJECTS | names::LIST_CHAT_MODELS => {
            json!({ "type": "object", "properties": {}, "additionalProperties": false })
        }
        names::LIST_SESSIONS => json!({
            "type": "object",
            "properties": {
                "project_id": { "type": "string", "minLength": 1, "description": "Only sessions of this project." },
                "running_only": { "type": "boolean", "description": "Only sessions that are currently working." },
                "limit": { "type": "integer", "minimum": 1, "maximum": MCP_MAX_READ_LIMIT, "description": "Maximum sessions returned." }
            },
            "additionalProperties": false
        }),
        names::GET_SESSION | names::STOP_SESSION => json!({
            "type": "object",
            "properties": { "session_id": session_id_schema() },
            "required": ["session_id"],
            "additionalProperties": false
        }),
        names::READ_MESSAGES => json!({
            "type": "object",
            "properties": {
                "session_id": session_id_schema(),
                "after_seq": { "type": "integer", "minimum": 0, "description": "Return events after this sequence number; omit for the latest events." },
                "limit": { "type": "integer", "minimum": 1, "maximum": MCP_MAX_READ_LIMIT, "description": "Maximum events scanned." },
                "include_tool_output": { "type": "boolean", "description": "Include a bounded preview of each tool's output." }
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
        names::WAIT_FOR_REPLY => json!({
            "type": "object",
            "properties": {
                "session_id": session_id_schema(),
                "after_seq": { "type": "integer", "minimum": 0, "description": "Cursor returned by send_message, create_session, or an earlier wait; messages after it are returned." },
                "wait_seconds": wait_seconds_schema()
            },
            "required": ["session_id"],
            "additionalProperties": false
        }),
        names::SEND_MESSAGE => json!({
            "type": "object",
            "properties": {
                "session_id": session_id_schema(),
                "message": { "type": "string", "minLength": 1, "description": "Message text sent as the user." },
                "model": model,
                "account": account,
                "reasoning_effort": reasoning,
                "plan_mode": { "type": "boolean", "description": "Run this turn in plan mode (read-only planning)." },
                "wait_seconds": wait_seconds_schema()
            },
            "required": ["session_id", "message"],
            "additionalProperties": false
        }),
        names::CREATE_SESSION => json!({
            "type": "object",
            "properties": {
                "project_id": { "type": "string", "minLength": 1, "description": "Project id from list_projects." },
                "message": { "type": "string", "minLength": 1, "description": "First message of the session." },
                "model": model,
                "account": account,
                "reasoning_effort": reasoning,
                "plan_mode": { "type": "boolean", "description": "Run the first turn in plan mode (read-only planning)." },
                "wait_seconds": wait_seconds_schema()
            },
            "required": ["project_id", "message"],
            "additionalProperties": false
        }),
        names::ANSWER_QUESTION => json!({
            "type": "object",
            "properties": {
                "session_id": session_id_schema(),
                "request_id": { "type": "string", "minLength": 1, "description": "Pending question request id." },
                "answers": {
                    "type": "array",
                    "items": { "type": "array", "items": { "type": "string" } },
                    "description": "One list of answers per question, in order: option labels or free text."
                },
                "dismiss": { "type": "boolean", "description": "Dismiss the question instead of answering." }
            },
            "required": ["session_id", "request_id"],
            "additionalProperties": false
        }),
        names::LIST_MEDIA_MODELS => json!({
            "type": "object",
            "properties": {
                "capability": { "type": "string", "enum": MCP_MEDIA_CAPABILITIES, "description": "Only models of this capability." },
                "model": { "type": "string", "minLength": 3, "description": "Return this model's constraints and exact arguments schema." }
            },
            "additionalProperties": false
        }),
        names::GENERATE_MEDIA => json!({
            "type": "object",
            "properties": {
                "model": { "type": "string", "minLength": 3, "description": "Exact `provider/model` id from list_media_models." },
                "capability": { "type": "string", "enum": MCP_MEDIA_CAPABILITIES, "description": "Needed only when the model offers several capabilities." },
                "arguments": { "type": "object", "description": "Model-native arguments matching the schema from list_media_models." },
                "include_images": { "type": "boolean", "description": "Return generated images inline (default true)." },
                "wait_seconds": wait_seconds_schema()
            },
            "required": ["model", "arguments"],
            "additionalProperties": false
        }),
        names::GET_MEDIA_JOB => json!({
            "type": "object",
            "properties": {
                "job_id": { "type": "string", "minLength": 1, "description": "Job id returned by generate_media." },
                "include_images": { "type": "boolean", "description": "Return generated images inline (default true)." },
                "wait_seconds": wait_seconds_schema()
            },
            "required": ["job_id"],
            "additionalProperties": false
        }),
        names::CANCEL_MEDIA_JOB => json!({
            "type": "object",
            "properties": { "job_id": { "type": "string", "minLength": 1, "description": "Job id returned by generate_media." } },
            "required": ["job_id"],
            "additionalProperties": false
        }),
        _ => json!({ "type": "object" }),
    }
}
