//! Stateless MCP Streamable HTTP endpoint: JSON-RPC handling, authorization, and tool execution.
//! 无状态 MCP Streamable HTTP 端点：JSON-RPC 处理、授权与工具执行。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use vulcan_provider_config::provider_config::{
    McpProjectScope, McpServiceClient, normalize_mcp_service_configuration,
};

use crate::auth::{McpAuthFailure, authenticate};
use crate::backend::{
    McpChatModel, McpMediaModel, McpMediaOutcome, McpMediaRun, McpModelSelection,
    McpServiceBackend, McpSessionRow, McpSessionStatus, McpSubmission, McpTranscriptEntry,
    McpTranscriptKind,
};
use crate::media_jobs::{MediaJob, MediaJobRegistry, MediaJobState};
use crate::tools::{MCP_MEDIA_CAPABILITIES, names, tool_definitions, tool_spec};
use crate::{
    MCP_DEFAULT_READ_LIMIT, MCP_DEFAULT_WAIT_SECONDS, MCP_INLINE_IMAGE_MAX_BYTES,
    MCP_MAX_READ_LIMIT, MCP_MAX_WAIT_SECONDS, MCP_STATUS_POLL_INTERVAL,
    MCP_SUPPORTED_PROTOCOL_VERSIONS, MCP_WAIT_ACTIVITY_LIMIT,
};

/// JSON-RPC parse error code.
/// JSON-RPC 解析错误码。
const JSONRPC_PARSE_ERROR: i64 = -32700;
/// JSON-RPC invalid request code.
/// JSON-RPC 无效请求错误码。
const JSONRPC_INVALID_REQUEST: i64 = -32600;
/// JSON-RPC unknown method code.
/// JSON-RPC 未知方法错误码。
const JSONRPC_METHOD_NOT_FOUND: i64 = -32601;
/// JSON-RPC invalid params code.
/// JSON-RPC 无效参数错误码。
const JSONRPC_INVALID_PARAMS: i64 = -32602;

/// Plain HTTP answer produced for one request, independent of the HTTP framework.
/// 为一次请求产生的普通 HTTP 应答，独立于 HTTP 框架。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpHttpResponse {
    /// HTTP status code.
    /// HTTP 状态码。
    pub status: u16,
    /// Response headers.
    /// 响应头。
    pub headers: Vec<(String, String)>,
    /// Response body.
    /// 响应体。
    pub body: Vec<u8>,
}

impl McpHttpResponse {
    /// Builds a JSON response.
    /// 构建 JSON 响应。
    fn json(status: u16, value: &Value) -> Self {
        Self {
            status,
            headers: vec![
                (
                    String::from("Content-Type"),
                    String::from("application/json"),
                ),
                (String::from("Cache-Control"), String::from("no-store")),
            ],
            body: serde_json::to_vec(value).unwrap_or_default(),
        }
    }

    /// Builds a plain-text response.
    /// 构建纯文本响应。
    fn text(status: u16, text: &str) -> Self {
        Self {
            status,
            headers: vec![(
                String::from("Content-Type"),
                String::from("text/plain; charset=utf-8"),
            )],
            body: text.as_bytes().to_vec(),
        }
    }

    /// Builds an empty `202 Accepted` response for notifications.
    /// 为通知构建空的 `202 Accepted` 响应。
    fn accepted() -> Self {
        Self {
            status: 202,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }
}

/// Per-client usage since the service started.
/// 本次服务启动以来的每客户端用量。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct McpClientUsage {
    /// Last authenticated request in Unix milliseconds.
    /// 最近一次已认证请求（Unix 毫秒）。
    pub last_used_at_ms: Option<u64>,
    /// Tool calls made.
    /// 已进行的工具调用次数。
    pub call_count: u64,
}

/// The MCP endpoint shared by every HTTP request of one service process.
/// 一个服务进程内所有 HTTP 请求共享的 MCP 端点。
pub struct McpServer {
    /// Service operations.
    /// 服务操作。
    backend: Arc<dyn McpServiceBackend>,
    /// Version reported in `serverInfo`.
    /// 在 `serverInfo` 中报告的版本。
    server_version: String,
    /// Media jobs of every client.
    /// 全部客户端的媒体任务。
    jobs: MediaJobRegistry,
    /// Usage keyed by client identifier.
    /// 按客户端标识索引的用量。
    usage: Mutex<HashMap<String, McpClientUsage>>,
}

/// Tool output before it is wrapped in an MCP `CallToolResult`.
/// 包装为 MCP `CallToolResult` 之前的工具输出。
struct ToolOutput {
    /// Structured result, also serialized as the text block.
    /// 结构化结果，同时序列化为文本块。
    structured: Value,
    /// Inline images as `(mime type, base64 data)`.
    /// 内联图片，形式为 `(MIME 类型, base64 数据)`。
    images: Vec<(String, String)>,
}

impl ToolOutput {
    /// Wraps a structured result without images.
    /// 包装不含图片的结构化结果。
    fn structured(structured: Value) -> Self {
        Self {
            structured,
            images: Vec::new(),
        }
    }
}

impl McpServer {
    /// Creates the endpoint.
    /// 创建端点。
    pub fn new(backend: Arc<dyn McpServiceBackend>, server_version: impl Into<String>) -> Self {
        Self {
            backend,
            server_version: server_version.into(),
            jobs: MediaJobRegistry::default(),
            usage: Mutex::new(HashMap::new()),
        }
    }

    /// Returns the usage of one client since the service started.
    /// 返回一个客户端自服务启动以来的用量。
    pub fn client_usage(&self, client_id: &str) -> McpClientUsage {
        self.usage
            .lock()
            .ok()
            .and_then(|usage| usage.get(client_id).copied())
            .unwrap_or_default()
    }

    /// Cancels and forgets the media jobs of clients that were deleted.
    /// 取消并遗忘已删除客户端的媒体任务。
    pub fn retain_clients(&self, live_client_ids: &[&str]) {
        for job in self.jobs.retain_clients(live_client_ids) {
            self.cancel_job(&job);
        }
        if let Ok(mut usage) = self.usage.lock() {
            usage.retain(|client_id, _| live_client_ids.contains(&client_id.as_str()));
        }
    }

    /// Handles one `POST /mcp` request.
    /// 处理一次 `POST /mcp` 请求。
    ///
    /// # Parameters
    /// 参数
    /// `authorization` is the raw `Authorization` header and `body` the raw request body.
    /// `authorization` 是原始 `Authorization` 头部，`body` 是原始请求体。
    ///
    /// # Returns
    /// 返回值
    /// `503` while disabled, `401` without a valid token, `202` for notifications, otherwise a
    /// JSON-RPC response.
    /// 关闭时返回 `503`；令牌无效时返回 `401`；通知返回 `202`；其余返回 JSON-RPC 响应。
    pub async fn handle_post(
        self: &Arc<Self>,
        authorization: Option<&str>,
        body: &[u8],
    ) -> McpHttpResponse {
        let configuration = match self
            .backend
            .configuration()
            .and_then(normalize_mcp_service_configuration)
        {
            Ok(configuration) => configuration,
            Err(error) => {
                return McpHttpResponse::text(
                    500,
                    &format!("The MCP service configuration is invalid: {error}"),
                );
            }
        };
        let client = match authenticate(&configuration, authorization) {
            Ok(client) => client.clone(),
            Err(McpAuthFailure::ServiceDisabled) => {
                return McpHttpResponse::text(
                    503,
                    "The Vulcan Code MCP service is disabled. Enable it in Settings > MCP service.",
                );
            }
            Err(McpAuthFailure::MissingToken | McpAuthFailure::InvalidToken) => {
                let mut response = McpHttpResponse::text(
                    401,
                    "A valid Vulcan Code MCP client token is required as `Authorization: Bearer <token>`.",
                );
                response.headers.push((
                    String::from("WWW-Authenticate"),
                    String::from("Bearer realm=\"vulcan-code-mcp\""),
                ));
                return response;
            }
        };
        self.record_usage(&client.id, false);
        let message: Value = match serde_json::from_slice(body) {
            Ok(message) => message,
            Err(error) => {
                return McpHttpResponse::json(
                    400,
                    &jsonrpc_error(
                        Value::Null,
                        JSONRPC_PARSE_ERROR,
                        &format!("Invalid JSON: {error}"),
                    ),
                );
            }
        };
        let Some(object) = message.as_object() else {
            return McpHttpResponse::json(
                400,
                &jsonrpc_error(
                    Value::Null,
                    JSONRPC_INVALID_REQUEST,
                    "Expected one JSON-RPC message object; batches are not supported.",
                ),
            );
        };
        let id = object.get("id").cloned();
        let method = object.get("method").and_then(Value::as_str);
        let (Some(id), Some(method)) = (id, method) else {
            // Notifications and responses to server requests need no answer.
            // 通知以及对服务端请求的响应无需应答。
            return McpHttpResponse::accepted();
        };
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return McpHttpResponse::json(
                400,
                &jsonrpc_error(
                    id,
                    JSONRPC_INVALID_REQUEST,
                    "Field `jsonrpc` must be \"2.0\".",
                ),
            );
        }
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = match method {
            "initialize" => Ok(self.initialize_result(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_definitions(&client) })),
            "tools/call" => self.call_tool(&client, params).await,
            _ => Err((
                JSONRPC_METHOD_NOT_FOUND,
                format!("Method `{method}` is not supported by this server."),
            )),
        };
        let response = match result {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => jsonrpc_error(id, code, &message),
        };
        McpHttpResponse::json(200, &response)
    }

    /// Builds the `initialize` result.
    /// 构建 `initialize` 结果。
    fn initialize_result(&self, params: &Value) -> Value {
        let requested = params.get("protocolVersion").and_then(Value::as_str);
        let protocol_version = requested
            .filter(|requested| MCP_SUPPORTED_PROTOCOL_VERSIONS.contains(requested))
            .unwrap_or(MCP_SUPPORTED_PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": protocol_version,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": {
                "name": "vulcan-code",
                "title": "Vulcan Code",
                "version": self.server_version,
            },
            "instructions": SERVER_INSTRUCTIONS,
        })
    }

    /// Increments the usage counters of one client.
    /// 增加一个客户端的用量计数。
    fn record_usage(&self, client_id: &str, tool_call: bool) {
        if let Ok(mut usage) = self.usage.lock() {
            let entry = usage.entry(client_id.to_string()).or_default();
            entry.last_used_at_ms = Some(unix_millis());
            if tool_call {
                entry.call_count = entry.call_count.saturating_add(1);
            }
        }
    }

    /// Executes one `tools/call` request.
    /// 执行一次 `tools/call` 请求。
    async fn call_tool(
        self: &Arc<Self>,
        client: &McpServiceClient,
        params: Value,
    ) -> Result<Value, (i64, String)> {
        let name = params.get("name").and_then(Value::as_str).ok_or((
            JSONRPC_INVALID_PARAMS,
            String::from("Field `name` must be a string."),
        ))?;
        let spec = tool_spec(name)
            .ok_or_else(|| (JSONRPC_INVALID_PARAMS, format!("Unknown tool `{name}`.")))?;
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => json!({}),
            Some(arguments) if arguments.is_object() => arguments.clone(),
            Some(_) => {
                return Err((
                    JSONRPC_INVALID_PARAMS,
                    String::from("Field `arguments` must be an object."),
                ));
            }
        };
        self.record_usage(&client.id, true);
        if !spec.grant.allows(client) {
            return Ok(tool_error(&format!(
                "This MCP client is not allowed to use `{name}`. The user can grant it in Vulcan Code Settings > MCP service."
            )));
        }
        let output = match spec.name {
            names::LIST_PROJECTS => match parse_arguments::<NoArguments>(arguments) {
                Ok(NoArguments {}) => self.list_projects(client).await,
                Err(error) => Err(error),
            },
            names::LIST_SESSIONS => self.list_sessions(client, arguments).await,
            names::GET_SESSION => self.get_session(client, arguments).await,
            names::READ_MESSAGES => self.read_messages(client, arguments).await,
            names::WAIT_FOR_REPLY => self.wait_for_reply(client, arguments).await,
            names::LIST_CHAT_MODELS => match parse_arguments::<NoArguments>(arguments) {
                Ok(NoArguments {}) => self.list_chat_models().await,
                Err(error) => Err(error),
            },
            names::SEND_MESSAGE => self.send_message(client, arguments).await,
            names::CREATE_SESSION => self.create_session(client, arguments).await,
            names::STOP_SESSION => self.stop_session(client, arguments).await,
            names::ANSWER_QUESTION => self.answer_question(client, arguments).await,
            names::LIST_MEDIA_MODELS => self.list_media_models(client, arguments).await,
            names::GENERATE_MEDIA => self.generate_media(client, arguments).await,
            names::GET_MEDIA_JOB => self.get_media_job(client, arguments).await,
            names::CANCEL_MEDIA_JOB => self.cancel_media_job(client, arguments),
            _ => Err(format!("Tool `{name}` has no implementation.")),
        };
        Ok(match output {
            Ok(output) => tool_success(output),
            Err(error) => tool_error(&error),
        })
    }

    /// `list_projects`.
    async fn list_projects(&self, client: &McpServiceClient) -> Result<ToolOutput, String> {
        let projects = self
            .backend
            .projects()
            .await?
            .into_iter()
            .filter(|project| client.sessions.covers_project(&project.id))
            .collect::<Vec<_>>();
        Ok(ToolOutput::structured(json!({ "projects": projects })))
    }

    /// `list_sessions`.
    async fn list_sessions(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: ListSessionsArguments = parse_arguments(arguments)?;
        let limit = bounded_limit(arguments.limit)?;
        let sessions = self
            .backend
            .sessions()
            .await?
            .into_iter()
            .filter(|session| session_in_scope(client, session))
            .filter(|session| {
                arguments
                    .project_id
                    .as_deref()
                    .is_none_or(|project_id| session.project_id.as_deref() == Some(project_id))
            })
            .filter(|session| !arguments.running_only.unwrap_or(false) || session.running)
            .collect::<Vec<_>>();
        let total = sessions.len();
        let sessions = sessions.into_iter().take(limit).collect::<Vec<_>>();
        Ok(ToolOutput::structured(json!({
            "sessions": sessions,
            "total": total,
        })))
    }

    /// `get_session`.
    async fn get_session(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: SessionArguments = parse_arguments(arguments)?;
        let status = self
            .authorized_status(client, &arguments.session_id)
            .await?;
        Ok(ToolOutput::structured(status_value(&status)))
    }

    /// `read_messages`.
    async fn read_messages(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: ReadMessagesArguments = parse_arguments(arguments)?;
        let limit = bounded_limit(arguments.limit)?;
        let status = self
            .authorized_status(client, &arguments.session_id)
            .await?;
        let after_seq = match arguments.after_seq {
            Some(after_seq) => after_seq,
            None => status
                .next_seq
                .saturating_sub(1)
                .saturating_sub(limit as u64),
        };
        let page = self
            .backend
            .read_transcript(
                &arguments.session_id,
                after_seq,
                limit,
                arguments.include_tool_output.unwrap_or(false),
            )
            .await?;
        let cursor = page.last_scanned_seq.unwrap_or(after_seq);
        Ok(ToolOutput::structured(json!({
            "session_id": arguments.session_id,
            "messages": page.entries,
            "cursor": cursor,
            "has_more": cursor.saturating_add(1) < page.next_seq,
        })))
    }

    /// `wait_for_reply`.
    async fn wait_for_reply(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: WaitArguments = parse_arguments(arguments)?;
        let wait = bounded_wait(arguments.wait_seconds)?;
        let status = self
            .authorized_status(client, &arguments.session_id)
            .await?;
        let after_seq = arguments
            .after_seq
            .unwrap_or_else(|| status.next_seq.saturating_sub(1));
        self.wait_and_report(&arguments.session_id, after_seq, wait, None)
            .await
    }

    /// `list_chat_models`.
    async fn list_chat_models(&self) -> Result<ToolOutput, String> {
        let models = self
            .backend
            .chat_models()
            .await?
            .into_iter()
            .map(|model| {
                json!({
                    "model": format!("{}/{}", model.provider_id, model.model_id),
                    "account": model.connection_id,
                    "model_name": model.model_name,
                    "provider_name": model.provider_name,
                    "account_name": model.account_name,
                    "preferred_account": model.preferred_account,
                })
            })
            .collect::<Vec<_>>();
        Ok(ToolOutput::structured(json!({ "models": models })))
    }

    /// `send_message`.
    async fn send_message(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: SendMessageArguments = parse_arguments(arguments)?;
        let wait = bounded_wait(arguments.wait_seconds)?;
        let message = non_empty_message(arguments.message)?;
        let status = self
            .authorized_status(client, &arguments.session_id)
            .await?;
        let model = self
            .resolve_model(arguments.model.as_deref(), arguments.account.as_deref())
            .await?;
        let after_seq = status.next_seq.saturating_sub(1);
        let ack = self
            .backend
            .submit(McpSubmission {
                session_id: Some(arguments.session_id.clone()),
                project_id: status.row.project_id.clone(),
                prompt: message,
                model,
                reasoning_effort: arguments.reasoning_effort,
                plan_mode: arguments.plan_mode.unwrap_or(false),
                submission_id: new_identifier(),
            })
            .await?;
        self.wait_and_report(&ack.session_id, after_seq, wait, Some(ack.queued))
            .await
    }

    /// `create_session`.
    async fn create_session(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: CreateSessionArguments = parse_arguments(arguments)?;
        let wait = bounded_wait(arguments.wait_seconds)?;
        let message = non_empty_message(arguments.message)?;
        if !client.sessions.covers_project(&arguments.project_id)
            || !self
                .backend
                .projects()
                .await?
                .iter()
                .any(|project| project.id == arguments.project_id)
        {
            return Err(format!(
                "Project `{}` was not found. Call `list_projects` for valid ids.",
                arguments.project_id
            ));
        }
        let model = self
            .resolve_model(arguments.model.as_deref(), arguments.account.as_deref())
            .await?;
        let ack = self
            .backend
            .submit(McpSubmission {
                session_id: None,
                project_id: Some(arguments.project_id),
                prompt: message,
                model,
                reasoning_effort: arguments.reasoning_effort,
                plan_mode: arguments.plan_mode.unwrap_or(false),
                submission_id: new_identifier(),
            })
            .await?;
        self.wait_and_report(&ack.session_id, 0, wait, Some(ack.queued))
            .await
    }

    /// `stop_session`.
    async fn stop_session(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: SessionArguments = parse_arguments(arguments)?;
        let status = self
            .authorized_status(client, &arguments.session_id)
            .await?;
        if !status.row.running && status.queued_prompts == 0 {
            return Ok(ToolOutput::structured(json!({
                "session_id": arguments.session_id,
                "stopped": false,
                "reason": "The session is not running.",
            })));
        }
        self.backend.stop(&arguments.session_id).await?;
        Ok(ToolOutput::structured(json!({
            "session_id": arguments.session_id,
            "stopped": true,
        })))
    }

    /// `answer_question`.
    async fn answer_question(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: AnswerQuestionArguments = parse_arguments(arguments)?;
        let status = self
            .authorized_status(client, &arguments.session_id)
            .await?;
        let question = status
            .questions
            .iter()
            .find(|question| question.request_id == arguments.request_id)
            .ok_or_else(|| {
                format!(
                    "Session `{}` has no pending question `{}`. Call `get_session` for current questions.",
                    arguments.session_id, arguments.request_id
                )
            })?;
        if question.approval_tool.is_some() {
            return Err(String::from(
                "This is a permission approval. Only the user can approve or deny it in Vulcan Code.",
            ));
        }
        let answers = if arguments.dismiss.unwrap_or(false) {
            if arguments.answers.is_some() {
                return Err(String::from(
                    "Pass either `answers` or `dismiss: true`, not both.",
                ));
            }
            None
        } else {
            let answers = arguments.answers.ok_or_else(|| {
                String::from("Pass `answers` (one list per question) or `dismiss: true`.")
            })?;
            if answers.len() != question.prompts.len() {
                return Err(format!(
                    "The request has {} questions but {} answer lists were given.",
                    question.prompts.len(),
                    answers.len()
                ));
            }
            Some(answers)
        };
        let dismissed = answers.is_none();
        let accepted = self
            .backend
            .answer_question(&arguments.request_id, answers)
            .await?;
        Ok(ToolOutput::structured(json!({
            "session_id": arguments.session_id,
            "request_id": arguments.request_id,
            "accepted": accepted,
            "dismissed": dismissed,
        })))
    }

    /// `list_media_models`.
    async fn list_media_models(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: ListMediaModelsArguments = parse_arguments(arguments)?;
        let capability = arguments.capability.map(media_capability).transpose()?;
        let models = self
            .granted_media_models(client)
            .await?
            .into_iter()
            .filter(|model| capability.is_none_or(|capability| model.capability == capability))
            .collect::<Vec<_>>();
        if let Some(requested) = arguments.model {
            let details = models
                .iter()
                .filter(|model| model.model == requested)
                .map(|model| {
                    json!({
                        "model": model.model,
                        "capability": model.capability,
                        "display_name": model.display_name,
                        "parameters": model.parameters_markdown,
                    })
                })
                .collect::<Vec<_>>();
            if details.is_empty() {
                return Err(format!(
                    "Model `{requested}` is not granted to this client. Call `list_media_models` without `model` to see the granted ones."
                ));
            }
            return Ok(ToolOutput::structured(json!({ "models": details })));
        }
        let models = models
            .iter()
            .map(|model| {
                json!({
                    "model": model.model,
                    "capability": model.capability,
                    "display_name": model.display_name,
                })
            })
            .collect::<Vec<_>>();
        Ok(ToolOutput::structured(json!({
            "models": models,
            "next_step": "Call list_media_models with `model` to read its exact arguments schema before generate_media.",
        })))
    }

    /// `generate_media`.
    async fn generate_media(
        self: &Arc<Self>,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: GenerateMediaArguments = parse_arguments(arguments)?;
        let wait = bounded_wait(arguments.wait_seconds)?;
        let capability = arguments.capability.map(media_capability).transpose()?;
        if !arguments.arguments.is_object() {
            return Err(String::from("Field `arguments` must be an object."));
        }
        let candidates = self
            .granted_media_models(client)
            .await?
            .into_iter()
            .filter(|model| model.model == arguments.model)
            .filter(|model| capability.is_none_or(|capability| model.capability == capability))
            .collect::<Vec<_>>();
        let model = match candidates.as_slice() {
            [] => {
                return Err(format!(
                    "Model `{}` is not granted to this client{}. Call `list_media_models`.",
                    arguments.model,
                    capability
                        .map(|capability| format!(" for `{capability}`"))
                        .unwrap_or_default()
                ));
            }
            [model] => model.clone(),
            [first, ..]
                if candidates
                    .iter()
                    .all(|model| model.capability == first.capability) =>
            {
                return Err(format!(
                    "Model `{}` is granted through several accounts. Ask the user to keep one in Settings > MCP service.",
                    arguments.model
                ));
            }
            _ => {
                return Err(format!(
                    "Model `{}` offers several capabilities; pass `capability`.",
                    arguments.model
                ));
            }
        };
        let job_id = new_identifier();
        let owner_id = format!("mcp-media:{job_id}");
        let job = self.jobs.register(
            job_id.clone(),
            &client.id,
            model.model.clone(),
            model.capability.clone(),
            owner_id.clone(),
        )?;
        let run = McpMediaRun {
            client_id: client.id.clone(),
            job_id,
            owner_id,
            capability: model.capability.clone(),
            connection_id: model.connection_id.clone(),
            target_id: model.target_id.clone(),
            arguments: arguments.arguments,
        };
        let backend = Arc::clone(&self.backend);
        let running_job = Arc::clone(&job);
        let handle = tokio::spawn(async move {
            let state = match backend.run_media(run).await {
                Ok(outcome) => MediaJobState::Succeeded(outcome),
                Err(error) => MediaJobState::Failed(error),
            };
            running_job.finish(state);
        });
        job.attach_abort(handle.abort_handle());
        let state = job.wait(wait).await;
        media_job_output(&job, state, arguments.include_images.unwrap_or(true)).await
    }

    /// `get_media_job`.
    async fn get_media_job(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: MediaJobArguments = parse_arguments(arguments)?;
        let wait = bounded_wait(arguments.wait_seconds)?;
        let job = self
            .jobs
            .find(&client.id, &arguments.job_id)
            .ok_or_else(|| media_job_not_found(&arguments.job_id))?;
        let state = job.wait(wait).await;
        media_job_output(&job, state, arguments.include_images.unwrap_or(true)).await
    }

    /// `cancel_media_job`.
    fn cancel_media_job(
        &self,
        client: &McpServiceClient,
        arguments: Value,
    ) -> Result<ToolOutput, String> {
        let arguments: CancelMediaJobArguments = parse_arguments(arguments)?;
        let job = self
            .jobs
            .find(&client.id, &arguments.job_id)
            .ok_or_else(|| media_job_not_found(&arguments.job_id))?;
        let was_running = job.state() == MediaJobState::Running;
        if was_running {
            self.cancel_job(&job);
        }
        Ok(ToolOutput::structured(json!({
            "job_id": job.id,
            "cancelled": was_running,
            "status": job.state().label(),
        })))
    }

    /// Aborts one job and records provider-side cancellation.
    /// 中止一个任务并记录供应商侧取消。
    fn cancel_job(&self, job: &MediaJob) {
        job.abort();
        if let Err(error) = self.backend.cancel_media(&job.owner_id) {
            tracing::warn!("[mcp-service] failed to record media cancellation: {error}");
        }
        job.finish(MediaJobState::Cancelled);
    }

    /// Resolves the granted media targets against live accounts, keeping only MCP media capabilities.
    /// 对照实时账号解析已授权媒体目标，只保留 MCP 媒体能力。
    async fn granted_media_models(
        &self,
        client: &McpServiceClient,
    ) -> Result<Vec<McpMediaModel>, String> {
        Ok(self
            .backend
            .media_models(&client.media_targets)
            .await?
            .into_iter()
            .filter(|model| MCP_MEDIA_CAPABILITIES.contains(&model.capability.as_str()))
            .collect())
    }

    /// Reads one session's status and enforces the client's project scope.
    /// 读取一个会话的状态并执行客户端的项目范围。
    async fn authorized_status(
        &self,
        client: &McpServiceClient,
        session_id: &str,
    ) -> Result<McpSessionStatus, String> {
        let not_found =
            || format!("Session `{session_id}` was not found. Call `list_sessions` for valid ids.");
        let status = self
            .backend
            .session_status(session_id)
            .await?
            .ok_or_else(not_found)?;
        if !session_in_scope(client, &status.row) {
            return Err(not_found());
        }
        Ok(status)
    }

    /// Resolves an optional `provider/model` and account into an exact selection.
    /// 将可选的 `provider/model` 与账号解析为精确选择。
    async fn resolve_model(
        &self,
        model: Option<&str>,
        account: Option<&str>,
    ) -> Result<Option<McpModelSelection>, String> {
        let Some(model) = model else {
            if account.is_some() {
                return Err(String::from("`account` needs `model`."));
            }
            return Ok(None);
        };
        let (provider_id, model_id) = model
            .split_once('/')
            .filter(|(provider, model)| !provider.is_empty() && !model.is_empty())
            .ok_or_else(|| {
                format!("Model `{model}` must be `provider_id/model_id` from `list_chat_models`.")
            })?;
        let candidates = self
            .backend
            .chat_models()
            .await?
            .into_iter()
            .filter(|candidate| {
                candidate.provider_id == provider_id && candidate.model_id == model_id
            })
            .filter(|candidate| account.is_none_or(|account| candidate.connection_id == account))
            .collect::<Vec<_>>();
        let chosen = choose_model_account(&candidates).ok_or_else(|| {
            if candidates.is_empty() {
                format!("Model `{model}` is not available. Call `list_chat_models`.")
            } else {
                let accounts = candidates
                    .iter()
                    .map(|candidate| {
                        format!("`{}` ({})", candidate.connection_id, candidate.account_name)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "Model `{model}` is offered by several accounts; pass `account`: {accounts}."
                )
            }
        })?;
        Ok(Some(McpModelSelection {
            provider_id: chosen.provider_id.clone(),
            model_id: chosen.model_id.clone(),
            connection_id: chosen.connection_id.clone(),
        }))
    }

    /// Waits until the session settles, then reports its reply and activity after `after_seq`.
    /// 等待会话稳定，然后报告 `after_seq` 之后的回复与活动。
    async fn wait_and_report(
        &self,
        session_id: &str,
        after_seq: u64,
        wait: Duration,
        queued: Option<bool>,
    ) -> Result<ToolOutput, String> {
        let deadline = Instant::now() + wait;
        let status = loop {
            let status = self
                .backend
                .session_status(session_id)
                .await?
                .ok_or_else(|| format!("Session `{session_id}` disappeared while waiting."))?;
            let settled =
                !status.questions.is_empty() || (!status.row.running && status.queued_prompts == 0);
            let now = Instant::now();
            if settled || now >= deadline {
                break status;
            }
            tokio::time::sleep(MCP_STATUS_POLL_INTERVAL.min(deadline - now)).await;
        };
        let last_seq = status.next_seq.saturating_sub(1);
        // Only the newest events are read when much happened, so the reply is never cut off.
        // 发生大量事件时只读取最新事件，确保回复不被截断。
        let window_start = after_seq.max(last_seq.saturating_sub(MCP_MAX_READ_LIMIT as u64));
        let page = self
            .backend
            .read_transcript(session_id, window_start, MCP_MAX_READ_LIMIT, false)
            .await?;
        let reply = reply_text(&page.entries);
        let activity = page
            .entries
            .iter()
            .filter(|entry| entry.kind != McpTranscriptKind::Assistant)
            .collect::<Vec<_>>();
        let activity = activity
            .iter()
            .skip(activity.len().saturating_sub(MCP_WAIT_ACTIVITY_LIMIT))
            .collect::<Vec<_>>();
        let mut value = json!({
            "session_id": session_id,
            "status": wait_status(&status),
            "run_phase": status.row.run_phase,
            "reply": reply,
            "activity": activity,
            "pending_questions": status.questions,
            "queued_prompts": status.queued_prompts,
            "cursor": page.last_scanned_seq.unwrap_or(window_start).max(after_seq),
            "earlier_events_skipped": window_start > after_seq,
        });
        if let Some(queued) = queued {
            value["queued"] = Value::Bool(queued);
        }
        Ok(ToolOutput::structured(value))
    }
}

/// Server-level guidance returned by `initialize`.
/// `initialize` 返回的服务器级指引。
const SERVER_INSTRUCTIONS: &str = "Vulcan Code is a local coding and work agent. Use list_projects and list_sessions to find work, create_session or send_message to delegate a task (the call waits for the reply up to wait_seconds), and wait_for_reply with the returned cursor when a result says `running`. A session may ask a question (answer_question) or need a permission approval, which only the user can grant inside Vulcan Code. Media tools run paid image and video models: read a model's exact schema with list_media_models before generate_media.";

/// Chooses the account to use among offers of one model.
/// 在同一模型的多个提供方中选择要使用的账号。
fn choose_model_account(candidates: &[McpChatModel]) -> Option<&McpChatModel> {
    match candidates {
        [only] => Some(only),
        _ => {
            let mut preferred = candidates
                .iter()
                .filter(|candidate| candidate.preferred_account);
            match (preferred.next(), preferred.next()) {
                (Some(chosen), None) => Some(chosen),
                _ => None,
            }
        }
    }
}

/// Returns whether a session lies inside the client's project scope.
/// 判断会话是否位于客户端的项目范围内。
fn session_in_scope(client: &McpServiceClient, session: &McpSessionRow) -> bool {
    match session.project_id.as_deref() {
        Some(project_id) => client.sessions.covers_project(project_id),
        None => client.sessions.projects == McpProjectScope::All,
    }
}

/// Builds the `get_session` result.
/// 构建 `get_session` 结果。
fn status_value(status: &McpSessionStatus) -> Value {
    json!({
        "session": status.row,
        "status": wait_status(status),
        "pending_questions": status.questions,
        "queued_prompts": status.queued_prompts,
        "cursor": status.next_seq.saturating_sub(1),
    })
}

/// Classifies a session for waiting callers.
/// 为等待调用方对会话分类。
fn wait_status(status: &McpSessionStatus) -> &'static str {
    if status
        .questions
        .iter()
        .any(|question| question.approval_tool.is_some())
    {
        "waiting_for_approval"
    } else if !status.questions.is_empty() {
        "waiting_for_answer"
    } else if status.row.running || status.queued_prompts > 0 {
        "running"
    } else {
        match status.row.run_phase.as_str() {
            "Failed" => "failed",
            "Stopped" => "stopped",
            _ => "idle",
        }
    }
}

/// Joins the assistant text written after the last user message in the window.
/// 拼接窗口内最后一条用户消息之后写出的助手文本。
fn reply_text(entries: &[McpTranscriptEntry]) -> String {
    let start = entries
        .iter()
        .rposition(|entry| entry.kind == McpTranscriptKind::User)
        .map_or(0, |position| position + 1);
    entries[start..]
        .iter()
        .filter(|entry| entry.kind == McpTranscriptKind::Assistant && !entry.text.trim().is_empty())
        .map(|entry| entry.text.trim())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Renders a media job result, attaching small images inline.
/// 渲染媒体任务结果，并内联附带较小的图片。
async fn media_job_output(
    job: &MediaJob,
    state: MediaJobState,
    include_images: bool,
) -> Result<ToolOutput, String> {
    let mut value = json!({
        "job_id": job.id,
        "model": job.model,
        "capability": job.capability,
        "status": state.label(),
    });
    let mut images = Vec::new();
    match state {
        MediaJobState::Running => {
            value["next_step"] =
                json!("Call get_media_job with this job_id to keep waiting, or cancel_media_job.");
        }
        MediaJobState::Succeeded(McpMediaOutcome { files, text }) => {
            if include_images {
                for file in &files {
                    if file.mime_type.starts_with("image/")
                        && file.byte_len <= MCP_INLINE_IMAGE_MAX_BYTES
                        && let Ok(bytes) = tokio::fs::read(&file.path).await
                    {
                        images.push((
                            file.mime_type.clone(),
                            base64::engine::general_purpose::STANDARD.encode(bytes),
                        ));
                    }
                }
            }
            value["files"] = json!(files);
            if let Some(text) = text {
                value["text"] = Value::String(text);
            }
        }
        MediaJobState::Failed(error) => {
            value["error"] = Value::String(error);
        }
        MediaJobState::Cancelled => {}
    }
    Ok(ToolOutput {
        structured: value,
        images,
    })
}

/// Error text for an unknown job.
/// 未知任务的错误文本。
fn media_job_not_found(job_id: &str) -> String {
    format!(
        "Media job `{job_id}` was not found. Jobs are kept in memory and end when Vulcan Code restarts."
    )
}

/// Validates a media capability name.
/// 校验媒体能力名称。
fn media_capability(capability: String) -> Result<&'static str, String> {
    MCP_MEDIA_CAPABILITIES
        .iter()
        .copied()
        .find(|candidate| *candidate == capability)
        .ok_or_else(|| {
            format!(
                "Capability `{capability}` is not one of {}.",
                MCP_MEDIA_CAPABILITIES.join(", ")
            )
        })
}

/// Wraps a successful tool output in a `CallToolResult`.
/// 将成功的工具输出包装为 `CallToolResult`。
fn tool_success(output: ToolOutput) -> Value {
    let text = serde_json::to_string_pretty(&output.structured).unwrap_or_default();
    let mut content = vec![json!({ "type": "text", "text": text })];
    content.extend(
        output.images.into_iter().map(
            |(mime_type, data)| json!({ "type": "image", "data": data, "mimeType": mime_type }),
        ),
    );
    json!({
        "content": content,
        "structuredContent": output.structured,
        "isError": false,
    })
}

/// Wraps a tool failure so the calling model can read and react to it.
/// 包装工具失败，使调用模型可以读取并应对。
fn tool_error(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true,
    })
}

/// Builds a JSON-RPC error response.
/// 构建 JSON-RPC 错误响应。
fn jsonrpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Parses strict tool arguments.
/// 解析严格的工具参数。
fn parse_arguments<T: DeserializeOwned>(arguments: Value) -> Result<T, String> {
    serde_json::from_value(arguments).map_err(|error| format!("Invalid arguments: {error}."))
}

/// Applies the read limit bounds.
/// 应用读取数量边界。
fn bounded_limit(limit: Option<usize>) -> Result<usize, String> {
    match limit {
        None => Ok(MCP_DEFAULT_READ_LIMIT),
        Some(limit) if (1..=MCP_MAX_READ_LIMIT).contains(&limit) => Ok(limit),
        Some(_) => Err(format!(
            "`limit` must be between 1 and {MCP_MAX_READ_LIMIT}."
        )),
    }
}

/// Applies the wait bounds.
/// 应用等待时长边界。
fn bounded_wait(wait_seconds: Option<u64>) -> Result<Duration, String> {
    match wait_seconds {
        None => Ok(Duration::from_secs(MCP_DEFAULT_WAIT_SECONDS)),
        Some(seconds) if seconds <= MCP_MAX_WAIT_SECONDS => Ok(Duration::from_secs(seconds)),
        Some(_) => Err(format!(
            "`wait_seconds` must be at most {MCP_MAX_WAIT_SECONDS}."
        )),
    }
}

/// Rejects blank messages.
/// 拒绝空白消息。
fn non_empty_message(message: String) -> Result<String, String> {
    if message.trim().is_empty() {
        return Err(String::from("`message` must not be empty."));
    }
    Ok(message)
}

/// Returns a new random identifier.
/// 返回一个新的随机标识。
fn new_identifier() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Returns the current Unix time in milliseconds.
/// 返回当前 Unix 毫秒时间。
fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

/// Arguments of tools that take none.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArguments {}

/// `list_sessions` arguments.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListSessionsArguments {
    project_id: Option<String>,
    running_only: Option<bool>,
    limit: Option<usize>,
}

/// Arguments naming one session.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionArguments {
    session_id: String,
}

/// `read_messages` arguments.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadMessagesArguments {
    session_id: String,
    after_seq: Option<u64>,
    limit: Option<usize>,
    include_tool_output: Option<bool>,
}

/// `wait_for_reply` arguments.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WaitArguments {
    session_id: String,
    after_seq: Option<u64>,
    wait_seconds: Option<u64>,
}

/// `send_message` arguments.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SendMessageArguments {
    session_id: String,
    message: String,
    model: Option<String>,
    account: Option<String>,
    reasoning_effort: Option<String>,
    plan_mode: Option<bool>,
    wait_seconds: Option<u64>,
}

/// `create_session` arguments.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateSessionArguments {
    project_id: String,
    message: String,
    model: Option<String>,
    account: Option<String>,
    reasoning_effort: Option<String>,
    plan_mode: Option<bool>,
    wait_seconds: Option<u64>,
}

/// `answer_question` arguments.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnswerQuestionArguments {
    session_id: String,
    request_id: String,
    answers: Option<Vec<Vec<String>>>,
    dismiss: Option<bool>,
}

/// `list_media_models` arguments.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListMediaModelsArguments {
    capability: Option<String>,
    model: Option<String>,
}

/// `generate_media` arguments.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GenerateMediaArguments {
    model: String,
    capability: Option<String>,
    arguments: Value,
    include_images: Option<bool>,
    wait_seconds: Option<u64>,
}

/// `get_media_job` arguments.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MediaJobArguments {
    job_id: String,
    include_images: Option<bool>,
    wait_seconds: Option<u64>,
}

/// `cancel_media_job` arguments.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelMediaJobArguments {
    job_id: String,
}
