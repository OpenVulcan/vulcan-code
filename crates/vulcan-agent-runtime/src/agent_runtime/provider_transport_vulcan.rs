//! Vulcan public-contract adapter for the existing host tool controller.
//! 面向现有宿主工具控制器的 Vulcan 公共契约适配器。

use crate::agent_runtime::events::provider_request_durability_barrier;
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot;
use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_entry_inputs::{
    PreparedProviderTransportStreamLaunch, prepare_provider_transport_stream_launch,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_request::ProviderTransportExecutionRequest;
use crate::agent_runtime::provider_transport_stream_runtime::execute_host_controlled_stream_with_prepared_inputs;
use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
use crate::attachment_assets::{AttachmentDescriptor, stage_attachment_bytes};
use async_stream::try_stream;
use async_trait::async_trait;
use base64::Engine;
use futures_util::StreamExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_ai_contract::{
    AiContinuation, AiExecutionRequest, AiMessage, AiOperation, AiStreamEvent, ContentItem,
    MessageRole, ModelParameterValue, NativeToolKind, ParallelToolCallPolicy, PromptCachePolicy,
    ProviderReplayEnvelope, ReasoningEffort, ReasoningOutputPolicy, RequestUsage, ResourceKind,
    ResourceReference, StructuredOutputPolicy, ToolCall as PublicToolCall, ToolChoicePolicy,
    ToolDefinition, ToolResult, UsageValue, canonicalize_codex_responses_replay_items,
    is_replayable_codex_responses_item,
};
use vulcan_context::{SessionConversationState, SessionMessageEntry};
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationAttachmentKind,
    ConversationPayload as CoreConversationPayload,
    ConversationSnapshot as CoreConversationSnapshot, ConversationToolCall,
    CoreError as VulcanCoreError, ProviderContinuation,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema, ProviderTurnDelta,
    ProviderTurnResponse as CoreProviderTurnResponse, ProviderUsageSnapshot,
};
use vulcan_provider_adapters::{
    CredentialExecutionSecret, InputResourceConstraint, ProviderExecutionClient,
    ProviderExecutionRoute, ProviderWireRequestCapture, ResolvedPublicAiExecution,
    ResourceReferencePolicy,
};
use vulcan_tools_manager::{
    VulcanToolSchemaProjectionContext, project_tool_parameters_schema,
    resolve_tool_schema_projection_profile,
};

/// Vulcan-owned default output budget used when the provider reports no independent output limit.
/// 当供应商未报告独立输出上限时使用的 Vulcan 本地默认输出预算。
const DEFAULT_PUBLIC_OUTPUT_TOKEN_LIMIT: u64 = 32_000;
/// Versioned on-disk contract for complete provider request traces.
/// 完整 Provider 请求追踪使用的版本化磁盘契约。
const PROVIDER_REQUEST_TRACE_SCHEMA_VERSION: &str = "vulcan.provider-request-trace.v2";
/// Process-local monotonic tie breaker used when multiple requests start within one millisecond.
/// 多个请求在同一毫秒启动时使用的进程本地单调消歧序号。
static PROVIDER_REQUEST_TRACE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Returns the current Unix timestamp in milliseconds for an ordered trace identity.
/// 返回用于有序追踪标识的当前 Unix 毫秒时间戳。
///
/// # Returns
/// 返回值
/// The current timestamp, or an explicit clock error when the host clock predates Unix epoch.
/// 返回当前时间戳；当宿主时钟早于 Unix 纪元时返回明确时钟错误。
fn provider_request_trace_timestamp_ms() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| format!("Provider request trace clock failed: {error}"))
}

/// Computes the stable SHA-256 digest of one serialized JSON stage.
/// 计算一个已序列化 JSON 阶段的稳定 SHA-256 摘要。
///
/// # Parameters
/// 参数
/// `value` is the exact diagnostic JSON value whose bytes must be compared.
/// `value` 是需要按字节比较的精确诊断 JSON 值。
///
/// # Returns
/// 返回值
/// A lowercase hexadecimal digest, or an explicit serialization error.
/// 返回小写十六进制摘要，或明确的序列化错误。
fn provider_request_trace_sha256(value: &Value) -> Result<String, String> {
    let encoded = serde_json::to_vec(value)
        .map_err(|error| format!("Provider request trace digest serialization failed: {error}"))?;
    let mut hasher = Sha256::new();
    hasher.update(encoded);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Computes per-field digests for every top-level JSON object member.
/// 为 JSON 对象的每个顶层成员计算逐字段摘要。
///
/// # Parameters
/// 参数
/// `value` is the complete canonical or wire request stage.
/// `value` 是完整的规范请求或 Wire 请求阶段。
///
/// # Returns
/// 返回值
/// A lexically ordered field-to-digest map; non-object values produce an empty map.
/// 返回按字典序排列的字段到摘要映射；非对象值返回空映射。
fn provider_request_trace_top_level_digests(
    value: &Value,
) -> Result<BTreeMap<String, String>, String> {
    let Some(object) = value.as_object() else {
        return Ok(BTreeMap::new());
    };
    object
        .iter()
        .map(|(field, value)| {
            provider_request_trace_sha256(value).map(|digest| (field.clone(), digest))
        })
        .collect()
}

/// Redacts credential-shaped header values before a trace reaches disk.
/// 在追踪写入磁盘前脱敏具有凭据形态的请求头值。
///
/// # Parameters
/// 参数
/// `headers` are the final protocol-projected non-transport headers.
/// `headers` 是协议最终投影的非传输请求头。
///
/// # Returns
/// 返回值
/// A complete header map with sensitive values replaced by a fixed marker.
/// 返回完整请求头映射，其中敏感值被固定标记替换。
fn provider_request_trace_headers(headers: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| {
            let normalized = name.trim().to_ascii_lowercase();
            let sensitive = matches!(
                normalized.as_str(),
                "authorization"
                    | "proxy-authorization"
                    | "x-api-key"
                    | "api-key"
                    | "x-goog-api-key"
                    | "anthropic-api-key"
                    | "cookie"
                    | "set-cookie"
            ) || normalized.contains("token")
                || normalized.contains("secret")
                || normalized.ends_with("-key");
            (
                name.clone(),
                if sensitive {
                    String::from("<redacted>")
                } else {
                    value.clone()
                },
            )
        })
        .collect()
}

/// Resolves the requested output budget while treating catalog output limits as upper bounds.
/// 在将目录输出限制视为上限的前提下解析请求输出预算。
///
/// # Parameters
/// 参数
/// `configured_limit` is the optional user narrowing and `catalog_limit` is the provider-reported
/// model maximum.
/// `configured_limit` 是可选的用户收窄值，`catalog_limit` 是供应商报告的模型最大值。
///
/// # Returns
/// 返回值
/// The configured or default request budget capped by the catalog maximum when present.
/// 返回配置值或默认请求预算；存在目录最大值时以其封顶。
fn resolved_public_output_token_limit(
    configured_limit: Option<u64>,
    catalog_limit: Option<u64>,
) -> u64 {
    let requested_limit = configured_limit.unwrap_or(DEFAULT_PUBLIC_OUTPUT_TOKEN_LIMIT);
    catalog_limit
        .map(|maximum| requested_limit.min(maximum))
        .unwrap_or(requested_limit)
}
/// Executes one public-contract provider run through the existing host controller.
/// 通过现有宿主控制器执行一次公共契约 Provider 运行。
///
/// # Parameters
/// 参数
/// `request` owns host controller inputs and `resolved` freezes the account, protocol, model,
/// secret, cache, and model-parameter route.
/// `request` 持有宿主控制器输入，`resolved` 冻结账号、协议、模型、秘密、缓存与模型参数路由。
///
/// # Returns
/// 返回值
/// A stream of existing workbench deltas projected from public events.
/// 返回从公共事件投影出的现有工作台增量流。
pub(crate) fn execute_vulcan_public_stream(
    request: ProviderTransportExecutionRequest,
    resolved: ResolvedPublicAiExecution,
    reasoning_effort: Option<ReasoningEffort>,
) -> ProviderDeltaStream {
    let provider_request_trace_enabled = request.provider_request_trace_enabled;
    let cache_scope_id = request.session_id.to_string();
    let workspace_path = request.workspace_path.clone();
    let output_token_limit = resolved_public_output_token_limit(
        resolved.overrides.output_limit,
        resolved.model.capabilities.output_limit,
    );
    let prompt_cache = resolved.overrides.prompt_cache.clone().unwrap_or_default();
    let PreparedProviderTransportStreamLaunch {
        runtime_inputs,
        core_snapshot,
        initial_provider_request,
        prompt_intervention,
    } = prepare_provider_transport_stream_launch(request);
    execute_host_controlled_stream_with_prepared_inputs(
        runtime_inputs,
        core_snapshot,
        move |live_sender, adapter_inputs| {
            VulcanPublicContractAdapter::new(
                provider_request_trace_enabled,
                resolved.route.clone(),
                resolved.model.id.clone(),
                cache_scope_id.clone(),
                resolved.secret.clone(),
                resolved.client.clone(),
                adapter_inputs.provider_tool_definitions,
                resolved.model.capabilities.provider_native_tools.clone(),
                output_token_limit,
                prompt_cache.clone(),
                reasoning_effort,
                resolved.overrides.model_parameters.clone(),
                resolved.model.capabilities.input_resources.clone(),
                resolved.model.capabilities.output_resources.clone(),
                workspace_path.clone(),
                live_sender,
                initial_provider_request.clone(),
            )
        },
        "Vulcan 公共契约",
        prompt_intervention,
    )
}

/// Captures one exact public wire request without dispatching network traffic.
/// 捕获一个精确的公共线级请求，但不分派网络流量。
///
/// # Parameters
/// 参数
/// `request` owns the same host inputs as normal execution and `resolved` freezes the route.
/// `request` 持有与正常执行相同的宿主输入，`resolved` 冻结执行路由。
///
/// # Returns
/// 返回值
/// A one-item stream containing the projected wire JSON or a controlled error.
/// 返回只包含投影线级 JSON 的单项流，或受控错误。
pub(crate) fn capture_vulcan_public_stream(
    request: ProviderTransportExecutionRequest,
    resolved: ResolvedPublicAiExecution,
    reasoning_effort: Option<ReasoningEffort>,
) -> ProviderDeltaStream {
    let cache_scope_id = request.session_id.to_string();
    let workspace_path = request.workspace_path.clone();
    let output_token_limit = resolved_public_output_token_limit(
        resolved.overrides.output_limit,
        resolved.model.capabilities.output_limit,
    );
    Box::pin(try_stream! {
        let prompt_cache = resolved.overrides.prompt_cache.clone().unwrap_or_default();
        let prepared = prepare_provider_transport_stream_launch(request);
        let controller_config = prepared.runtime_inputs.entry_inputs.controller_config;
        let provider_tool_definitions = prepared
            .runtime_inputs
            .tool_context
            .provider_tool_definitions;
        let (live_sender, _live_receiver) = tokio::sync::mpsc::unbounded_channel();
        let adapter = VulcanPublicContractAdapter::new(
            false,
            resolved.route.clone(),
            resolved.model.id.clone(),
            cache_scope_id,
            resolved.secret,
            resolved.client.clone(),
            provider_tool_definitions,
            resolved.model.capabilities.provider_native_tools,
            output_token_limit,
            prompt_cache,
            reasoning_effort,
            resolved.overrides.model_parameters,
            resolved.model.capabilities.input_resources,
            resolved.model.capabilities.output_resources,
            workspace_path,
            live_sender,
            prepared.initial_provider_request,
        );
        let public_request = adapter
            .build_request(&prepared.core_snapshot, &controller_config)
            .await
            .map_err(|error| error.to_string())?;
        let encoded = resolved
            .client
            .encode_effective_request_with_credential(
                &resolved.route,
                &public_request.public,
                &adapter.secret,
            )
            .await
            .map_err(|failure| failure.to_string())?;
        yield ProviderStreamDelta::WireRequest(encoded.body);
    })
}

/// Bridges Vulcan public requests and events into the host-owned controller.
/// 将 Vulcan 公共请求与事件桥接到宿主控制器。
#[derive(Clone)]
pub(crate) struct VulcanPublicContractAdapter {
    /// Frozen system-setting decision for complete request tracing during this controller run.
    /// 当前控制器运行期间冻结的完整请求追踪系统设置决策。
    provider_request_trace_enabled: bool,
    /// Immutable provider execution route.
    /// 不可变供应商执行路由。
    route: ProviderExecutionRoute,
    /// Exact model identifier fixed for this controller execution.
    /// 为本次控制器执行固定的精确模型标识。
    model_id: String,
    /// Stable session cache scope shared by every physical request in this controller run.
    /// 本次控制器运行中每个物理请求共享的稳定会话缓存作用域。
    cache_scope_id: String,
    /// Exact credential secret loaded by the host.
    /// 宿主加载的精确凭据秘钥。
    secret: CredentialExecutionSecret,
    /// Shared public-contract execution client.
    /// 共享公共契约执行客户端。
    client: ProviderExecutionClient,
    /// Model-visible host tool definitions.
    /// 模型可见的宿主工具定义。
    tools: Vec<ToolDefinition>,
    /// Source kind and label kept in the same order as the projected tool definitions.
    /// 与投影后工具定义保持同序的来源类型与标签。
    tool_sources: Vec<(String, String)>,
    /// Provider-native tools declared by the exact discovered model route.
    /// 精确发现模型路由声明的供应商原生工具。
    provider_native_tools: Vec<NativeToolKind>,
    /// Resolved output limit for every controller turn.
    /// 每个控制器回合使用的已解析输出限制。
    output_token_limit: u64,
    /// Resolved prompt-cache policy snapshot.
    /// 已解析提示词缓存策略快照。
    prompt_cache: PromptCachePolicy,
    /// Optional user-selected reasoning effort validated against the active catalog.
    /// 根据当前目录校验后的可选用户推理强度。
    reasoning_effort: Option<ReasoningEffort>,
    /// Validated model-specific values.
    /// 已校验模型独立参数值。
    model_parameters: std::collections::BTreeMap<String, ModelParameterValue>,
    /// Resource kinds declared by the exact discovered model.
    /// 精确发现模型声明的资源类别。
    input_resources: Vec<ResourceKind>,
    /// Output resource kinds declared by the exact discovered model.
    /// 精确发现模型声明的输出资源类别。
    output_resources: Vec<ResourceKind>,
    /// Canonical workspace path that owns materialized provider-generated resources.
    /// 拥有供应商生成资源落盘文件的规范工作区路径。
    workspace_path: String,
    /// Live host sender for tool request construction.
    /// 工具请求构造使用的宿主实时发送器。
    live_sender: ProviderLiveDeltaSender,
    /// One-shot exact public request restored from a claimed retry snapshot.
    /// 从已认领重试快照恢复的一次性精确公共请求。
    initial_provider_request: std::sync::Arc<std::sync::Mutex<Option<AiExecutionRequest>>>,
}

/// Carries the public request together with the controller budget snapshot.
/// 同时承载公共请求与控制器预算快照。
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct VulcanPublicContractRequest {
    /// Provider-neutral execution request.
    /// 供应商无关执行请求。
    pub(crate) public: AiExecutionRequest,
    /// Exact controller configuration used to build this public request.
    /// 用于构建当前公共请求的精确控制器配置。
    pub(crate) controller_config: CoreControllerConfig,
    /// Exact canonical controller snapshot used to build this public request.
    /// 用于构建当前公共请求的精确规范控制器快照。
    pub(crate) controller_snapshot: CoreConversationSnapshot,
    /// Exact canonical snapshot used by the request budget guard.
    /// 请求预算门使用的精确规范快照。
    pub(crate) budget: CoreProviderRequestSnapshot,
    /// Host context-budget decision captured from this exact canonical request.
    /// 从当前精确规范请求捕获的宿主上下文预算决策。
    #[serde(skip)]
    pub(crate) context_budget: Option<crate::context_budget::ContextBudgetSnapshot>,
}

impl VulcanPublicContractAdapter {
    /// Creates one immutable public-contract controller adapter.
    /// 创建一个不可变公共契约控制器适配器。
    ///
    /// # Parameters
    /// 参数
    /// `provider_request_trace_enabled` freezes the persisted diagnostic switch; `route`,
    /// `secret`, and `client` identify the exact execution path; `tools` are host definitions;
    /// `output_token_limit`, `prompt_cache`, and `model_parameters` are resolved before dispatch;
    /// `live_sender` projects tool construction to the WebUI.
    /// `provider_request_trace_enabled` 冻结持久化诊断开关；`route`、`secret` 与 `client`
    /// 标识精确执行路径；`tools` 是宿主工具定义；`output_token_limit`、`prompt_cache` 与
    /// `model_parameters` 在分派前解析；`live_sender` 将工具构造投影到 WebUI。
    ///
    /// # Returns
    /// 返回值
    /// A reusable adapter whose route remains fixed for the controller run.
    /// 返回在控制器运行期间路由保持固定的可复用适配器。
    pub(crate) fn new(
        provider_request_trace_enabled: bool,
        route: ProviderExecutionRoute,
        model_id: String,
        cache_scope_id: String,
        secret: CredentialExecutionSecret,
        client: ProviderExecutionClient,
        tools: Vec<ProviderToolDefinition>,
        provider_native_tools: Vec<NativeToolKind>,
        output_token_limit: u64,
        prompt_cache: PromptCachePolicy,
        reasoning_effort: Option<ReasoningEffort>,
        model_parameters: std::collections::BTreeMap<String, ModelParameterValue>,
        input_resources: Vec<ResourceKind>,
        output_resources: Vec<ResourceKind>,
        workspace_path: String,
        live_sender: ProviderLiveDeltaSender,
        initial_provider_request: Option<AiExecutionRequest>,
    ) -> Self {
        // toolSources preserves exact source ownership before protocol projection removes host metadata.
        // toolSources 在协议投影移除宿主元数据前保留精确来源归属。
        let tool_sources = tools
            .iter()
            .map(|tool| (tool.source_kind.clone(), tool.source_label.clone()))
            .collect();
        // publicTools applies the same route-bound schema projection used by standalone remote compaction.
        // publicTools 应用与独立远端压缩相同的路由绑定 Schema 投影。
        let public_tools = project_public_tool_definitions(&route, model_id.as_str(), tools);
        Self {
            provider_request_trace_enabled,
            route,
            model_id,
            cache_scope_id,
            secret,
            client,
            tools: public_tools,
            tool_sources,
            provider_native_tools,
            output_token_limit,
            prompt_cache,
            reasoning_effort,
            model_parameters,
            input_resources,
            output_resources,
            workspace_path,
            live_sender,
            initial_provider_request: std::sync::Arc::new(std::sync::Mutex::new(
                initial_provider_request,
            )),
        }
    }

    /// Persists the controller, canonical, and actually dispatched API request for one execution.
    /// 为一次执行持久化控制器请求、规范请求与实际分派的 API 请求。
    ///
    /// # Parameters
    /// 参数
    /// `controller_request` is the complete secret-free controller request captured before ownership moves.
    /// `controller_request` 是所有权移动前捕获的不含秘钥完整控制器请求。
    /// `public` is the immutable provider-neutral request used by dispatch.
    /// `public` 是分派使用的不可变供应商无关请求。
    /// `execution_id` is the physical call identity shared with the real execution client.
    /// `execution_id` 是与真实执行客户端共享的物理调用标识。
    /// `captured` is the exact secret-safe request projected immediately before HTTP or WebSocket dispatch.
    /// `captured` 是在 HTTP 或 WebSocket 分派前立即投影的精确无秘密请求。
    ///
    /// # Returns
    /// 返回值
    /// Success only after the trace has been atomically published in the owning session directory.
    /// 仅在追踪已原子发布到所属会话目录后返回成功。
    async fn persist_complete_provider_request_trace(
        &self,
        controller_request: Value,
        public: &AiExecutionRequest,
        execution_id: &str,
        captured: ProviderWireRequestCapture,
    ) -> Result<(), VulcanCoreError> {
        let captured_at_unix_ms = provider_request_trace_timestamp_ms()
            .map_err(|error| provider_failure(self.provider_id(), &error))?;
        let sequence = PROVIDER_REQUEST_TRACE_SEQUENCE
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        let session_id = uuid::Uuid::parse_str(self.cache_scope_id.as_str()).map_err(|error| {
            provider_failure(
                self.provider_id(),
                &format!(
                    "Provider request trace session id `{}` is invalid: {error}",
                    self.cache_scope_id
                ),
            )
        })?;
        let public_request = serde_json::to_value(public).map_err(|error| {
            provider_failure(
                self.provider_id(),
                &format!("Canonical provider request trace serialization failed: {error}"),
            )
        })?;
        let controller_sha256 = provider_request_trace_sha256(&controller_request)
            .map_err(|error| provider_failure(self.provider_id(), &error))?;
        let public_sha256 = provider_request_trace_sha256(&public_request)
            .map_err(|error| provider_failure(self.provider_id(), &error))?;
        let wire_body_sha256 = provider_request_trace_sha256(&captured.body)
            .map_err(|error| provider_failure(self.provider_id(), &error))?;
        let public_field_sha256 = provider_request_trace_top_level_digests(&public_request)
            .map_err(|error| provider_failure(self.provider_id(), &error))?;
        let wire_field_sha256 = provider_request_trace_top_level_digests(&captured.body)
            .map_err(|error| provider_failure(self.provider_id(), &error))?;
        let headers = provider_request_trace_headers(&captured.headers);
        let trace = json!({
            "schema_version": PROVIDER_REQUEST_TRACE_SCHEMA_VERSION,
            "captured_at_unix_ms": captured_at_unix_ms,
            "process_sequence": sequence,
            "session_id": self.cache_scope_id,
            "execution_id": execution_id,
            "model_id": self.model_id,
            "route": {
                "provider": &self.route.provider,
                "service_entry": &self.route.entry,
                "connection": &self.route.connection,
                "protocol_route": &self.route.protocol_route,
                "secondary_route": &self.route.secondary_route,
                "custom_model_projection": &self.route.custom_model_projection,
                "request_projection_profiles": &self.route.request_projection_profiles,
                "response_projection_profiles": &self.route.response_projection_profiles,
                "upload_channels": &self.route.upload_channels,
                "manifest_revision": self.route.manifest_revision,
                "manifest_generation": self.route.manifest_generation,
                "route_revision": self.route.route_revision,
            },
            "digests": {
                "controller_request_sha256": controller_sha256,
                "canonical_public_request_sha256": public_sha256,
                "api_request_body_sha256": wire_body_sha256,
                "canonical_public_top_level_sha256": public_field_sha256,
                "api_request_top_level_sha256": wire_field_sha256,
            },
            "stages": {
                "controller_request": controller_request,
                "canonical_public_request": public_request,
            },
            "api_request": {
                "capture_point": "transport_dispatch",
                "transport": captured.transport,
                "method": captured.method,
                "path": captured.path,
                "resolved_url": captured.resolved_url,
                "headers": headers,
                "credential_values_redacted": true,
                "body": captured.body,
            },
        });
        let file_stem = format!("{captured_at_unix_ms}-{sequence:06}-{execution_id}");
        let data_root = crate::data_plane::application_data_root();
        let trace_path = tokio::task::spawn_blocking(move || {
            vulcan_workspace_store::workspace_store::persist_provider_request_trace(
                &data_root,
                session_id,
                file_stem.as_str(),
                &trace,
            )
        })
        .await
        .map_err(|error| {
            provider_failure(
                self.provider_id(),
                &format!("Provider request trace writer task failed: {error}"),
            )
        })?
        .map_err(|error| provider_failure(self.provider_id(), &error))?;
        eprintln!(
            "[provider-request-trace] session={} execution={} path={}",
            self.cache_scope_id,
            execution_id,
            trace_path.display()
        );
        Ok(())
    }
}

/// Maps one public protocol profile to the stable API-family key consumed by the shared tool-schema projector.
/// 将一个公共协议配置映射为共享工具 Schema 投影器使用的稳定 API 族键。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` is the immutable protocol profile selected for the execution route.
/// `protocol_profile_id` 是执行路由选定的不可变协议配置标识。
///
/// # Returns
/// 返回值
/// The exact compatibility family key used by `vulcan-tools-manager`.
/// 返回 `vulcan-tools-manager` 使用的精确兼容族键。
fn tool_schema_api_family_key(protocol_profile_id: &str) -> &'static str {
    match protocol_profile_id {
        vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID
        | vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID
        | vulcan_ai_protocols::GEMINI_INTERACTIONS_PROFILE_ID => {
            "google-gemini-function-declarations"
        }
        vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
        | vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID => "openai-responses-compatible",
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => "openai-chat-compatible",
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
        | vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | vulcan_ai_protocols::CLAUDE_CODE_MESSAGES_PROFILE_ID => "anthropic-messages",
        vulcan_ai_protocols::COHERE_CHAT_PROFILE_ID => "cohere-chat",
        _ => "unknown",
    }
}

/// Projects host-owned provider tools through the exact route/model compatibility profile.
/// 通过精确路由与模型兼容配置投影宿主持有的 Provider 工具。
///
/// # Parameters
/// 参数
/// `route` and `model_id` freeze the public protocol target; `tools` contains the already resolved
/// provider-visible definitions.
/// `route` 与 `model_id` 冻结公共协议目标；`tools` 包含已经解析的 Provider 可见定义。
///
/// # Returns
/// 返回值
/// Route-normalized public tool definitions suitable for ordinary responses and remote compaction.
/// 返回适用于普通 Responses 与远端压缩的路由正规化公共工具定义。
pub(crate) fn project_public_tool_definitions(
    route: &ProviderExecutionRoute,
    model_id: &str,
    tools: Vec<ProviderToolDefinition>,
) -> Vec<ToolDefinition> {
    // schemaProjectionContext freezes the exact protocol family and model before projection.
    // schemaProjectionContext 在投影前冻结精确协议族与模型。
    let schema_projection_context = VulcanToolSchemaProjectionContext {
        api_family_key: tool_schema_api_family_key(
            route.protocol_route.protocol_profile_id.as_str(),
        ),
        model_id,
    };
    // schemaProjectionProfile reuses the host-owned compatibility policy for every request path.
    // schemaProjectionProfile 为每条请求路径复用宿主自有兼容策略。
    let schema_projection_profile =
        resolve_tool_schema_projection_profile(&schema_projection_context);
    tools
        .into_iter()
        .map(|tool| ToolDefinition {
            name: tool.name,
            description: tool.description,
            input_schema: project_tool_parameters_schema(
                &tool.parameters,
                schema_projection_profile,
            ),
            strict: false,
        })
        .collect()
}

/// Resolves provider-native tools exactly as the ordinary public Responses request path does.
/// 按普通公共 Responses 请求路径的相同规则解析 Provider 原生工具。
///
/// # Parameters
/// 参数
/// `protocol_profile_id`, `output_resources`, and `native_tools` are frozen from one resolved
/// provider execution.
/// `protocol_profile_id`、`output_resources` 与 `native_tools` 来自同一份已解析 Provider 执行。
///
/// # Returns
/// 返回值
/// The deduplicated native-tool list, including Codex image generation when the route supports image output.
/// 返回去重后的原生工具列表；当路由支持图片输出时包含 Codex 图片生成。
pub(crate) fn resolve_public_native_tools(
    protocol_profile_id: &str,
    output_resources: &[ResourceKind],
    mut native_tools: Vec<NativeToolKind>,
) -> Vec<NativeToolKind> {
    if protocol_profile_id == vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID
        && output_resources.contains(&ResourceKind::Image)
        && !native_tools.contains(&NativeToolKind::ImageGeneration)
    {
        native_tools.push(NativeToolKind::ImageGeneration);
    }
    native_tools
}

#[async_trait]
impl VulcanProviderAdapter for VulcanPublicContractAdapter {
    type Request = VulcanPublicContractRequest;
    type Response = CoreProviderTurnResponse;

    fn provider_id(&self) -> &str {
        self.route.provider.id.as_str()
    }

    fn model_id(&self) -> &str {
        self.model_id.as_str()
    }

    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let system_prompt = config
            .provider_prompt_layers_with_persistent_injection()
            .render_or_legacy(config.system_prompt.as_str());
        let mut messages = Vec::new();
        if !system_prompt.is_empty() {
            messages.push(AiMessage {
                role: MessageRole::System,
                content: vec![ContentItem::Text {
                    text: system_prompt.clone(),
                }],
                provider_context_items: Vec::new(),
            });
        }
        for message in &snapshot.messages {
            let mut public_message = core_message_to_public(
                message,
                self.provider_id(),
                self.route.connection.id.as_str(),
                self.route.request_url("/responses").as_str(),
                self.route.protocol_route.protocol_profile_id.as_str(),
                self.model_id(),
            )?;
            resolve_public_message_resources(
                &mut public_message,
                &self.route,
                self.model_id.as_str(),
                self.input_resources.as_slice(),
                &self.client,
            )
            .await?;
            messages.push(public_message);
        }
        // continuation is emitted only after the Core configuration proves the exact append-only boundary.
        // continuation 仅在 Core 配置证明精确追加边界后才会发出。
        let continuation = build_public_continuation(
            snapshot,
            config,
            self.route.provider.id.as_str(),
            self.route.protocol_route.protocol_profile_id.as_str(),
            self.model_id.as_str(),
            usize::from(!system_prompt.is_empty()),
        );
        // nativeTools follows the shared ordinary/remote Codex capability projection.
        // nativeTools 遵循普通与远端 Codex 共享的能力投影。
        let native_tools = resolve_public_native_tools(
            self.route.protocol_route.protocol_profile_id.as_str(),
            self.output_resources.as_slice(),
            self.provider_native_tools.clone(),
        );
        let rebuilt_public = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: config.model_id.clone(),
            cache_scope_id: Some(self.cache_scope_id.clone()),
            continuation,
            messages,
            tools: self.tools.clone(),
            native_tools,
            tool_choice: ToolChoicePolicy::Auto,
            parallel_tool_calls: ParallelToolCallPolicy::ProviderDefault,
            structured_output: StructuredOutputPolicy::None,
            stop_sequences: Vec::new(),
            prompt_cache: self.prompt_cache.clone(),
            reasoning_effort: self.reasoning_effort,
            reasoning_output: ReasoningOutputPolicy::ProviderDefault,
            model_parameters: self.model_parameters.clone(),
            resources: Vec::new(),
            output_token_limit: Some(self.output_token_limit),
        };
        let prompt = snapshot
            .messages
            .last()
            .map(core_message_text)
            .transpose()?
            .unwrap_or_default();
        let budget = CoreProviderRequestSnapshot {
            provider_id: self.route.provider.id.clone(),
            model_id: config.model_id.clone(),
            system_prompt,
            prompt_layers: config.provider_prompt_layers_with_persistent_injection(),
            prompt,
            prompt_in_history: true,
            history: snapshot.messages.clone(),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: self
                .tools
                .iter()
                .zip(self.tool_sources.iter())
                .map(
                    |(tool, (source_kind, source_label))| CoreProviderRequestToolSchema {
                        id: tool.name.clone(),
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        source_kind: source_kind.clone(),
                        source_label: source_label.clone(),
                        parameters_schema: tool.input_schema.clone(),
                    },
                )
                .collect(),
            additional_params: None,
            request_headers: None,
            continuation: snapshot.continuation.clone(),
        };
        // initialPublicRequest is consumed exactly once; every later tool-result turn is rebuilt from the restored controller.
        // initialPublicRequest 仅消费一次；后续每个工具结果回合都从已恢复控制器重新构建。
        let initial_public = self
            .initial_provider_request
            .lock()
            .map_err(|_| {
                provider_failure(self.provider_id(), "The retry request lock is poisoned.")
            })?
            .take();
        if let Some(initial_public) = initial_public.as_ref()
            && (initial_public.tools != rebuilt_public.tools
                || initial_public.native_tools != rebuilt_public.native_tools)
        {
            return Err(provider_failure(
                self.provider_id(),
                "The frozen retry request tool contract no longer matches the active runtime.",
            ));
        }
        let public = initial_public.unwrap_or(rebuilt_public);
        Ok(VulcanPublicContractRequest {
            public,
            controller_config: config.clone(),
            controller_snapshot: snapshot.clone(),
            budget,
            context_budget: None,
        })
    }

    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Err(provider_failure(
            self.provider_id(),
            "The Vulcan public-contract adapter requires streaming execution.",
        ))
    }
}

/// Builds one public continuation only when the exact local boundary and context mutation proof both succeed.
/// 仅在精确本地边界与上下文变更证明同时成功时构建公共延续信息。
///
/// # Parameters
/// 参数
/// `snapshot` and `config` are the Core-owned canonical state and safety policy; route identifiers
/// freeze continuation ownership; `system_message_count` accounts for the synthetic public system message.
/// `snapshot` 与 `config` 是 Core 持有的规范状态和安全策略；路由标识冻结延续归属；
/// `system_message_count` 计入合成的公共系统消息。
///
/// # Returns
/// 返回值
/// A proven public continuation, or `None` when any ownership, boundary, or mutation proof fails.
/// 返回已证明安全的公共延续；任一归属、边界或变更证明失败时返回 `None`。
fn build_public_continuation(
    snapshot: &CoreConversationSnapshot,
    config: &CoreControllerConfig,
    provider_id: &str,
    protocol_profile_id: &str,
    model_id: &str,
    system_message_count: usize,
) -> Option<AiContinuation> {
    let continuation = &snapshot.continuation;
    if continuation.provider_id.as_deref() != Some(provider_id)
        || continuation.protocol_profile_id.as_deref() != Some(protocol_profile_id)
        || continuation.model_id.as_deref() != Some(model_id)
    {
        return None;
    }
    let provider_message_id = continuation
        .last_provider_message_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())?;
    let covered_local_message_id = continuation
        .last_local_message_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())?;
    let mut matching_indices = snapshot
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| (message.id == covered_local_message_id).then_some(index));
    let covered_index = matching_indices.next()?;
    if matching_indices.next().is_some() {
        return None;
    }
    let uncovered_message_ids = snapshot.messages[covered_index + 1..]
        .iter()
        .map(|message| message.id.as_str())
        .collect::<Vec<_>>();
    if !config.continuation_reuse_is_proven(continuation, &uncovered_message_ids) {
        return None;
    }
    Some(AiContinuation {
        provider_id: provider_id.to_string(),
        protocol_profile_id: protocol_profile_id.to_string(),
        model_id: model_id.to_string(),
        provider_message_id: provider_message_id.to_string(),
        covered_local_message_id: covered_local_message_id.to_string(),
        covered_message_count: system_message_count
            .saturating_add(covered_index)
            .saturating_add(1),
    })
}

/// Reports whether one public stream event is an explicit protocol terminal boundary.
/// 报告一项公共流事件是否为显式协议终态边界。
///
/// # Parameters
/// 参数
/// `event` is the normalized provider event produced by the selected protocol decoder.
/// `event` 是所选协议解码器产出的规范化 Provider 事件。
///
/// # Returns
/// 返回值
/// `true` for completed, failed, or cancelled responses; otherwise `false`.
/// 对已完成、失败或已取消响应返回 `true`，否则返回 `false`。
fn public_stream_event_is_terminal(event: &AiStreamEvent) -> bool {
    matches!(
        event,
        AiStreamEvent::ResponseCompleted
            | AiStreamEvent::ResponseFailed { .. }
            | AiStreamEvent::ResponseCancelled
    )
}

#[async_trait]
impl VulcanStreamingProviderAdapter for VulcanPublicContractAdapter {
    async fn execute_request_streaming<S>(
        &self,
        request: Self::Request,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let trace_controller_request = if self.provider_request_trace_enabled {
            let serialized_request = serde_json::to_value(&request).map_err(|error| {
                provider_failure(
                    self.provider_id(),
                    &format!("Controller provider request trace serialization failed: {error}"),
                )
            })?;
            Some(json!({
                "serialized_request": serialized_request,
                "context_budget_debug": request
                    .context_budget
                    .as_ref()
                    .map(|snapshot| format!("{snapshot:#?}")),
            }))
        } else {
            None
        };
        let VulcanPublicContractRequest {
            public,
            controller_config,
            controller_snapshot,
            budget,
            context_budget,
        } = request;
        let (prepared_completion, prepared_receiver) = provider_request_durability_barrier();
        self.live_sender
            .send(Ok(ProviderStreamDelta::RequestPrepared {
                snapshot: budget,
                exact_request: public.clone(),
                controller_config,
                controller_snapshot,
                context_budget,
                completion: prepared_completion,
            }))
            .map_err(|_| {
                provider_failure(
                    self.provider_id(),
                    "The provider request stream closed before dispatch.",
                )
            })?;
        prepared_receiver
            .await
            .map_err(|_| {
                provider_failure(
                    self.provider_id(),
                    "The host dropped the Provider start persistence barrier before dispatch.",
                )
            })?
            .map_err(|error| {
                provider_failure(
                    self.provider_id(),
                    &format!(
                        "The Provider request was not dispatched because its start record could not be persisted: {error}"
                    ),
                )
            })?;
        let execution_id = uuid::Uuid::new_v4().to_string();
        // requestTraceTask waits for the exact object produced inside the selected transport.
        // requestTraceTask 等待所选传输内部产生的精确对象。
        // Trace I/O runs independently so it cannot delay the API dispatch being diagnosed.
        // 追踪 I/O 独立运行，因此不会延迟待诊断的 API 分派。
        let (wire_request_capture, request_trace_task) =
            if let Some(controller_request) = trace_controller_request {
                // captureSender and captureReceiver transfer the one object used by this dispatch.
                // captureSender 与 captureReceiver 传递本次分派实际使用的唯一请求对象。
                let (capture_sender, capture_receiver) = tokio::sync::oneshot::channel();
                // traceAdapter owns the immutable route and storage context for background writing.
                // traceAdapter 持有后台写入所需的不可变路由与存储上下文。
                let trace_adapter = self.clone();
                // tracePublic preserves the canonical request only for diagnostic comparison.
                // tracePublic 仅为诊断对比保留规范请求。
                let trace_public = public.clone();
                // traceExecutionId binds the captured request to the physical provider call.
                // traceExecutionId 将捕获请求绑定到真实 Provider 调用。
                let trace_execution_id = execution_id.clone();
                // task persists the captured API request without blocking transport progress.
                // task 在不阻塞传输进度的情况下持久化捕获的 API 请求。
                let task = tokio::spawn(async move {
                    let Ok(captured) = capture_receiver.await else {
                        // A closed capture channel means encoding failed before an API request existed.
                        // 捕获通道关闭表示编码在 API 请求形成前失败，因此没有请求可保存。
                        return Ok(());
                    };
                    trace_adapter
                        .persist_complete_provider_request_trace(
                            controller_request,
                            &trace_public,
                            trace_execution_id.as_str(),
                            captured,
                        )
                        .await
                });
                (Some(capture_sender), Some(task))
            } else {
                (None, None)
            };
        let mut stream = if let Some(capture_sender) = wire_request_capture {
            self.client.execute_with_wire_request_capture(
                self.route.clone(),
                public.clone(),
                self.secret.clone(),
                execution_id,
                capture_sender,
            )
        } else {
            self.client.execute(
                self.route.clone(),
                public.clone(),
                self.secret.clone(),
                execution_id,
            )
        };
        // routeFingerprint binds replay signatures to the exact resolved endpoint used by this request.
        // routeFingerprint 将重放签名绑定到本次请求使用的精确已解析端点。
        let route_fingerprint = self.route.request_url("/responses");
        let mut response = PublicTurnAccumulator::new(
            self.provider_id(),
            self.route.connection.id.as_str(),
            route_fingerprint.as_str(),
            self.route.protocol_route.protocol_profile_id.as_str(),
            self.model_id(),
        );
        while let Some(item) = stream.next().await {
            let event = item.map_err(|mut failure| {
                attach_execution_route_context(
                    &mut failure.error,
                    self.provider_id(),
                    self.route.entry.id.as_str(),
                    self.model_id(),
                );
                public_execution_failure(self.provider_id(), *failure.error)
            })?;
            let mut event = materialize_provider_resource_event(
                self.provider_id(),
                self.workspace_path.as_str(),
                event,
            )
            .await?;
            if let AiStreamEvent::ResponseFailed { error } = &mut event {
                attach_execution_route_context(
                    error,
                    self.provider_id(),
                    self.route.entry.id.as_str(),
                    self.model_id(),
                );
            }
            let event = match event {
                AiStreamEvent::TransportObserved {
                    transport,
                    dispatched_at_ms,
                    headers_at_ms,
                    http_status,
                    provider_request_id,
                    rate_limit_headers,
                } => {
                    let (dispatched_completion, dispatched_receiver) =
                        provider_request_durability_barrier();
                    self.live_sender
                        .send(Ok(ProviderStreamDelta::TransportObserved {
                            transport,
                            dispatched_at_ms,
                            headers_at_ms,
                            http_status,
                            provider_request_id,
                            rate_limit_headers,
                            completion: dispatched_completion,
                        }))
                        .map_err(|_| {
                            provider_failure(
                                self.provider_id(),
                                "The provider request stream closed before dispatch persistence.",
                            )
                        })?;
                    dispatched_receiver
                        .await
                        .map_err(|_| {
                            provider_failure(
                                self.provider_id(),
                                "The host dropped the Provider dispatch persistence barrier.",
                            )
                        })?
                        .map_err(|error| {
                            provider_failure(
                                self.provider_id(),
                                &format!(
                                    "The Provider request was dispatched but its dispatched state could not be persisted: {error}"
                                ),
                            )
                        })?;
                    continue;
                }
                event => event,
            };
            // terminalEvent closes the normalized response at its explicit protocol boundary instead of waiting for transport EOF.
            // terminalEvent 在显式协议终态边界关闭规范化响应，而不是继续等待传输 EOF。
            let terminal_event = public_stream_event_is_terminal(&event);
            let usage_updated = matches!(&event, AiStreamEvent::UsageUpdated { .. });
            response.apply_event(self.provider_id(), &self.live_sender, sink, event)?;
            if usage_updated && let Some(usage) = response.usage.as_ref() {
                let _ = self.live_sender.send(Ok(ProviderStreamDelta::UsageUpdated(
                    public_usage_to_core(usage),
                )));
            }
            if terminal_event {
                break;
            }
        }
        if let Some(task) = request_trace_task {
            task.await.map_err(|error| {
                provider_failure(
                    self.provider_id(),
                    &format!("Provider API request trace task failed: {error}"),
                )
            })??;
        }
        let final_usage = response.usage.as_ref().map(public_usage_to_core);
        let final_response = response.finish()?;
        let (completed_completion, completed_receiver) = provider_request_durability_barrier();
        self.live_sender
            .send(Ok(ProviderStreamDelta::RequestSucceeded {
                usage: final_usage,
                completion: completed_completion,
            }))
            .map_err(|_| {
                provider_failure(
                    self.provider_id(),
                    "The provider request stream closed before terminal persistence.",
                )
            })?;
        completed_receiver
            .await
            .map_err(|_| {
                provider_failure(
                    self.provider_id(),
                    "The host dropped the Provider terminal persistence barrier.",
                )
            })?
            .map_err(|error| {
                provider_failure(
                    self.provider_id(),
                    &format!(
                        "The Provider response completed but its terminal record could not be persisted: {error}"
                    ),
                )
            })?;
        Ok(final_response)
    }
}

/// Attaches the exact resolved route to one structured execution failure.
/// 为一个结构化执行失败附加精确解析后的执行路由。
///
/// # Parameters
/// 参数
/// `error` is the normalized failure; the remaining values identify the immutable physical route.
/// `error` 是归一化失败；其余值标识不可变的物理执行路由。
fn attach_execution_route_context(
    error: &mut vulcan_ai_contract::AiExecutionError,
    provider_id: &str,
    entry_id: &str,
    model_id: &str,
) {
    error.provider_id = Some(provider_id.to_string());
    error.entry_id = Some(entry_id.to_string());
    error.model_id = Some(model_id.to_string());
}

/// Builds a Core provider failure that retains the normalized public error.
/// 构建一项保留归一化公共错误的 Core Provider 失败。
///
/// # Parameters
/// 参数
/// `provider_id` identifies the controller route and `error` is the structured failure.
/// `provider_id` 标识控制器路由，`error` 是结构化失败。
///
/// # Returns
/// 返回值
/// A Core error carrying both display text and typed details.
/// 返回同时承载展示文本与强类型细节的 Core 错误。
fn public_execution_failure(
    provider_id: &str,
    error: vulcan_ai_contract::AiExecutionError,
) -> VulcanCoreError {
    VulcanCoreError::ProviderFailure {
        provider_id: provider_id.to_string(),
        message: public_execution_failure_message(&error),
        details: Some(Box::new(error)),
    }
}

/// Serializes one public execution error as JSON for lossless user-facing diagnostics.
/// 将一个公共执行错误序列化为 JSON，以便面向用户的诊断信息无损传递。
fn public_execution_failure_message(error: &vulcan_ai_contract::AiExecutionError) -> String {
    // AiExecutionError contains only serde-supported scalar, option, enum, and nested struct fields.
    // AiExecutionError 仅包含 serde 支持的标量、可选值、枚举与嵌套结构字段。
    serde_json::to_string(error).expect("AiExecutionError serialization is infallible")
}

/// Accumulates public stream events into the controller's completed-turn shape.
/// 将公共流事件累积为控制器的完整回合结构。
#[derive(Debug)]
struct PublicTurnAccumulator {
    /// Exact provider definition scope for durable replay.
    /// 可持久化重放使用的精确供应商定义作用域。
    provider_id: String,
    /// Exact credential connection scope for durable replay.
    /// 可持久化重放使用的精确凭据连接作用域。
    connection_id: String,
    /// Exact non-secret resolved request URL for durable replay.
    /// 可持久化重放使用的精确不含秘密的已解析请求 URL。
    route_fingerprint: String,
    /// Exact protocol profile scope for provider-specific replay signatures.
    /// 供应商专属重放签名使用的精确协议 Profile 作用域。
    protocol_profile_id: String,
    /// Exact model scope for durable replay.
    /// 可持久化重放使用的精确模型作用域。
    model_id: String,
    /// Flattened visible assistant output.
    /// 扁平化的可见助手输出。
    text: String,
    /// Flattened reasoning output.
    /// 扁平化的推理输出。
    reasoning: String,
    /// Latest provider response or assistant-message identifier.
    /// 最新供应商响应或助手消息标识。
    provider_message_id: Option<String>,
    /// Complete tool calls.
    /// 完整工具调用。
    tool_calls: Vec<ConversationToolCall>,
    /// Last public usage update.
    /// 最后一个公共用量更新。
    usage: Option<RequestUsage>,
    /// Stream item lookup keyed by stable public item identifiers.
    /// 按稳定公共项目标识索引的流项目查找表。
    replay_item_indices: BTreeMap<String, usize>,
    /// Assistant items retained in original stream order.
    /// 按原始流顺序保留的助手项目。
    replay_items: Vec<ReplayAccumulatorItem>,
    /// Exact provider output items retained in completed-event order.
    /// 按完成事件顺序保留的原始供应商输出项。
    provider_context_items: Vec<Value>,
    /// Whether a successful terminal response event was observed.
    /// 是否已经观察到成功响应终态事件。
    completed: bool,
}

/// Stores one in-progress assistant item for deterministic replay assembly.
/// 为确定性重放组装保存一个进行中的助手项目。
#[derive(Debug)]
struct ReplayAccumulatorItem {
    /// Stable public stream item identifier.
    /// 稳定公共流项目标识。
    item_id: String,
    /// Current semantic item state.
    /// 当前语义项目状态。
    state: ReplayAccumulatorItemState,
}

/// Represents the closed set of assistant item states assembled from stream events.
/// 表示从流事件组装出的封闭助手项目状态集合。
#[derive(Debug)]
enum ReplayAccumulatorItemState {
    /// Incrementally assembled visible text.
    /// 增量组装的可见文本。
    Text(String),
    /// Incrementally assembled reasoning and its optional replay signature.
    /// 增量组装的推理及其可选重放签名。
    Reasoning {
        /// Visible reasoning summary or thinking text.
        /// 可见推理摘要或思考文本。
        text: String,
        /// Provider-specific opaque continuation signature.
        /// 供应商专属不透明延续签名。
        signature: Option<String>,
    },
    /// Complete non-incremental public content.
    /// 完整的非增量公共内容。
    Complete(ContentItem),
    /// Tool identity is known but its arguments are still incomplete.
    /// 工具身份已知但参数仍不完整。
    PendingTool,
}

/// Returns whether captured Responses items fully cover the provider-neutral assistant replay.
/// 返回捕获的 Responses 项是否完整覆盖供应商无关的助手重放内容。
///
/// # Parameters
/// 参数
/// `content` is the complete normalized assistant output and `provider_context_items` is the
/// ordered provider-owned output-item list observed from completion events.
/// `content` 是完整的规范助手输出；`provider_context_items` 是从完成事件观察到的有序供应商输出项列表。
///
/// # Returns
/// 返回值
/// `true` only when raw replay cannot hide any normalized text, reasoning, tool call, or resource.
/// 仅当原始重放不会隐藏任何规范文本、推理、工具调用或资源时返回 `true`。
fn provider_context_items_cover_replay_content(
    content: &[ContentItem],
    provider_context_items: &[Value],
) -> bool {
    // normalizedCounts describe every semantic category that would otherwise be re-encoded.
    // normalizedCounts 描述原本需要重新编码的每一种语义类别。
    let normalized_message_count = content
        .iter()
        .filter(|item| {
            matches!(
                item,
                ContentItem::Text { .. }
                    | ContentItem::StructuredJson { .. }
                    | ContentItem::Refusal { .. }
            )
        })
        .count();
    let normalized_tool_count = content
        .iter()
        .filter(|item| matches!(item, ContentItem::ToolCall { .. }))
        .count();
    let normalized_resource_count = content
        .iter()
        .filter(|item| matches!(item, ContentItem::Resource { .. }))
        .count();
    let normalized_has_reasoning = content
        .iter()
        .any(|item| matches!(item, ContentItem::Reasoning { .. }));
    if content.iter().any(|item| {
        matches!(
            item,
            ContentItem::ToolResult { .. } | ContentItem::Citation { .. }
        )
    }) {
        return false;
    }

    // providerCounts reflect exact output-item variants accepted by Responses history input.
    // providerCounts 反映 Responses 历史输入接受的精确输出项变体。
    let provider_message_count = provider_context_items
        .iter()
        .filter(|item| {
            matches!(
                item.get("type").and_then(Value::as_str),
                Some("message" | "agent_message")
            )
        })
        .count();
    let provider_tool_count = provider_context_items
        .iter()
        .filter(|item| {
            matches!(
                item.get("type").and_then(Value::as_str),
                Some("function_call" | "custom_tool_call")
            )
        })
        .count();
    let provider_resource_count = provider_context_items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("image_generation_call"))
        .count();
    let provider_has_reasoning = provider_context_items
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("reasoning"));

    provider_message_count >= normalized_message_count
        && provider_tool_count >= normalized_tool_count
        && provider_resource_count >= normalized_resource_count
        && (!normalized_has_reasoning || provider_has_reasoning)
}

impl PublicTurnAccumulator {
    /// Creates one accumulator bound to an immutable execution route.
    /// 创建一个绑定到不可变执行路由的累积器。
    ///
    /// # Parameters
    /// 参数
    /// The identifiers are the provider definition, credential connection, resolved request URL,
    /// protocol profile, and model selected for this physical request.
    /// 各标识分别是本次物理请求选定的供应商定义、凭据连接、已解析请求 URL、协议 Profile 与模型。
    ///
    /// # Returns
    /// 返回值
    /// A new empty turn accumulator.
    /// 返回一个新的空回合累积器。
    fn new(
        provider_id: &str,
        connection_id: &str,
        route_fingerprint: &str,
        protocol_profile_id: &str,
        model_id: &str,
    ) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            connection_id: connection_id.to_string(),
            route_fingerprint: route_fingerprint.to_string(),
            protocol_profile_id: protocol_profile_id.to_string(),
            model_id: model_id.to_string(),
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: None,
            tool_calls: Vec::new(),
            usage: None,
            replay_item_indices: BTreeMap::new(),
            replay_items: Vec::new(),
            provider_context_items: Vec::new(),
            completed: false,
        }
    }

    /// Registers one stream item exactly once while preserving arrival order.
    /// 精确注册一次流项目并保留到达顺序。
    ///
    /// # Parameters
    /// 参数
    /// `item_id` is the stable public identifier and `state` is its initial semantic state.
    /// `item_id` 是稳定公共标识，`state` 是其初始语义状态。
    ///
    /// # Returns
    /// 返回值
    /// Success when the identifier is new; duplicate identifiers are controlled failures.
    /// 标识为新值时成功；重复标识会产生受控失败。
    fn register_replay_item(
        &mut self,
        item_id: String,
        state: ReplayAccumulatorItemState,
    ) -> Result<(), VulcanCoreError> {
        if self.replay_item_indices.contains_key(item_id.as_str()) {
            return Err(provider_failure(
                self.provider_id.as_str(),
                format!("Public stream item `{item_id}` started more than once.").as_str(),
            ));
        }
        let index = self.replay_items.len();
        self.replay_item_indices.insert(item_id.clone(), index);
        self.replay_items
            .push(ReplayAccumulatorItem { item_id, state });
        Ok(())
    }

    /// Resolves the mutable semantic state for one previously started item.
    /// 解析一个已启动项目的可变语义状态。
    ///
    /// # Parameters
    /// 参数
    /// `item_id` is the stable identifier from a delta or completion event.
    /// `item_id` 是增量或完成事件携带的稳定标识。
    ///
    /// # Returns
    /// 返回值
    /// The exact registered state, or a controlled ordering failure.
    /// 返回精确注册状态；事件顺序无效时返回受控失败。
    fn replay_item_state_mut(
        &mut self,
        item_id: &str,
    ) -> Result<&mut ReplayAccumulatorItemState, VulcanCoreError> {
        let index = self
            .replay_item_indices
            .get(item_id)
            .copied()
            .ok_or_else(|| {
                provider_failure(
                    self.provider_id.as_str(),
                    format!("Public stream item `{item_id}` was updated before it started.")
                        .as_str(),
                )
            })?;
        Ok(&mut self.replay_items[index].state)
    }

    /// Materializes all completed assistant items in original stream order.
    /// 按原始流顺序实体化所有已完成助手项目。
    ///
    /// # Returns
    /// 返回值
    /// Ordered public content, or a controlled failure when a tool remains incomplete.
    /// 返回有序公共内容；工具仍不完整时返回受控失败。
    fn replay_content(&self) -> Result<Vec<ContentItem>, VulcanCoreError> {
        self.replay_items
            .iter()
            .map(|item| match &item.state {
                ReplayAccumulatorItemState::Text(text) => {
                    Ok(ContentItem::Text { text: text.clone() })
                }
                ReplayAccumulatorItemState::Reasoning { text, signature } => {
                    Ok(ContentItem::Reasoning {
                        text: text.clone(),
                        signature: signature.clone(),
                    })
                }
                ReplayAccumulatorItemState::Complete(content) => Ok(content.clone()),
                ReplayAccumulatorItemState::PendingTool => Err(provider_failure(
                    self.provider_id.as_str(),
                    format!(
                        "Public tool stream item `{}` remained incomplete at response completion.",
                        item.item_id
                    )
                    .as_str(),
                )),
            })
            .collect()
    }

    /// Applies one public stream event.
    /// 应用一个公共流事件。
    fn apply_event<S>(
        &mut self,
        provider_id: &str,
        live_sender: &ProviderLiveDeltaSender,
        sink: &mut S,
        event: AiStreamEvent,
    ) -> Result<(), VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        match event {
            AiStreamEvent::TransportObserved {
                transport: _,
                dispatched_at_ms: _,
                headers_at_ms: _,
                http_status: _,
                provider_request_id: _,
                rate_limit_headers: _,
            } => {
                return Err(provider_failure(
                    provider_id,
                    "TransportObserved must pass through the dispatch persistence barrier.",
                ));
            }
            AiStreamEvent::ResponseMetadataUpdated { continuation } => {
                if let Some(existing) = self.provider_message_id.as_deref()
                    && existing != continuation.provider_message_id
                {
                    return Err(provider_failure(
                        provider_id,
                        "Provider response identity changed during one physical request.",
                    ));
                }
                self.provider_message_id = Some(continuation.provider_message_id);
            }
            AiStreamEvent::ResponseTextDelta { item_id, delta } => {
                let ReplayAccumulatorItemState::Text(text) =
                    self.replay_item_state_mut(item_id.as_str())?
                else {
                    return Err(provider_failure(
                        provider_id,
                        "Public text delta targeted a non-text item.",
                    ));
                };
                text.push_str(delta.as_str());
                self.text.push_str(delta.as_str());
                // liveDelta preserves the same decoded text for the workbench stream while Core keeps its canonical copy.
                // liveDelta 在 Core 保留规范副本的同时，将同一段已解码文本投影到工作台流。
                let live_delta = delta.clone();
                sink.push_delta(ProviderTurnDelta::Output { delta });
                let _ = live_sender.send(Ok(ProviderStreamDelta::Output(live_delta)));
            }
            AiStreamEvent::ResponseItemStarted { item_id, item } => {
                let replay_state = match &item {
                    ContentItem::Text { text } => ReplayAccumulatorItemState::Text(text.clone()),
                    _ => ReplayAccumulatorItemState::Complete(item.clone()),
                };
                self.register_replay_item(item_id, replay_state)?;
                if let ContentItem::Resource {
                    resource:
                        ResourceReference {
                            kind: ResourceKind::Image,
                            local_path,
                            url,
                            ..
                        },
                } = item
                {
                    let location = local_path.or(url).ok_or_else(|| {
                        provider_failure(
                            provider_id,
                            "Provider-generated image has no local path or user-safe URL.",
                        )
                    })?;
                    let markdown = format!(
                        "{}![Generated image](<{}>)",
                        if self.text.ends_with('\n') || self.text.is_empty() {
                            ""
                        } else {
                            "\n\n"
                        },
                        location
                    );
                    self.text.push_str(markdown.as_str());
                    // liveMarkdown keeps generated-resource output visible before the terminal checkpoint.
                    // liveMarkdown 让生成资源输出在终态检查点到达前即可见。
                    let live_markdown = markdown.clone();
                    sink.push_delta(ProviderTurnDelta::Output { delta: markdown });
                    let _ = live_sender.send(Ok(ProviderStreamDelta::Output(live_markdown)));
                }
            }
            AiStreamEvent::ReasoningStarted { item_id } => {
                self.register_replay_item(
                    item_id,
                    ReplayAccumulatorItemState::Reasoning {
                        text: String::new(),
                        signature: None,
                    },
                )?;
            }
            AiStreamEvent::ReasoningDelta { item_id, delta } => {
                let ReplayAccumulatorItemState::Reasoning { text, .. } =
                    self.replay_item_state_mut(item_id.as_str())?
                else {
                    return Err(provider_failure(
                        provider_id,
                        "Public reasoning delta targeted a non-reasoning item.",
                    ));
                };
                text.push_str(delta.as_str());
                self.reasoning.push_str(delta.as_str());
                // liveDelta projects provider reasoning without removing it from Core's canonical response.
                // liveDelta 投影 Provider 推理内容，同时不从 Core 规范响应中移除该内容。
                let live_delta = delta.clone();
                sink.push_delta(ProviderTurnDelta::Thinking { delta });
                let _ = live_sender.send(Ok(ProviderStreamDelta::Thinking(live_delta)));
            }
            AiStreamEvent::ReasoningCompleted { item_id, signature } => {
                let ReplayAccumulatorItemState::Reasoning {
                    signature: current_signature,
                    ..
                } = self.replay_item_state_mut(item_id.as_str())?
                else {
                    return Err(provider_failure(
                        provider_id,
                        "Public reasoning completion targeted a non-reasoning item.",
                    ));
                };
                *current_signature = signature;
            }
            AiStreamEvent::ToolCallStarted {
                item_id,
                call_id,
                tool_name,
                ..
            } => {
                self.register_replay_item(
                    item_id.clone(),
                    ReplayAccumulatorItemState::PendingTool,
                )?;
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::Detected {
                        request_stream_id: item_id,
                        tool_call_id: call_id,
                        tool_name,
                        initial_arguments: String::new(),
                        argument_assembler: None,
                    },
                )));
            }
            AiStreamEvent::ToolCallArgumentsDelta { item_id, delta, .. } => {
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::ArgumentsDelta {
                        request_stream_id: item_id,
                        delta,
                    },
                )));
            }
            AiStreamEvent::ToolCallCompleted { item_id, call } => {
                let replay_state = self.replay_item_state_mut(item_id.as_str())?;
                if !matches!(replay_state, ReplayAccumulatorItemState::PendingTool) {
                    return Err(provider_failure(
                        provider_id,
                        "Public tool completion targeted a non-tool item.",
                    ));
                }
                *replay_state = ReplayAccumulatorItemState::Complete(ContentItem::ToolCall {
                    call: call.clone(),
                });
                let final_arguments = serde_json::to_string(&call.arguments).map_err(|error| {
                    provider_failure(
                        provider_id,
                        format!("Public tool arguments could not be serialized: {error}").as_str(),
                    )
                })?;
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::Completed {
                        request_stream_id: item_id,
                        canonical_tool_call_id: Some(call.id.clone()),
                        final_arguments: Some(final_arguments),
                    },
                )));
                self.tool_calls.push(ConversationToolCall {
                    id: call.id.clone(),
                    tool_call_id: Some(call.id),
                    tool_name: call.name,
                    arguments: call.arguments,
                });
            }
            AiStreamEvent::ToolCallAborted { item_id, message } => {
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::Aborted {
                        request_stream_id: item_id,
                        reason: message.clone(),
                    },
                )));
                return Err(provider_failure(provider_id, message.as_str()));
            }
            AiStreamEvent::UsageUpdated { usage } => {
                if let Some(current) = self.usage.as_mut() {
                    current.merge_update(*usage);
                } else {
                    self.usage = Some(*usage);
                }
            }
            AiStreamEvent::ResponseCompleted => {
                self.completed = true;
            }
            AiStreamEvent::ResponseFailed { error } => {
                return Err(public_execution_failure(provider_id, error));
            }
            AiStreamEvent::ResponseCancelled => {
                return Err(provider_failure(
                    provider_id,
                    "Provider execution was cancelled.",
                ));
            }
            AiStreamEvent::ProviderContextItemCaptured { item } => {
                if !is_replayable_codex_responses_item(&item) {
                    return Err(provider_failure(
                        provider_id,
                        "Provider context capture contained a non-replayable Responses item.",
                    ));
                }
                self.provider_context_items.push(item);
            }
            AiStreamEvent::ResponseStarted { .. }
            | AiStreamEvent::ResourceProgress { .. }
            | AiStreamEvent::ImageGenerationPartial { .. }
            | AiStreamEvent::ResponseItemCompleted { .. } => {}
        }
        Ok(())
    }

    /// Finishes the controller response after a public terminal event.
    /// 在公共终态事件后完成控制器响应。
    fn finish(self) -> Result<CoreProviderTurnResponse, VulcanCoreError> {
        if !self.completed {
            return Err(provider_failure(
                self.provider_id.as_str(),
                "Provider stream ended without a successful terminal response event.",
            ));
        }
        let replay_content = self.replay_content()?;
        // providerContextItems are enabled only when they can replace the entire normalized output.
        // providerContextItems 仅在能够替代全部规范输出时启用。
        let provider_context_items = if provider_context_items_cover_replay_content(
            replay_content.as_slice(),
            self.provider_context_items.as_slice(),
        ) {
            self.provider_context_items
        } else {
            Vec::new()
        };
        let provider_message_state = Some(json!({
            "vulcan_ai": {
                "replay": ProviderReplayEnvelope::new(
                    self.provider_id.clone(),
                    self.connection_id.clone(),
                    self.route_fingerprint.clone(),
                    self.protocol_profile_id.clone(),
                    self.model_id.clone(),
                    replay_content,
                    provider_context_items,
                ),
            }
        }));
        Ok(CoreProviderTurnResponse {
            assistant_text: self.text,
            assistant_reasoning: self.reasoning,
            provider_message_id: self.provider_message_id.clone(),
            provider_message_state,
            usage: self.usage.as_ref().map(public_usage_to_core),
            tool_calls: self.tool_calls,
            continuation: Some(ProviderContinuation {
                provider_id: Some(self.provider_id),
                protocol_profile_id: Some(self.protocol_profile_id),
                model_id: Some(self.model_id),
                last_provider_message_id: self.provider_message_id,
                ..ProviderContinuation::default()
            }),
        })
    }
}

/// Materializes one provider-generated Data URL into a host-owned workspace file.
/// 将供应商生成的 Data URL 落盘为宿主自有的工作区文件。
///
/// # Parameters
/// 参数
/// `provider_id` identifies the failing route in controlled diagnostics.
/// `provider_id` 在受控诊断中标识失败路由。
/// `workspace_path` is the canonical workspace that owns generated outputs.
/// `workspace_path` 是拥有生成输出的规范工作区。
/// `event` is one normalized provider event.
/// `event` 是一项规范化供应商事件。
///
/// # Returns
/// 返回值
/// The unchanged event when no generated resource is present, or the same resource event with
/// a local path after managed attachment staging and index publication.
/// 当不存在生成资源时原样返回事件；否则在托管附件暂存与索引发布后返回带本地路径的同一资源事件。
async fn materialize_provider_resource_event(
    provider_id: &str,
    workspace_path: &str,
    event: AiStreamEvent,
) -> Result<AiStreamEvent, VulcanCoreError> {
    let AiStreamEvent::ResponseItemStarted {
        item_id,
        item: ContentItem::Resource { mut resource },
    } = event
    else {
        return Ok(event);
    };
    let Some(data_url) = resource.url.as_deref() else {
        return Ok(AiStreamEvent::ResponseItemStarted {
            item_id,
            item: ContentItem::Resource { resource },
        });
    };
    if !data_url.starts_with("data:") {
        return Ok(AiStreamEvent::ResponseItemStarted {
            item_id,
            item: ContentItem::Resource { resource },
        });
    }
    let (media_type, encoded) = parse_generated_data_url(provider_id, data_url)?;
    if resource.kind != ResourceKind::Image {
        return Err(provider_failure(
            provider_id,
            "Only provider-generated image Data URLs are currently registered.",
        ));
    }
    let descriptor = generated_image_descriptor(provider_id, media_type)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| {
            provider_failure(
                provider_id,
                format!("Provider-generated image Base64 is invalid: {error}").as_str(),
            )
        })?;
    const MAX_GENERATED_IMAGE_BYTES: usize = 50 * 1024 * 1024;
    if bytes.len() > MAX_GENERATED_IMAGE_BYTES {
        return Err(provider_failure(
            provider_id,
            "Provider-generated image exceeds the 50 MiB host limit.",
        ));
    }
    if workspace_path.trim().is_empty() {
        return Err(provider_failure(
            provider_id,
            "Generated output requires a workspace path.",
        ));
    }
    let workspace_path = workspace_path.to_string();
    let suggested_file_name = format!(
        "{}.{}",
        safe_generated_resource_stem(resource.id.as_str()),
        descriptor.extension
    );
    let attachment = tokio::task::spawn_blocking(move || {
        stage_attachment_bytes(
            workspace_path.as_str(),
            &descriptor,
            suggested_file_name.as_str(),
            None,
            bytes.as_slice(),
        )
    })
    .await
    .map_err(|error| {
        provider_failure(
            provider_id,
            format!("Generated output staging task failed: {error}").as_str(),
        )
    })?
    .map_err(|error| {
        provider_failure(
            provider_id,
            format!("Generated output could not enter managed attachment storage: {error}")
                .as_str(),
        )
    })?;
    resource.media_type = Some(media_type.to_string());
    resource.local_path = Some(attachment.local_path);
    resource.url = None;
    Ok(AiStreamEvent::ResponseItemStarted {
        item_id,
        item: ContentItem::Resource { resource },
    })
}

/// Parses one exact Base64 Data URL returned by a provider resource event.
/// 解析供应商资源事件返回的一项精确 Base64 Data URL。
///
/// # Parameters
/// 参数
/// `provider_id` identifies controlled errors and `data_url` is the exact returned URL.
/// `provider_id` 标识受控错误，`data_url` 是精确返回的 URL。
///
/// # Returns
/// 返回值
/// The declared MIME type and encoded payload.
/// 返回声明的 MIME 类型与编码载荷。
fn parse_generated_data_url<'a>(
    provider_id: &str,
    data_url: &'a str,
) -> Result<(&'a str, &'a str), VulcanCoreError> {
    let body = data_url.strip_prefix("data:").ok_or_else(|| {
        provider_failure(
            provider_id,
            "Provider-generated resource URL is not a Data URL.",
        )
    })?;
    let (declaration, encoded) = body.split_once(',').ok_or_else(|| {
        provider_failure(
            provider_id,
            "Provider-generated Data URL is missing its payload separator.",
        )
    })?;
    let media_type = declaration.strip_suffix(";base64").ok_or_else(|| {
        provider_failure(
            provider_id,
            "Provider-generated Data URL is not Base64 encoded.",
        )
    })?;
    if encoded.is_empty() {
        return Err(provider_failure(
            provider_id,
            "Provider-generated Data URL payload is empty.",
        ));
    }
    Ok((media_type, encoded))
}

/// Resolves a registered generated-image MIME type to its managed attachment descriptor.
/// 将已注册的生成图片 MIME 类型解析为托管附件描述。
///
/// # Parameters
/// 参数
/// `provider_id` identifies controlled errors and `media_type` is the exact declared MIME type.
/// `provider_id` 标识受控错误，`media_type` 是精确声明的 MIME 类型。
///
/// # Returns
/// 返回值
/// A stable managed attachment descriptor or a controlled error for an unregistered type.
/// 返回稳定托管附件描述；未注册类型则返回受控错误。
fn generated_image_descriptor(
    provider_id: &str,
    media_type: &str,
) -> Result<AttachmentDescriptor, VulcanCoreError> {
    match media_type {
        "image/png" => Ok(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/png",
            extension: "png",
        }),
        "image/jpeg" => Ok(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/jpeg",
            extension: "jpg",
        }),
        "image/webp" => Ok(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/webp",
            extension: "webp",
        }),
        other => Err(provider_failure(
            provider_id,
            format!("Generated image MIME type `{other}` is not registered.").as_str(),
        )),
    }
}

/// Produces a filesystem-safe stem from one provider-neutral resource identifier.
/// 从一个供应商无关资源标识生成文件系统安全的主文件名。
///
/// # Parameters
/// 参数
/// `resource_id` is the stable normalized resource identifier.
/// `resource_id` 是稳定的规范化资源标识。
///
/// # Returns
/// 返回值
/// A non-empty ASCII filename stem.
/// 返回非空 ASCII 文件主名。
fn safe_generated_resource_stem(resource_id: &str) -> String {
    let stem = resource_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .take(80)
        .collect::<String>();
    if stem.is_empty() {
        String::from("generated-image")
    } else {
        stem
    }
}

/// Resolves every local message resource into an immutable request-scoped URL or data URL.
/// 将每个本地消息资源解析为不可变的请求作用域原生 URL 或 Data URL。
///
/// # Parameters
/// 参数
/// `message` is the public message being prepared, `route` fixes the selected protocol,
/// and `model_resource_kinds` is the exact discovered model capability snapshot.
/// `message` 是正在准备的公共消息，`route` 固定所选协议，
/// `model_resource_kinds` 是精确发现的模型能力快照。
///
/// # Returns
/// 返回值
/// Returns success after every resource is validated and resolved, or a controlled
/// pre-dispatch error before any provider request is sent.
/// 在每个资源完成校验与解析后返回成功；否则在发送任何供应商请求前返回受控错误。
async fn resolve_public_message_resources(
    message: &mut AiMessage,
    route: &ProviderExecutionRoute,
    model_id: &str,
    model_resource_kinds: &[ResourceKind],
    client: &ProviderExecutionClient,
) -> Result<(), VulcanCoreError> {
    for constraint in &route.protocol_route.input_resource_constraints {
        let resource_count = message
            .content
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    ContentItem::Resource { resource }
                        if resource.kind == constraint.resource_kind
                )
            })
            .count() as u64;
        if let Some(max_count) = constraint.max_count
            && resource_count > max_count
        {
            return Err(provider_failure(
                route.provider.id.as_str(),
                format!(
                    "Message contains {resource_count} {:?} resources and exceeds the frozen route limit of {max_count}.",
                    constraint.resource_kind
                )
                .as_str(),
            ));
        }
    }
    for item in &mut message.content {
        let ContentItem::Resource { resource } = item else {
            continue;
        };
        validate_resource_projection(route, model_id, model_resource_kinds, resource)?;
        if resource.url.is_some() {
            continue;
        }
        let local_path = resource.local_path.as_deref().ok_or_else(|| {
            provider_failure(
                route.provider.id.as_str(),
                format!(
                    "Resource `{}` has neither a local path nor a URL.",
                    resource.id
                )
                .as_str(),
            )
        })?;
        let media_type = resource.media_type.as_deref().ok_or_else(|| {
            provider_failure(
                route.provider.id.as_str(),
                format!("Resource `{}` is missing its MIME type.", resource.id).as_str(),
            )
        })?;
        let constraint = input_resource_constraint(route, resource.kind);
        if let Some(byte_limit) = constraint.and_then(|constraint| constraint.max_bytes) {
            let metadata = tokio::fs::metadata(local_path).await.map_err(|error| {
                provider_failure(
                    route.provider.id.as_str(),
                    format!(
                        "Resource `{}` metadata could not be read: {error}",
                        resource.id
                    )
                    .as_str(),
                )
            })?;
            if metadata.len() > byte_limit {
                return Err(provider_failure(
                    route.provider.id.as_str(),
                    format!(
                        "Resource `{}` is {} bytes and exceeds the frozen route limit of {byte_limit} bytes.",
                        resource.id,
                        metadata.len()
                    )
                    .as_str(),
                ));
            }
        }
        if let Some(prepared_url) = client
            .prepared_media_url(
                route,
                model_id,
                std::path::Path::new(local_path),
                resource.kind,
                media_type,
            )
            .await
        {
            resource.url = Some(prepared_url);
            continue;
        }
        let bytes = tokio::fs::read(local_path).await.map_err(|error| {
            provider_failure(
                route.provider.id.as_str(),
                format!("Resource `{}` could not be read: {error}", resource.id).as_str(),
            )
        })?;
        if let Some(byte_limit) = constraint.and_then(|constraint| constraint.max_bytes)
            && u64::try_from(bytes.len()).unwrap_or(u64::MAX) > byte_limit
        {
            return Err(provider_failure(
                route.provider.id.as_str(),
                format!(
                    "Resource `{}` changed while reading and now exceeds the frozen route limit of {byte_limit} bytes.",
                    resource.id
                )
                .as_str(),
            ));
        }
        resource.url = Some(format!(
            "data:{media_type};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ));
    }
    Ok(())
}

/// Validates one resource against both model discovery and the selected protocol projector.
/// 同时依据模型发现结果与所选协议投影器校验一个资源。
///
/// # Parameters
/// 参数
/// `route` identifies the provider and selected protocol, `model_resource_kinds` contains
/// exact model declarations, and `resource` is the requested resource.
/// `route` 标识供应商与所选协议，`model_resource_kinds` 包含精确模型声明，
/// `resource` 是请求使用的资源。
///
/// # Returns
/// 返回值
/// Returns success only when both capability layers support the resource.
/// 仅当两层能力均支持该资源时返回成功。
fn validate_resource_projection(
    route: &ProviderExecutionRoute,
    model_id: &str,
    model_resource_kinds: &[ResourceKind],
    resource: &ResourceReference,
) -> Result<(), VulcanCoreError> {
    if !model_resource_kinds.contains(&resource.kind) {
        return Err(provider_failure(
            route.provider.id.as_str(),
            format!(
                "Model `{}` does not declare {:?} input.",
                model_id, resource.kind
            )
            .as_str(),
        ));
    }
    let protocol = route.protocol_route.protocol_profile_id.as_str();
    let protocol_supports = match protocol {
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => true,
        vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
        | vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID => {
            resource.kind == ResourceKind::Image
        }
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
        | vulcan_ai_protocols::CLAUDE_CODE_MESSAGES_PROFILE_ID => {
            resource.kind == ResourceKind::Image
        }
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => {
            matches!(resource.kind, ResourceKind::Image | ResourceKind::Video)
        }
        vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID
        | vulcan_ai_protocols::GEMINI_INTERACTIONS_PROFILE_ID
        | vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID => true,
        _ => false,
    };
    if !protocol_supports {
        return Err(provider_failure(
            route.provider.id.as_str(),
            format!(
                "Protocol profile `{protocol}` has no registered {:?} input projector.",
                resource.kind
            )
            .as_str(),
        ));
    }
    if let Some(constraint) = input_resource_constraint(route, resource.kind) {
        validate_frozen_input_resource_constraint(
            route.provider.id.as_str(),
            constraint,
            resource,
        )?;
    }
    Ok(())
}

/// Validates one resource against one frozen manifest constraint without provider-specific logic.
/// 使用单条冻结清单约束校验资源，不包含供应商专属逻辑。
///
/// # Parameters
/// 参数
/// `provider_id` labels controlled failures, `constraint` is the immutable manifest value,
/// and `resource` is the canonical request resource.
/// `provider_id` 标记受控失败，`constraint` 是不可变清单值，`resource` 是规范请求资源。
///
/// # Returns
/// 返回值
/// Success when source representation and MIME type are both admitted; a pre-dispatch failure otherwise.
/// 来源表示与 MIME 类型均获准时返回成功，否则返回分派前失败。
fn validate_frozen_input_resource_constraint(
    provider_id: &str,
    constraint: &InputResourceConstraint,
    resource: &ResourceReference,
) -> Result<(), VulcanCoreError> {
    let reference_allowed = match constraint.reference_policy {
        ResourceReferencePolicy::Any => true,
        ResourceReferencePolicy::LocalOnly => {
            resource.local_path.is_some() && resource.url.is_none()
        }
        ResourceReferencePolicy::RemoteOnly => {
            resource.url.is_some() && resource.local_path.is_none()
        }
    };
    if !reference_allowed {
        return Err(provider_failure(
            provider_id,
            format!(
                "Resource `{}` violates the frozen {:?} reference policy for {:?} input.",
                resource.id, constraint.reference_policy, resource.kind
            )
            .as_str(),
        ));
    }
    if !constraint.accepted_mime_types.is_empty()
        && !resource
            .media_type
            .as_ref()
            .is_some_and(|media_type| constraint.accepted_mime_types.contains(media_type))
    {
        return Err(provider_failure(
            provider_id,
            format!(
                "Resource `{}` MIME type is not accepted by the frozen {:?} input constraint.",
                resource.id, resource.kind
            )
            .as_str(),
        ));
    }
    Ok(())
}

/// Returns the frozen constraint for one resource kind on the selected route.
/// 返回所选路由上一种资源类别的冻结约束。
///
/// # Parameters
/// 参数
/// `route` is the immutable execution route and `resource_kind` selects one closed kind.
/// `route` 是不可变执行路由，`resource_kind` 选择一种封闭类别。
///
/// # Returns
/// 返回值
/// The exact frozen constraint, or `None` when the route accepts the projector baseline.
/// 返回精确冻结约束；路由接受投影器基线时返回 `None`。
fn input_resource_constraint(
    route: &ProviderExecutionRoute,
    resource_kind: ResourceKind,
) -> Option<&InputResourceConstraint> {
    route
        .protocol_route
        .input_resource_constraints
        .iter()
        .find(|constraint| constraint.resource_kind == resource_kind)
}

/// Converts one canonical controller message into the public contract.
/// 将一条规范控制器消息转换为公共契约。
/// Projects the exact planned canonical source through the same route used for compact dispatch.
/// 通过压缩分派使用的同一路由投影精确规划的规范来源。
///
/// # Parameters
/// 参数
/// `conversation_state` owns lossless canonical messages, `source` selects the exact planner window,
/// and `resolved` freezes route, model capabilities, media projection, and opaque replay identity.
/// `conversation_state` 持有无损规范消息，`source` 选择精确规划窗口，`resolved` 冻结路由、
/// 模型能力、媒体投影与不透明重放身份。
///
/// # Returns
/// 返回值
/// Ordered provider-neutral messages with tools, results, images, and prior compact items preserved,
/// or a pre-dispatch projection error without sending a provider request.
/// 返回保留工具、结果、图片与既有压缩项的有序供应商无关消息；投影失败时在发送前返回错误。
pub(crate) async fn build_responses_compaction_messages(
    conversation_state: &SessionConversationState,
    source: &[SessionMessageEntry],
    resolved: &ResolvedPublicAiExecution,
) -> Result<Vec<AiMessage>, String> {
    // sourceIds is the exact planner-selected canonical window and rejects ambiguous duplicates.
    // sourceIds 是规划器精确选中的规范窗口，并拒绝有歧义的重复标识。
    let source_ids = source
        .iter()
        .map(|message| message.id.as_str())
        .collect::<BTreeSet<_>>();
    if source_ids.len() != source.len() {
        return Err(String::from(
            "Remote Responses compaction source contains duplicate message identities.",
        ));
    }
    // snapshot validates canonical identities before any route-specific projection runs.
    // snapshot 在执行任何路由专属投影前校验规范身份。
    let snapshot = build_core_conversation_snapshot(conversation_state)?;
    // routeFingerprint must match both existing opaque checkpoints and the immutable dispatch route.
    // routeFingerprint 必须同时匹配既有不透明检查点与不可变分派路由。
    let route_fingerprint = resolved.route.request_url("/responses");
    // projectedIds proves every planner-selected source row was found in the canonical snapshot.
    // projectedIds 证明规划器选择的每条来源记录都存在于规范快照中。
    let mut projected_ids = BTreeSet::new();
    // messages retains canonical order after exact-window filtering.
    // messages 在精确窗口过滤后保留规范顺序。
    let mut messages = Vec::new();
    for message in &snapshot.messages {
        if !source_ids.contains(message.id.as_str()) {
            continue;
        }
        projected_ids.insert(message.id.as_str());
        // publicMessage uses the normal request bridge so tool and provider item shapes cannot diverge.
        // publicMessage 使用普通请求桥接，避免工具与供应商项形态发生分歧。
        let mut public_message = core_message_to_public(
            message,
            resolved.route.provider.id.as_str(),
            resolved.route.connection.id.as_str(),
            route_fingerprint.as_str(),
            resolved.route.protocol_route.protocol_profile_id.as_str(),
            resolved.model.id.as_str(),
        )
        .map_err(|error| error.to_string())?;
        resolve_public_message_resources(
            &mut public_message,
            &resolved.route,
            resolved.model.id.as_str(),
            resolved.model.capabilities.input_resources.as_slice(),
            &resolved.client,
        )
        .await
        .map_err(|error| error.to_string())?;
        if public_message.content.is_empty() && public_message.provider_context_items.is_empty() {
            continue;
        }
        messages.push(public_message);
    }
    if projected_ids != source_ids {
        return Err(String::from(
            "Remote Responses compaction source no longer matches the canonical conversation.",
        ));
    }
    Ok(messages)
}

fn core_message_to_public(
    message: &vulcan_core::types::ConversationMessage,
    provider_id: &str,
    connection_id: &str,
    route_fingerprint: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> Result<AiMessage, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let mut content = vec![ContentItem::Text { text: text.clone() }];
            for (index, attachment) in attachments.iter().enumerate() {
                let resource_id = attachment
                    .asset_id
                    .clone()
                    .unwrap_or_else(|| format!("{}:attachment:{index}", message.id));
                let kind = match attachment.kind {
                    ConversationAttachmentKind::Audio => ResourceKind::Audio,
                    ConversationAttachmentKind::Image => ResourceKind::Image,
                    ConversationAttachmentKind::Video => ResourceKind::Video,
                    ConversationAttachmentKind::Pdf => ResourceKind::File,
                    ConversationAttachmentKind::File => ResourceKind::File,
                };
                content.push(ContentItem::Resource {
                    resource: ResourceReference {
                        id: resource_id,
                        kind,
                        media_type: Some(attachment.mime_type.clone()),
                        source_kind: vulcan_ai_contract::ResourceSourceKind::HostResolved,
                        file_name: None,
                        image_detail: None,
                        local_path: Some(attachment.local_path.clone()),
                        url: None,
                    },
                });
            }
            Ok(AiMessage {
                role: MessageRole::User,
                content,
                provider_context_items: Vec::new(),
            })
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_state,
            tool_calls,
            ..
        } => {
            // providerContextItems prefer complete ordinary output replay before remote compaction state.
            // providerContextItems 优先采用完整普通输出重放，其次才采用远端压缩状态。
            let mut provider_context_items = replay_provider_context_items_for_route(
                provider_message_state.as_ref(),
                provider_id,
                connection_id,
                route_fingerprint,
                protocol_profile_id,
                model_id,
            )?;
            if provider_context_items.is_empty() {
                provider_context_items = responses_compaction_items_for_route(
                    provider_message_state.as_ref(),
                    provider_id,
                    connection_id,
                    route_fingerprint,
                    protocol_profile_id,
                    model_id,
                )?;
            }
            if let Some(content) = replay_content_for_route(
                provider_message_state.as_ref(),
                provider_id,
                connection_id,
                route_fingerprint,
                protocol_profile_id,
                model_id,
            )? {
                return Ok(AiMessage {
                    role: MessageRole::Assistant,
                    content: if provider_context_items.is_empty() {
                        content
                    } else {
                        Vec::new()
                    },
                    provider_context_items,
                });
            }
            let mut content = Vec::new();
            // Opaque compaction items replace the anchor's local diagnostic text on their exact route.
            // 不透明压缩项在精确路由上替代锚点的本地诊断文本。
            if provider_context_items.is_empty() {
                if !reasoning.is_empty() {
                    content.push(ContentItem::Reasoning {
                        text: reasoning.clone(),
                        signature: None,
                    });
                }
                if !text.is_empty() {
                    content.push(ContentItem::Text { text: text.clone() });
                }
                for call in tool_calls {
                    // Outbound encoding consumes the canonical wire id only; local runtime aliases never leak.
                    // 出站编码只使用规范线协议标识；本地运行时别名绝不泄漏。
                    let call_id = call
                        .tool_call_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            provider_failure(
                                "vulcan",
                                "Canonical tool call is missing its provider-visible call identifier.",
                            )
                        })?;
                    content.push(ContentItem::ToolCall {
                        call: PublicToolCall {
                            id: call_id.to_string(),
                            name: call.tool_name.clone(),
                            arguments: call.arguments.clone(),
                            signature: None,
                        },
                    });
                }
            }
            Ok(AiMessage {
                role: MessageRole::Assistant,
                content,
                provider_context_items,
            })
        }
        CoreConversationPayload::ToolResult {
            tool_call_id,
            is_error,
            text,
        } => {
            let call_id = tool_call_id.clone().ok_or_else(|| {
                provider_failure(
                    "vulcan",
                    "Canonical tool result is missing its provider-visible call identifier.",
                )
            })?;
            Ok(AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: ToolResult {
                        call_id,
                        is_error: *is_error,
                        content: Value::String(text.clone()),
                    },
                }],
                provider_context_items: Vec::new(),
            })
        }
        CoreConversationPayload::AgentMessage { .. } => Ok(AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Text {
                text: message
                    .payload
                    .agent_message_model_text()
                    .ok_or_else(|| provider_failure("vulcan", "Agent message rendering failed."))?,
            }],
            provider_context_items: Vec::new(),
        }),
    }
}

/// Reads durable ordered replay content only for the exact active execution route.
/// 仅为当前精确执行路由读取可持久化的有序重放内容。
///
/// # Parameters
/// 参数
/// `provider_message_state` is the Core-persisted assistant extension; the remaining identifiers
/// define the currently selected execution route.
/// `provider_message_state` 是 Core 持久化的助手扩展；其余标识定义当前选定执行路由。
///
/// # Returns
/// 返回值
/// Exact ordered content for a matching route, no replay for a different route, or a controlled
/// error when a declared replay envelope is malformed.
/// 路由匹配时返回精确有序内容；路由不同时不重放；已声明封套损坏时返回受控错误。
fn replay_content_for_route(
    provider_message_state: Option<&Value>,
    provider_id: &str,
    connection_id: &str,
    route_fingerprint: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> Result<Option<Vec<ContentItem>>, VulcanCoreError> {
    let Some(replay_value) =
        provider_message_state.and_then(|state| state.pointer("/vulcan_ai/replay"))
    else {
        return Ok(None);
    };
    let envelope: ProviderReplayEnvelope =
        serde_json::from_value(replay_value.clone()).map_err(|error| {
            provider_failure(
                provider_id,
                format!("Persisted public replay envelope is invalid: {error}").as_str(),
            )
        })?;
    if !envelope.matches_route(
        provider_id,
        connection_id,
        route_fingerprint,
        protocol_profile_id,
        model_id,
    ) {
        return Ok(None);
    }
    Ok(Some(envelope.content))
}

/// Reads exact Responses output items from one route-matched replay envelope.
/// 从一份路由匹配的重放封套读取精确 Responses 输出项。
///
/// # Parameters
/// 参数
/// `provider_message_state` is the persisted assistant extension; the remaining identifiers
/// define the only physical route allowed to replay provider-owned output items.
/// `provider_message_state` 是持久化的助手扩展；其余标识定义唯一允许重放供应商输出项的物理路由。
///
/// # Returns
/// 返回值
/// The validated ordered provider item list for an exact route, or an empty list otherwise.
/// 精确路由匹配时返回已验证的有序供应商项列表，否则返回空列表。
fn replay_provider_context_items_for_route(
    provider_message_state: Option<&Value>,
    provider_id: &str,
    connection_id: &str,
    route_fingerprint: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> Result<Vec<Value>, VulcanCoreError> {
    // replayValue is the only durable extension allowed to own ordinary provider output items.
    // replayValue 是唯一允许持有普通供应商输出项的持久化扩展。
    let Some(replay_value) =
        provider_message_state.and_then(|state| state.pointer("/vulcan_ai/replay"))
    else {
        return Ok(Vec::new());
    };
    // envelope validates the closed format before any route-owned payload becomes visible.
    // envelope 在任何路由专属载荷可见前校验封闭格式。
    let envelope: ProviderReplayEnvelope =
        serde_json::from_value(replay_value.clone()).map_err(|error| {
            provider_failure(
                provider_id,
                format!("Persisted public replay envelope is invalid: {error}").as_str(),
            )
        })?;
    if !envelope.matches_route(
        provider_id,
        connection_id,
        route_fingerprint,
        protocol_profile_id,
        model_id,
    ) {
        return Ok(Vec::new());
    }
    if !envelope
        .provider_context_items
        .iter()
        .all(is_replayable_codex_responses_item)
    {
        return Err(provider_failure(
            provider_id,
            "Persisted public replay envelope contains a non-replayable provider context item.",
        ));
    }
    Ok(envelope.provider_context_items)
}

/// Reads route-bound remote Responses compaction items from one canonical assistant state.
/// 从一条规范助手状态读取绑定路由的远端 Responses 压缩项。
///
/// # Parameters
/// 参数
/// `provider_message_state` is the persisted assistant extension; the remaining identifiers freeze
/// the only route allowed to replay the ordered opaque Responses item list.
/// `provider_message_state` 是持久化助手扩展；其余标识冻结唯一允许重放有序不透明 Responses 项列表的路由。
///
/// # Returns
/// 返回值
/// A validated ordered Responses-item list, or an empty list when no exact route-bound state exists.
/// 返回已验证的有序 Responses 项列表；不存在精确路由绑定状态时返回空列表。
fn responses_compaction_items_for_route(
    provider_message_state: Option<&Value>,
    provider_id: &str,
    connection_id: &str,
    route_fingerprint: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> Result<Vec<Value>, VulcanCoreError> {
    let Some(value) =
        provider_message_state.and_then(|state| state.pointer("/vulcan_ai/responses_compaction"))
    else {
        return Ok(Vec::new());
    };
    let envelope = value.as_object().ok_or_else(|| {
        provider_failure(
            provider_id,
            "Persisted Responses compaction envelope must be an object.",
        )
    })?;
    if envelope.get("provider_id").and_then(Value::as_str) != Some(provider_id)
        || envelope.get("connection_id").and_then(Value::as_str) != Some(connection_id)
        || envelope.get("route_fingerprint").and_then(Value::as_str) != Some(route_fingerprint)
        || envelope.get("protocol_profile_id").and_then(Value::as_str) != Some(protocol_profile_id)
        || envelope.get("model_id").and_then(Value::as_str) != Some(model_id)
    {
        return Err(provider_failure(
            provider_id,
            "Persisted Responses compaction context belongs to a different provider route and cannot be discarded safely.",
        ));
    }
    let items = envelope
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            provider_failure(
                provider_id,
                "Persisted Responses compaction envelope is missing its item list.",
            )
        })?;
    // canonicalItems mirrors the official Codex deserializer alias before replay validation.
    // canonicalItems 在重放校验前镜像官方 Codex 反序列化器别名。
    let mut canonical_items = items.clone();
    canonicalize_codex_responses_replay_items(&mut canonical_items);
    let contains_compaction = canonical_items
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("compaction"));
    if canonical_items.is_empty()
        || !canonical_items
            .iter()
            .all(is_replayable_codex_responses_item)
        || !contains_compaction
    {
        return Err(provider_failure(
            provider_id,
            "Persisted Responses compaction envelope contains an invalid ordered replayable item.",
        ));
    }
    Ok(canonical_items)
}

/// Returns the model-visible text of one canonical controller message.
/// 返回一条规范控制器消息的模型可见文本。
fn core_message_text(
    message: &vulcan_core::types::ConversationMessage,
) -> Result<String, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, .. } => Ok(text.clone()),
        CoreConversationPayload::Assistant { text, .. } => Ok(text.clone()),
        CoreConversationPayload::ToolResult { text, .. } => Ok(text.clone()),
        CoreConversationPayload::AgentMessage { .. } => message
            .payload
            .agent_message_model_text()
            .ok_or_else(|| provider_failure("vulcan", "Agent message rendering failed.")),
    }
}

/// Converts public usage into the controller's temporary accounting shape.
/// 将公共用量转换为控制器的临时统计结构。
pub(super) fn public_usage_to_core(usage: &RequestUsage) -> ProviderUsageSnapshot {
    ProviderUsageSnapshot {
        update_kind: Some(usage.update_kind),
        input_tokens: known_usage(&usage.input_tokens),
        uncached_input_tokens: known_usage(&usage.uncached_input_tokens),
        output_tokens: known_usage(&usage.output_tokens),
        reasoning_tokens: known_usage(&usage.reasoning_tokens),
        total_tokens: known_usage(&usage.total_tokens),
        cached_input_tokens: known_usage(&usage.cache_read_tokens),
        cache_creation_input_tokens: known_usage(&usage.cache_write_tokens),
        audio_input_tokens: known_usage(&usage.audio_input_tokens),
        audio_output_tokens: known_usage(&usage.audio_output_tokens),
        extraction_profile_id: usage.extraction_profile_id.clone(),
        extraction_profile_revision: usage.extraction_profile_revision,
        source_paths: usage.source_paths.clone(),
        validation_flags: usage.validation_flags.clone(),
        response_model: usage.response_model.clone(),
        response_id: usage.response_id.clone(),
        billing_details: usage.billing_details.clone(),
    }
}

/// Converts one known public token count into the controller's platform size.
/// 将一个已知公共 Token 数转换为控制器平台大小。
fn known_usage(value: &UsageValue) -> Option<usize> {
    match value {
        UsageValue::Unknown => None,
        UsageValue::Known { value, .. } => usize::try_from(*value).ok(),
    }
}

/// Creates one controller provider failure.
/// 创建一个控制器供应商失败。
fn provider_failure(provider_id: &str, message: &str) -> VulcanCoreError {
    VulcanCoreError::ProviderFailure {
        provider_id: provider_id.to_string(),
        message: message.to_string(),
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    /// Verifies field digests isolate one changed wire component without obscuring stable prefixes.
    /// 验证逐字段摘要能够隔离单个变化的 Wire 组件，同时保留稳定前缀证据。
    #[test]
    fn provider_request_trace_digests_isolate_changed_wire_field() {
        let first = json!({
            "instructions": "stable system prompt",
            "input": [{"role": "user", "content": "first"}],
            "tools": [{"name": "shell_command"}],
        });
        let second = json!({
            "instructions": "stable system prompt",
            "input": [{"role": "user", "content": "second"}],
            "tools": [{"name": "shell_command"}],
        });

        let first_digests = provider_request_trace_top_level_digests(&first)
            .expect("first trace digests should encode");
        let second_digests = provider_request_trace_top_level_digests(&second)
            .expect("second trace digests should encode");

        assert_eq!(
            first_digests.get("instructions"),
            second_digests.get("instructions")
        );
        assert_eq!(first_digests.get("tools"), second_digests.get("tools"));
        assert_ne!(first_digests.get("input"), second_digests.get("input"));
    }

    /// Verifies trace headers preserve ordinary protocol fields while removing access material.
    /// 验证追踪请求头保留普通协议字段，同时移除访问凭据材料。
    #[test]
    fn provider_request_trace_headers_redact_access_material() {
        let headers = BTreeMap::from([
            (
                String::from("content-type"),
                String::from("application/json"),
            ),
            (String::from("Authorization"), String::from("Bearer secret")),
            (
                String::from("x-session-token"),
                String::from("secret-token"),
            ),
        ]);

        let traced = provider_request_trace_headers(&headers);

        assert_eq!(
            traced.get("content-type").map(String::as_str),
            Some("application/json")
        );
        assert_eq!(
            traced.get("Authorization").map(String::as_str),
            Some("<redacted>")
        );
        assert_eq!(
            traced.get("x-session-token").map(String::as_str),
            Some("<redacted>")
        );
    }

    /// Verifies every native Gemini protocol profile resolves to the shared function-declaration schema family.
    /// 验证每个原生 Gemini 协议配置都会解析到共享函数声明 Schema 族。
    #[test]
    fn native_gemini_profiles_share_the_function_declaration_schema_family() {
        for protocol_profile_id in [
            vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID,
            vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID,
            vulcan_ai_protocols::GEMINI_INTERACTIONS_PROFILE_ID,
        ] {
            assert_eq!(
                tool_schema_api_family_key(protocol_profile_id),
                "google-gemini-function-declarations"
            );
        }
    }

    /// RecordingProviderDeltaSink retains canonical Core deltas for adapter projection tests.
    /// RecordingProviderDeltaSink 为适配器投影测试保留规范 Core 增量。
    #[derive(Default)]
    struct RecordingProviderDeltaSink {
        /// deltas contains every canonical delta received from the public adapter.
        /// deltas 包含公共适配器发出的每一项规范增量。
        deltas: Vec<ProviderTurnDelta>,
    }

    impl VulcanProviderDeltaSink for RecordingProviderDeltaSink {
        /// Stores one canonical provider delta.
        /// 保存一项规范 Provider 增量。
        ///
        /// # Parameters
        /// 参数
        /// `delta` is the exact decoded delta produced by the public protocol.
        /// `delta` 是公共协议产出的精确解码增量。
        fn push_delta(&mut self, delta: ProviderTurnDelta) {
            self.deltas.push(delta);
        }
    }

    /// Verifies explicit response completion terminates the adapter without requiring transport EOF.
    /// 验证显式响应完成会终止适配器，而不要求传输层先到达 EOF。
    #[test]
    fn response_completion_is_a_protocol_terminal_boundary() {
        assert!(public_stream_event_is_terminal(
            &AiStreamEvent::ResponseCompleted
        ));
        assert!(public_stream_event_is_terminal(
            &AiStreamEvent::ResponseCancelled
        ));
        assert!(!public_stream_event_is_terminal(
            &AiStreamEvent::ResponseStarted {
                execution_id: String::from("response-1"),
            }
        ));
    }

    /// Verifies split Anthropic usage events retain start-event cache fields and terminal output fields.
    /// 验证拆分的 Anthropic 用量事件会同时保留开始事件的缓存字段与终态输出字段。
    #[test]
    fn partial_usage_updates_merge_without_erasing_known_fields() {
        let reported = |value| UsageValue::Known {
            value,
            source: vulcan_ai_contract::UsageValueSource::Reported,
        };
        let mut usage = RequestUsage {
            input_tokens: reported(1_000),
            cache_read_tokens: reported(800),
            cache_write_tokens: reported(120),
            ..RequestUsage::default()
        };

        usage.merge_update(RequestUsage {
            output_tokens: reported(75),
            total_tokens: reported(1_075),
            ..RequestUsage::default()
        });

        assert_eq!(usage.input_tokens, reported(1_000));
        assert_eq!(usage.cache_read_tokens, reported(800));
        assert_eq!(usage.cache_write_tokens, reported(120));
        assert_eq!(usage.output_tokens, reported(75));
        assert_eq!(usage.total_tokens, reported(1_075));
    }

    /// Verifies opaque compaction context requires the exact connection and resolved route.
    /// 验证不透明压缩上下文必须匹配精确连接与已解析路由。
    #[test]
    fn responses_compaction_context_requires_exact_route_identity() {
        let state = json!({
            "vulcan_ai": {
                "responses_compaction": {
                    "provider_id": "custom:proxy",
                    "connection_id": "connection-a",
                    "route_fingerprint": "https://proxy.example/v1/responses",
                    "protocol_profile_id": "openai-codex-responses.v1",
                    "model_id": "gpt-5-codex",
                    "items": [
                        {"type": "compaction_summary", "encrypted_content": "opaque"},
                        {"type": "message", "role": "assistant", "content": []}
                    ]
                }
            }
        });
        // matchingItems must normalize the official alias before replaying it to the provider.
        // matchingItems 必须在重放给供应商前规范化官方别名。
        let matching_items = responses_compaction_items_for_route(
            Some(&state),
            "custom:proxy",
            "connection-a",
            "https://proxy.example/v1/responses",
            "openai-codex-responses.v1",
            "gpt-5-codex",
        )
        .expect("matching route should replay context");
        assert_eq!(matching_items.len(), 2);
        assert_eq!(matching_items[0]["type"], "compaction");
        // publicMessage proves local checkpoint diagnostics are not emitted before the opaque item.
        // publicMessage 证明本地检查点诊断不会先于不透明项发出。
        let public_message = core_message_to_public(
            &vulcan_core::types::ConversationMessage {
                id: String::from("summary-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("local checkpoint diagnostics"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: Some(state.clone()),
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
            "custom:proxy",
            "connection-a",
            "https://proxy.example/v1/responses",
            "openai-codex-responses.v1",
            "gpt-5-codex",
        )
        .expect("matching compact anchor should project");
        assert!(public_message.content.is_empty());
        assert_eq!(public_message.provider_context_items.len(), 2);
        assert!(
            responses_compaction_items_for_route(
                Some(&state),
                "custom:proxy",
                "connection-b",
                "https://proxy.example/v1/responses",
                "openai-codex-responses.v1",
                "gpt-5-codex",
            )
            .is_err()
        );
        assert!(
            responses_compaction_items_for_route(
                Some(&state),
                "custom:proxy",
                "connection-a",
                "https://proxy.example/changed/responses",
                "openai-codex-responses.v1",
                "gpt-5-codex",
            )
            .is_err()
        );
    }

    /// Verifies ordinary replay content falls back locally when the credential or endpoint changes.
    /// 验证凭据或端点变化时，普通重放内容会安全回退到本地规范内容。
    #[test]
    fn ordinary_replay_content_requires_exact_connection_route() {
        // state contains one signed reasoning item produced by the original physical route.
        // state 包含由原始物理路由生成的一项带签名推理内容。
        let state = json!({
            "vulcan_ai": {
                "replay": ProviderReplayEnvelope::new(
                    "custom:proxy",
                    "connection-a",
                    "https://proxy.example/v1/responses",
                    "openai-codex-responses.v1",
                    "gpt-5-codex",
                    vec![ContentItem::Reasoning {
                        text: String::from("reasoning"),
                        signature: Some(String::from("opaque-signature")),
                    }],
                    vec![json!({
                        "type": "reasoning",
                        "id": "reasoning-item",
                        "status": "completed",
                        "summary": [{"type": "summary_text", "text": "reasoning"}],
                    })],
                )
            }
        });

        assert!(
            replay_content_for_route(
                Some(&state),
                "custom:proxy",
                "connection-a",
                "https://proxy.example/v1/responses",
                "openai-codex-responses.v1",
                "gpt-5-codex",
            )
            .expect("matching route should decode")
            .is_some()
        );
        assert!(
            replay_content_for_route(
                Some(&state),
                "custom:proxy",
                "connection-b",
                "https://proxy.example/v1/responses",
                "openai-codex-responses.v1",
                "gpt-5-codex",
            )
            .expect("credential mismatch should be a safe local fallback")
            .is_none()
        );
        assert!(
            replay_content_for_route(
                Some(&state),
                "custom:proxy",
                "connection-a",
                "https://proxy.example/changed/responses",
                "openai-codex-responses.v1",
                "gpt-5-codex",
            )
            .expect("endpoint mismatch should be a safe local fallback")
            .is_none()
        );
        assert_eq!(
            replay_provider_context_items_for_route(
                Some(&state),
                "custom:proxy",
                "connection-a",
                "https://proxy.example/v1/responses",
                "openai-codex-responses.v1",
                "gpt-5-codex",
            )
            .expect("matching provider context should decode")[0]["id"],
            "reasoning-item"
        );
        assert!(
            replay_provider_context_items_for_route(
                Some(&state),
                "custom:proxy",
                "connection-b",
                "https://proxy.example/v1/responses",
                "openai-codex-responses.v1",
                "gpt-5-codex",
            )
            .expect("credential mismatch should not replay provider context")
            .is_empty()
        );
        // switchedRouteMessage proves provider-owned items never cross a protocol boundary.
        // switchedRouteMessage 证明供应商持有项绝不会跨越协议边界。
        let switched_route_message = core_message_to_public(
            &vulcan_core::types::ConversationMessage {
                id: String::from("assistant-switched"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::from("reasoning"),
                    provider_message_id: None,
                    provider_message_state: Some(state.clone()),
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
            "custom:proxy",
            "connection-a",
            "https://proxy.example/v1/messages",
            "anthropic-messages.v1",
            "gpt-5-codex",
        )
        .expect("protocol switch should project provider-neutral content");
        assert!(switched_route_message.provider_context_items.is_empty());
        assert_eq!(
            switched_route_message.content,
            vec![ContentItem::Reasoning {
                text: String::from("reasoning"),
                signature: None,
            }]
        );
        // exactRouteMessage proves raw provider items replace canonical re-encoding only on their owner route.
        // exactRouteMessage 证明原始供应商项仅在归属路由上替代规范重编码。
        let exact_route_message = core_message_to_public(
            &vulcan_core::types::ConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::from("reasoning"),
                    provider_message_id: None,
                    provider_message_state: Some(state),
                    usage: None,
                    tool_calls: Vec::new(),
                },
            },
            "custom:proxy",
            "connection-a",
            "https://proxy.example/v1/responses",
            "openai-codex-responses.v1",
            "gpt-5-codex",
        )
        .expect("matching assistant should project exact provider context");
        assert!(exact_route_message.content.is_empty());
        assert_eq!(exact_route_message.provider_context_items.len(), 1);
    }

    /// Verifies captured Responses output items survive stream accumulation and durable replay.
    /// 验证捕获的 Responses 输出项可完整经过流累积与持久化重放。
    #[test]
    fn public_turn_persists_exact_responses_provider_context() {
        // liveChannel and sink satisfy the normal accumulator projection interfaces.
        // liveChannel 与 sink 满足累积器的常规投影接口。
        let (live_sender, _live_receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut sink = RecordingProviderDeltaSink::default();
        let mut accumulator = PublicTurnAccumulator::new(
            "deepseek",
            "connection-deepseek",
            "https://api.deepseek.com/responses",
            "openai-responses.v1",
            "deepseek-v4-flash",
        );
        // providerItem mirrors a completed assistant message emitted by Responses SSE.
        // providerItem 镜像 Responses SSE 发出的一条已完成助手消息。
        let provider_item = json!({
            "type": "message",
            "id": "message-item",
            "status": "completed",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "done"}],
        });

        for event in [
            AiStreamEvent::ResponseItemStarted {
                item_id: String::from("message-item"),
                item: ContentItem::Text {
                    text: String::new(),
                },
            },
            AiStreamEvent::ResponseTextDelta {
                item_id: String::from("message-item"),
                delta: String::from("done"),
            },
            AiStreamEvent::ResponseItemCompleted {
                item_id: String::from("message-item"),
            },
            AiStreamEvent::ProviderContextItemCaptured {
                item: provider_item.clone(),
            },
            AiStreamEvent::ResponseCompleted,
        ] {
            accumulator
                .apply_event("deepseek", &live_sender, &mut sink, event)
                .expect("completed Responses event should accumulate");
        }

        // responseState is the exact extension persisted into the assistant conversation node.
        // responseState 是持久化到助手会话节点中的精确扩展状态。
        let response = accumulator
            .finish()
            .expect("completed Responses turn should finish");
        let replay_value = response
            .provider_message_state
            .as_ref()
            .and_then(|state| state.pointer("/vulcan_ai/replay"))
            .expect("replay envelope should be persisted");
        let replay: ProviderReplayEnvelope = serde_json::from_value(replay_value.clone())
            .expect("replay envelope should deserialize");
        assert_eq!(
            replay.content,
            vec![ContentItem::Text {
                text: String::from("done"),
            }]
        );
        assert_eq!(replay.provider_context_items, vec![provider_item]);
    }

    /// Verifies partial provider captures cannot replace complete normalized assistant history.
    /// 验证不完整的供应商捕获项无法替代完整的规范助手历史。
    #[test]
    fn partial_provider_context_falls_back_to_normalized_replay() {
        // content requires both reasoning and visible assistant text.
        // content 同时要求推理与可见助手文本。
        let content = vec![
            ContentItem::Reasoning {
                text: String::from("inspect"),
                signature: None,
            },
            ContentItem::Text {
                text: String::from("done"),
            },
        ];
        // partialItems intentionally omit the provider message item that owns visible text.
        // partialItems 刻意缺少承载可见文本的供应商消息项。
        let partial_items = vec![json!({
            "type": "reasoning",
            "id": "reasoning-item",
            "status": "completed",
            "summary": [{"type": "summary_text", "text": "inspect"}],
        })];

        assert!(!provider_context_items_cover_replay_content(
            content.as_slice(),
            partial_items.as_slice(),
        ));
    }

    /// Verifies public text and reasoning deltas reach both Core and the live workbench stream.
    /// 验证公共文本与推理增量同时到达 Core 和实时工作台流。
    #[test]
    fn public_turn_deltas_are_forwarded_to_live_workbench_stream() {
        let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut sink = RecordingProviderDeltaSink::default();
        let mut accumulator = PublicTurnAccumulator::new(
            "minimax",
            "connection-minimax",
            "https://api.minimax.io/v1/messages",
            "anthropic-messages",
            "MiniMax-M3",
        );

        accumulator
            .apply_event(
                "minimax",
                &live_sender,
                &mut sink,
                AiStreamEvent::ResponseItemStarted {
                    item_id: String::from("text-item"),
                    item: ContentItem::Text {
                        text: String::new(),
                    },
                },
            )
            .expect("text item should start");
        accumulator
            .apply_event(
                "minimax",
                &live_sender,
                &mut sink,
                AiStreamEvent::ResponseTextDelta {
                    item_id: String::from("text-item"),
                    delta: String::from("最终答案"),
                },
            )
            .expect("text delta should be accepted");
        accumulator
            .apply_event(
                "minimax",
                &live_sender,
                &mut sink,
                AiStreamEvent::ReasoningStarted {
                    item_id: String::from("reasoning-item"),
                },
            )
            .expect("reasoning item should start");
        accumulator
            .apply_event(
                "minimax",
                &live_sender,
                &mut sink,
                AiStreamEvent::ReasoningDelta {
                    item_id: String::from("reasoning-item"),
                    delta: String::from("先分析"),
                },
            )
            .expect("reasoning delta should be accepted");

        assert!(matches!(
            sink.deltas.as_slice(),
            [
                ProviderTurnDelta::Output { delta: output },
                ProviderTurnDelta::Thinking {
                    delta: reasoning
                },
            ] if output == "最终答案" && reasoning == "先分析"
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::Output(output))) if output == "最终答案"
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::Thinking(reasoning))) if reasoning == "先分析"
        ));
    }

    /// Verifies generated image bytes enter managed attachment storage and the Data URL is discarded.
    /// 验证生成图片字节会进入托管附件存储，且 Data URL 会被丢弃。
    #[tokio::test]
    async fn generated_image_event_materializes_inside_workspace() {
        let workspace = std::env::temp_dir().join(format!(
            "vulcan-generated-resource-test-{}",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::create_dir_all(&workspace)
            .await
            .expect("test workspace should be created");
        let event = AiStreamEvent::ResponseItemStarted {
            item_id: String::from("item"),
            item: ContentItem::Resource {
                resource: ResourceReference {
                    id: String::from("image:item"),
                    kind: ResourceKind::Image,
                    media_type: Some(String::from("image/png")),
                    source_kind: vulcan_ai_contract::ResourceSourceKind::InlineData,
                    file_name: None,
                    image_detail: None,
                    local_path: None,
                    url: Some(String::from("data:image/png;base64,aW1hZ2U=")),
                },
            },
        };
        let materialized = materialize_provider_resource_event(
            "openai",
            workspace.to_string_lossy().as_ref(),
            event,
        )
        .await
        .expect("generated image should materialize");
        let AiStreamEvent::ResponseItemStarted {
            item: ContentItem::Resource { resource },
            ..
        } = materialized
        else {
            panic!("materialized event should remain a resource item");
        };
        assert!(resource.url.is_none());
        let local_path = PathBuf::from(
            resource
                .local_path
                .expect("materialized resource should have a local path"),
        );
        assert!(local_path.starts_with(&workspace));
        assert_eq!(
            tokio::fs::read(&local_path)
                .await
                .expect("materialized image should be readable"),
            b"image"
        );
        tokio::fs::remove_dir_all(&workspace)
            .await
            .expect("test workspace should be removed");
    }

    /// Verifies a user-safe remote resource URL is preserved instead of being decoded as Base64.
    /// 验证用户安全的远程资源 URL 会被保留，而不会被当作 Base64 解码。
    #[tokio::test]
    async fn remote_generated_image_url_remains_remote() {
        let event = AiStreamEvent::ResponseItemStarted {
            item_id: String::from("item"),
            item: ContentItem::Resource {
                resource: ResourceReference {
                    id: String::from("remote-image"),
                    kind: ResourceKind::Image,
                    media_type: Some(String::from("image/png")),
                    source_kind: vulcan_ai_contract::ResourceSourceKind::RemoteUrl,
                    file_name: None,
                    image_detail: None,
                    local_path: None,
                    url: Some(String::from("https://example.invalid/image.png")),
                },
            },
        };
        let preserved = materialize_provider_resource_event("openai", "C:\\workspace", event)
            .await
            .expect("remote resource should remain valid");
        assert!(matches!(
            preserved,
            AiStreamEvent::ResponseItemStarted {
                item: ContentItem::Resource { resource },
                ..
            } if resource.url.as_deref() == Some("https://example.invalid/image.png")
                && resource.local_path.is_none()
        ));
    }

    /// Verifies generated resource file stems cannot escape the host-owned output directory.
    /// 验证生成资源文件主名无法逃逸宿主自有输出目录。
    #[test]
    fn generated_resource_stem_removes_path_syntax() {
        assert_eq!(
            safe_generated_resource_stem("../../image:item"),
            "------image-item"
        );
    }

    /// Verifies a catalog maximum does not become the default request output budget.
    /// 验证目录最大值不会变成默认请求输出预算。
    #[test]
    fn public_output_limit_uses_default_below_catalog_maximum() {
        assert_eq!(
            resolved_public_output_token_limit(None, Some(393_216)),
            DEFAULT_PUBLIC_OUTPUT_TOKEN_LIMIT
        );
    }

    /// Verifies user narrowing remains authoritative and cannot exceed the catalog maximum.
    /// 验证用户收窄值保持权威且不能超过目录最大值。
    #[test]
    fn public_output_limit_honors_user_narrowing_and_catalog_cap() {
        assert_eq!(
            resolved_public_output_token_limit(Some(16_000), Some(393_216)),
            16_000
        );
        assert_eq!(
            resolved_public_output_token_limit(Some(500_000), Some(393_216)),
            393_216
        );
    }

    /// Verifies a cursor produced by another protocol profile is never relabeled and reused.
    /// 验证由其他协议配置产生的游标绝不会被重新标记并复用。
    #[test]
    fn continuation_rejects_protocol_profile_switch() {
        let config = CoreControllerConfig {
            provider_id: String::from("minimax"),
            model_id: String::from("MiniMax-M3"),
            system_prompt: String::from("system"),
            ..CoreControllerConfig::default()
        };
        let snapshot = CoreConversationSnapshot {
            messages: vec![vulcan_core::types::ConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("answer"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("response-1")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }],
            continuation: ProviderContinuation {
                provider_id: Some(String::from("minimax")),
                protocol_profile_id: Some(String::from("anthropic-messages")),
                model_id: Some(String::from("MiniMax-M3")),
                last_provider_message_id: Some(String::from("response-1")),
                last_local_message_id: Some(String::from("assistant-1")),
                context_rewrite_fingerprint: Some(config.provider_rewrite_fingerprint()),
                ..ProviderContinuation::default()
            },
            ..CoreConversationSnapshot::default()
        };

        assert!(
            build_public_continuation(
                &snapshot,
                &config,
                "minimax",
                "openai-chat",
                "MiniMax-M3",
                1,
            )
            .is_none()
        );
    }

    /// Verifies public failures preserve every serialized execution field instead of delimiter-splitting text.
    /// 验证公共失败会保留全部序列化执行字段，而不是通过分隔符拆分文本。
    #[test]
    fn public_execution_failure_message_is_json() {
        let error = vulcan_ai_contract::AiExecutionError {
            category: vulcan_ai_contract::AiErrorCategory::Unclassified,
            subcategory: Some(String::from("upstream")),
            stage: String::from("stream"),
            http_status: Some(503),
            transport: Some(vulcan_ai_contract::AiTransportKind::Http),
            websocket_close_code: None,
            upstream_code: Some(String::from("server_is_overloaded")),
            upstream_type: Some(String::from("server_error")),
            upstream_parameter: Some(String::from("model")),
            upstream_message: Some(String::from("upstream detail | key=value")),
            request_id: Some(String::from("req-123")),
            provider_id: Some(String::from("custom:provider")),
            entry_id: Some(String::from("entry-1")),
            model_id: Some(String::from("model-1")),
            message: String::from("Our servers are currently overloaded. Please try again later."),
            dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        };

        let serialized = public_execution_failure_message(&error);
        let value: Value = serde_json::from_str(&serialized).expect("public failure must be JSON");
        assert_eq!(value["upstream_code"], "server_is_overloaded");
        assert_eq!(value["upstream_message"], "upstream detail | key=value");
        assert_eq!(value["request_id"], "req-123");
        assert_eq!(value["message"], error.message);
    }

    /// Verifies frozen manifest constraints enforce source and MIME rules for any provider id.
    /// 验证冻结清单约束会针对任意供应商标识执行来源与 MIME 规则。
    #[test]
    fn frozen_resource_constraints_are_provider_neutral() {
        // localOnly is a generic route rule and contains no provider identity.
        // localOnly 是不含供应商身份的通用路由规则。
        let local_only = InputResourceConstraint {
            resource_kind: ResourceKind::Audio,
            accepted_mime_types: Vec::new(),
            max_bytes: None,
            max_count: None,
            reference_policy: ResourceReferencePolicy::LocalOnly,
            upload_channel_refs: Vec::new(),
        };
        // remoteAudio intentionally violates the generic local-only policy.
        // remoteAudio 刻意违反通用的仅本地策略。
        let remote_audio = ResourceReference {
            id: String::from("audio-1"),
            kind: ResourceKind::Audio,
            media_type: Some(String::from("audio/mpeg")),
            source_kind: vulcan_ai_contract::ResourceSourceKind::RemoteUrl,
            file_name: None,
            image_detail: None,
            local_path: None,
            url: Some(String::from("https://media.example/audio.mp3")),
        };
        assert!(
            validate_frozen_input_resource_constraint(
                "custom:constraint-fixture",
                &local_only,
                &remote_audio,
            )
            .is_err()
        );

        // pdfOnly proves MIME admission comes entirely from frozen manifest data.
        // pdfOnly 证明 MIME 准入完全来自冻结清单数据。
        let pdf_only = InputResourceConstraint {
            resource_kind: ResourceKind::File,
            accepted_mime_types: vec![String::from("application/pdf")],
            max_bytes: None,
            max_count: None,
            reference_policy: ResourceReferencePolicy::Any,
            upload_channel_refs: Vec::new(),
        };
        // textFile uses an otherwise valid remote representation with the wrong MIME type.
        // textFile 使用其他方面有效但 MIME 类型错误的远程表示。
        let text_file = ResourceReference {
            id: String::from("file-1"),
            kind: ResourceKind::File,
            media_type: Some(String::from("text/plain")),
            source_kind: vulcan_ai_contract::ResourceSourceKind::RemoteUrl,
            file_name: None,
            image_detail: None,
            local_path: None,
            url: Some(String::from("https://media.example/file.txt")),
        };
        assert!(
            validate_frozen_input_resource_constraint(
                "custom:constraint-fixture",
                &pdf_only,
                &text_file,
            )
            .is_err()
        );
    }
}
