//! Strongly typed Provider facts consumed by the V1 session bridge.
//! 供 V1 会话桥消费的强类型 Provider 事实。

use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionPromptPurpose};
use crate::luaskills_runtime::RUNLUA_SESSION_TOOL_NAME;
use crate::provider_config::ReasoningEffortLevel;
use crate::provider_tool_bridge::GENERATED_ATTACHMENTS_HOST_RESULT_KIND;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::fmt;

use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionError, DispatchOutcome, ProviderReplayEnvelope,
};
use vulcan_core::types::{
    ConversationAttachment, ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderTurnResponse as CoreProviderTurnResponse,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot, ToolInvocationResult,
};
use vulcan_session_core::v1::ClientActionId;
use vulcan_session_core::v1::event::{
    ApiCallStatus, ApiEndReason, ApiErrorCategory, ApiPurposeKind, ApiRequestSnapshot, ApiTimings,
    ContinuationEligibility, ExternalResourceKind, ExternalResourceRef, ProviderSourceInput,
    RedactedApiError,
};
use vulcan_session_core::v1::stats::UsageSnapshot;
use vulcan_subagent_core::{SPAWN_AGENT_TOOL, SUBAGENT_TASK_ID_KEY, SUBAGENT_TASK_KIND};

/// Stable public tool name for the host-owned persistent terminal.
/// 宿主持有的持久化终端稳定公开工具名。
const TERMINAL_SESSION_TOOL_NAME: &str = "terminal_session";
/// Stable host-result kind for the host-owned persistent terminal.
/// 宿主持有的持久化终端稳定宿主结果类型。
const TERMINAL_SESSION_HOST_RESULT_KIND: &str = "terminal_session";
/// Stable host-result kind for the Lua persistent-session runtime.
/// Lua 持久化会话运行时的稳定宿主结果类型。
const RUNLUA_SESSION_HOST_RESULT_KIND: &str = "lua_runtime_session";

/// Canonical runtime envelope surrounding every structured host result.
/// 包围每一份结构化宿主结果的规范运行时封套。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolHostResultEnvelope {
    /// Exact producer-owned result kind.
    /// 生产者拥有的精确结果类型。
    kind: String,
    /// Exact producer-owned result payload.
    /// 生产者拥有的精确结果载荷。
    payload: serde_json::Value,
}

/// Exact producer-owned payload for managed generated attachments.
/// 托管生成附件的精确生产者自有载荷。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratedAttachmentsPayload {
    /// Ordered canonical attachments staged by the provider tool bridge.
    /// 由 Provider 工具桥按顺序暂存的规范附件。
    outputs: Vec<ConversationAttachment>,
    /// Optional upstream response identity retained only for diagnostics.
    /// 仅用于诊断保留的可选上游响应身份。
    #[serde(default, rename = "upstream_response_id")]
    _upstream_response_id: Option<String>,
}

/// Failures while extracting exact generated attachments from a tool result.
/// 从工具结果提取精确生成附件时产生的失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderV1GeneratedResourceError {
    /// The matching producer envelope failed strict deserialization.
    /// 匹配的生产者封套未通过严格反序列化。
    MalformedHostResult {
        /// Exact public tool name.
        /// 精确公开工具名。
        tool_name: String,
        /// Structured deserialization failure text.
        /// 结构化反序列化失败文本。
        message: String,
    },
    /// The matching producer payload failed strict deserialization.
    /// 匹配的生产者载荷未通过严格反序列化。
    MalformedPayload {
        /// Exact public tool name.
        /// 精确公开工具名。
        tool_name: String,
        /// Structured deserialization failure text.
        /// 结构化反序列化失败文本。
        message: String,
    },
}

impl fmt::Display for ProviderV1GeneratedResourceError {
    /// Formats one controlled generated-resource extraction failure.
    /// 格式化一条受控生成资源提取失败。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedHostResult { tool_name, message } => write!(
                formatter,
                "Tool `{tool_name}` returned a malformed generated-resource host result: {message}"
            ),
            Self::MalformedPayload { tool_name, message } => write!(
                formatter,
                "Tool `{tool_name}` returned a malformed generated-resource payload: {message}"
            ),
        }
    }
}

impl std::error::Error for ProviderV1GeneratedResourceError {}

/// Extracts generated attachments only from the exact producer-owned host-result discriminator.
/// 仅从精确的生产者自有宿主结果判别符提取生成附件。
///
/// # Parameters
/// 参数
/// `result` is the trusted runtime result after provider resource staging completed.
/// `result` 是 Provider 资源暂存完成后的可信运行时结果。
///
/// # Returns
/// 返回值
/// Returns the ordered canonical attachments for the generated-attachment contract, an empty list
/// for unrelated or failed tools, and an explicit error for a malformed matching contract.
/// 对生成附件契约返回有序规范附件；无关或失败工具返回空列表；匹配契约畸形时返回显式错误。
pub fn provider_v1_generated_attachments_from_tool_result(
    result: &ToolInvocationResult,
) -> Result<Vec<ConversationAttachment>, ProviderV1GeneratedResourceError> {
    if result.is_error {
        return Ok(Vec::new());
    }
    let Some(host_result) = result.host_result.as_ref() else {
        return Ok(Vec::new());
    };
    if host_result.get("kind").and_then(serde_json::Value::as_str)
        != Some(GENERATED_ATTACHMENTS_HOST_RESULT_KIND)
    {
        return Ok(Vec::new());
    }
    let envelope =
        serde_json::from_value::<ToolHostResultEnvelope>(host_result.clone()).map_err(|error| {
            ProviderV1GeneratedResourceError::MalformedHostResult {
                tool_name: result.tool_name.clone(),
                message: error.to_string(),
            }
        })?;
    let payload = serde_json::from_value::<GeneratedAttachmentsPayload>(envelope.payload).map_err(
        |error| ProviderV1GeneratedResourceError::MalformedPayload {
            tool_name: result.tool_name.clone(),
            message: error.to_string(),
        },
    )?;
    Ok(payload.outputs)
}

/// Terminal-session actions that determine whether the payload owns one or many resources.
/// 决定载荷拥有单个还是多个资源的终端会话动作。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TerminalSessionResourceAction {
    /// Lists every persistent terminal owned by the conversation.
    /// 列出会话拥有的全部持久化终端。
    List,
    /// Starts one persistent terminal.
    /// 启动一个持久化终端。
    Start,
    /// Sends input to one persistent terminal.
    /// 向一个持久化终端发送输入。
    Send,
    /// Reads output from one persistent terminal.
    /// 读取一个持久化终端的输出。
    Read,
    /// Captures one persistent terminal screen.
    /// 捕获一个持久化终端屏幕。
    Snapshot,
    /// Stops one persistent terminal.
    /// 停止一个持久化终端。
    Stop,
}

/// One terminal entry returned by the exact `terminal_session list` payload.
/// 精确 `terminal_session list` 载荷返回的一条终端记录。
#[derive(Debug, Deserialize)]
struct TerminalSessionListResource {
    /// Stable host terminal-session identity.
    /// 稳定的宿主终端会话身份。
    terminal_session_id: String,
}

/// Navigation fields owned by the exact `terminal_session` host-result schema.
/// 精确 `terminal_session` 宿主结果 schema 拥有的导航字段。
#[derive(Debug, Deserialize)]
struct TerminalSessionResourcePayload {
    /// Exact terminal action that produced this payload.
    /// 产生此载荷的精确终端动作。
    action: TerminalSessionResourceAction,
    /// Single terminal identity for non-list actions.
    /// 非列表动作对应的单个终端身份。
    #[serde(default)]
    terminal_session_id: Option<String>,
    /// Ordered terminal identities returned by the list action.
    /// 列表动作返回的有序终端身份。
    #[serde(default)]
    terminal_sessions: Vec<TerminalSessionListResource>,
}

/// Lua persistent-session actions accepted by the exact host-result schema.
/// 精确宿主结果 schema 接受的 Lua 持久化会话动作。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LuaSessionResourceAction {
    /// Lists owner-bound Lua sessions without exposing their identities in the compact host payload.
    /// 列出所有者绑定的 Lua 会话，但紧凑宿主载荷不暴露其身份。
    List,
    /// Starts one Lua session.
    /// 启动一个 Lua 会话。
    Start,
    /// Evaluates code in one Lua session.
    /// 在一个 Lua 会话中执行代码。
    Eval,
    /// Reads one Lua session status.
    /// 读取一个 Lua 会话状态。
    Status,
    /// Closes one Lua session.
    /// 关闭一个 Lua 会话。
    Close,
}

/// Lua persistent-session completion status from the exact host-result schema.
/// 精确宿主结果 schema 返回的 Lua 持久化会话完成状态。
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LuaSessionResourceStatus {
    /// The requested lifecycle action completed.
    /// 请求的生命周期动作已完成。
    Completed,
    /// The requested lifecycle action failed without creating a new resource.
    /// 请求的生命周期动作失败且未创建新资源。
    Failed,
}

/// Navigation fields owned by the exact `runlua_session` host-result schema.
/// 精确 `runlua_session` 宿主结果 schema 拥有的导航字段。
#[derive(Debug, Deserialize)]
struct LuaSessionResourcePayload {
    /// Exact Lua-session action that produced this payload.
    /// 产生此载荷的精确 Lua 会话动作。
    action: LuaSessionResourceAction,
    /// Exact lifecycle completion status.
    /// 精确生命周期完成状态。
    status: LuaSessionResourceStatus,
    /// Stable opaque Lua lease identity when one session is addressed.
    /// 涉及单个会话时的稳定不透明 Lua 租约身份。
    #[serde(rename = "sessionId", default)]
    session_id: Option<String>,
}

/// Navigation fields owned by the exact subagent task host-result schema.
/// 精确子代理任务宿主结果 schema 拥有的导航字段。
#[derive(Debug, Deserialize)]
struct SubagentResourcePayload {
    /// Stable internal child-session identity.
    /// 稳定的内部子会话身份。
    #[serde(rename = "subagent_task_id")]
    session_id: String,
}

/// Failures while extracting navigation-only resources from a typed tool result.
/// 从强类型工具结果提取仅导航资源时产生的失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderV1ExternalResourceError {
    /// A successful resource-owning tool omitted its structured host result.
    /// 成功的资源型工具缺少结构化宿主结果。
    MissingHostResult {
        /// Exact public tool name.
        /// 精确公开工具名。
        tool_name: String,
    },
    /// The canonical host-result envelope failed strict deserialization.
    /// 规范宿主结果封套未通过严格反序列化。
    MalformedHostResult {
        /// Exact public tool name.
        /// 精确公开工具名。
        tool_name: String,
        /// Structured deserialization failure text.
        /// 结构化反序列化失败文本。
        message: String,
    },
    /// A resource-owning tool returned another producer's host-result kind.
    /// 资源型工具返回了其他生产者的宿主结果类型。
    HostResultKindMismatch {
        /// Exact public tool name.
        /// 精确公开工具名。
        tool_name: String,
        /// Required producer-owned result kind.
        /// 要求的生产者自有结果类型。
        expected: &'static str,
        /// Actual result kind.
        /// 实际结果类型。
        actual: String,
    },
    /// A successful resource-owning action omitted or blanked its unique identity.
    /// 成功的资源型动作缺少或清空了唯一身份。
    MissingResourceId {
        /// Exact public tool name.
        /// 精确公开工具名。
        tool_name: String,
        /// Exact producer-owned payload field.
        /// 精确的生产者自有载荷字段。
        field: &'static str,
    },
}

impl fmt::Display for ProviderV1ExternalResourceError {
    /// Formats one controlled external-resource extraction failure.
    /// 格式化一条受控外部资源提取失败。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHostResult { tool_name } => {
                write!(
                    formatter,
                    "Tool `{tool_name}` omitted its structured host result"
                )
            }
            Self::MalformedHostResult { tool_name, message } => write!(
                formatter,
                "Tool `{tool_name}` returned a malformed structured host result: {message}"
            ),
            Self::HostResultKindMismatch {
                tool_name,
                expected,
                actual,
            } => write!(
                formatter,
                "Tool `{tool_name}` returned host-result kind `{actual}` instead of `{expected}`"
            ),
            Self::MissingResourceId { tool_name, field } => write!(
                formatter,
                "Tool `{tool_name}` omitted required resource identity `{field}`"
            ),
        }
    }
}

impl std::error::Error for ProviderV1ExternalResourceError {}

/// Extracts navigation-only external resources from the exact producer-owned tool result schema.
/// 从精确的生产者自有工具结果 schema 提取仅导航外部资源。
///
/// # Parameters
/// 参数
/// `result` is the trusted runtime result after invocation identity has been attached.
/// `result` 是附加调用身份后的可信运行时结果。
///
/// # Returns
/// 返回值
/// Returns ordered resource references for supported stateful tools, an empty list for ordinary,
/// list-without-identities, or failed tools, and an explicit error for malformed successful
/// resource contracts.
/// 对受支持有状态工具返回有序资源引用；普通工具、不含身份的列表动作或失败工具返回空列表；
/// 成功资源契约畸形时返回显式错误。
pub fn provider_v1_external_refs_from_tool_result(
    result: &ToolInvocationResult,
) -> Result<Vec<ExternalResourceRef>, ProviderV1ExternalResourceError> {
    if result.is_error {
        return Ok(Vec::new());
    }
    match result.tool_name.as_str() {
        TERMINAL_SESSION_TOOL_NAME => terminal_session_external_refs(result),
        RUNLUA_SESSION_TOOL_NAME => lua_session_external_refs(result),
        SPAWN_AGENT_TOOL => subagent_external_refs(result),
        _ => Ok(Vec::new()),
    }
}

/// Extracts one or many terminal identities from the exact terminal host schema.
/// 从精确终端宿主 schema 提取一个或多个终端身份。
fn terminal_session_external_refs(
    result: &ToolInvocationResult,
) -> Result<Vec<ExternalResourceRef>, ProviderV1ExternalResourceError> {
    let payload = typed_resource_payload::<TerminalSessionResourcePayload>(
        result,
        TERMINAL_SESSION_HOST_RESULT_KIND,
    )?;
    match payload.action {
        TerminalSessionResourceAction::List => payload
            .terminal_sessions
            .into_iter()
            .map(|entry| {
                required_external_ref(
                    result,
                    ExternalResourceKind::Terminal,
                    entry.terminal_session_id,
                    "terminal_sessions[].terminal_session_id",
                )
            })
            .collect(),
        TerminalSessionResourceAction::Start
        | TerminalSessionResourceAction::Send
        | TerminalSessionResourceAction::Read
        | TerminalSessionResourceAction::Snapshot
        | TerminalSessionResourceAction::Stop => Ok(vec![required_external_ref(
            result,
            ExternalResourceKind::Terminal,
            payload.terminal_session_id.unwrap_or_default(),
            "terminal_session_id",
        )?]),
    }
}

/// Extracts the addressed Lua lease identity from the exact Lua host schema.
/// 从精确 Lua 宿主 schema 提取被寻址的 Lua 租约身份。
fn lua_session_external_refs(
    result: &ToolInvocationResult,
) -> Result<Vec<ExternalResourceRef>, ProviderV1ExternalResourceError> {
    let payload = typed_resource_payload::<LuaSessionResourcePayload>(
        result,
        RUNLUA_SESSION_HOST_RESULT_KIND,
    )?;
    if matches!(payload.action, LuaSessionResourceAction::List) {
        return Ok(Vec::new());
    }
    match payload.session_id {
        Some(session_id) => Ok(vec![required_external_ref(
            result,
            ExternalResourceKind::LuaSession,
            session_id,
            "sessionId",
        )?]),
        None if payload.status == LuaSessionResourceStatus::Failed => Ok(Vec::new()),
        None => Err(ProviderV1ExternalResourceError::MissingResourceId {
            tool_name: result.tool_name.clone(),
            field: "sessionId",
        }),
    }
}

/// Extracts the stable child-session identity from the exact spawn-agent result schema.
/// 从精确 spawn-agent 结果 schema 提取稳定子会话身份。
fn subagent_external_refs(
    result: &ToolInvocationResult,
) -> Result<Vec<ExternalResourceRef>, ProviderV1ExternalResourceError> {
    let payload = typed_resource_payload::<SubagentResourcePayload>(result, SUBAGENT_TASK_KIND)?;
    Ok(vec![required_external_ref(
        result,
        ExternalResourceKind::Subagent,
        payload.session_id,
        SUBAGENT_TASK_ID_KEY,
    )?])
}

/// Deserializes one exact resource-owning host envelope and its producer-owned payload.
/// 反序列化一个精确的资源型宿主封套及其生产者自有载荷。
fn typed_resource_payload<Payload>(
    result: &ToolInvocationResult,
    expected_kind: &'static str,
) -> Result<Payload, ProviderV1ExternalResourceError>
where
    Payload: DeserializeOwned,
{
    let host_result = result.host_result.as_ref().ok_or_else(|| {
        ProviderV1ExternalResourceError::MissingHostResult {
            tool_name: result.tool_name.clone(),
        }
    })?;
    let envelope =
        serde_json::from_value::<ToolHostResultEnvelope>(host_result.clone()).map_err(|error| {
            ProviderV1ExternalResourceError::MalformedHostResult {
                tool_name: result.tool_name.clone(),
                message: error.to_string(),
            }
        })?;
    if envelope.kind != expected_kind {
        return Err(ProviderV1ExternalResourceError::HostResultKindMismatch {
            tool_name: result.tool_name.clone(),
            expected: expected_kind,
            actual: envelope.kind,
        });
    }
    serde_json::from_value::<Payload>(envelope.payload).map_err(|error| {
        ProviderV1ExternalResourceError::MalformedHostResult {
            tool_name: result.tool_name.clone(),
            message: error.to_string(),
        }
    })
}

/// Builds one non-empty navigation reference without resolving the external resource.
/// 构建一条非空导航引用，且不解析外部资源是否存在。
fn required_external_ref(
    result: &ToolInvocationResult,
    kind: ExternalResourceKind,
    id: String,
    field: &'static str,
) -> Result<ExternalResourceRef, ProviderV1ExternalResourceError> {
    if id.trim().is_empty() {
        return Err(ProviderV1ExternalResourceError::MissingResourceId {
            tool_name: result.tool_name.clone(),
            field,
        });
    }
    Ok(ExternalResourceRef { kind, id })
}

/// Immutable request facts created before one physical Provider dispatch.
/// 在一次物理 Provider 分派前创建的不可变请求事实。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderV1RequestFacts {
    /// Host-generated identity of the physical Provider request.
    /// 宿主生成的物理 Provider 请求身份。
    pub provider_call_id: String,
    /// Exact V1 purpose derived from typed execution state.
    /// 从强类型执行状态派生的精确 V1 用途。
    pub purpose: ApiPurposeKind,
    /// Durable client action bound to a V1 Continue request.
    /// 绑定到 V1 Continue 请求的可靠客户端动作。
    pub continuation_action_id: Option<ClientActionId>,
    /// Exact non-secret request configuration selected for dispatch.
    /// 为分派选定的精确非秘密请求配置。
    pub request: ApiRequestSnapshot,
    /// Request preparation time in Unix milliseconds.
    /// 请求准备时间（Unix 毫秒）。
    pub prepared_at_ms: i64,
}

/// Typed transport observation bound to the exact prepared physical request.
/// 绑定到精确已准备物理请求的强类型传输观测。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderV1TransportObservedFacts {
    /// Immutable start facts identifying the same physical request.
    /// 标识同一次物理请求的不可变起始事实。
    pub request_facts: ProviderV1RequestFacts,
    /// Exact timestamp observed immediately before the wire dispatch.
    /// 在线路分派前立即观测到的精确时间戳。
    pub dispatched_at_ms: i64,
}

/// Terminal facts of one physical Provider request.
/// 一次物理 Provider 请求的终态事实。
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderV1ApiTerminalFacts {
    /// Immutable start facts that identify the same physical request.
    /// 标识同一次物理请求的不可变起始事实。
    pub request_facts: ProviderV1RequestFacts,
    /// Official normalized Provider usage.
    /// Provider 官方规范化用量。
    pub usage: UsageSnapshot,
    /// Exact terminal API status.
    /// 精确 API 终态。
    pub status: ApiCallStatus,
    /// Stable terminal reason derived from structured failure facts.
    /// 从结构化失败事实派生的稳定终态原因。
    pub end_reason: Option<ApiEndReason>,
    /// Optional controlled and redacted error detail.
    /// 可选的受控脱敏错误详情。
    pub error: Option<RedactedApiError>,
    /// Frozen transport-policy decision controlling later explicit continuation.
    /// 控制后续显式继续的已冻结传输策略判定。
    pub continuation_eligibility: ContinuationEligibility,
    /// Exact request timing checkpoints observed by the transport.
    /// 传输层观测到的精确请求时间点。
    pub timing: ApiTimings,
    /// Exact Provider-native replay source for a successful response.
    /// 成功响应携带的精确 Provider 原生重放来源。
    pub provider_source: Option<ProviderSourceInput>,
    /// Number of model tool calls that must complete before V1 API statistics may close the group.
    /// V1 API 统计闭合该组前必须完成的模型工具调用数量。
    pub tool_call_count: usize,
    /// Completion time in Unix milliseconds.
    /// 完成时间（Unix 毫秒）。
    pub completed_at_ms: i64,
}

/// Failures while extracting one closed Provider replay source from a completed response.
/// 从已完成响应提取封闭 Provider 重放来源时产生的失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderV1SourceError {
    /// The successful response omitted its continuation identity.
    /// 成功响应缺少 continuation 身份。
    MissingContinuation,
    /// The successful response omitted its closed replay envelope.
    /// 成功响应缺少封闭重放封套。
    MissingReplayEnvelope,
    /// The replay envelope failed strict typed deserialization.
    /// 重放封套未通过严格强类型反序列化。
    MalformedReplayEnvelope(String),
    /// Continuation and replay facts identify different physical routes.
    /// continuation 与 replay 事实标识了不同物理路由。
    RouteIdentityMismatch {
        /// Exact identity field that disagreed.
        /// 发生不一致的精确身份字段。
        field: &'static str,
    },
}

impl fmt::Display for ProviderV1SourceError {
    /// Formats one controlled source-extraction failure.
    /// 格式化一条受控来源提取失败。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingContinuation => {
                formatter.write_str("Successful Provider response is missing continuation facts")
            }
            Self::MissingReplayEnvelope => {
                formatter.write_str("Successful Provider response is missing /vulcan_ai/replay")
            }
            Self::MalformedReplayEnvelope(error) => {
                write!(formatter, "Provider replay envelope is malformed: {error}")
            }
            Self::RouteIdentityMismatch { field } => {
                write!(formatter, "Provider replay route disagrees on `{field}`")
            }
        }
    }
}

impl std::error::Error for ProviderV1SourceError {}

/// Extracts the exact V1 replay source from one completed typed Provider response.
/// 从一份已完成的强类型 Provider 响应提取精确 V1 重放来源。
///
/// # Parameters
/// 参数
/// `response` is the final normalized response returned by `response.finish()`.
/// `response` 是 `response.finish()` 返回的最终规范化响应。
///
/// # Returns
/// 返回值
/// Returns one route-validated source input without computing local seq or fingerprint.
/// 返回经过路由校验的来源输入，且不计算本地 seq 或指纹。
pub(crate) fn provider_v1_source_from_completed_response(
    response: &CoreProviderTurnResponse,
) -> Result<ProviderSourceInput, ProviderV1SourceError> {
    let continuation = response
        .continuation
        .as_ref()
        .ok_or(ProviderV1SourceError::MissingContinuation)?;
    let replay_value = response
        .provider_message_state
        .as_ref()
        .and_then(|state| state.pointer("/vulcan_ai/replay"))
        .ok_or(ProviderV1SourceError::MissingReplayEnvelope)?;
    let replay = serde_json::from_value::<ProviderReplayEnvelope>(replay_value.clone())
        .map_err(|error| ProviderV1SourceError::MalformedReplayEnvelope(error.to_string()))?;

    ensure_optional_identity_matches(
        "provider_id",
        continuation.provider_id.as_deref(),
        replay.provider_id.as_str(),
    )?;
    ensure_optional_identity_matches(
        "protocol_profile_id",
        continuation.protocol_profile_id.as_deref(),
        replay.protocol_profile_id.as_str(),
    )?;
    ensure_optional_identity_matches(
        "model_id",
        continuation.model_id.as_deref(),
        replay.model_id.as_str(),
    )?;
    if response.provider_message_id != continuation.last_provider_message_id {
        return Err(ProviderV1SourceError::RouteIdentityMismatch {
            field: "provider_message_id",
        });
    }

    Ok(ProviderSourceInput {
        provider: replay.provider_id.clone(),
        profile: Some(replay.protocol_profile_id.clone()),
        model: replay.model_id.clone(),
        connection_id: Some(replay.connection_id.clone()),
        message_id: response.provider_message_id.clone(),
        response_id: None,
        replay: Some(replay),
    })
}

/// Requires one continuation identity to exist and equal its replay identity.
/// 要求一项 continuation 身份存在且等于对应 replay 身份。
///
/// # Parameters
/// 参数
/// `field` names the contract field, `continuation` is the cursor identity, and `replay` is the
/// closed envelope identity.
/// `field` 命名契约字段，`continuation` 是游标身份，`replay` 是封闭封套身份。
///
/// # Returns
/// 返回值
/// Returns success only for one exact non-empty identity match.
/// 仅在非空身份精确匹配时返回成功。
fn ensure_optional_identity_matches(
    field: &'static str,
    continuation: Option<&str>,
    replay: &str,
) -> Result<(), ProviderV1SourceError> {
    if continuation == Some(replay) && !replay.is_empty() {
        return Ok(());
    }
    Err(ProviderV1SourceError::RouteIdentityMismatch { field })
}

/// Typed V1 lifecycle fact attached to an existing Provider telemetry boundary.
/// 附着到现有 Provider 遥测边界的强类型 V1 生命周期事实。
#[derive(Debug, Clone, PartialEq)]
pub enum ProviderV1LifecycleFacts {
    /// The typed execution purpose is intentionally outside the V1 session event stream.
    /// 强类型执行用途被有意排除在 V1 会话事件流之外。
    ExcludedNonSession,
    /// A request is prepared and must become durable before dispatch.
    /// 请求已准备完成，必须在分派前可靠落盘。
    RequestPrepared(ProviderV1RequestFacts),
    /// A prepared request crossed the real transport boundary.
    /// 一次已准备请求越过了真实传输边界。
    TransportObserved(ProviderV1TransportObservedFacts),
    /// A physical request reached one terminal result.
    /// 一次物理请求到达一个终态结果。
    ApiTerminal(ProviderV1ApiTerminalFacts),
}

/// Frozen typed template used to derive every physical request fact in one execution stream.
/// 用于派生单个执行流内每次物理请求事实的冻结强类型模板。
#[derive(Debug, Clone)]
pub(crate) struct ProviderV1RequestTemplate {
    /// Default purpose for physical requests, or no V1 projection for non-session work.
    /// 物理请求的默认用途；非会话工作则不产生 V1 投影。
    pub(crate) base_purpose: Option<ApiPurposeKind>,
    /// Durable client action bound to a V1 Continue execution.
    /// 绑定到 V1 Continue 执行的可靠客户端动作。
    pub(crate) continuation_action_id: Option<ClientActionId>,
    /// Exact protocol profile selected by execution preparation.
    /// 执行准备选定的精确协议 Profile。
    pub(crate) profile: String,
    /// Exact non-secret connection identity selected by execution preparation.
    /// 执行准备选定的精确非秘密连接身份。
    pub(crate) connection_id: String,
    /// Exact resolved reasoning level sent to supported Provider protocols.
    /// 发送到受支持 Provider 协议的精确已解析推理档位。
    pub(crate) thinking_level: Option<String>,
}

impl ProviderV1RequestTemplate {
    /// Builds one template exclusively from typed preparation and continuation facts.
    /// 仅从强类型执行准备与延续事实构建一个模板。
    ///
    /// # Parameters
    /// 参数
    /// `preparation` is the validated execution plan. `is_continuation` is retained only for
    /// transport compatibility; a V1 Continue purpose requires a durable ClientActionId binding.
    /// `preparation` 是已校验执行计划。`is_continuation` 仅为传输兼容保留；V1 Continue 用途必须具备
    /// 可靠的 ClientActionId 绑定。
    ///
    /// # Returns
    /// 返回值
    /// Returns an immutable template; Git summary requests remain explicitly outside session V1.
    /// 返回不可变模板；Git 总结请求显式保持在会话 V1 之外。
    pub(crate) fn from_preparation(
        preparation: &ExecutionPreparation,
        _is_continuation: bool,
    ) -> Self {
        // basePurpose is selected only from typed purpose and subagent ownership.
        // basePurpose 仅根据强类型用途与子代理归属选择。
        let base_purpose = match preparation.prompt_purpose {
            // Child identity supplies ownership, but cannot override an auxiliary request's execution purpose.
            // 子代理身份提供归属，但不能覆盖辅助请求的执行用途。
            ExecutionPromptPurpose::QuestionExecution => Some(ApiPurposeKind::QuestionExecution),
            ExecutionPromptPurpose::ContextCompaction
            | ExecutionPromptPurpose::ContextCompactionCachePreserving => {
                Some(ApiPurposeKind::Compaction)
            }
            _ if preparation.subagent_prompt_context.is_some() => Some(ApiPurposeKind::Subagent),
            ExecutionPromptPurpose::ExecutionRetry => Some(ApiPurposeKind::RetryGeneration),
            ExecutionPromptPurpose::UserTurn | ExecutionPromptPurpose::AgentTask => {
                Some(ApiPurposeKind::MainGeneration)
            }
            ExecutionPromptPurpose::GitCommitSummary => None,
        };
        Self {
            base_purpose,
            continuation_action_id: preparation.continuation_action_id.clone(),
            profile: preparation.model_blueprint.protocol_profile_id.clone(),
            connection_id: preparation.model_blueprint.connection_id.clone(),
            thinking_level: reasoning_level_key(
                preparation.model_blueprint.reasoning.requested_effort,
            ),
        }
    }

    /// Creates exact start facts for one physical request without inspecting telemetry JSON.
    /// 在不读取遥测 JSON 的情况下为一次物理请求创建精确起始事实。
    ///
    /// # Parameters
    /// 参数
    /// `provider_call_id`, `snapshot`, `retry_count`, `bind_continuation`, and `prepared_at_ms` are
    /// typed facts owned by the Provider request state machine. `bind_continuation` is true only
    /// for the first physical request accepted after one manual Continue action.
    /// `provider_call_id`、`snapshot`、`retry_count`、`bind_continuation` 与 `prepared_at_ms` 均为
    /// Provider 请求状态机拥有的强类型事实。`bind_continuation` 仅在一次手动继续后接纳的首个物理请求为真。
    ///
    /// # Returns
    /// 返回值
    /// Returns no facts for intentionally excluded non-session requests.
    /// 对明确排除的非会话请求不返回事实。
    pub(crate) fn request_facts(
        &self,
        provider_call_id: String,
        snapshot: &CoreProviderRequestSnapshot,
        retry_count: u32,
        bind_continuation: bool,
        prepared_at_ms: i64,
    ) -> Option<ProviderV1RequestFacts> {
        // Excluded owners stay outside the V1 ledger even when retry or Continue metadata is present.
        // 即使带有重试或继续元数据，排除所有者仍必须保持在 V1 账本之外。
        let base_purpose = self.base_purpose?;
        // continuationActionId binds only the first physical request owned by one manual Continue.
        // continuationActionId 仅绑定一次手动 Continue 所有的首个物理请求。
        // Auxiliary retries retain their purpose and never consume a business Continue action.
        // 辅助重试保留自身用途，绝不消费业务继续动作。
        let is_auxiliary = matches!(
            base_purpose,
            ApiPurposeKind::QuestionExecution | ApiPurposeKind::Compaction
        );
        let continuation_action_id = (bind_continuation && !is_auxiliary)
            .then(|| self.continuation_action_id.clone())
            .flatten();
        // Only business generation changes purpose on retry; isolated execution owners remain stable.
        // 只有业务生成在重试时改变用途；隔离执行所有者保持稳定。
        let purpose = if is_auxiliary {
            base_purpose
        } else if continuation_action_id.is_some() {
            ApiPurposeKind::Continue
        } else if retry_count > 0 {
            ApiPurposeKind::RetryGeneration
        } else {
            base_purpose
        };
        Some(ProviderV1RequestFacts {
            provider_call_id,
            purpose,
            continuation_action_id,
            request: ApiRequestSnapshot {
                provider: snapshot.provider_id.clone(),
                profile: Some(self.profile.clone()),
                model: snapshot.model_id.clone(),
                connection_id: Some(self.connection_id.clone()),
                thinking_level: self.thinking_level.clone(),
                temperature: snapshot.temperature,
            },
            prepared_at_ms,
        })
    }
}

impl ProviderV1ApiTerminalFacts {
    /// Builds a native success from already validated compaction `request_facts`, `usage`, `timing`, and completion time.
    /// 根据已验证的压缩`request_facts`、`usage`、`timing`及完成时间构建原生成功。
    /// Returns a tool-free terminal without a conversational replay source; the telemetry owner validates purpose before consumption.
    /// 返回无工具且不带对话重放来源的终态；遥测所有者在消费前验证用途。
    pub(crate) fn native_compaction_succeeded(
        request_facts: ProviderV1RequestFacts,
        usage: &CoreProviderUsageSnapshot,
        timing: ApiTimings,
        completed_at_ms: i64,
    ) -> Self {
        Self {
            request_facts,
            usage: usage_snapshot(usage),
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing,
            provider_source: None,
            tool_call_count: 0,
            completed_at_ms,
        }
    }

    /// Builds one successful terminal fact from typed request, usage, and timing data.
    /// 从强类型请求、用量与时间数据构建一个成功终态事实。
    ///
    /// # Parameters
    /// 参数
    /// All arguments are direct state-machine facts; `tool_call_count` controls deferred group closure.
    /// 全部参数均为状态机直接事实；`tool_call_count` 控制延迟闭合 API 组。
    ///
    /// # Returns
    /// 返回值
    /// Returns one complete successful terminal projection.
    /// 返回一份完整的成功终态投影。
    pub(crate) fn succeeded(
        request_facts: ProviderV1RequestFacts,
        usage: &CoreProviderUsageSnapshot,
        timing: ApiTimings,
        provider_source: ProviderSourceInput,
        tool_call_count: usize,
        completed_at_ms: i64,
    ) -> Self {
        Self {
            request_facts,
            usage: usage_snapshot(usage),
            status: ApiCallStatus::Completed,
            end_reason: Some(ApiEndReason::CompletedSuccessfully),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            timing,
            provider_source: Some(provider_source),
            tool_call_count,
            completed_at_ms,
        }
    }

    /// Builds one failed terminal fact only from a structured execution error.
    /// 仅从结构化执行错误构建一个失败终态事实。
    ///
    /// # Parameters
    /// 参数
    /// `error` is optional for internal invariant failures, `retry_exhausted` records a typed
    /// terminal retry decision, and `continuation_eligibility` is the exact replay-safety policy
    /// frozen by the transport state machine.
    /// `error` 对内部不变量失败可为空，`retry_exhausted` 记录强类型终态重试决定，
    /// `continuation_eligibility` 是传输状态机冻结的精确重放安全策略。
    ///
    /// # Returns
    /// 返回值
    /// Returns one complete failed or cancelled terminal projection.
    /// 返回一份完整的失败或取消终态投影。
    pub(crate) fn failed(
        request_facts: ProviderV1RequestFacts,
        usage: &CoreProviderUsageSnapshot,
        timing: ApiTimings,
        error: Option<&AiExecutionError>,
        retry_exhausted: bool,
        continuation_eligibility: ContinuationEligibility,
        completed_at_ms: i64,
    ) -> Self {
        // status is derived from the structured provider-neutral category.
        // status 从结构化 Provider 中立错误类别派生。
        let status = match error.map(|value| value.category) {
            Some(AiErrorCategory::Cancelled) => ApiCallStatus::Cancelled,
            _ => ApiCallStatus::Failed,
        };
        // endReason preserves retry exhaustion only when the transport made that terminal decision.
        // endReason 仅在传输层作出终态决定时保留重试耗尽事实。
        let end_reason = if status == ApiCallStatus::Cancelled {
            Some(ApiEndReason::Cancelled)
        } else if retry_exhausted {
            Some(ApiEndReason::RetryExhausted)
        } else {
            Some(provider_end_reason(error))
        };
        Self {
            request_facts,
            usage: usage_snapshot(usage),
            status,
            end_reason,
            // Cancellation is a control outcome; the domain forbids attaching a failure to it.
            // 取消属于控制结果；领域禁止在取消终态附加失败错误。
            error: (status != ApiCallStatus::Cancelled).then(|| redacted_api_error(error)),
            continuation_eligibility: if status == ApiCallStatus::Cancelled {
                ContinuationEligibility::Forbidden
            } else {
                continuation_eligibility
            },
            timing,
            provider_source: None,
            tool_call_count: 0,
            completed_at_ms,
        }
    }

    /// Builds a host-cancelled terminal fact from retained typed request identity.
    /// 从保留的强类型请求身份构建宿主取消终态事实。
    ///
    /// # Parameters
    /// 参数
    /// `request_facts` is the exact retained start fact and `completed_at_ms` is captured once.
    /// `request_facts` 是精确保留的起始事实，`completed_at_ms` 仅捕获一次。
    ///
    /// # Returns
    /// 返回值
    /// Returns a cancelled terminal fact without synthesizing usage.
    /// 返回不伪造用量的取消终态事实。
    pub(crate) fn cancelled(request_facts: ProviderV1RequestFacts, completed_at_ms: i64) -> Self {
        Self {
            timing: ApiTimings {
                prepared_at_ms: Some(request_facts.prepared_at_ms),
                completed_at_ms: Some(completed_at_ms),
                ..ApiTimings::default()
            },
            request_facts,
            usage: UsageSnapshot::default(),
            status: ApiCallStatus::Cancelled,
            end_reason: Some(ApiEndReason::Cancelled),
            error: None,
            continuation_eligibility: ContinuationEligibility::Forbidden,
            provider_source: None,
            tool_call_count: 0,
            completed_at_ms,
        }
    }
}

/// Returns the stable V1 reasoning key for one resolved workbench level.
/// 返回一个已解析工作台推理档位对应的稳定 V1 键。
fn reasoning_level_key(level: ReasoningEffortLevel) -> Option<String> {
    let key = match level {
        ReasoningEffortLevel::Off => return None,
        ReasoningEffortLevel::None => "none",
        ReasoningEffortLevel::Adaptive => "adaptive",
        ReasoningEffortLevel::Minimal => "minimal",
        ReasoningEffortLevel::Low => "low",
        ReasoningEffortLevel::Medium => "medium",
        ReasoningEffortLevel::High => "high",
        ReasoningEffortLevel::XHigh => "x-high",
        ReasoningEffortLevel::Max => "max",
        ReasoningEffortLevel::Ultra => "ultra",
    };
    Some(key.to_string())
}

/// Converts normalized Core usage to the V1 official usage contract.
/// 把规范化 Core 用量转换为 V1 官方用量契约。
fn usage_snapshot(usage: &CoreProviderUsageSnapshot) -> UsageSnapshot {
    UsageSnapshot {
        input_tokens: usage.input_tokens.map(usize_to_u64),
        uncached_input_tokens: usage.uncached_input_tokens.map(usize_to_u64),
        output_tokens: usage.output_tokens.map(usize_to_u64),
        reasoning_tokens: usage.reasoning_tokens.map(usize_to_u64),
        total_tokens: usage.total_tokens.map(usize_to_u64),
        cache_read_tokens: usage.cached_input_tokens.map(usize_to_u64),
        cache_write_tokens: usage.cache_creation_input_tokens.map(usize_to_u64),
        audio_input_tokens: usage.audio_input_tokens.map(usize_to_u64),
        audio_output_tokens: usage.audio_output_tokens.map(usize_to_u64),
        billing_tier: None,
    }
}

/// Converts a platform-sized token count without truncation on supported targets.
/// 在受支持目标上无截断地转换平台字长 Token 计数。
fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).expect("usize must fit into the V1 u64 token contract")
}

/// Freezes the explicit continuation policy from the same structured facts used by transport retry safety.
/// 使用传输重试安全策略所依赖的同一组结构化事实冻结显式继续策略。
///
/// # Parameters
/// 参数
/// `error` is the exact normalized transport failure and `retry_exhausted` is the terminal retry
/// decision for the physical request.
/// `error` 是精确的规范化传输失败，`retry_exhausted` 是该物理请求的终态重试判定。
///
/// # Returns
/// 返回值
/// Returns `Allowed` only for Core-mandated recoverable reasons or a Provider failure that the
/// existing replay-safety predicate proves has no observed semantic output, side effect, or
/// unknown dispatch outcome.
/// 仅当 Core 规定该原因可恢复，或既有重放安全谓词证明 Provider 失败未观察到语义输出、副作用及未知派发结果时返回 `Allowed`。
pub(crate) fn provider_failure_continuation_eligibility(
    error: Option<&AiExecutionError>,
    retry_exhausted: bool,
) -> ContinuationEligibility {
    let reason = if error.is_some_and(|value| value.category == AiErrorCategory::Cancelled) {
        ApiEndReason::Cancelled
    } else if retry_exhausted {
        ApiEndReason::RetryExhausted
    } else {
        provider_end_reason(error)
    };
    match reason {
        ApiEndReason::UserInterrupted
        | ApiEndReason::ProcessInterrupted
        | ApiEndReason::NetworkError
        | ApiEndReason::BalanceInsufficient
        | ApiEndReason::RetryExhausted => ContinuationEligibility::Allowed,
        ApiEndReason::ProviderError
            if error.is_some_and(|value| {
                !value.semantic_output_observed
                    && !value.side_effect_observed
                    && value.dispatch_outcome != DispatchOutcome::Unknown
            }) =>
        {
            ContinuationEligibility::Allowed
        }
        ApiEndReason::CompletedSuccessfully
        | ApiEndReason::ProviderError
        | ApiEndReason::Cancelled
        | ApiEndReason::Unknown => ContinuationEligibility::Forbidden,
    }
}

/// Maps one structured execution error to the stable V1 end reason.
/// 把一个结构化执行错误映射为稳定 V1 结束原因。
fn provider_end_reason(error: Option<&AiExecutionError>) -> ApiEndReason {
    let Some(error) = error else {
        return ApiEndReason::Unknown;
    };
    match error.category {
        AiErrorCategory::PaymentRequired => ApiEndReason::BalanceInsufficient,
        AiErrorCategory::Cancelled => ApiEndReason::Cancelled,
        AiErrorCategory::Timeout => ApiEndReason::NetworkError,
        _ if error.dispatch_outcome == DispatchOutcome::Unknown && error.transport.is_some() => {
            ApiEndReason::NetworkError
        }
        AiErrorCategory::Authentication
        | AiErrorCategory::PermissionDenied
        | AiErrorCategory::RateLimited
        | AiErrorCategory::ProviderOverloaded
        | AiErrorCategory::ProviderUnavailable
        | AiErrorCategory::InvalidRequest
        | AiErrorCategory::ContextLengthExceeded
        | AiErrorCategory::ContentPolicy => ApiEndReason::ProviderError,
        AiErrorCategory::Unclassified => ApiEndReason::Unknown,
    }
}

/// Creates one V1 redacted error from structured provider-neutral facts.
/// 从结构化 Provider 中立事实创建一条 V1 脱敏错误。
fn redacted_api_error(error: Option<&AiExecutionError>) -> RedactedApiError {
    let Some(error) = error else {
        return RedactedApiError {
            category: ApiErrorCategory::Unknown,
            error_code: None,
            message: String::from("The Provider request failed without structured error facts."),
        };
    };
    // Use the same transport classification as the terminal reason, including unknown dispatch.
    // 与终态原因使用同一传输分类，包含分派结果未知的场景。
    let category = match error.category {
        AiErrorCategory::Timeout => ApiErrorCategory::Timeout,
        _ if provider_end_reason(Some(error)) == ApiEndReason::NetworkError => {
            ApiErrorCategory::Network
        }
        AiErrorCategory::Authentication | AiErrorCategory::PermissionDenied => {
            ApiErrorCategory::Auth
        }
        AiErrorCategory::PaymentRequired => ApiErrorCategory::Balance,
        AiErrorCategory::InvalidRequest | AiErrorCategory::ContextLengthExceeded => {
            ApiErrorCategory::Protocol
        }
        AiErrorCategory::RateLimited
        | AiErrorCategory::ProviderOverloaded
        | AiErrorCategory::ProviderUnavailable
        | AiErrorCategory::ContentPolicy => ApiErrorCategory::Provider,
        AiErrorCategory::Cancelled | AiErrorCategory::Unclassified => ApiErrorCategory::Unknown,
    };
    RedactedApiError {
        category,
        error_code: error.upstream_code.clone(),
        message: error.message.clone(),
    }
}

/// Builds one deterministic typed Provider source for crate-local tests.
/// 为 crate 内测试构建一份确定性的强类型 Provider 来源。
#[cfg(test)]
pub(crate) fn test_provider_v1_source_input() -> ProviderSourceInput {
    ProviderSourceInput {
        provider: String::from("provider-key"),
        profile: Some(String::from("test-profile")),
        model: String::from("model-key"),
        connection_id: Some(String::from("connection-1")),
        message_id: Some(String::from("provider-message-1")),
        response_id: None,
        replay: Some(ProviderReplayEnvelope::new(
            "provider-key",
            "connection-1",
            "https://provider.invalid/v1",
            "test-profile",
            "model-key",
            Vec::new(),
            Vec::new(),
        )),
    }
}

#[cfg(test)]
mod terminal_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_runtime::provider_request_telemetry::ProviderRequestTelemetryContext;
    use vulcan_core::types::{ConversationAttachmentKind, ProviderContinuation};

    /// Builds one deterministic successful tool result for resource-contract tests.
    /// 为资源契约测试构建一条确定性的成功工具结果。
    fn resource_tool_result(
        tool_name: &str,
        host_result: serde_json::Value,
    ) -> ToolInvocationResult {
        ToolInvocationResult {
            tool_call_id: String::from("tool-call-1"),
            tool_name: tool_name.to_string(),
            model_output: String::from("model output must not be parsed"),
            host_result: Some(host_result),
            is_error: false,
        }
    }

    /// Generated attachments are read only from the exact producer-owned host payload.
    /// 生成附件只从精确的生产者自有宿主载荷读取。
    #[test]
    fn generated_attachments_use_exact_host_payload() {
        let result = resource_tool_result(
            "image_generation_dynamic_tool",
            serde_json::json!({
                "kind": GENERATED_ATTACHMENTS_HOST_RESULT_KIND,
                "payload": {
                    "outputs": [{
                        "kind": "image",
                        "file_name": "generated.png",
                        "local_path": "D:/managed/generated.png",
                        "byte_len": 4,
                        "source_sha256": "00",
                        "mime_type": "image/png"
                    }],
                    "upstream_response_id": "response-1"
                }
            }),
        );

        let attachments = provider_v1_generated_attachments_from_tool_result(&result)
            .expect("generated attachment payload should be accepted");

        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].kind, ConversationAttachmentKind::Image);
        assert_eq!(attachments[0].file_name, "generated.png");
        assert_eq!(attachments[0].local_path, "D:/managed/generated.png");
        assert_eq!(attachments[0].byte_len, Some(4));
        assert_eq!(attachments[0].source_sha256.as_deref(), Some("00"));
    }

    /// Matching generated-resource payloads reject unknown attachment fields instead of ignoring drift.
    /// 匹配的生成资源载荷会拒绝未知附件字段，而不是忽略契约漂移。
    #[test]
    fn generated_attachments_reject_unknown_attachment_fields() {
        let result = resource_tool_result(
            "image_generation_dynamic_tool",
            serde_json::json!({
                "kind": GENERATED_ATTACHMENTS_HOST_RESULT_KIND,
                "payload": {
                    "outputs": [{
                        "kind": "image",
                        "file_name": "generated.png",
                        "local_path": "D:/managed/generated.png",
                        "mime_type": "image/png",
                        "unexpected": true
                    }]
                }
            }),
        );

        assert!(matches!(
            provider_v1_generated_attachments_from_tool_result(&result),
            Err(ProviderV1GeneratedResourceError::MalformedPayload { .. })
        ));
    }

    /// Unrelated host results never become generated resources through output-text inference.
    /// 无关宿主结果绝不会通过输出文本推断为生成资源。
    #[test]
    fn unrelated_host_result_does_not_produce_generated_attachments() {
        let result = resource_tool_result(
            "ordinary_tool",
            serde_json::json!({
                "kind": "ordinary_result",
                "payload": { "path": "D:/must-not-be-inferred.png" }
            }),
        );

        assert!(
            provider_v1_generated_attachments_from_tool_result(&result)
                .expect("unrelated host result should be ignored")
                .is_empty()
        );
    }

    /// Stateful tools expose only their exact producer-owned navigation identities.
    /// 有状态工具只暴露其精确生产者自有导航身份。
    #[test]
    fn stateful_tool_external_refs_use_exact_host_schemas() {
        let terminal = resource_tool_result(
            TERMINAL_SESSION_TOOL_NAME,
            serde_json::json!({
                "kind": TERMINAL_SESSION_HOST_RESULT_KIND,
                "payload": {
                    "action": "start",
                    "terminal_session_id": "terminal-1"
                }
            }),
        );
        let lua = resource_tool_result(
            RUNLUA_SESSION_TOOL_NAME,
            serde_json::json!({
                "kind": RUNLUA_SESSION_HOST_RESULT_KIND,
                "payload": {
                    "action": "eval",
                    "status": "completed",
                    "sessionId": "lua-1"
                }
            }),
        );
        let subagent = resource_tool_result(
            SPAWN_AGENT_TOOL,
            serde_json::json!({
                "kind": SUBAGENT_TASK_KIND,
                "payload": {(SUBAGENT_TASK_ID_KEY): "subagent-1"}
            }),
        );

        assert_eq!(
            provider_v1_external_refs_from_tool_result(&terminal).expect("terminal resource"),
            vec![ExternalResourceRef {
                kind: ExternalResourceKind::Terminal,
                id: String::from("terminal-1"),
            }]
        );
        assert_eq!(
            provider_v1_external_refs_from_tool_result(&lua).expect("Lua resource"),
            vec![ExternalResourceRef {
                kind: ExternalResourceKind::LuaSession,
                id: String::from("lua-1"),
            }]
        );
        assert_eq!(
            provider_v1_external_refs_from_tool_result(&subagent).expect("subagent resource"),
            vec![ExternalResourceRef {
                kind: ExternalResourceKind::Subagent,
                id: String::from("subagent-1"),
            }]
        );
    }

    /// Terminal list keeps producer order while ordinary tools and failed starts add no references.
    /// 终端列表保持生产者顺序，普通工具与失败启动不增加引用。
    #[test]
    fn list_and_non_resource_tool_results_have_explicit_reference_rules() {
        let terminal_list = resource_tool_result(
            TERMINAL_SESSION_TOOL_NAME,
            serde_json::json!({
                "kind": TERMINAL_SESSION_HOST_RESULT_KIND,
                "payload": {
                    "action": "list",
                    "terminal_sessions": [
                        {"terminal_session_id": "terminal-1"},
                        {"terminal_session_id": "terminal-2"}
                    ]
                }
            }),
        );
        let failed_lua_start = resource_tool_result(
            RUNLUA_SESSION_TOOL_NAME,
            serde_json::json!({
                "kind": RUNLUA_SESSION_HOST_RESULT_KIND,
                "payload": {"action": "start", "status": "failed"}
            }),
        );
        let ordinary = resource_tool_result(
            "shell_command",
            serde_json::json!({"kind": "command_result", "payload": {}}),
        );

        assert_eq!(
            provider_v1_external_refs_from_tool_result(&terminal_list)
                .expect("terminal list resources")
                .into_iter()
                .map(|reference| reference.id)
                .collect::<Vec<_>>(),
            vec![String::from("terminal-1"), String::from("terminal-2")]
        );
        assert!(
            provider_v1_external_refs_from_tool_result(&failed_lua_start)
                .expect("failed Lua start has no lease")
                .is_empty()
        );
        assert!(
            provider_v1_external_refs_from_tool_result(&ordinary)
                .expect("ordinary tools have no navigation resource")
                .is_empty()
        );
    }

    /// A successful stateful tool with a wrong host-result kind fails instead of guessing fields.
    /// 成功的有状态工具返回错误宿主结果类型时显式失败而不是猜测字段。
    #[test]
    fn stateful_tool_rejects_wrong_host_result_kind() {
        let result = resource_tool_result(
            TERMINAL_SESSION_TOOL_NAME,
            serde_json::json!({
                "kind": "command_session",
                "payload": {"terminal_session_id": "terminal-1"}
            }),
        );
        assert!(matches!(
            provider_v1_external_refs_from_tool_result(&result),
            Err(ProviderV1ExternalResourceError::HostResultKindMismatch { .. })
        ));
    }

    /// A retry changes only the physical API purpose and preserves exact request configuration.
    /// 重试只改变物理 API 用途，并保留精确请求配置。
    #[test]
    fn retry_facts_use_typed_retry_purpose() {
        let context = ProviderRequestTelemetryContext::for_test();
        let snapshot = CoreProviderRequestSnapshot {
            provider_id: String::from("mock-provider"),
            model_id: String::from("mock-model"),
            system_prompt: String::new(),
            prompt_layers: Default::default(),
            prompt: String::new(),
            prompt_in_history: false,
            history: Vec::new(),
            temperature: Some(0.25),
            max_turns: 1,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: Default::default(),
        };
        let facts = context
            .v1_request_template
            .request_facts(String::from("call-2"), &snapshot, 1, false, 10)
            .expect("conversation requests should produce V1 facts");
        assert_eq!(facts.purpose, ApiPurposeKind::RetryGeneration);
        assert_eq!(facts.request.provider, "mock-provider");
        assert_eq!(facts.request.temperature, Some(0.25));
    }

    /// Manual Continue binds its action only to the resumed run's first physical request.
    /// 手动 Continue 仅将其动作绑定到恢复运行的首个物理请求。
    #[test]
    fn continue_action_is_not_reused_after_the_first_physical_request() {
        let snapshot = CoreProviderRequestSnapshot {
            provider_id: String::from("mock-provider"),
            model_id: String::from("mock-model"),
            system_prompt: String::new(),
            prompt_layers: Default::default(),
            prompt: String::new(),
            prompt_in_history: false,
            history: Vec::new(),
            temperature: None,
            max_turns: 1,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: Default::default(),
        };
        let action_id = ClientActionId::try_new("continue-action").expect("action id is valid");
        let template = ProviderV1RequestTemplate {
            base_purpose: Some(ApiPurposeKind::MainGeneration),
            continuation_action_id: Some(action_id.clone()),
            profile: String::from("openai-responses.v1"),
            connection_id: String::from("connection-1"),
            thinking_level: None,
        };

        let first = template
            .request_facts(String::from("call-1"), &snapshot, 0, true, 10)
            .expect("first continued request should produce facts");
        assert_eq!(first.purpose, ApiPurposeKind::Continue);
        assert_eq!(first.continuation_action_id, Some(action_id));

        let after_tool = template
            .request_facts(String::from("call-2"), &snapshot, 0, false, 20)
            .expect("tool-followup request should produce facts");
        assert_eq!(after_tool.purpose, ApiPurposeKind::MainGeneration);
        assert!(after_tool.continuation_action_id.is_none());

        // Auxiliary requests retain purpose through the same continuation and retry machinery.
        // 辅助请求经过同一续跑和重试链时保留自身用途。
        for auxiliary_purpose in [
            ApiPurposeKind::QuestionExecution,
            ApiPurposeKind::Compaction,
        ] {
            // Each auxiliary kind exercises both retries and accidental continuation binding.
            // 每类辅助用途都覆盖重试及意外继续绑定。
            let auxiliary = ProviderV1RequestTemplate {
                base_purpose: Some(auxiliary_purpose),
                ..template.clone()
            };
            for retry_count in [0, 1, 2] {
                for bind_continuation in [false, true] {
                    // Each physical attempt must remain excluded from business turn closure.
                    // 每次物理尝试都必须保持在业务回合闭合逻辑之外。
                    let facts = auxiliary
                        .request_facts(
                            String::from("qa-call"),
                            &snapshot,
                            retry_count,
                            bind_continuation,
                            30,
                        )
                        .expect("auxiliary execution produces request facts");
                    assert_eq!(facts.purpose, auxiliary_purpose);
                    assert!(facts.continuation_action_id.is_none());
                }
            }
        }
    }

    /// Structured payment failures map to balance facts without reading telemetry JSON.
    /// 结构化支付失败无需读取遥测 JSON 即可映射为余额事实。
    #[test]
    fn payment_failure_maps_to_balance_terminal() {
        let request_facts = ProviderV1RequestFacts {
            provider_call_id: String::from("call-1"),
            purpose: ApiPurposeKind::MainGeneration,
            continuation_action_id: None,
            request: ApiRequestSnapshot::new("provider", "model"),
            prepared_at_ms: 1,
        };
        let error = AiExecutionError {
            category: AiErrorCategory::PaymentRequired,
            subcategory: None,
            stage: String::from("response"),
            http_status: Some(402),
            transport: None,
            websocket_close_code: None,
            upstream_code: Some(String::from("insufficient_balance")),
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: String::from("Insufficient balance."),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        };
        let terminal = ProviderV1ApiTerminalFacts::failed(
            request_facts,
            &CoreProviderUsageSnapshot::default(),
            ApiTimings::default(),
            Some(&error),
            false,
            provider_failure_continuation_eligibility(Some(&error), false),
            2,
        );
        assert_eq!(terminal.end_reason, Some(ApiEndReason::BalanceInsufficient));
        assert_eq!(
            terminal.error.as_ref().map(|value| value.category),
            Some(ApiErrorCategory::Balance)
        );
    }

    /// Completed response extraction preserves the exact typed replay envelope and cursor identity.
    /// 已完成响应提取会保留精确强类型重放封套与游标身份。
    #[test]
    fn completed_response_produces_exact_provider_source() {
        let replay = ProviderReplayEnvelope::new(
            "provider-1",
            "connection-1",
            "https://provider.invalid/v1/responses",
            "profile-1",
            "model-1",
            Vec::new(),
            Vec::new(),
        );
        let response = CoreProviderTurnResponse {
            assistant_text: String::from("answer"),
            assistant_reasoning: String::new(),
            provider_message_id: Some(String::from("response-1")),
            provider_message_state: Some(serde_json::json!({
                "vulcan_ai": {"replay": replay}
            })),
            usage: None,
            tool_calls: Vec::new(),
            continuation: Some(ProviderContinuation {
                provider_id: Some(String::from("provider-1")),
                protocol_profile_id: Some(String::from("profile-1")),
                model_id: Some(String::from("model-1")),
                last_provider_message_id: Some(String::from("response-1")),
                ..ProviderContinuation::default()
            }),
        };

        let source =
            provider_v1_source_from_completed_response(&response).expect("exact response source");
        assert_eq!(source.provider, "provider-1");
        assert_eq!(source.connection_id.as_deref(), Some("connection-1"));
        assert_eq!(source.message_id.as_deref(), Some("response-1"));
        assert_eq!(source.replay, Some(replay));
    }

    /// A continuation that disagrees with the replay route fails explicitly.
    /// 与重放路由不一致的 continuation 会显式失败。
    #[test]
    fn completed_response_rejects_replay_route_mismatch() {
        let response = CoreProviderTurnResponse {
            assistant_text: String::new(),
            assistant_reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: Some(serde_json::json!({
                "vulcan_ai": {
                    "replay": ProviderReplayEnvelope::new(
                        "provider-1",
                        "connection-1",
                        "route-1",
                        "profile-1",
                        "model-1",
                        Vec::new(),
                        Vec::new(),
                    )
                }
            })),
            usage: None,
            tool_calls: Vec::new(),
            continuation: Some(ProviderContinuation {
                provider_id: Some(String::from("provider-2")),
                protocol_profile_id: Some(String::from("profile-1")),
                model_id: Some(String::from("model-1")),
                ..ProviderContinuation::default()
            }),
        };

        assert!(matches!(
            provider_v1_source_from_completed_response(&response),
            Err(ProviderV1SourceError::RouteIdentityMismatch {
                field: "provider_id"
            })
        ));
    }

    /// Builds one provider failure with the exact dispatch facts the eligibility rule inspects.
    /// 构建一个供应商失败，其分派事实正是可续期规则所检查的内容。
    fn eligibility_error(
        category: AiErrorCategory,
        dispatch_outcome: DispatchOutcome,
        semantic_output_observed: bool,
        side_effect_observed: bool,
    ) -> AiExecutionError {
        AiExecutionError {
            category,
            subcategory: None,
            stage: String::from("response"),
            http_status: Some(500),
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: String::from("provider failed"),
            dispatch_outcome,
            semantic_output_observed,
            side_effect_observed,
            terminal: true,
            retryable: false,
            retry_after: None,
        }
    }

    /// Verifies provider errors only resume when the failed attempt is proven replay-safe.
    /// 验证供应商错误仅在失败尝试被证明可安全重放时才允许续跑。
    ///
    /// # Notes
    /// 说明
    /// This rule decides whether a failed provider attempt may be resumed at all, yet it previously had
    /// no direct assertion: its result was only carried into terminal facts. The unsafe directions are
    /// asserted first because silently allowing a continuation after visible output or an observed side
    /// effect would replay work the provider may already have performed.
    /// 本规则决定失败尝试能否续跑，但此前没有任何直接断言：其返回值只被带进终态事实。这里先断言不安全
    /// 方向，因为在已产生可见输出或已观测副作用后静默允许续跑会重放供应商可能已完成的工作。
    #[test]
    fn provider_continuation_eligibility_requires_a_replay_safe_attempt() {
        // Any visible semantic output or observed side effect forbids resumption regardless of category.
        // 任何可见语义输出或已观测副作用都会禁止续跑，与类别无关。
        for (semantic, side_effect) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                provider_failure_continuation_eligibility(
                    Some(&eligibility_error(
                        AiErrorCategory::ProviderUnavailable,
                        DispatchOutcome::Dispatched,
                        semantic,
                        side_effect,
                    )),
                    false,
                ),
                ContinuationEligibility::Forbidden,
                "replay-unsafe attempt must forbid continuation (semantic={semantic}, side_effect={side_effect})"
            );
        }
        // An unknown dispatch outcome cannot prove the attempt was never performed.
        // 未知分派结果无法证明该尝试从未被执行。
        assert_eq!(
            provider_failure_continuation_eligibility(
                Some(&eligibility_error(
                    AiErrorCategory::ProviderUnavailable,
                    DispatchOutcome::Unknown,
                    false,
                    false,
                )),
                false,
            ),
            ContinuationEligibility::Forbidden,
            "an unknown dispatch outcome must forbid continuation"
        );
        // A dispatched, replay-safe provider error may resume.
        // 已分派且可安全重放的供应商错误可以续跑。
        assert_eq!(
            provider_failure_continuation_eligibility(
                Some(&eligibility_error(
                    AiErrorCategory::ProviderUnavailable,
                    DispatchOutcome::Dispatched,
                    false,
                    false,
                )),
                false,
            ),
            ContinuationEligibility::Allowed,
            "a replay-safe provider error must allow continuation"
        );
        // Exhausted retries and interrupted attempts remain resumable, and a missing error never is.
        // 重试耗尽与被打断的尝试仍可续跑，而缺失错误一律不可续跑。
        assert_eq!(
            provider_failure_continuation_eligibility(None, true),
            ContinuationEligibility::Allowed,
            "exhausted retries must allow continuation"
        );
        assert_eq!(
            provider_failure_continuation_eligibility(
                Some(&eligibility_error(
                    AiErrorCategory::Cancelled,
                    DispatchOutcome::Dispatched,
                    false,
                    false,
                )),
                false,
            ),
            ContinuationEligibility::Forbidden,
            "a completed cancellation must forbid continuation"
        );
        assert_eq!(
            provider_failure_continuation_eligibility(None, false),
            ContinuationEligibility::Forbidden,
            "an absent error cannot authorize continuation"
        );
    }
}
