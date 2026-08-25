//! Vulcan public-contract adapter for the existing host tool controller.
//! 面向现有宿主工具控制器的 Vulcan 公共契约适配器。

use crate::agent_runtime::events::{
    TransientReasoningStatus, TransientReasoningStatusKind, TransientReasoningSummaryItem,
    provider_request_durability_barrier,
};
use crate::agent_runtime::provider_attachment::{
    build_missing_attachment_replay_text, model_visible_attachment_field,
};
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
use crate::attachment_assets::{
    AttachmentDescriptor, resolve_attachment_replay_path, stage_attachment_bytes,
};
use async_stream::try_stream;
use async_trait::async_trait;
use base64::Engine;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_ai_contract::{
    AiContinuation, AiExecutionError, AiExecutionRequest, AiMessage, AiOperation, AiStreamEvent,
    ContentItem, MessageRole, ModelParameterValue, NativeToolKind, ParallelToolCallPolicy,
    PromptCachePolicy, ProviderReplayEnvelope, ReasoningEffort, ReasoningOutputPolicy,
    RequestUsage, ResourceKind, ResourceReference, ResourceSourceKind, StructuredOutputPolicy,
    ToolCall as PublicToolCall, ToolChoicePolicy, ToolDefinition, ToolResult, UsageValue,
    canonicalize_codex_responses_replay_items, is_replayable_codex_responses_item,
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
    InputResourceConstraint, ProviderExecutionClient, ProviderExecutionCredentialLease,
    ProviderExecutionRoute, ProviderExecutionStream, ProviderWireRequestCapture,
    ProviderWireRequestTraceDispatch, ProviderWireResponseCapture, ResolvedPublicAiExecution,
    ResourceReferencePolicy, ResponseProjectionProfile, ToolSchemaProjectionMode,
    TransientReasoningStatusMode, UploadReferenceEncoding,
};
use vulcan_tools_manager::{VulcanToolSchemaProjectionProfile, project_tool_parameters_schema};

/// Vulcan-owned default output budget used when the provider reports no independent output limit.
/// 当供应商未报告独立输出上限时使用的 Vulcan 本地默认输出预算。
const DEFAULT_PUBLIC_OUTPUT_TOKEN_LIMIT: u64 = 32_000;
/// Maximum aggregate Base64 payload bytes allowed for host-inline resources in one request.
/// 单次请求中宿主内联资源允许使用的 Base64 聚合载荷最大字节数。
const HOST_INLINE_RESOURCE_BASE64_BUDGET_BYTES: u64 = 2 * 1024 * 1024;
/// Versioned on-disk contract for grouped Provider API request and response traces.
/// 分组 Provider API 请求与响应追踪使用的版本化磁盘契约。
const PROVIDER_API_TRACE_SCHEMA_VERSION: &str = "vulcan.provider-api-trace.v1";
/// Structured subcategory emitted only after the exact OAuth account secret was refreshed.
/// 仅在精确 OAuth 账号秘密已经刷新后发出的结构化子类别。
pub(crate) const OAUTH_CREDENTIAL_REFRESHED_SUBCATEGORY: &str = "oauth_credential_refreshed";
/// Process-local allocator synchronized to each opened session's persisted generation floor.
/// 会同步到每个已打开会话持久化代际下限的进程内分配器。
static PROVIDER_API_TRACE_GROUP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Identifies one physical API attempt inside a stable logical retry group.
/// 标识稳定逻辑重试组中的一次真实 API 尝试。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderApiTraceAttemptIdentity {
    /// Process-local monotonic generation used to own one rotating slot.
    /// 用于占有一个轮转槽位的进程内单调代际。
    group_sequence: u64,
    /// Stable opaque identity shared by all physical retries.
    /// 所有真实重试共享的稳定不透明标识。
    group_id: String,
    /// Group creation timestamp retained unchanged across retries.
    /// 在重试间保持不变的组创建时间戳。
    group_created_at_unix_ms: u128,
    /// Zero-based physical attempt index.
    /// 从零开始的真实尝试索引。
    attempt_index: u32,
}

/// Selects the response evidence persisted for one trace-attempt snapshot.
/// 选择一次追踪尝试快照要持久化的响应证据。
enum ProviderApiTraceResponseState {
    /// The request is durable and transport dispatch has not started yet.
    /// 请求已经可靠落盘且传输分派尚未开始。
    AwaitingResponse,
    /// The response capture channel closed without publishing a capture.
    /// 响应捕获通道关闭且没有发布捕获结果。
    CaptureChannelClosed,
    /// A partial or complete raw response capture is available.
    /// 已取得部分或完整的原始响应捕获。
    Captured(ProviderWireResponseCapture),
}

/// Describes the first host-side processing failure observed after raw Provider transport capture.
/// 描述 Provider 原始传输捕获之后观察到的首个宿主侧处理失败。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderApiTraceProcessingFailure {
    /// Stable pipeline boundary used by `outcome.json` diagnostics.
    /// `outcome.json` 诊断使用的稳定流水线边界。
    stage: String,
    /// Controlled error text that excludes credentials and raw request payloads.
    /// 不包含凭据与原始请求载荷的受控错误文本。
    message: String,
}

/// Publishes at most one post-transport processing failure to the detached trace writer.
/// 向独立追踪写入任务至多发布一次传输后处理失败。
struct ProviderApiTraceProcessingFailureSender {
    /// One-shot channel retained until successful completion or the first processing failure.
    /// 保留到成功完成或首次处理失败的一次性通道。
    sender: Option<tokio::sync::oneshot::Sender<ProviderApiTraceProcessingFailure>>,
}

impl ProviderApiTraceProcessingFailureSender {
    /// Creates a failure publisher for one traced physical Provider attempt.
    /// 为一次已追踪的真实 Provider 尝试创建失败发布器。
    ///
    /// # Parameters
    /// 参数
    /// `sender` is absent when persisted Provider tracing is disabled.
    /// 关闭 Provider 持久化追踪时 `sender` 为空。
    ///
    /// # Returns
    /// 返回值
    /// A publisher that silently drops diagnostics when tracing is disabled or already finalized.
    /// 返回一个在追踪关闭或已结束时静默丢弃诊断的发布器。
    fn new(
        sender: Option<tokio::sync::oneshot::Sender<ProviderApiTraceProcessingFailure>>,
    ) -> Self {
        Self { sender }
    }

    /// Records the first exact post-transport failure without changing business error semantics.
    /// 记录首个精确的传输后失败，且不改变业务错误语义。
    ///
    /// # Parameters
    /// 参数
    /// `stage` is the stable processing boundary and `error` is the original controlled failure.
    /// `stage` 是稳定处理边界，`error` 是原始受控失败。
    fn observe_failure(&mut self, stage: &str, error: &VulcanCoreError) {
        let Some(sender) = self.sender.take() else {
            return;
        };
        let _ = sender.send(ProviderApiTraceProcessingFailure {
            stage: stage.to_string(),
            message: error.to_string(),
        });
    }
}

/// Owns the currently retryable Provider API trace group for one controller adapter.
/// 持有一个控制器适配器当前可重试的 Provider API 追踪组。
#[derive(Debug, Default)]
struct ProviderApiTraceGroupState {
    /// Active logical group, absent before dispatch and after success.
    /// 活动逻辑组；分派前与成功后为空。
    active: Option<ProviderApiTraceActiveGroup>,
}

/// Tracks the next physical attempt index for one active logical group.
/// 跟踪一个活动逻辑组的下一次真实尝试索引。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderApiTraceActiveGroup {
    /// Monotonic group generation.
    /// 单调组代际。
    group_sequence: u64,
    /// Stable opaque group identity.
    /// 稳定不透明组标识。
    group_id: String,
    /// Frozen creation timestamp.
    /// 冻结的创建时间戳。
    group_created_at_unix_ms: u128,
    /// Index assigned to the next physical request.
    /// 分配给下一次真实请求的索引。
    next_attempt_index: u32,
}

/// Tracks request-wide encoded media consumption before Provider request serialization.
/// 在 Provider 请求序列化前跟踪请求级编码媒体消耗。
#[derive(Debug, Default)]
struct InlineResourceProjectionBudget {
    /// Request-wide native resource counts keyed by canonical modality.
    /// 按规范模态索引的请求级原生资源数量。
    retained_resource_counts: BTreeMap<ResourceKind, u64>,
    /// Aggregate Base64 bytes already committed by earlier canonical resources.
    /// 先前规范资源已经占用的 Base64 聚合字节数。
    used_base64_bytes: u64,
}

impl InlineResourceProjectionBudget {
    /// Reserves the exact Base64 expansion for one local resource.
    /// 为一个本地资源保留精确的 Base64 膨胀字节数。
    ///
    /// # Parameters
    /// 参数
    /// `resource_kind` and `max_count` freeze the route count contract; `raw_byte_len` is the
    /// optional verified local payload byte count when Base64 projection is required.
    /// `resource_kind` 与 `max_count` 冻结路线数量契约；`raw_byte_len` 是需要 Base64 投影时
    /// 可选的已验证本地载荷字节数。
    ///
    /// # Returns
    /// 返回值
    /// Success after both budgets are atomically committed, or the exact request-wide limit name.
    /// 两项预算原子提交后返回成功，否则返回精确的请求级限制名称。
    fn reserve(
        &mut self,
        resource_kind: ResourceKind,
        max_count: Option<u64>,
        raw_byte_len: Option<u64>,
    ) -> Result<(), &'static str> {
        let current_count = self
            .retained_resource_counts
            .get(&resource_kind)
            .copied()
            .unwrap_or_default();
        let Some(next_count) = current_count.checked_add(1) else {
            return Err("the frozen route request-wide resource-count limit");
        };
        if max_count.is_some_and(|limit| next_count > limit) {
            return Err("the frozen route request-wide resource-count limit");
        }
        // encodedByteLen uses the closed four-by-three Base64 expansion without approximation.
        // encodedByteLen 使用封闭的四比三 Base64 膨胀公式，不采用近似值。
        let encoded_byte_len = raw_byte_len
            .unwrap_or_default()
            .saturating_add(2)
            .checked_div(3)
            .unwrap_or_default()
            .saturating_mul(4);
        let Some(next_used) = self.used_base64_bytes.checked_add(encoded_byte_len) else {
            return Err("the request-wide host inline-media budget");
        };
        if next_used > HOST_INLINE_RESOURCE_BASE64_BUDGET_BYTES {
            return Err("the request-wide host inline-media budget");
        }
        self.retained_resource_counts
            .insert(resource_kind, next_count);
        self.used_base64_bytes = next_used;
        Ok(())
    }

    /// Reports whether the request-wide route count is already exhausted without mutating budget state.
    /// 在不修改预算状态的情况下报告请求级路线数量是否已经耗尽。
    ///
    /// # Parameters
    /// 参数
    /// `resource_kind` selects one modality and `max_count` is the exact frozen route bound.
    /// `resource_kind` 选择一种模态，`max_count` 是精确冻结路线边界。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when another native resource would exceed the declared request maximum.
    /// 仅当再加入一个原生资源会超过声明的请求上限时返回 `true`。
    fn count_limit_reached(&self, resource_kind: ResourceKind, max_count: Option<u64>) -> bool {
        max_count.is_some_and(|limit| {
            self.retained_resource_counts
                .get(&resource_kind)
                .copied()
                .unwrap_or_default()
                >= limit
        })
    }
}

/// Resolves the single transient reasoning mode frozen into one response-profile chain.
/// 解析冻结在单条响应 Profile 链中的唯一瞬态推理模式。
///
/// # Parameters
/// 参数
/// `provider_id` identifies controlled failures and `profiles` is the exact frozen chain.
/// `provider_id` 标识受控失败，`profiles` 是精确冻结的 Profile 链。
///
/// # Returns
/// 返回值
/// The configured runtime status kind, or an error when multiple profiles claim the channel.
/// 返回已配置的运行时状态类型；多个 Profile 同时声明该通道时返回错误。
fn transient_reasoning_status_kind(
    provider_id: &str,
    profiles: &[ResponseProjectionProfile],
) -> Result<Option<TransientReasoningStatusKind>, VulcanCoreError> {
    // configured iterates only profiles that explicitly claim the transient status channel.
    // configured 只遍历显式声明瞬态状态通道的 Profile。
    let mut configured = profiles
        .iter()
        .filter_map(|profile| profile.transient_reasoning_status);
    // mode is the sole frozen presentation kind accepted for this request.
    // mode 是当前请求唯一接受的冻结展示类型。
    let Some(mode) = configured.next() else {
        return Ok(None);
    };
    if configured.next().is_some() {
        return Err(provider_failure(
            provider_id,
            "Multiple response profiles declared the transient reasoning status channel.",
        ));
    }
    Ok(Some(match mode {
        TransientReasoningStatusMode::Summary => TransientReasoningStatusKind::Summary,
        TransientReasoningStatusMode::Encrypted => TransientReasoningStatusKind::Encrypted,
    }))
}

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

/// Raises one process-local trace generation allocator to an observed persisted floor.
/// 将一个进程内追踪代际分配器提升到已观察到的持久化下限。
///
/// # Parameters
/// 参数
/// `sequence` is the allocator and `persisted_floor` is the greatest generation already on disk.
/// `sequence` 是分配器，`persisted_floor` 是磁盘上已经存在的最大代际。
fn synchronize_provider_api_trace_group_sequence(sequence: &AtomicU64, persisted_floor: u64) {
    sequence.fetch_max(persisted_floor, Ordering::AcqRel);
}

/// Reserves the next unique process-local Provider API trace group generation.
/// 预留下一个进程内唯一的 Provider API 追踪组代际。
///
/// # Parameters
/// 参数
/// `sequence` is already synchronized with the owning session's persisted generation floor.
/// `sequence` 已经与所属会话的持久化代际下限完成同步。
///
/// # Returns
/// 返回值
/// The next generation, or an explicit error instead of wrapping at the integer boundary.
/// 返回下一代际；达到整数边界时返回明确错误而不是回绕。
fn reserve_provider_api_trace_group_sequence(sequence: &AtomicU64) -> Result<u64, String> {
    // previous remains unchanged when the integer boundary prevents a safe reservation.
    // previous 在整数边界阻止安全预留时保持不变。
    let previous = sequence
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            current.checked_add(1)
        })
        .map_err(|_| String::from("Provider API trace group generation is exhausted."))?;
    Ok(previous + 1)
}

/// Synchronizes trace generation allocation with one retained session before dispatch begins.
/// 在分派开始前将追踪代际分配与一个保留会话同步。
///
/// # Parameters
/// 参数
/// `session_storage_directory` is the exact session owner and `cache_scope_id` is its UUID.
/// `session_storage_directory` 是精确会话归属目录，`cache_scope_id` 是其 UUID。
///
/// # Returns
/// 返回值
/// Success after the process allocator is no lower than every persisted rotating slot.
/// 当进程分配器不低于全部已持久化轮转槽位后返回成功。
fn synchronize_provider_api_trace_generation_with_session(
    session_storage_directory: Option<&Path>,
    cache_scope_id: &str,
) -> Result<(), String> {
    // sessionDirectory is mandatory because the allocator must observe its durable predecessor.
    // sessionDirectory 是必需的，因为分配器必须观察其可靠持久化的前序代际。
    let session_directory = session_storage_directory.ok_or_else(|| {
        String::from(
            "Provider request tracing is enabled but the execution has no exact session storage directory.",
        )
    })?;
    // sessionId proves the supplied directory belongs to this exact Provider execution scope.
    // sessionId 证明给定目录属于这次精确 Provider 执行作用域。
    let session_id = uuid::Uuid::parse_str(cache_scope_id).map_err(|error| {
        format!("Provider request trace session id `{cache_scope_id}` is invalid: {error}")
    })?;
    // persistedFloor prevents a fresh process from reusing generations retained by an earlier run.
    // persistedFloor 防止新进程复用更早运行已经保留的代际。
    let persisted_floor =
        vulcan_workspace_store::workspace_store::load_provider_api_trace_group_sequence_floor(
            session_directory,
            session_id,
        )?;
    synchronize_provider_api_trace_group_sequence(
        &PROVIDER_API_TRACE_GROUP_SEQUENCE,
        persisted_floor,
    );
    Ok(())
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

/// Selects the authoritative failure stage for one persisted Provider trace attempt.
/// 为一次持久化 Provider 追踪尝试选择权威失败阶段。
///
/// # Parameters
/// 参数
/// `transport_stage` and `transport_message` come from pre-parser wire capture;
/// `processing_failure` comes from the host-side public response accumulator.
/// `transport_stage` 与 `transport_message` 来自解析前 Wire 捕获；`processing_failure`
/// 来自宿主侧公共响应累积器。
///
/// # Returns
/// 返回值
/// The effective stage, effective message, and whether only local post-transport processing failed.
/// 返回有效阶段、有效消息，以及是否仅本地传输后处理失败。
fn select_provider_api_trace_failure(
    transport_stage: Option<&str>,
    transport_message: Option<&str>,
    processing_failure: Option<&ProviderApiTraceProcessingFailure>,
) -> (Option<String>, Option<String>, bool) {
    let local_processing_failed = transport_stage.is_none() && processing_failure.is_some();
    let stage = transport_stage
        .map(str::to_string)
        .or_else(|| processing_failure.map(|failure| failure.stage.clone()));
    let message = transport_message
        .map(str::to_string)
        .or_else(|| processing_failure.map(|failure| failure.message.clone()));
    (stage, message, local_processing_failed)
}

/// Encodes retained Provider response bytes with the smaller exact JSON-safe representation.
/// 使用较小且可精确还原的 JSON 安全表示编码已保留的 Provider 响应字节。
///
/// # Parameters
/// 参数
/// `body` is the bounded response prefix captured before protocol parsing.
/// `body` 是在协议解析前捕获的有界响应前缀。
///
/// # Returns
/// 返回值
/// The declared body encoding and its exact persisted payload.
/// 返回声明的正文编码及其可精确还原的持久化载荷。
#[cfg(test)]
fn encode_provider_response_trace_body(body: Vec<u8>) -> (&'static str, String) {
    match String::from_utf8(body) {
        Ok(utf8_body) => {
            // escapedUtf8Bytes measures the actual JSON string cost, including its surrounding quotes.
            // escapedUtf8Bytes 测量实际 JSON 字符串开销，包括外围引号。
            let escaped_utf8_bytes = serde_json::to_vec(&utf8_body)
                .expect("serializing one UTF-8 response string should be infallible")
                .len();
            // base64Body is retained only when it prevents JSON escaping from exceeding that exact form.
            // base64Body 仅在能避免 JSON 转义超过该精确表示时才会被保留。
            let base64_body =
                base64::engine::general_purpose::STANDARD.encode(utf8_body.as_bytes());
            let base64_json_bytes = base64_body.len().saturating_add(2);
            if escaped_utf8_bytes <= base64_json_bytes {
                ("utf8", utf8_body)
            } else {
                ("base64", base64_body)
            }
        }
        Err(error) => (
            "base64",
            base64::engine::general_purpose::STANDARD.encode(error.into_bytes()),
        ),
    }
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
    let session_storage_directory = request.session_storage_directory.clone();
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
        recovery_action,
        prompt_intervention,
    } = prepare_provider_transport_stream_launch(request);
    execute_host_controlled_stream_with_prepared_inputs(
        runtime_inputs,
        core_snapshot,
        move |live_sender, adapter_inputs| {
            VulcanPublicContractAdapter::new(
                provider_request_trace_enabled,
                session_storage_directory.clone(),
                resolved.route.clone(),
                resolved.model.id.clone(),
                cache_scope_id.clone(),
                resolved.credential.clone(),
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
        recovery_action,
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
            None,
            resolved.route.clone(),
            resolved.model.id.clone(),
            cache_scope_id,
            resolved.credential.clone(),
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
        // secret is resolved at capture time so previews observe the same latest-revision boundary as execution.
        // secret 在捕获时解析，使预览与执行观察到相同的最新修订版边界。
        let secret = resolved
            .credential
            .resolve_for_request()
            .await
            .map_err(|error| error.to_string())?;
        let encoded = resolved
            .client
            .encode_effective_request_with_credential(
                &resolved.route,
                &public_request.public,
                &secret,
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
    /// Frozen system-setting decision for grouped request and raw-response tracing.
    /// 当前控制器运行期间冻结的分组请求与原始返回追踪系统设置决策。
    provider_request_trace_enabled: bool,
    /// Exact session directory for trace writes; absent when trace persistence is disabled.
    /// 追踪写入使用的精确会话目录；关闭追踪持久化时为空。
    session_storage_directory: Option<PathBuf>,
    /// Shared logical trace group state retained across transparent physical retries.
    /// 在透明真实重试间保留的共享逻辑追踪组状态。
    provider_api_trace_group_state: std::sync::Arc<std::sync::Mutex<ProviderApiTraceGroupState>>,
    /// Immutable provider execution route.
    /// 不可变供应商执行路由。
    route: ProviderExecutionRoute,
    /// Exact model identifier fixed for this controller execution.
    /// 为本次控制器执行固定的精确模型标识。
    model_id: String,
    /// Stable session cache scope shared by every physical request in this controller run.
    /// 本次控制器运行中每个物理请求共享的稳定会话缓存作用域。
    cache_scope_id: String,
    /// Exact account lease that resolves the latest secret before every physical request.
    /// 在每次真实请求前解析最新秘密的精确账号租约。
    credential: ProviderExecutionCredentialLease,
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
    /// `credential`, and `client` identify the exact execution path; `tools` are host definitions;
    /// `output_token_limit`, `prompt_cache`, and `model_parameters` are resolved before dispatch;
    /// `live_sender` projects tool construction to the WebUI.
    /// `provider_request_trace_enabled` 冻结持久化诊断开关；`route`、`credential` 与 `client`
    /// 标识精确执行路径；`tools` 是宿主工具定义；`output_token_limit`、`prompt_cache` 与
    /// `model_parameters` 在分派前解析；`live_sender` 将工具构造投影到 WebUI。
    ///
    /// # Returns
    /// 返回值
    /// A reusable adapter whose route remains fixed for the controller run.
    /// 返回在控制器运行期间路由保持固定的可复用适配器。
    pub(crate) fn new(
        provider_request_trace_enabled: bool,
        session_storage_directory: Option<PathBuf>,
        route: ProviderExecutionRoute,
        model_id: String,
        cache_scope_id: String,
        credential: ProviderExecutionCredentialLease,
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
        let public_tools = project_public_tool_definitions(&route, tools);
        // effectiveTraceEnabled disables only diagnostics when its durable generation cannot be established.
        // effectiveTraceEnabled 仅在无法建立可靠代际时关闭诊断，不影响真实 Provider 请求。
        let provider_request_trace_enabled = if provider_request_trace_enabled {
            match synchronize_provider_api_trace_generation_with_session(
                session_storage_directory.as_deref(),
                cache_scope_id.as_str(),
            ) {
                Ok(()) => true,
                Err(error) => {
                    eprintln!(
                        "[provider-api-trace] session={} initialization_failed={error}",
                        cache_scope_id
                    );
                    false
                }
            }
        } else {
            false
        };
        Self {
            provider_request_trace_enabled,
            session_storage_directory,
            provider_api_trace_group_state: std::sync::Arc::new(std::sync::Mutex::new(
                ProviderApiTraceGroupState::default(),
            )),
            route,
            model_id,
            cache_scope_id,
            credential,
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

    /// Claims one physical attempt in the active logical Provider API trace group.
    /// 在活动逻辑 Provider API 追踪组中认领一次真实尝试。
    ///
    /// # Returns
    /// 返回值
    /// A stable group identity and a unique attempt index when tracing is enabled, otherwise `None`.
    /// 启用追踪时返回稳定组标识与唯一尝试索引，否则返回 `None`。
    fn claim_provider_api_trace_attempt(
        &self,
    ) -> Result<Option<ProviderApiTraceAttemptIdentity>, String> {
        if !self.provider_request_trace_enabled {
            return Ok(None);
        }
        let mut state = self
            .provider_api_trace_group_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.active.as_ref().is_some_and(|group| {
            group.next_attempt_index
                >= vulcan_workspace_store::workspace_store::PROVIDER_API_TRACE_MAX_ATTEMPTS_PER_GROUP
        }) {
            return Ok(None);
        }
        if state.active.is_none() {
            state.active = Some(ProviderApiTraceActiveGroup {
                group_sequence: reserve_provider_api_trace_group_sequence(
                    &PROVIDER_API_TRACE_GROUP_SEQUENCE,
                )?,
                group_id: uuid::Uuid::new_v4().to_string(),
                group_created_at_unix_ms: provider_request_trace_timestamp_ms().unwrap_or_default(),
                next_attempt_index: 0,
            });
        }
        let group = state
            .active
            .as_mut()
            .ok_or_else(|| String::from("Provider API trace group reservation was lost."))?;
        let identity = ProviderApiTraceAttemptIdentity {
            group_sequence: group.group_sequence,
            group_id: group.group_id.clone(),
            group_created_at_unix_ms: group.group_created_at_unix_ms,
            attempt_index: group.next_attempt_index,
        };
        group.next_attempt_index = group.next_attempt_index.saturating_add(1);
        Ok(Some(identity))
    }

    /// Closes one logical Provider API trace group after the matching request succeeds durably.
    /// 在匹配请求可靠成功后关闭一个逻辑 Provider API 追踪组。
    ///
    /// # Parameters
    /// 参数
    /// `identity` is the exact attempt that completed the logical request successfully.
    /// `identity` 是成功完成逻辑请求的精确尝试。
    fn complete_provider_api_trace_group(&self, identity: &ProviderApiTraceAttemptIdentity) {
        let mut state = self
            .provider_api_trace_group_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.active.as_ref().is_some_and(|group| {
            group.group_sequence == identity.group_sequence && group.group_id == identity.group_id
        }) {
            state.active = None;
        }
    }

    /// Persists one bounded dispatched API request paired with its pre-parser response evidence.
    /// 为一次执行持久化一份有界的实际分派 API 请求及其解析前响应证据。
    ///
    /// # Parameters
    /// 参数
    /// `execution_id` is the physical call identity shared with the real execution client.
    /// `execution_id` 是与真实执行客户端共享的物理调用标识。
    /// `turn_anchor_message_id` is the latest canonical message that anchored this Provider turn.
    /// `turn_anchor_message_id` 是锚定当前 Provider 回合的最新规范消息。
    /// `captured_request` is the exact secret-safe request projected immediately before dispatch;
    /// `captured_response` retains bounded transport bytes before local protocol parsing.
    /// `captured_request` 是分派前立即投影的精确无秘密请求；`captured_response` 保留本地协议
    /// 解析前的有界传输字节。
    ///
    /// # Returns
    /// 返回值
    /// Success only after the trace has been atomically published in the owning session directory.
    /// 仅在追踪已原子发布到所属会话目录后返回成功。
    async fn persist_provider_api_trace_snapshot(
        &self,
        identity: ProviderApiTraceAttemptIdentity,
        execution_id: &str,
        turn_anchor_message_id: Option<&str>,
        captured_request: ProviderWireRequestCapture,
        response_state: ProviderApiTraceResponseState,
        processing_failure: Option<ProviderApiTraceProcessingFailure>,
    ) -> Result<(), VulcanCoreError> {
        let captured_at_unix_ms = provider_request_trace_timestamp_ms()
            .map_err(|error| provider_failure(self.provider_id(), &error))?;
        let slot_index = identity.group_sequence.saturating_sub(1)
            % vulcan_workspace_store::workspace_store::PROVIDER_REQUEST_TRACE_SLOT_COUNT;
        let session_id = uuid::Uuid::parse_str(self.cache_scope_id.as_str()).map_err(|error| {
            provider_failure(
                self.provider_id(),
                &format!(
                    "Provider request trace session id `{}` is invalid: {error}",
                    self.cache_scope_id
                ),
            )
        })?;
        let headers = provider_request_trace_headers(&captured_request.headers);
        let (captured_response, missing_capture_state) = match response_state {
            ProviderApiTraceResponseState::AwaitingResponse => (None, "awaiting_response"),
            ProviderApiTraceResponseState::CaptureChannelClosed => {
                (None, "response_capture_channel_closed")
            }
            ProviderApiTraceResponseState::Captured(captured) => (Some(captured), "captured"),
        };
        // processingFailure remains a separate field while filling the primary stage only when transport capture succeeded.
        // processingFailure 保持为独立字段，并且仅在传输捕获成功时补充主失败阶段。
        let processing_failure_stage = processing_failure
            .as_ref()
            .map(|failure| failure.stage.clone());
        let processing_failure_message = processing_failure
            .as_ref()
            .map(|failure| failure.message.clone());
        // responseArtifacts keep raw bytes separate from JSON metadata so failed partial payloads remain exact.
        // responseArtifacts 将原始字节与 JSON 元数据分离，使失败时的部分载荷保持精确。
        let (response_meta, response_body, chunks, frames, outcome) = captured_response
            .map_or_else(
                || {
                    (
                        json!({
                            "capture_point": "before_local_response_framing_and_protocol_decode",
                            "capture_state": missing_capture_state,
                            "retained_bytes": 0,
                            "received_bytes": 0,
                        }),
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        json!({
                            "classification": missing_capture_state,
                            "stage": if missing_capture_state == "awaiting_response" {
                                "request_persisted_before_dispatch"
                            } else {
                                "response_capture"
                            },
                            "complete": false,
                            "body_truncated": false,
                            "chunk_index_truncated": false,
                            "frame_index_truncated": false,
                            "normalized_tool_call_abort": Value::Null,
                            "failure_stage": Value::Null,
                            "failure_message": Value::Null,
                            "transport_failure_stage": Value::Null,
                            "transport_failure_message": Value::Null,
                            "processing_failure_stage": processing_failure_stage.clone(),
                            "processing_failure_message": processing_failure_message.clone(),
                        }),
                    )
                },
                |captured| {
                    let retained_bytes = captured.body.len();
                    let transport_failure_stage = captured.failure_stage.clone();
                    let transport_failure_message = captured.failure_message.clone();
                    let (
                        effective_failure_stage,
                        effective_failure_message,
                        local_processing_failed,
                    ) = select_provider_api_trace_failure(
                        transport_failure_stage.as_deref(),
                        transport_failure_message.as_deref(),
                        processing_failure.as_ref(),
                    );
                    let capture_complete = captured.outcome != "stream_failed_or_cancelled"
                        && !captured.body_truncated
                        && !captured.chunk_index_truncated
                        && !captured.frame_index_truncated
                        && !local_processing_failed;
                    let outcome_stage = effective_failure_stage
                        .as_deref()
                        .unwrap_or("response_capture_complete")
                        .to_string();
                    let chunks = captured
                        .chunks
                        .iter()
                        .enumerate()
                        .map(|(index, chunk)| {
                            json!({
                                "index": index,
                                "offset": chunk.offset,
                                "length": chunk.length,
                                "observed_at_unix_ms": chunk.observed_at_unix_ms,
                                "sha256": chunk.sha256,
                            })
                        })
                        .collect();
                    let frames = captured
                        .frames
                        .iter()
                        .enumerate()
                        .map(|(index, frame)| {
                            json!({
                                "index": index,
                                "kind": frame.kind,
                                "offset": frame.offset,
                                "length": frame.length,
                                "observed_at_unix_ms": frame.observed_at_unix_ms,
                                "sha256": frame.sha256,
                            })
                        })
                        .collect();
                    let outcome = json!({
                        "classification": if local_processing_failed {
                            "local_processing_failed"
                        } else {
                            captured.outcome.as_str()
                        },
                        "stage": outcome_stage,
                        "complete": capture_complete,
                        "body_truncated": captured.body_truncated,
                        "chunk_index_truncated": captured.chunk_index_truncated,
                        "frame_index_truncated": captured.frame_index_truncated,
                        "normalized_tool_call_abort": captured.normalized_tool_call_abort,
                        "failure_stage": effective_failure_stage,
                        "failure_message": effective_failure_message,
                        "transport_failure_stage": transport_failure_stage,
                        "transport_failure_message": transport_failure_message,
                        "processing_failure_stage": processing_failure_stage.clone(),
                        "processing_failure_message": processing_failure_message.clone(),
                        "received_bytes": captured.received_bytes,
                        "retained_bytes": retained_bytes,
                        "chunk_count": captured.chunks.len(),
                        "frame_count": captured.frames.len(),
                        "body_sha256": captured.body_sha256,
                    });
                    (
                        json!({
                            "capture_point": "before_local_response_framing_and_protocol_decode",
                            "capture_state": "captured",
                            "transport": captured.transport,
                            "http_status": captured.http_status,
                            "content_type": captured.content_type,
                            "content_encoding": captured.content_encoding,
                            "received_bytes": captured.received_bytes,
                            "retained_bytes": retained_bytes,
                        }),
                        captured.body,
                        chunks,
                        frames,
                        outcome,
                    )
                },
            );
        let group = json!({
            "schema_version": PROVIDER_API_TRACE_SCHEMA_VERSION,
            "created_at_unix_ms": identity.group_created_at_unix_ms,
            "session_id": self.cache_scope_id,
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
        });
        let request = json!({
            "schema_version": PROVIDER_API_TRACE_SCHEMA_VERSION,
            "captured_at_unix_ms": captured_at_unix_ms,
            "execution_id": execution_id,
            "logical_request_id": identity.group_id.as_str(),
            "retry_index": identity.attempt_index,
            "turn_anchor_message_id": turn_anchor_message_id,
            "capture_point": "transport_dispatch",
            "transport": captured_request.transport,
            "method": captured_request.method,
            "path": captured_request.path,
            "resolved_url": captured_request.resolved_url,
            "headers": headers,
            "credential_values_redacted": true,
            "body": captured_request.body,
        });
        let session_directory = self.session_storage_directory.clone().ok_or_else(|| {
            provider_failure(
                self.provider_id(),
                "Provider request tracing is enabled but the execution has no exact session storage directory.",
            )
        })?;
        let trace_group_id = identity.group_id.clone();
        let trace_attempt_index = identity.attempt_index;
        let trace_path = tokio::task::spawn_blocking(move || {
            vulcan_workspace_store::workspace_store::persist_provider_api_trace_attempt(
                &session_directory,
                session_id,
                slot_index,
                vulcan_workspace_store::workspace_store::ProviderApiTraceAttemptArtifacts {
                    group_sequence: identity.group_sequence,
                    group_id: identity.group_id.as_str(),
                    attempt_index: identity.attempt_index,
                    group: &group,
                    request: &request,
                    response_meta: &response_meta,
                    response_body: response_body.as_slice(),
                    chunks: chunks.as_slice(),
                    frames: frames.as_slice(),
                    outcome: &outcome,
                },
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
            "[provider-api-trace] session={} group={} attempt={} execution={} path={}",
            self.cache_scope_id,
            trace_group_id,
            trace_attempt_index,
            execution_id,
            trace_path.display()
        );
        Ok(())
    }
}

/// Maps one configured provider contract onto the shared tool-schema projection implementation.
/// 将一项已配置 Provider 契约映射到共享工具 Schema 投影实现。
///
/// # Parameters
/// 参数
/// `mode` is the exact value frozen into the execution route.
/// `mode` 是冻结进执行路由的精确值。
///
/// # Returns
/// 返回值
/// The corresponding shared projection profile without protocol or model-name inference.
/// 返回对应共享投影配置，不进行协议或模型名推断。
fn configured_tool_schema_projection_profile(
    mode: ToolSchemaProjectionMode,
) -> VulcanToolSchemaProjectionProfile {
    match mode {
        ToolSchemaProjectionMode::Lightweight => {
            VulcanToolSchemaProjectionProfile::StandardObjectRoot
        }
        ToolSchemaProjectionMode::Passthrough => VulcanToolSchemaProjectionProfile::PassThrough,
        ToolSchemaProjectionMode::Strict => VulcanToolSchemaProjectionProfile::GeminiSlim,
    }
}

/// Projects host-owned provider tools through the exact route-bound compatibility profile.
/// 通过精确路由绑定的兼容配置投影宿主持有的 Provider 工具。
///
/// # Parameters
/// 参数
/// `route` freezes the configured schema contract; `tools` contains the already resolved
/// provider-visible definitions.
/// `route` 冻结已配置的 Schema 契约；`tools` 包含已经解析的 Provider 可见定义。
///
/// # Returns
/// 返回值
/// Route-normalized public tool definitions suitable for ordinary responses and remote compaction.
/// 返回适用于普通 Responses 与远端压缩的路由正规化公共工具定义。
pub(crate) fn project_public_tool_definitions(
    route: &ProviderExecutionRoute,
    tools: Vec<ProviderToolDefinition>,
) -> Vec<ToolDefinition> {
    // schemaProjectionProfile maps only the route-frozen configuration; model names and protocol
    // families never silently change the schema contract.
    // schemaProjectionProfile 只映射路由冻结配置；模型名与协议族绝不会静默改变 Schema 契约。
    let schema_projection_profile =
        configured_tool_schema_projection_profile(route.tool_schema_projection);
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
            let public_message = core_message_to_public(
                message,
                self.provider_id(),
                self.route.connection.id.as_str(),
                self.route.request_url("/responses").as_str(),
                self.route.protocol_route.protocol_profile_id.as_str(),
                self.model_id(),
            )?;
            messages.push(public_message);
        }
        // Newer canonical messages reserve the bounded inline budget first while final wire order stays unchanged.
        // 较新的规范消息优先占用有界内联预算，同时最终 Wire 顺序保持不变。
        let mut inline_resource_budget = InlineResourceProjectionBudget::default();
        for public_message in messages.iter_mut().rev() {
            resolve_public_message_resources(
                public_message,
                &self.route,
                self.model_id.as_str(),
                self.input_resources.as_slice(),
                &self.client,
                Some(self.cache_scope_id.as_str()),
                &mut inline_resource_budget,
            )
            .await?;
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

/// Classifies the host-side accumulator boundary responsible for one normalized Provider event.
/// 对一项规范化 Provider 事件负责的宿主侧累积边界进行分类。
///
/// # Parameters
/// 参数
/// `event` is inspected before ownership moves into the public turn accumulator.
/// `event` 在所有权移入公共回合累积器之前接受检查。
///
/// # Returns
/// 返回值
/// The stable trace stage used when applying this event fails.
/// 返回应用该事件失败时使用的稳定追踪阶段。
fn provider_trace_processing_stage_for_event(event: &AiStreamEvent) -> &'static str {
    if matches!(
        event,
        AiStreamEvent::ToolCallStarted { .. }
            | AiStreamEvent::ToolCallArgumentsDelta { .. }
            | AiStreamEvent::ToolCallCompleted { .. }
            | AiStreamEvent::ToolCallAborted { .. }
    ) {
        "tool_request_coordinator"
    } else {
        "response_projection"
    }
}

/// Drains a failed physical Provider stream only to complete opted-in response diagnostics.
/// 仅为完成显式启用的响应诊断而排空失败的真实 Provider 流。
///
/// # Parameters
/// 参数
/// `stream` is the remaining normalized stream whose underlying transport still owns uncaptured bytes.
/// `stream` 是剩余的规范化流，其底层传输仍可能持有尚未捕获的字节。
fn drain_provider_stream_for_trace(mut stream: ProviderExecutionStream) {
    // traceDrain deliberately discards semantic events: failed turns must never resume projection or tools.
    // traceDrain 会刻意丢弃语义事件：失败回合绝不能恢复事件投影或工具执行。
    tokio::spawn(async move { while stream.next().await.is_some() {} });
}

/// Refreshes one exact renewable credential after a replay-safe upstream 401 response.
/// 在收到可安全重放的上游 401 响应后刷新一份精确可续期凭据。
///
/// # Parameters
/// 参数
///
/// `credential` owns the frozen account reference, `rejected_revision` identifies the token
/// used by this physical request, and `error` is the normalized upstream failure.
/// `credential` 持有冻结账号引用，`rejected_revision` 标识当前真实请求使用的 Token，
/// `error` 是归一化上游失败。
async fn recover_rejected_oauth_credential(
    credential: &ProviderExecutionCredentialLease,
    rejected_revision: u64,
    error: &mut AiExecutionError,
) {
    if error.http_status != Some(401)
        || error.semantic_output_observed
        || error.side_effect_observed
    {
        return;
    }
    match credential
        .refresh_after_unauthorized(rejected_revision)
        .await
    {
        Ok(true) => {
            error.subcategory = Some(String::from(OAUTH_CREDENTIAL_REFRESHED_SUBCATEGORY));
            error.retryable = true;
        }
        Ok(false) => {}
        Err(refresh_error) => {
            error.retryable = false;
            error.message = format!(
                "{} Host OAuth credential refresh failed after the upstream 401 response: {refresh_error}",
                error.message
            );
        }
    }
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
        let VulcanPublicContractRequest {
            public,
            controller_config,
            controller_snapshot,
            budget,
            context_budget,
        } = request;
        let trace_turn_anchor_message_id = controller_snapshot
            .messages
            .last()
            .map(|message| message.id.clone());
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
        self.live_sender
            .send(Ok(
                ProviderStreamDelta::RequestStartDurabilityConfirmed,
            ))
            .map_err(|_| {
                provider_failure(
                    self.provider_id(),
                    "The provider request stream closed after start persistence and before dispatch.",
                )
            })?;
        // secret is re-read after the durable start boundary and immediately before this physical dispatch.
        // secret 在可靠起始边界之后、当前真实分派之前立即重新读取。
        let secret = self
            .credential
            .resolve_for_request()
            .await
            .map_err(|error| {
                provider_failure(
                    self.provider_id(),
                    &format!(
                        "The Provider credential could not be resolved before dispatch: {error}"
                    ),
                )
            })?;
        // rejectedSecretRevision binds any later 401 recovery to the exact token used on the wire.
        // rejectedSecretRevision 将后续任何 401 恢复绑定到线上实际使用的精确 Token 修订版。
        let rejected_secret_revision = secret.revision();
        let execution_id = uuid::Uuid::new_v4().to_string();
        let trace_attempt_identity = match self.claim_provider_api_trace_attempt() {
            Ok(identity) => identity,
            Err(error) => {
                eprintln!(
                    "[provider-api-trace] session={} group_claim_failed={error}",
                    self.cache_scope_id
                );
                None
            }
        };
        // wireCaptures pair the exact dispatched request with bytes observed before local response parsing.
        // wireCaptures 将实际分派请求与本地响应解析前观察到的字节成对关联。
        let wire_captures = if let Some(trace_identity) = trace_attempt_identity.as_ref() {
            let (request_sender, request_receiver) = tokio::sync::oneshot::channel();
            let (response_sender, response_receiver) = tokio::sync::oneshot::channel();
            let (processing_failure_sender, processing_failure_receiver) =
                tokio::sync::oneshot::channel();
            let trace_adapter = self.clone();
            let trace_execution_id = execution_id.clone();
            let trace_identity = trace_identity.clone();
            let trace_turn_anchor_message_id = trace_turn_anchor_message_id.clone();
            tokio::spawn(async move {
                let Ok(ProviderWireRequestTraceDispatch {
                    request: captured_request,
                    persistence_result,
                }) = request_receiver.await
                else {
                    // A closed capture channel means encoding failed before an API request existed.
                    // 捕获通道关闭表示编码在 API 请求形成前失败，因此没有请求可保存。
                    return;
                };
                if let Err(error) = trace_adapter
                    .persist_provider_api_trace_snapshot(
                        trace_identity.clone(),
                        trace_execution_id.as_str(),
                        trace_turn_anchor_message_id.as_deref(),
                        captured_request.clone(),
                        ProviderApiTraceResponseState::AwaitingResponse,
                        None,
                    )
                    .await
                {
                    let message = error.to_string();
                    let _ = persistence_result.send(Err(message.clone()));
                    eprintln!(
                        "[provider-api-trace] session={} execution={} request_persist_failed={message}",
                        trace_adapter.cache_scope_id, trace_execution_id
                    );
                    return;
                }
                if persistence_result.send(Ok(())).is_err() {
                    return;
                }
                // A closed response channel remains explicit in the trace instead of discarding the request.
                // 响应通道关闭会在追踪中显式记录，而不是丢弃请求。
                let response_state = response_receiver.await.map_or(
                    ProviderApiTraceResponseState::CaptureChannelClosed,
                    ProviderApiTraceResponseState::Captured,
                );
                // processingFailure closes only after the public accumulator has accepted or rejected the captured response.
                // processingFailure 仅在公共累积器接受或拒绝已捕获响应后关闭。
                let processing_failure = processing_failure_receiver.await.ok();
                if let Err(error) = trace_adapter
                    .persist_provider_api_trace_snapshot(
                        trace_identity,
                        trace_execution_id.as_str(),
                        trace_turn_anchor_message_id.as_deref(),
                        captured_request,
                        response_state,
                        processing_failure,
                    )
                    .await
                {
                    eprintln!(
                        "[provider-api-trace] session={} execution={} failed={error}",
                        trace_adapter.cache_scope_id, trace_execution_id
                    );
                }
            });
            Some((request_sender, response_sender, processing_failure_sender))
        } else {
            None
        };
        let (mut stream, processing_failure_sender) =
            if let Some((request_sender, response_sender, processing_failure_sender)) =
                wire_captures
            {
                (
                    self.client.execute_with_wire_api_trace_capture(
                        self.route.clone(),
                        public.clone(),
                        secret.clone(),
                        execution_id,
                        request_sender,
                        response_sender,
                    ),
                    Some(processing_failure_sender),
                )
            } else {
                (
                    self.client
                        .execute(self.route.clone(), public.clone(), secret, execution_id),
                    None,
                )
            };
        let mut trace_processing_failure =
            ProviderApiTraceProcessingFailureSender::new(processing_failure_sender);
        // routeFingerprint binds replay signatures to the exact resolved endpoint used by this request.
        // routeFingerprint 将重放签名绑定到本次请求使用的精确已解析端点。
        let route_fingerprint = self.route.request_url("/responses");
        let transient_reasoning_kind = transient_reasoning_status_kind(
            self.provider_id(),
            self.route.response_projection_profiles.as_slice(),
        )?;
        let mut response = PublicTurnAccumulator::new(
            self.provider_id(),
            self.route.connection.id.as_str(),
            route_fingerprint.as_str(),
            self.route.protocol_route.protocol_profile_id.as_str(),
            self.model_id(),
            transient_reasoning_kind,
        );
        while let Some(item) = stream.next().await {
            // event preserves the normalized provider item after credential recovery annotates failures.
            // event 在凭据恢复标注失败后保留归一化供应商事件。
            let event = match item {
                Ok(event) => event,
                Err(mut failure) => {
                    attach_execution_route_context(
                        &mut failure.error,
                        self.provider_id(),
                        self.route.entry.id.as_str(),
                        self.model_id(),
                    );
                    recover_rejected_oauth_credential(
                        &self.credential,
                        rejected_secret_revision,
                        &mut failure.error,
                    )
                    .await;
                    return Err(public_execution_failure(self.provider_id(), *failure.error));
                }
            };
            let mut event = match materialize_provider_resource_event(
                self.provider_id(),
                self.workspace_path.as_str(),
                event,
            )
            .await
            {
                Ok(event) => event,
                Err(error) => {
                    trace_processing_failure.observe_failure("response_projection", &error);
                    return Err(error);
                }
            };
            if let AiStreamEvent::ResponseFailed { error } = &mut event {
                attach_execution_route_context(
                    error,
                    self.provider_id(),
                    self.route.entry.id.as_str(),
                    self.model_id(),
                );
                recover_rejected_oauth_credential(
                    &self.credential,
                    rejected_secret_revision,
                    error,
                )
                .await;
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
            let processing_stage = provider_trace_processing_stage_for_event(&event);
            // applyResult is separated from `?` so trace mode can retain the rest of a failed API body.
            // applyResult 与 `?` 分离，使追踪模式可以保留失败 API 正文的剩余部分。
            let apply_result =
                response.apply_event(self.provider_id(), &self.live_sender, sink, event);
            if let Err(error) = apply_result {
                trace_processing_failure.observe_failure(processing_stage, &error);
                if self.provider_request_trace_enabled {
                    drain_provider_stream_for_trace(stream);
                }
                return Err(error);
            }
            if usage_updated && let Some(usage) = response.usage.as_ref() {
                let _ = self.live_sender.send(Ok(ProviderStreamDelta::UsageUpdated(
                    public_usage_to_core(usage),
                )));
            }
            if terminal_event {
                if self.provider_request_trace_enabled {
                    // terminalTraceDrain preserves transport bytes that follow the semantic terminal event
                    // without delaying tool execution or projecting any later semantic event.
                    // terminalTraceDrain 会保留语义终态事件之后的传输字节，且不会延迟工具执行或投影任何
                    // 后续语义事件。
                    drain_provider_stream_for_trace(stream);
                }
                break;
            }
        }
        let final_usage = response.usage.as_ref().map(public_usage_to_core);
        let finish_stage = if response.has_pending_tool_request() {
            "tool_request_coordinator"
        } else {
            "response_projection"
        };
        let final_response = match response.finish() {
            Ok(response) => response,
            Err(error) => {
                trace_processing_failure.observe_failure(finish_stage, &error);
                return Err(error);
            }
        };
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
        if let Some(identity) = trace_attempt_identity.as_ref() {
            self.complete_provider_api_trace_group(identity);
        }
        self.credential.mark_request_succeeded();
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
    /// Replace-in-place reasoning status excluded from every durable response field.
    /// 排除在全部持久化响应字段之外的原地替换式推理状态。
    transient_reasoning_status: Option<TransientReasoningStatus>,
    /// Whether non-empty durable reasoning has superseded every transient fallback.
    /// 非空持久化思考是否已经取代全部瞬态回退状态。
    durable_reasoning_observed: bool,
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
    PendingTool {
        /// Provider-visible call identifier required to pair a controlled rejection result.
        /// 用于配对受控拒绝结果的 Provider 可见调用标识。
        call_id: String,
        /// Exact tool name selected by the model before argument parsing completed.
        /// 参数解析完成前模型选定的精确工具名称。
        tool_name: String,
    },
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
        transient_reasoning_kind: Option<TransientReasoningStatusKind>,
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
            durable_reasoning_observed: false,
            transient_reasoning_status: transient_reasoning_kind.map(|kind| {
                TransientReasoningStatus {
                    kind,
                    summaries: Vec::new(),
                    reasoning_tokens: None,
                }
            }),
        }
    }

    /// Emits the latest transient reasoning status without touching semantic state.
    /// 发送最新瞬态推理状态，且不触碰任何语义状态。
    ///
    /// # Parameters
    /// 参数
    /// `live_sender` is the request-local WebUI delta channel.
    /// `live_sender` 是当前请求局部的 WebUI 增量通道。
    fn emit_transient_reasoning_status(&self, live_sender: &ProviderLiveDeltaSender) {
        let _ = live_sender.send(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(
            self.transient_reasoning_status.clone(),
        )));
    }

    /// Clears the transient reasoning collection without recording a transcript delta.
    /// 清除瞬态推理状态集合，且不记录转录增量。
    ///
    /// # Parameters
    /// 参数
    /// `live_sender` is the request-local WebUI delta channel.
    /// `live_sender` 是当前请求局部的 WebUI 增量通道。
    fn clear_transient_reasoning_status(&mut self, live_sender: &ProviderLiveDeltaSender) {
        if self.transient_reasoning_status.take().is_some() {
            let _ = live_sender.send(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(
                None,
            )));
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
                ReplayAccumulatorItemState::PendingTool { .. } => Err(provider_failure(
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
                if !delta.is_empty() && !self.durable_reasoning_observed {
                    // Full readable reasoning is authoritative and removes every transient fallback.
                    // 完整可读思考具有权威性，并会移除全部瞬态回退状态。
                    self.durable_reasoning_observed = true;
                    self.clear_transient_reasoning_status(live_sender);
                }
                // liveDelta projects provider reasoning without removing it from Core's canonical response.
                // liveDelta 投影 Provider 推理内容，同时不从 Core 规范响应中移除该内容。
                let live_delta = delta.clone();
                sink.push_delta(ProviderTurnDelta::Thinking { delta });
                let _ = live_sender.send(Ok(ProviderStreamDelta::Thinking(live_delta)));
            }
            AiStreamEvent::ReasoningSummaryUpdated { item_id, text } => {
                if self.durable_reasoning_observed {
                    return Ok(());
                }
                let reasoning_tokens = self
                    .transient_reasoning_status
                    .as_ref()
                    .and_then(|status| status.reasoning_tokens);
                let status = self.transient_reasoning_status.get_or_insert_with(|| {
                    TransientReasoningStatus {
                        kind: TransientReasoningStatusKind::Summary,
                        summaries: Vec::new(),
                        reasoning_tokens,
                    }
                });
                status.kind = TransientReasoningStatusKind::Summary;
                if let Some(summary) = status
                    .summaries
                    .iter_mut()
                    .find(|summary| summary.item_id == item_id)
                {
                    summary.text = text;
                } else {
                    status
                        .summaries
                        .push(TransientReasoningSummaryItem { item_id, text });
                }
                self.emit_transient_reasoning_status(live_sender);
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
                    ReplayAccumulatorItemState::PendingTool {
                        call_id: call_id.clone(),
                        tool_name: tool_name.clone(),
                    },
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
                if !matches!(replay_state, ReplayAccumulatorItemState::PendingTool { .. }) {
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
                    argument_parse_error: None,
                });
            }
            AiStreamEvent::ToolCallAborted { item_id, message } => {
                // rejectedIdentity is retained from ToolCallStarted because a model-visible tool result requires the original call id and name.
                // rejectedIdentity 来自 ToolCallStarted，因为模型可见工具结果必须使用原始调用标识与名称。
                let (call_id, tool_name) = {
                    let replay_state = self.replay_item_state_mut(item_id.as_str())?;
                    let ReplayAccumulatorItemState::PendingTool { call_id, tool_name } =
                        replay_state
                    else {
                        return Err(provider_failure(
                            provider_id,
                            "Public tool abort targeted a non-pending tool item.",
                        ));
                    };
                    (call_id.clone(), tool_name.clone())
                };
                // rejectedCall closes canonical replay with valid JSON while the separate parse diagnostic forbids host execution.
                // rejectedCall 使用有效 JSON 封闭规范重放，同时由独立解析诊断禁止宿主执行。
                let rejected_call = PublicToolCall {
                    id: call_id.clone(),
                    name: tool_name.clone(),
                    arguments: Value::Null,
                    signature: None,
                };
                *self.replay_item_state_mut(item_id.as_str())? =
                    ReplayAccumulatorItemState::Complete(ContentItem::ToolCall {
                        call: rejected_call,
                    });
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::Invalid {
                        request_stream_id: item_id,
                        reason: message.clone(),
                    },
                )));
                self.tool_calls.push(ConversationToolCall {
                    id: call_id.clone(),
                    tool_call_id: Some(call_id),
                    tool_name,
                    arguments: Value::Null,
                    argument_parse_error: Some(message),
                });
            }
            AiStreamEvent::UsageUpdated { usage } => {
                if let Some(current) = self.usage.as_mut() {
                    current.merge_update(*usage);
                } else {
                    self.usage = Some(*usage);
                }
                if let Some(reasoning_tokens) = self
                    .usage
                    .as_ref()
                    .and_then(|usage| known_usage_u64(&usage.reasoning_tokens))
                    && let Some(status) = self.transient_reasoning_status.as_mut()
                    && status.reasoning_tokens != Some(reasoning_tokens)
                {
                    status.reasoning_tokens = Some(reasoning_tokens);
                    self.emit_transient_reasoning_status(live_sender);
                }
            }
            AiStreamEvent::ResponseCompleted => {
                self.clear_transient_reasoning_status(live_sender);
                self.completed = true;
            }
            AiStreamEvent::ResponseFailed { error } => {
                self.clear_transient_reasoning_status(live_sender);
                return Err(public_execution_failure(provider_id, error));
            }
            AiStreamEvent::ResponseCancelled => {
                self.clear_transient_reasoning_status(live_sender);
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
            AiStreamEvent::ResponseStarted { .. } => {
                if self.transient_reasoning_status.is_some() {
                    self.emit_transient_reasoning_status(live_sender);
                }
            }
            AiStreamEvent::ResourceProgress { .. }
            | AiStreamEvent::ImageGenerationPartial { .. }
            | AiStreamEvent::ResponseItemCompleted { .. } => {}
        }
        Ok(())
    }

    /// Reports whether response completion still owns an unfinished tool request.
    /// 报告响应完成时是否仍持有未完成的工具请求。
    ///
    /// # Returns
    /// 返回值
    /// `true` when final response projection would fail at the tool request coordinator boundary.
    /// 当最终响应投影会在工具请求协调边界失败时返回 `true`。
    fn has_pending_tool_request(&self) -> bool {
        self.replay_items
            .iter()
            .any(|item| matches!(item.state, ReplayAccumulatorItemState::PendingTool { .. }))
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
    session_scope_id: Option<&str>,
    inline_resource_budget: &mut InlineResourceProjectionBudget,
) -> Result<(), VulcanCoreError> {
    for item in &mut message.content {
        let ContentItem::Resource { resource } = item else {
            continue;
        };
        if let Some(reason) = resource_projection_unavailability_reason(
            route,
            model_id,
            model_resource_kinds,
            resource,
        ) {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(resource, reason.as_str(), None),
            };
            continue;
        }
        let constraint = input_resource_constraint(route, resource.kind);
        let max_count = constraint.and_then(|constraint| constraint.max_count);
        if inline_resource_budget.count_limit_reached(resource.kind, max_count) {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "the frozen route request-wide resource-count limit",
                    None,
                ),
            };
            continue;
        }
        if resource.url.is_some() {
            // inlineRawByteLen closes the aggregate-budget gap for canonical data URLs that were
            // already materialized before this final transport pass.
            // inlineRawByteLen 为在最终传输阶段前已物化的规范 Data URL 补齐聚合预算边界。
            let inline_raw_byte_len = match inline_data_resource_raw_byte_len(resource) {
                Ok(byte_len) => byte_len,
                Err(reason) => {
                    *item = ContentItem::Text {
                        text: resource_projection_budget_fallback_text(resource, reason, None),
                    };
                    continue;
                }
            };
            if let (Some(byte_limit), Some(byte_len)) = (
                constraint.and_then(|constraint| constraint.max_bytes),
                inline_raw_byte_len,
            ) && byte_len > byte_limit
            {
                *item = ContentItem::Text {
                    text: resource_projection_budget_fallback_text(
                        resource,
                        "the frozen route byte limit",
                        Some(byte_len),
                    ),
                };
                continue;
            }
            if let Err(reason) =
                inline_resource_budget.reserve(resource.kind, max_count, inline_raw_byte_len)
            {
                *item = ContentItem::Text {
                    text: resource_projection_budget_fallback_text(
                        resource,
                        reason,
                        inline_raw_byte_len,
                    ),
                };
            }
            continue;
        }
        let Some(local_path) = resource.local_path.as_deref() else {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "the managed resource has neither a local path nor a remote reference",
                    None,
                ),
            };
            continue;
        };
        let Some(media_type) = resource.media_type.as_deref() else {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "the managed resource has no verified MIME type",
                    None,
                ),
            };
            continue;
        };
        if let Some(byte_limit) = constraint.and_then(|constraint| constraint.max_bytes) {
            let Ok(metadata) = tokio::fs::metadata(local_path).await else {
                *item = ContentItem::Text {
                    text: resource_projection_budget_fallback_text(
                        resource,
                        "the managed resource is no longer readable",
                        None,
                    ),
                };
                continue;
            };
            if metadata.len() > byte_limit {
                *item = ContentItem::Text {
                    text: resource_projection_budget_fallback_text(
                        resource,
                        "the frozen route byte limit",
                        Some(metadata.len()),
                    ),
                };
                continue;
            }
        }
        if let Some(prepared_reference) = client
            .prepared_media_reference(
                route,
                std::path::Path::new(local_path),
                resource.kind,
                media_type,
                session_scope_id,
                None,
            )
            .await
        {
            match inline_resource_budget.reserve(resource.kind, max_count, None) {
                Ok(()) => apply_prepared_media_reference(resource, prepared_reference),
                Err(reason) => {
                    *item = ContentItem::Text {
                        text: resource_projection_budget_fallback_text(resource, reason, None),
                    };
                }
            }
            continue;
        }
        let remote_reference_required = constraint.is_some_and(|constraint| {
            constraint.reference_policy == ResourceReferencePolicy::RemoteOnly
        });
        // Native video and every remote-only resource must have a prepared provider reference before projection.
        // 原生视频与所有仅远程资源在协议投影前都必须拥有已准备的供应商引用。
        if resource.kind == ResourceKind::Video || remote_reference_required {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "a required Provider reference was unavailable",
                    None,
                ),
            };
            continue;
        }
        let Ok(bytes) = tokio::fs::read(local_path).await else {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "the managed resource is no longer readable",
                    None,
                ),
            };
            continue;
        };
        if let Some(byte_limit) = constraint.and_then(|constraint| constraint.max_bytes)
            && u64::try_from(bytes.len()).unwrap_or(u64::MAX) > byte_limit
        {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    "the resource changed and exceeded the frozen route byte limit",
                    Some(u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
                ),
            };
            continue;
        }
        let raw_byte_len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if let Err(reason) =
            inline_resource_budget.reserve(resource.kind, max_count, Some(raw_byte_len))
        {
            *item = ContentItem::Text {
                text: resource_projection_budget_fallback_text(
                    resource,
                    reason,
                    Some(raw_byte_len),
                ),
            };
            continue;
        }
        resource.url = Some(format!(
            "data:{media_type};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ));
    }
    Ok(())
}

/// Applies one cached provider reference according to its frozen Manifest encoding.
/// 根据冻结的 Manifest 编码应用一条已缓存供应商引用。
///
/// # Parameters
/// 参数
/// `resource` is the request-local resource projection and `prepared` carries the exact reference
/// string plus its closed upload-channel encoding.
/// `resource` 是单次请求局部资源投影；`prepared` 携带精确引用字符串及其封闭上传通道编码。
///
/// # Returns
/// 返回值
/// This function has no return value and rewrites only the request-local resource source fields.
/// 本函数无返回值，且只改写单次请求局部资源的来源字段。
fn apply_prepared_media_reference(
    resource: &mut ResourceReference,
    prepared: vulcan_provider_adapters::PreparedMediaReferenceSnapshot,
) {
    match prepared.reference_encoding {
        UploadReferenceEncoding::ProviderFileId => {
            resource.id = prepared.provider_reference;
            resource.source_kind = ResourceSourceKind::ProviderFileId;
            resource.url = None;
        }
        UploadReferenceEncoding::ResponseUrl | UploadReferenceEncoding::Template => {
            resource.source_kind = ResourceSourceKind::RemoteUrl;
            resource.url = Some(prepared.provider_reference);
        }
    }
}

/// Decodes the exact raw byte length of an already materialized inline Data URL.
/// 解码一个已物化内联 Data URL 的精确原始字节长度。
///
/// # Parameters
/// 参数
/// `resource` is the request-local resource whose source-kind contract controls interpretation.
/// `resource` 是单次请求局部资源，其来源类型契约决定解释方式。
///
/// # Returns
/// 返回值
/// `Some` raw bytes for inline data, `None` for non-inline URLs, or a controlled malformed-data reason.
/// 内联数据返回 `Some` 原始字节数，非内联 URL 返回 `None`，畸形数据返回受控原因。
fn inline_data_resource_raw_byte_len(
    resource: &ResourceReference,
) -> Result<Option<u64>, &'static str> {
    if resource.source_kind != ResourceSourceKind::InlineData {
        return Ok(None);
    }
    let encoded = resource
        .url
        .as_deref()
        .and_then(|value| value.strip_prefix("data:"))
        .and_then(|value| value.split_once(";base64,"))
        .map(|(_, encoded)| encoded)
        .ok_or("the inline resource is not a canonical Base64 Data URL")?;
    if u64::try_from(encoded.len()).unwrap_or(u64::MAX) > HOST_INLINE_RESOURCE_BASE64_BUDGET_BYTES {
        return Err("the request-wide host inline-media budget");
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "the inline resource contains invalid Base64 data")?;
    Ok(Some(u64::try_from(decoded.len()).unwrap_or(u64::MAX)))
}

/// Builds an AI-safe metadata-only replacement for one request-local resource projection.
/// 为单次请求局部资源投影构建对 AI 安全的纯元数据替代文本。
///
/// # Parameters
/// 参数
/// `resource` supplies only provider-neutral identity and MIME facts, `reason` names the exact
/// host boundary, and `byte_len` carries the verified local size when available.
/// `resource` 只提供供应商无关身份与 MIME 事实，`reason` 命名精确宿主边界，`byte_len`
/// 在可用时携带已验证本地大小。
///
/// # Returns
/// 返回值
/// A deterministic line that never exposes the managed absolute path or Provider reference.
/// 返回一条确定性文本，绝不暴露托管绝对路径或供应商引用。
fn resource_projection_budget_fallback_text(
    resource: &ResourceReference,
    reason: &str,
    byte_len: Option<u64>,
) -> String {
    // resourceLocator must not derive a model-visible locator from a Provider file id. A missing
    // canonical locator is represented explicitly instead of echoing transport-only identity.
    // resourceLocator 绝不能从供应商文件标识派生模型可见定位符；缺失规范定位符时显式标记不可用，
    // 而不是回显仅用于传输的身份。
    let resource_locator = resource
        .resource_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if resource.source_kind == ResourceSourceKind::ProviderFileId {
                String::from("attachment://unavailable")
            } else {
                format!("attachment://{}", resource.id)
            }
        });
    // provider file ids are transport-only secrets; a degraded model-visible line falls back to
    // the disclosed canonical locator instead of echoing the internal Provider reference.
    // 供应商文件标识仅用于传输；降级后的模型可见文本改用已公开的规范定位符，绝不回显内部供应商引用。
    let model_resource_id = if resource.source_kind == ResourceSourceKind::ProviderFileId {
        resource
            .resource_path
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("unavailable")
    } else {
        resource.id.as_str()
    };
    let resource_id = model_visible_attachment_field(model_resource_id);
    let resource_locator = model_visible_attachment_field(&resource_locator);
    let file_name =
        model_visible_attachment_field(resource.file_name.as_deref().unwrap_or("unavailable"));
    let media_type = model_visible_attachment_field(
        resource
            .media_type
            .as_deref()
            .unwrap_or("application/octet-stream"),
    );
    let reason = model_visible_attachment_field(reason);
    let byte_len = byte_len
        .map(|value| value.to_string())
        .unwrap_or_else(|| String::from("unknown"));
    format!(
        "ATTACHMENT: id={resource_id}; resource_path={resource_locator}; file_name={file_name}; mime_type={media_type}; byte_len={byte_len}. NATIVE_DELIVERY: metadata_only (reason={reason}). CONTENT_VISIBILITY: resource bytes were not provided; do not infer or guess the attachment contents. Use the disclosed resource locator with an eligible registered understanding tool only when required by the user's actual intent.",
    )
}

/// Explains why one resource cannot use native projection on the selected route.
/// 说明一个资源为何无法在所选路由上使用原生投影。
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
/// Returns the exact Provider-neutral degradation reason, or `None` when native projection is eligible.
/// 返回精确且供应商无关的降级原因；可使用原生投影时返回 `None`。
fn resource_projection_unavailability_reason(
    route: &ProviderExecutionRoute,
    model_id: &str,
    model_resource_kinds: &[ResourceKind],
    resource: &ResourceReference,
) -> Option<String> {
    if !model_resource_kinds.contains(&resource.kind) {
        return Some(format!(
            "model `{model_id}` does not declare {:?} input",
            resource.kind
        ));
    }
    let protocol = route.protocol_route.protocol_profile_id.as_str();
    let protocol_supports = protocol_profile_supports_resource_kind(protocol, resource.kind);
    if !protocol_supports {
        return Some(format!(
            "protocol profile `{protocol}` has no registered {:?} input projector",
            resource.kind
        ));
    }
    if let Some(media_type) = resource.media_type.as_deref()
        && !vulcan_ai_protocols::protocol_profile_supports_local_media_type(
            protocol,
            resource.kind,
            media_type,
        )
    {
        return Some(format!(
            "protocol profile `{protocol}` has no registered {:?} projector for MIME type `{media_type}`",
            resource.kind
        ));
    }
    if let Some(constraint) = input_resource_constraint(route, resource.kind) {
        if let Some(reason) = frozen_resource_constraint_unavailability_reason(constraint, resource)
        {
            return Some(reason);
        }
    }
    None
}

/// Reports whether one protocol profile has any native projector for a resource modality.
/// 报告一个协议 Profile 是否拥有某种资源模态的原生投影器。
///
/// # Parameters
/// 参数
/// `protocol` is the frozen protocol profile and `kind` is the canonical resource modality.
/// `protocol` 是冻结协议 Profile，`kind` 是规范资源模态。
///
/// # Returns
/// 返回值
/// `true` only for modality/profile pairs implemented by the protocol registry; generic files
/// always remain metadata-only until a concrete file projector is registered.
/// 仅协议注册表已实现的模态/Profile 组合返回 `true`；通用文件在注册具体投影器前始终保持纯元数据。
fn protocol_profile_supports_resource_kind(protocol: &str, kind: ResourceKind) -> bool {
    match protocol {
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => matches!(
            kind,
            ResourceKind::Image | ResourceKind::Video | ResourceKind::Audio
        ),
        vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
        | vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID => kind == ResourceKind::Image,
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
        | vulcan_ai_protocols::CLAUDE_CODE_MESSAGES_PROFILE_ID => kind == ResourceKind::Image,
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => {
            matches!(kind, ResourceKind::Image | ResourceKind::Video)
        }
        vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID
        | vulcan_ai_protocols::GEMINI_INTERACTIONS_PROFILE_ID
        | vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID => matches!(
            kind,
            ResourceKind::Image | ResourceKind::Video | ResourceKind::Audio
        ),
        _ => false,
    }
}

/// Explains one frozen route-constraint mismatch without dispatching or mutating canonical state.
/// 说明一项冻结路由约束不匹配的原因，不进行分派，也不修改规范状态。
///
/// # Parameters
/// 参数
/// `constraint` is the immutable route declaration and `resource` is the request-local projection candidate.
/// `constraint` 是不可变路由声明，`resource` 是单次请求局部投影候选。
///
/// # Returns
/// 返回值
/// A deterministic metadata-only reason, or `None` when the frozen constraint admits the candidate.
/// 返回确定性的纯元数据降级原因；冻结约束允许候选资源时返回 `None`。
fn frozen_resource_constraint_unavailability_reason(
    constraint: &InputResourceConstraint,
    resource: &ResourceReference,
) -> Option<String> {
    if !constraint.accepted_mime_types.is_empty()
        && !resource.media_type.as_ref().is_some_and(|media_type| {
            constraint
                .accepted_mime_types
                .iter()
                .any(|accepted| accepted.eq_ignore_ascii_case(media_type))
        })
    {
        return Some(format!(
            "the frozen {:?} input constraint does not accept this MIME type",
            resource.kind
        ));
    }
    let source_admitted = match constraint.reference_policy {
        ResourceReferencePolicy::Any => true,
        ResourceReferencePolicy::LocalOnly => {
            resource.local_path.is_some()
                && (resource.url.is_none()
                    || resource.source_kind == ResourceSourceKind::HostResolved)
        }
        ResourceReferencePolicy::RemoteOnly => {
            (resource.url.is_some() && resource.local_path.is_none())
                || (resource.local_path.is_some() && !constraint.upload_channel_refs.is_empty())
        }
    };
    (!source_admitted).then(|| {
        format!(
            "the resource source violates the frozen {:?} reference policy",
            constraint.reference_policy
        )
    })
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
    session_scope_id: Option<&str>,
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
        let public_message = core_message_to_public(
            message,
            resolved.route.provider.id.as_str(),
            resolved.route.connection.id.as_str(),
            route_fingerprint.as_str(),
            resolved.route.protocol_route.protocol_profile_id.as_str(),
            resolved.model.id.as_str(),
        )
        .map_err(|error| error.to_string())?;
        messages.push(public_message);
    }
    if projected_ids != source_ids {
        return Err(String::from(
            "Remote Responses compaction source no longer matches the canonical conversation.",
        ));
    }
    // Newer compacted messages reserve inline bytes before older history to preserve current-turn relevance.
    // 较新的压缩消息先于旧历史保留内联字节，以保持当前回合相关性。
    let mut inline_resource_budget = InlineResourceProjectionBudget::default();
    for public_message in messages.iter_mut().rev() {
        resolve_public_message_resources(
            public_message,
            &resolved.route,
            resolved.model.id.as_str(),
            resolved.model.capabilities.input_resources.as_slice(),
            &resolved.client,
            session_scope_id,
            &mut inline_resource_budget,
        )
        .await
        .map_err(|error| error.to_string())?;
    }
    messages.retain(|message| {
        !message.content.is_empty() || !message.provider_context_items.is_empty()
    });
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
                // replay_path is the only host path allowed to reach provider projection after index-backed integrity verification.
                // replay_path 是经过索引完整性校验后唯一允许进入 Provider 投影的宿主路径。
                let Some(replay_path) = resolve_attachment_replay_path(attachment) else {
                    content.push(ContentItem::Text {
                        text: build_missing_attachment_replay_text(attachment),
                    });
                    continue;
                };
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
                        file_name: Some(attachment.file_name.clone()),
                        image_detail: None,
                        resource_path: attachment.resource_path.clone(),
                        local_path: Some(replay_path.to_string_lossy().into_owned()),
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

/// Returns one known public usage value without platform-size truncation.
/// 返回一个已知公共用量值，且不发生平台尺寸截断。
///
/// # Parameters
/// 参数
/// `value` is one normalized usage dimension.
/// `value` 是一个规范化用量维度。
///
/// # Returns
/// 返回值
/// The exact count when known, otherwise `None`.
/// 已知时返回精确计数，否则返回 `None`。
fn known_usage_u64(value: &UsageValue) -> Option<u64> {
    match value {
        UsageValue::Unknown => None,
        UsageValue::Known { value, .. } => Some(*value),
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

    /// Verifies final provider projection replaces an integrity-invalid historical attachment with model-safe metadata.
    /// 验证最终 Provider 投影会用模型安全元数据替换完整性失效的历史附件。
    #[test]
    fn provider_projection_rejects_same_length_mutated_managed_attachment() {
        // workspace owns the indexed attachment and is removed after the projection assertion.
        // workspace 持有索引附件，并在投影断言后移除。
        let workspace = std::env::temp_dir().join(format!(
            "vulcan-provider-attachment-integrity-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workspace).expect("test workspace should be created");
        // attachment retains immutable byte facts while omitting an original-source fallback.
        // attachment 保留不可变字节事实，同时不提供原始来源回退路径。
        let attachment = stage_attachment_bytes(
            workspace.to_string_lossy().as_ref(),
            &AttachmentDescriptor {
                kind: ConversationAttachmentKind::Image,
                mime_type: "image/png",
                extension: "png",
            },
            "integrity.png",
            None,
            b"original-image",
        )
        .expect("test attachment should stage");
        // mutated_bytes deliberately preserves length while changing the indexed content identity.
        // mutated_bytes 故意保持长度不变，同时改变已索引的内容身份。
        let mutated_bytes = vec![b'x'; b"original-image".len()];
        std::fs::write(&attachment.local_path, mutated_bytes)
            .expect("same-length attachment mutation should write");
        // message exercises the final canonical-message bridge shared by ordinary requests and compaction.
        // message 覆盖普通请求与压缩共同使用的最终规范消息桥接。
        let message = vulcan_core::types::ConversationMessage {
            id: String::from("user-integrity"),
            payload: CoreConversationPayload::UserText {
                text: String::from("inspect attachment"),
                attachments: vec![attachment],
            },
        };

        let projected = core_message_to_public(
            &message,
            "provider",
            "connection",
            "https://provider.invalid/v1/responses",
            "openai-responses.v1",
            "model",
        )
        .expect("integrity-invalid attachment should degrade to metadata");

        assert!(
            !projected
                .content
                .iter()
                .any(|item| matches!(item, ContentItem::Resource { .. }))
        );
        assert!(projected.content.iter().any(|item| {
            matches!(item, ContentItem::Text { text } if text.contains("managed resource no longer exists"))
        }));

        std::fs::remove_dir_all(&workspace).expect("test workspace should be removed");
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

    /// Verifies a fresh process allocator advances beyond generations retained by an earlier run.
    /// 验证新进程分配器会越过更早运行已经保留的代际。
    #[test]
    fn provider_api_trace_group_sequence_advances_from_persisted_floor() {
        // sequence simulates the zeroed allocator owned by a newly started process.
        // sequence 模拟新启动进程拥有的归零分配器。
        let sequence = AtomicU64::new(0);

        synchronize_provider_api_trace_group_sequence(&sequence, 4);
        assert_eq!(
            reserve_provider_api_trace_group_sequence(&sequence)
                .expect("reserve generation after persisted floor"),
            5
        );
        assert_eq!(
            reserve_provider_api_trace_group_sequence(&sequence)
                .expect("reserve following process-local generation"),
            6
        );

        synchronize_provider_api_trace_group_sequence(&sequence, 3);
        assert_eq!(
            reserve_provider_api_trace_group_sequence(&sequence)
                .expect("a lower persisted floor must not move the allocator backward"),
            7
        );
        synchronize_provider_api_trace_group_sequence(&sequence, 9);
        assert_eq!(
            reserve_provider_api_trace_group_sequence(&sequence)
                .expect("a higher persisted floor must advance the allocator"),
            10
        );
    }

    /// Verifies host-side failures fill the trace stage only when no lower transport failure exists.
    /// 验证仅当不存在更底层传输失败时，宿主侧失败才会补充追踪阶段。
    #[test]
    fn provider_api_trace_failure_selection_preserves_the_earliest_boundary() {
        let processing_failure = ProviderApiTraceProcessingFailure {
            stage: String::from("tool_request_coordinator"),
            message: String::from("tool stream ordering failed"),
        };

        let (stage, message, local_processing_failed) =
            select_provider_api_trace_failure(None, None, Some(&processing_failure));
        assert_eq!(stage.as_deref(), Some("tool_request_coordinator"));
        assert_eq!(message.as_deref(), Some("tool stream ordering failed"));
        assert!(local_processing_failed);

        let (stage, message, local_processing_failed) = select_provider_api_trace_failure(
            Some("protocol_decoder"),
            Some("invalid frame"),
            Some(&processing_failure),
        );
        assert_eq!(stage.as_deref(), Some("protocol_decoder"));
        assert_eq!(message.as_deref(), Some("invalid frame"));
        assert!(!local_processing_failed);
    }

    /// Verifies tool construction events are attributed to the request coordinator trace stage.
    /// 验证工具构造事件会归属到工具请求协调器追踪阶段。
    #[test]
    fn provider_api_trace_classifies_tool_request_coordinator_events() {
        let event = AiStreamEvent::ToolCallStarted {
            item_id: String::from("item-1"),
            call_id: String::from("call-1"),
            tool_name: String::from("file_edit"),
            index: 0,
        };

        assert_eq!(
            provider_trace_processing_stage_for_event(&event),
            "tool_request_coordinator"
        );
        assert_eq!(
            provider_trace_processing_stage_for_event(&AiStreamEvent::ResponseCompleted),
            "response_projection"
        );
    }

    /// Verifies the detached trace writer receives one controlled host-side processing failure.
    /// 验证独立追踪写入器会收到一项受控的宿主侧处理失败。
    #[tokio::test]
    async fn provider_api_trace_processing_failure_sender_publishes_once() {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let mut publisher = ProviderApiTraceProcessingFailureSender::new(Some(sender));
        let error = provider_failure("provider", "tool stream ordering failed");

        publisher.observe_failure("tool_request_coordinator", &error);
        publisher.observe_failure("response_projection", &error);

        let failure = receiver
            .await
            .expect("the first processing failure should be published");
        assert_eq!(failure.stage, "tool_request_coordinator");
        assert!(failure.message.contains("tool stream ordering failed"));
    }

    /// Verifies response trace encoding stays readable unless JSON escaping would be larger.
    /// 验证响应追踪编码默认保持可读，仅在 JSON 转义体积更大时切换编码。
    #[test]
    fn provider_response_trace_body_uses_the_smaller_exact_encoding() {
        // readableBody represents ordinary SSE text that should remain directly inspectable.
        // readableBody 表示应保持可直接检查的普通 SSE 文本。
        let readable_body = b"data: {\"type\":\"response.completed\"}\n\n".to_vec();
        let (readable_encoding, readable_payload) =
            encode_provider_response_trace_body(readable_body.clone());
        assert_eq!(readable_encoding, "utf8");
        assert_eq!(readable_payload.as_bytes(), readable_body);

        // escapedBody forces six-byte JSON escapes whose Base64 form is smaller and fully reversible.
        // escapedBody 强制使用六字节 JSON 转义，其 Base64 表示更小且可完整还原。
        let escaped_body = vec![0_u8; 4_096];
        let (escaped_encoding, escaped_payload) =
            encode_provider_response_trace_body(escaped_body.clone());
        assert_eq!(escaped_encoding, "base64");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(escaped_payload)
                .expect("Base64 trace response should decode"),
            escaped_body
        );
    }

    /// Verifies the closed configuration enum maps directly onto the three shared projection profiles.
    /// 验证封闭配置枚举直接映射到三项共享投影配置。
    #[test]
    fn configured_tool_schema_projection_modes_map_without_heuristics() {
        for (mode, expected) in [
            (
                ToolSchemaProjectionMode::Lightweight,
                VulcanToolSchemaProjectionProfile::StandardObjectRoot,
            ),
            (
                ToolSchemaProjectionMode::Passthrough,
                VulcanToolSchemaProjectionProfile::PassThrough,
            ),
            (
                ToolSchemaProjectionMode::Strict,
                VulcanToolSchemaProjectionProfile::GeminiSlim,
            ),
        ] {
            assert_eq!(configured_tool_schema_projection_profile(mode), expected);
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

    /// Verifies diagnostic draining consumes events after the host has already rejected a turn.
    /// 验证诊断排空会在宿主已拒绝回合后继续消费剩余事件。
    #[tokio::test]
    async fn provider_trace_drain_consumes_the_remaining_stream() {
        // tailObserved reports that the detached diagnostic consumer reached the simulated body tail.
        // tailObserved 用于报告分离式诊断消费者已经到达模拟正文尾部。
        let (tail_observed_sender, tail_observed_receiver) = tokio::sync::oneshot::channel();
        // remainingStream models normalized events still backed by unread Provider response bytes.
        // remainingStream 模拟仍由未读取 Provider 响应字节支撑的规范化事件。
        let remaining_stream: ProviderExecutionStream = Box::pin(async_stream::stream! {
            yield Ok(AiStreamEvent::ResponseStarted {
                execution_id: String::from("trace-drain-response"),
            });
            let _ = tail_observed_sender.send(());
            yield Ok(AiStreamEvent::ResponseCompleted);
        });

        drain_provider_stream_for_trace(remaining_stream);

        // observedTail fails if the detached drain stops after the first remaining event.
        // observedTail 在分离式排空于首个剩余事件后停止时失败。
        tokio::time::timeout(std::time::Duration::from_secs(1), tail_observed_receiver)
            .await
            .expect("trace drain should reach the response tail")
            .expect("trace drain should keep the tail observer alive");
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

    /// Verifies a frozen response-profile chain selects exactly one transient status mode.
    /// 验证冻结响应 Profile 链只允许选择一种瞬态状态模式。
    #[test]
    fn transient_reasoning_mode_fails_closed_on_multiple_profile_claims() {
        // profile builds one minimal response Profile with an explicit transient-mode claim.
        // profile 构建一个带显式瞬态模式声明的最小响应 Profile。
        let profile = |profile_id: &str, mode| ResponseProjectionProfile {
            profile_id: profile_id.to_string(),
            revision: 1,
            steps: Vec::new(),
            transient_reasoning_status: Some(mode),
        };
        assert_eq!(
            transient_reasoning_status_kind(
                "provider",
                &[profile("summary", TransientReasoningStatusMode::Summary,)],
            )
            .expect("one transient profile should resolve"),
            Some(TransientReasoningStatusKind::Summary)
        );
        assert!(
            transient_reasoning_status_kind(
                "provider",
                &[
                    profile("summary", TransientReasoningStatusMode::Summary),
                    profile("encrypted", TransientReasoningStatusMode::Encrypted),
                ],
            )
            .expect_err("multiple transient profiles must fail closed")
            .to_string()
            .contains("Multiple response profiles")
        );
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
            None,
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
        // response is the durable controller value after all transient state was cleared.
        // response 是全部瞬态状态清除后的持久化控制器值。
        let response = accumulator
            .finish()
            .expect("completed Responses turn should finish");
        // replay_value contains only the canonical durable replay envelope.
        // replay_value 只包含规范持久化重放信封。
        let replay_value = response
            .provider_message_state
            .as_ref()
            .and_then(|state| state.pointer("/vulcan_ai/replay"))
            .expect("replay envelope should be persisted");
        // replay proves no summary text or status metadata crossed the persistence boundary.
        // replay 证明没有摘要文本或状态元数据越过持久化边界。
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

    /// Verifies malformed streamed tool arguments become one non-executable tool call instead of a retryable Provider failure.
    /// 验证格式错误的流式工具参数会成为一条不可执行工具调用，而不是可重试 Provider 失败。
    #[test]
    fn public_tool_abort_becomes_non_executable_tool_call() {
        // liveChannel and sink capture both the visible tool draft and its abort boundary.
        // liveChannel 与 sink 同时捕获可见工具草稿及其中止边界。
        let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut sink = RecordingProviderDeltaSink::default();
        // accumulator represents one physical Provider request after the latest controller checkpoint.
        // accumulator 表示最近控制器检查点之后的一次真实 Provider 请求。
        let mut accumulator = PublicTurnAccumulator::new(
            "opencode-go",
            "connection-opencode-go",
            "https://opencode.example/v1/responses",
            "openai-responses.v1",
            "deepseek-v4-flash",
            None,
        );

        accumulator
            .apply_event(
                "opencode-go",
                &live_sender,
                &mut sink,
                AiStreamEvent::ToolCallStarted {
                    item_id: String::from("tool-item"),
                    call_id: String::from("call-1"),
                    tool_name: String::from("vulcan_file_edit"),
                    index: 0,
                },
            )
            .expect("tool draft should start");
        // decoderMessage mirrors the controlled error emitted by OpenAI Responses argument parsing.
        // decoderMessage 镜像 OpenAI Responses 参数解析发出的受控错误。
        let decoder_message = "OpenAI Responses tool input ended before valid JSON was complete";
        accumulator
            .apply_event(
                "opencode-go",
                &live_sender,
                &mut sink,
                AiStreamEvent::ToolCallAborted {
                    item_id: String::from("tool-item"),
                    message: decoder_message.to_string(),
                },
            )
            .expect("identified malformed tool call should remain inside the successful turn");
        accumulator
            .apply_event(
                "opencode-go",
                &live_sender,
                &mut sink,
                AiStreamEvent::ResponseCompleted,
            )
            .expect("response completion should remain valid after the controlled rejection");

        let response = accumulator
            .finish()
            .expect("controlled malformed tool call should finish the Provider turn");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].tool_name, "vulcan_file_edit");
        assert_eq!(response.tool_calls[0].arguments, Value::Null);
        assert_eq!(
            response.tool_calls[0].argument_parse_error.as_deref(),
            Some(decoder_message)
        );

        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::ToolRequestFragment(
                ToolRequestFragment::Detected { .. }
            )))
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::ToolRequestFragment(
                ToolRequestFragment::Invalid { .. }
            )))
        ));
        assert!(live_receiver.try_recv().is_err());
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
            None,
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

    /// Verifies summary status replacement and token updates never enter durable reasoning state.
    /// 验证摘要状态替换与 Token 更新绝不进入持久化思考状态。
    #[test]
    fn transient_reasoning_summary_is_live_only_and_clears_at_terminal() {
        // live_sender and live_receiver observe the request-local status stream.
        // live_sender 与 live_receiver 观察请求局部状态流。
        let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
        // sink proves no transient content enters canonical Core deltas.
        // sink 证明没有瞬态内容进入规范 Core 增量。
        let mut sink = RecordingProviderDeltaSink::default();
        // accumulator owns a profile-configured summary status for one physical request.
        // accumulator 为单次物理请求持有 Profile 配置的摘要状态。
        let mut accumulator = PublicTurnAccumulator::new(
            "commandcode",
            "connection-commandcode",
            "https://api.commandcode.ai/provider/v1/chat/completions",
            "openai-chat-completions.v1",
            "gpt-5.6-luna",
            Some(TransientReasoningStatusKind::Summary),
        );

        for event in [
            AiStreamEvent::ResponseStarted {
                execution_id: String::from("response-1"),
            },
            AiStreamEvent::ReasoningSummaryUpdated {
                item_id: String::from("summary-0"),
                text: String::from("正在检查协议"),
            },
            AiStreamEvent::ReasoningSummaryUpdated {
                item_id: String::from("summary-0"),
                text: String::from("正在检查协议响应"),
            },
            AiStreamEvent::ReasoningSummaryUpdated {
                item_id: String::from("summary-1"),
                text: String::from("正在核对清单"),
            },
            AiStreamEvent::UsageUpdated {
                usage: Box::new(RequestUsage {
                    reasoning_tokens: UsageValue::Known {
                        value: 321,
                        source: vulcan_ai_contract::UsageValueSource::Reported,
                    },
                    ..RequestUsage::default()
                }),
            },
            AiStreamEvent::ResponseCompleted,
        ] {
            accumulator
                .apply_event("commandcode", &live_sender, &mut sink, event)
                .expect("transient summary lifecycle should accumulate");
        }

        assert!(sink.deltas.is_empty());
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
                if status.kind == TransientReasoningStatusKind::Summary
                    && status.summaries.is_empty()
                    && status.reasoning_tokens.is_none()
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
                if status.kind == TransientReasoningStatusKind::Summary
                    && matches!(status.summaries.as_slice(), [summary]
                        if summary.item_id == "summary-0" && summary.text == "正在检查协议")
                    && status.reasoning_tokens.is_none()
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
                if status.kind == TransientReasoningStatusKind::Summary
                    && matches!(status.summaries.as_slice(), [summary]
                        if summary.item_id == "summary-0" && summary.text == "正在检查协议响应")
                    && status.reasoning_tokens.is_none()
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
                if status.kind == TransientReasoningStatusKind::Summary
                    && matches!(status.summaries.as_slice(), [first, second]
                        if first.item_id == "summary-0"
                            && first.text == "正在检查协议响应"
                            && second.item_id == "summary-1"
                            && second.text == "正在核对清单")
                    && status.reasoning_tokens.is_none()
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
                if status.kind == TransientReasoningStatusKind::Summary
                    && status.summaries.len() == 2
                    && status.reasoning_tokens == Some(321)
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(
                None
            )))
        ));

        let response = accumulator
            .finish()
            .expect("completed transient summary turn should finish");
        assert!(response.assistant_reasoning.is_empty());
        let replay_value = response
            .provider_message_state
            .as_ref()
            .and_then(|state| state.pointer("/vulcan_ai/replay"))
            .expect("replay envelope should exist without transient status");
        let replay: ProviderReplayEnvelope = serde_json::from_value(replay_value.clone())
            .expect("replay envelope should deserialize");
        assert!(replay.content.is_empty());
        assert!(replay.provider_context_items.is_empty());
    }

    /// Verifies non-empty full reasoning permanently supersedes summary and encrypted fallbacks.
    /// 验证非空完整思考会永久取代摘要与加密回退状态。
    #[test]
    fn durable_reasoning_supersedes_transient_fallbacks() {
        // liveSender and liveReceiver observe the exact fallback-clear ordering.
        // liveSender 与 liveReceiver 观察精确的回退清除顺序。
        let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
        // sink records only durable reasoning after arbitration.
        // sink 仅记录仲裁后的持久化思考。
        let mut sink = RecordingProviderDeltaSink::default();
        // accumulator starts in encrypted fallback mode before readable summary arrives.
        // accumulator 在可读摘要到达前以加密回退模式启动。
        let mut accumulator = PublicTurnAccumulator::new(
            "commandcode",
            "connection-commandcode",
            "https://api.commandcode.ai/provider/v1/chat/completions",
            "openai-chat-completions.v1",
            "gpt-5.6-luna",
            Some(TransientReasoningStatusKind::Encrypted),
        );
        for event in [
            AiStreamEvent::ResponseStarted {
                execution_id: String::from("response-priority"),
            },
            AiStreamEvent::ReasoningSummaryUpdated {
                item_id: String::from("summary-0"),
                text: String::from("正在分析"),
            },
            AiStreamEvent::ReasoningStarted {
                item_id: String::from("reasoning-0"),
            },
            AiStreamEvent::ReasoningDelta {
                item_id: String::from("reasoning-0"),
                delta: String::from("完整思考"),
            },
            AiStreamEvent::ReasoningSummaryUpdated {
                item_id: String::from("summary-1"),
                text: String::from("不得重新出现"),
            },
            AiStreamEvent::ReasoningCompleted {
                item_id: String::from("reasoning-0"),
                signature: None,
            },
            AiStreamEvent::ResponseCompleted,
        ] {
            accumulator
                .apply_event("commandcode", &live_sender, &mut sink, event)
                .expect("priority arbitration should remain valid");
        }

        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
                if status.kind == TransientReasoningStatusKind::Encrypted
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(Some(status))))
                if status.kind == TransientReasoningStatusKind::Summary
                    && status.summaries.len() == 1
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::TransientReasoningStatusChanged(
                None
            )))
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::Thinking(reasoning))) if reasoning == "完整思考"
        ));
        assert!(live_receiver.try_recv().is_err());
        assert!(matches!(
            sink.deltas.as_slice(),
            [ProviderTurnDelta::Thinking { delta }] if delta == "完整思考"
        ));
        assert_eq!(
            accumulator
                .finish()
                .expect("completed priority turn should finish")
                .assistant_reasoning,
            "完整思考"
        );
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
                    resource_path: None,
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
                    resource_path: None,
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

    /// Verifies frozen manifest constraints produce Provider-neutral degradation reasons.
    /// 验证冻结清单约束会生成供应商无关的降级原因。
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
            resource_path: None,
            local_path: None,
            url: Some(String::from("https://media.example/audio.mp3")),
        };
        assert!(
            frozen_resource_constraint_unavailability_reason(&local_only, &remote_audio).is_some()
        );
        // preparedLocalAudio retains local provenance while carrying immutable bytes prepared by the host.
        // preparedLocalAudio 保留本地来源，同时携带由宿主准备的不可变字节。
        let prepared_local_audio = ResourceReference {
            id: String::from("audio-prepared"),
            kind: ResourceKind::Audio,
            media_type: Some(String::from("audio/mpeg")),
            source_kind: ResourceSourceKind::HostResolved,
            file_name: Some(String::from("audio.mp3")),
            image_detail: None,
            resource_path: None,
            local_path: Some(String::from("C:/workspace/audio.mp3")),
            url: Some(String::from("data:audio/mpeg;base64,AA==")),
        };
        assert!(
            frozen_resource_constraint_unavailability_reason(&local_only, &prepared_local_audio,)
                .is_none(),
            "host-prepared local media should retain local-only provenance"
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
            resource_path: None,
            local_path: None,
            url: Some(String::from("https://media.example/file.txt")),
        };
        assert!(frozen_resource_constraint_unavailability_reason(&pdf_only, &text_file).is_some());

        // remoteOnlyUpload proves local canonical resources remain eligible only with an exact upload channel.
        // remoteOnlyUpload 证明本地规范资源只有在存在精确上传通道时才可进入仅远程投影。
        let remote_only_upload = InputResourceConstraint {
            resource_kind: ResourceKind::Audio,
            accepted_mime_types: vec![String::from("audio/mpeg")],
            max_bytes: None,
            max_count: None,
            reference_policy: ResourceReferencePolicy::RemoteOnly,
            upload_channel_refs: vec![String::from("audio-upload")],
        };
        let local_audio = ResourceReference {
            id: String::from("audio-local"),
            kind: ResourceKind::Audio,
            media_type: Some(String::from("audio/mpeg")),
            source_kind: ResourceSourceKind::HostResolved,
            file_name: Some(String::from("audio.mp3")),
            image_detail: None,
            resource_path: None,
            local_path: Some(String::from("C:/workspace/audio.mp3")),
            url: None,
        };
        assert!(
            frozen_resource_constraint_unavailability_reason(&remote_only_upload, &local_audio)
                .is_none()
        );
    }

    /// Verifies metadata-only projection exposes only the safe locator and never the managed host path.
    /// 验证纯元数据投影只暴露安全定位符，绝不暴露宿主管理路径。
    #[test]
    fn resource_projection_fallback_uses_the_safe_resource_path() {
        let resource = ResourceReference {
            id: String::from("asset-1"),
            kind: ResourceKind::Image,
            media_type: Some(String::from("image/png")),
            source_kind: ResourceSourceKind::HostResolved,
            file_name: Some(String::from("screen\"\nINJECTED=1.png")),
            image_detail: None,
            resource_path: Some(String::from(
                ".vulcan/code/attachments/images/screen\"\nINJECTED=1.png",
            )),
            local_path: Some(String::from("C:/private/screen.png")),
            url: None,
        };

        let text = resource_projection_budget_fallback_text(
            &resource,
            "the request-wide host inline-media budget",
            Some(128),
        );

        assert!(text.contains(".vulcan/code/attachments/images/screen\\\"\\nINJECTED=1.png"));
        assert!(!text.contains("\nINJECTED=1"));
        assert!(!text.contains("C:/private"));
        assert!(!text.contains("file_id"));
    }

    /// Verifies provider file ids use the dedicated source channel instead of masquerading as URLs.
    /// 验证供应商文件标识使用专用来源通道，而不会伪装成 URL。
    #[test]
    fn prepared_provider_file_id_preserves_reference_encoding() {
        let mut resource = ResourceReference {
            id: String::from("asset-1"),
            kind: ResourceKind::Image,
            media_type: Some(String::from("image/png")),
            source_kind: ResourceSourceKind::HostResolved,
            file_name: Some(String::from("screen.png")),
            image_detail: None,
            resource_path: Some(String::from(".vulcan/code/attachments/screen.png")),
            local_path: Some(String::from("C:/private/screen.png")),
            url: None,
        };

        apply_prepared_media_reference(
            &mut resource,
            vulcan_provider_adapters::PreparedMediaReferenceSnapshot {
                provider_reference: String::from("file-provider-1"),
                reference_encoding: UploadReferenceEncoding::ProviderFileId,
                created_at_unix_millis: 1,
                expires_at_unix_millis: 2,
            },
        );

        assert_eq!(resource.id, "file-provider-1");
        assert_eq!(resource.source_kind, ResourceSourceKind::ProviderFileId);
        assert!(resource.url.is_none());
        assert_eq!(
            resource.resource_path.as_deref(),
            Some(".vulcan/code/attachments/screen.png")
        );
        let fallback = resource_projection_budget_fallback_text(
            &resource,
            "the request-wide host inline-media budget",
            Some(128),
        );
        assert!(!fallback.contains("file-provider-1"));
        assert!(fallback.contains(".vulcan/code/attachments/screen.png"));
    }

    /// Verifies a Provider file id cannot leak through the fallback locator when canonical metadata is absent.
    /// 验证缺失规范元数据时，供应商文件标识不能通过降级定位符泄漏。
    #[test]
    fn provider_file_id_fallback_without_resource_path_is_redacted() {
        let resource = ResourceReference {
            id: String::from("secret-provider-file-id"),
            kind: ResourceKind::Image,
            media_type: Some(String::from("image/png")),
            source_kind: ResourceSourceKind::ProviderFileId,
            file_name: Some(String::from("screen.png")),
            image_detail: None,
            resource_path: None,
            local_path: None,
            url: None,
        };

        let fallback = resource_projection_budget_fallback_text(
            &resource,
            "the request-wide host inline-media budget",
            Some(128),
        );

        assert!(!fallback.contains("secret-provider-file-id"));
        assert!(fallback.contains("attachment://unavailable"));
        assert!(fallback.contains("id=\"unavailable\""));
    }

    /// Verifies URL-shaped upload references retain the canonical attachment identity.
    /// 验证 URL 形上传引用会保留规范附件身份。
    #[test]
    fn prepared_response_url_preserves_canonical_resource_identity() {
        let mut resource = ResourceReference {
            id: String::from("asset-1"),
            kind: ResourceKind::Video,
            media_type: Some(String::from("video/mp4")),
            source_kind: ResourceSourceKind::HostResolved,
            file_name: Some(String::from("clip.mp4")),
            image_detail: None,
            resource_path: Some(String::from(".vulcan/code/attachments/clip.mp4")),
            local_path: Some(String::from("C:/private/clip.mp4")),
            url: None,
        };

        apply_prepared_media_reference(
            &mut resource,
            vulcan_provider_adapters::PreparedMediaReferenceSnapshot {
                provider_reference: String::from("https://media.example/clip.mp4"),
                reference_encoding: UploadReferenceEncoding::ResponseUrl,
                created_at_unix_millis: 1,
                expires_at_unix_millis: 2,
            },
        );

        assert_eq!(resource.id, "asset-1");
        assert_eq!(resource.source_kind, ResourceSourceKind::RemoteUrl);
        assert_eq!(
            resource.url.as_deref(),
            Some("https://media.example/clip.mp4")
        );
    }

    /// Verifies aggregate Base64 accounting uses exact expansion and fails closed at the host limit.
    /// 验证 Base64 聚合计费使用精确膨胀值，并在宿主上限处封闭失败。
    #[test]
    fn inline_resource_budget_accounts_for_exact_base64_expansion() {
        let mut budget = InlineResourceProjectionBudget::default();
        assert_eq!(budget.reserve(ResourceKind::Image, None, Some(1)), Ok(()));
        assert_eq!(budget.used_base64_bytes, 4);
        assert_eq!(
            budget.reserve(
                ResourceKind::Image,
                None,
                Some((HOST_INLINE_RESOURCE_BASE64_BUDGET_BYTES - 4) / 4 * 3),
            ),
            Ok(())
        );
        assert_eq!(
            budget.reserve(ResourceKind::Image, None, Some(1)),
            Err("the request-wide host inline-media budget")
        );
        assert_eq!(
            budget.retained_resource_counts.get(&ResourceKind::Image),
            Some(&2)
        );
    }

    /// Verifies native resource counts are shared across the whole request instead of reset per message.
    /// 验证原生资源数量在整个请求中共享，而不是按消息重置。
    #[test]
    fn inline_resource_budget_enforces_request_wide_resource_count() {
        let mut budget = InlineResourceProjectionBudget::default();
        assert_eq!(budget.reserve(ResourceKind::Image, Some(1), None), Ok(()));
        assert!(budget.count_limit_reached(ResourceKind::Image, Some(1)));
        assert_eq!(
            budget.reserve(ResourceKind::Image, Some(1), None),
            Err("the frozen route request-wide resource-count limit")
        );
        assert_eq!(budget.reserve(ResourceKind::Audio, Some(1), None), Ok(()));
    }

    /// Verifies pre-materialized inline data is decoded for the same aggregate budget as local files.
    /// 验证预先物化的内联数据会被解码，并与本地文件使用同一聚合预算。
    #[test]
    fn inline_data_resource_reports_exact_raw_byte_length() {
        let resource = ResourceReference {
            id: String::from("inline-image"),
            kind: ResourceKind::Image,
            media_type: Some(String::from("image/png")),
            source_kind: ResourceSourceKind::InlineData,
            file_name: Some(String::from("image.png")),
            image_detail: None,
            resource_path: None,
            local_path: None,
            url: Some(String::from("data:image/png;base64,AQIDBA==")),
        };

        assert_eq!(inline_data_resource_raw_byte_len(&resource), Ok(Some(4)));
    }

    /// Verifies generic files never enter media projectors merely because a profile supports media.
    /// 验证通用文件不会仅因协议 Profile 支持媒体而进入媒体投影器。
    #[test]
    fn protocol_resource_kind_gate_keeps_generic_files_metadata_only() {
        for protocol in [
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID,
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID,
        ] {
            assert!(!protocol_profile_supports_resource_kind(
                protocol,
                ResourceKind::File,
            ));
        }
        assert!(protocol_profile_supports_resource_kind(
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
            ResourceKind::Image,
        ));
    }
}
