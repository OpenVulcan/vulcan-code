//! Deterministic provider execution routing and SSE transport.
//! 确定性的供应商执行路由与 SSE 传输。

use crate::media::{MediaUploadCancellation, MediaUploadProgressCallback};
use crate::{
    AuthenticationProjection, CredentialConnection, CredentialExecutionSecret,
    CustomModelRequestProjection, ProtocolRoute, ProviderDescriptor, ProviderToolTarget,
    RequestProjectorKind, SAMPLING_TEMPERATURE_PARAMETER_ID, SecondaryRouteSelection, ServiceEntry,
};
use async_stream::try_stream;
use futures_util::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant, SystemTime};
use vulcan_ai_contract::{
    AiErrorCategory, AiExecutionError, AiExecutionRequest, AiStreamEvent, AiStreamStateMachine,
    AiTransportKind, ContentItem, DispatchOutcome, InputTokenCount, ModelParameterValue,
    PromptCachePolicy, ResourceKind, RetryAfterHint, RetryAfterSource, StructuredOutputPolicy,
    canonicalize_codex_responses_replay_items, is_replayable_codex_responses_item,
};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    CLAUDE_CODE_MESSAGES_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID,
    GEMINI_INTERACTIONS_PROFILE_ID, GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID, ProtocolConversionError,
    ProtocolStreamFraming, decode_gemini_count_tokens_response, encode_gemini_count_tokens_request,
    gemini_replay_signature_payload, wrap_openai_reasoning_signature,
};
use vulcan_llm_core::{
    AnthropicReasoningMode, CostCalculator, DEFAULT_RESPONSES_WEBSOCKET_MAX_AGE,
    EncodedWireRequest, LlmCoreError, LlmProtocolRuntime, ModelPricing, ProtocolTransportProfile,
    ProviderRouteContext, RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE, ResponsesWebsocketConfig,
    ResponsesWebsocketSession, UpstreamProtocol, UsageAccumulator, UsageExtractionProfile,
    UsageRecordKind, converted_protocol_headers, encode_codex_responses_request,
    project_responses_for_provider,
};

use tokio::sync::Mutex as TokioMutex;

/// Supplies the core-owned protocol runtime to the provider execution client.
/// 为供应商执行客户端提供由核心拥有的协议运行时。
pub trait ProtocolRuntimeSource {
    /// Converts the source into the immutable runtime shared by one client.
    /// 将来源转换成一个客户端共享的不可变运行时。
    fn into_protocol_runtime(self) -> Arc<LlmProtocolRuntime>;
}

impl ProtocolRuntimeSource for Arc<LlmProtocolRuntime> {
    /// Keeps the already core-owned runtime unchanged.
    /// 保持已经由核心拥有的运行时不变。
    fn into_protocol_runtime(self) -> Arc<LlmProtocolRuntime> {
        self
    }
}

/// Official Codex multiplier from stream idle timeout to the complete unary compact timeout.
/// 官方 Codex 从流空闲超时换算完整一元压缩超时所使用的倍数。
const RESPONSES_COMPACT_TIMEOUT_IDLE_MULTIPLIER: u32 = 4;

/// Opt-in environment switch for secret-safe provider transport timing diagnostics.
/// 用于启用不含密钥的供应商传输计时诊断的可选环境变量。
const PROVIDER_TRANSPORT_TRACE_ENV: &str = "VULCAN_PROVIDER_TRANSPORT_TRACE";

/// Maximum bytes retained from one provider-controlled diagnostic response-header value.
/// 单个供应商控制的诊断响应头值允许保留的最大字节数。
const PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES: usize = 1_024;

/// Explicit suffix appended when a provider diagnostic header value is truncated.
/// 供应商诊断响应头值被截断时附加的显式后缀。
const PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX: &str = "...[truncated]";

/// Codex Responses WebSocket protocol revision frozen from the adopted source baseline.
/// 从已采纳源码基线冻结的 Codex Responses WebSocket 协议修订值。
const CODEX_RESPONSES_WEBSOCKET_BETA: &str = "responses_websockets=2026-02-06";

/// Official control error indicating that the active socket no longer owns the referenced chain.
/// 表示活动 Socket 已不再持有被引用链的官方控制错误。
const RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE: &str = "previous_response_not_found";

/// Maximum process-local prepared media references retained across all providers and accounts.
/// 全部供应商与账号合计允许保留的最大进程内媒体准备引用数。
const PREPARED_MEDIA_CACHE_MAX_ENTRIES: usize = 256;
/// Maximum process-local Responses WebSocket sessions retained across all roots.
/// 全部根会话合计允许保留的最大进程内 Responses WebSocket 会话数。
const RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES: usize = 256;
/// Maximum sticky Responses WebSocket circuit breakers retained across all roots.
/// 全部根会话合计允许保留的最大粘性 Responses WebSocket 熔断器数量。
const RESPONSES_WEBSOCKET_BREAKER_MAX_ENTRIES: usize = 4_096;
/// Stable provider-neutral operation identity for pre-dispatch conversation media materialization.
/// 分派前对话媒体物化使用的稳定供应商无关 operation 身份。
pub const CONVERSATION_MEDIA_INPUT_OPERATION_ID: &str = "conversation.media_input";

/// Immutable route selected before an execution starts.
/// 在执行开始前选择的不可变路由。
#[derive(Debug, Clone)]
pub struct ProviderExecutionRoute {
    /// Provider descriptor snapshot.
    /// 供应商描述快照。
    pub provider: ProviderDescriptor,
    /// Service entry snapshot.
    /// 服务入口快照。
    pub entry: ServiceEntry,
    /// Exact credential connection snapshot without secret material.
    /// 不含秘钥材料的精确凭据连接快照。
    pub connection: CredentialConnection,
    /// Exact verified protocol route.
    /// 精确的已验证协议路由。
    pub protocol_route: ProtocolRoute,
    /// Optional provider-declared secondary route.
    /// 可选供应商声明二级路由。
    pub secondary_route: SecondaryRouteSelection,
    /// Optional user-owned safe projection for one exact custom model.
    /// 可选的单个精确自定义模型用户自有安全投影。
    pub custom_model_projection: Option<CustomModelRequestProjection>,
    /// Optional frozen local billing profile resolved from explicit configuration.
    /// 从显式配置解析出的可选冻结本地计费 Profile。
    pub billing_profile: Option<ResolvedBillingProfile>,
    /// Ordered request-projection profile chain effective for this exact model route; route
    /// defaults form the base and an explicitly present model-rule list replaces them, including
    /// an explicit empty list that clears the chain (plan §R8.4/§R8.5).
    /// 当前精确模型路由生效的有序请求投影 Profile 链；路由默认引用构成基座，模型规则中
    /// 显式存在的列表会替换基座，其中显式空列表会清空该链（方案 §R8.4/§R8.5）。
    pub request_projection_profiles: Vec<crate::provider_manifest::RequestProjectionProfile>,
    /// Ordered response-projection profile chain effective for this exact model route.
    /// 当前精确模型路由生效的有序响应投影 Profile 链。
    pub response_projection_profiles: Vec<crate::provider_manifest::ResponseProjectionProfile>,
    /// Upload channels frozen from the same manifest generation as this route.
    /// 与当前路由来自同一 Manifest 代际的冻结上传通道。
    pub upload_channels: Vec<crate::UploadChannelDeclaration>,
    /// Manifest revision that owns the selected model-protocol binding.
    /// 持有所选模型协议绑定的 Manifest 修订号。
    pub manifest_revision: u64,
    /// Runtime manifest generation frozen with this route.
    /// 与当前路由一同冻结的运行时 Manifest 代际。
    pub manifest_generation: u64,
    /// Content-addressed identity of the complete effective execution route.
    /// 完整生效执行路由的内容寻址身份。
    pub route_revision: String,
}

/// Frozen exact pricing facts applied to normalized usage updates.
/// 应用于规范化用量更新的冻结精确价格事实。
#[derive(Debug, Clone)]
pub struct ResolvedBillingProfile {
    /// Immutable content-addressed pricing revision.
    /// 内容寻址的不可变价格修订号。
    pub pricing_revision: String,
    /// ISO currency for the local estimate.
    /// 本地估算使用的 ISO 币种。
    pub currency: String,
    /// Per-million-token prices parsed without floating point.
    /// 不经过浮点解析的每百万 Token 价格。
    pub pricing: ModelPricing,
    /// Exact provider-specific multiplier.
    /// 精确供应商专属倍率。
    pub multiplier: Decimal,
    /// Serializable effective pricing evidence persisted with usage facts.
    /// 随用量事实持久化的可序列化生效价格证据。
    pub snapshot: vulcan_ai_contract::PricingProfileSnapshot,
}

impl ProviderExecutionRoute {
    /// Resolves an immutable execution route from registered snapshots.
    /// 从已注册快照解析不可变执行路由。
    ///
    /// # Parameters
    /// 参数
    /// `provider`, `entry`, and `connection` are exact configuration snapshots;
    /// `secondary_route` is the user-selected provider-internal route.
    /// `provider`、`entry` 与 `connection` 是精确配置快照；`secondary_route` 是用户选择的供应商内部路由。
    ///
    /// # Returns
    /// 返回值
    /// A validated route or a controlled pre-dispatch failure.
    /// 返回已校验路由或受控的分派前失败。
    pub fn resolve(
        provider: ProviderDescriptor,
        entry: ServiceEntry,
        connection: CredentialConnection,
        secondary_route: SecondaryRouteSelection,
        effective_protocol_id: &str,
    ) -> Result<Self, ProviderExecutionFailure> {
        if provider.id != connection.provider_id {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` does not belong to provider `{}`.",
                connection.id, provider.id
            )));
        }
        if entry.id != connection.service_entry_id {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` does not use service entry `{}`.",
                connection.id, entry.id
            )));
        }
        let credential_profile = entry
            .credential_profile(connection.credential_profile_id.as_str())
            .ok_or_else(|| {
                pre_dispatch_failure(format!(
                    "Credential profile `{}` is not registered for service entry `{}`.",
                    connection.credential_profile_id, entry.id
                ))
            })?;
        if credential_profile.credential_mode != connection.credential_mode {
            return Err(pre_dispatch_failure(format!(
                "Connection `{}` credential mode does not match profile `{}`.",
                connection.id, credential_profile.id
            )));
        }
        let mut protocol_route = entry
            .protocol_routes
            .iter()
            .find(|route| route.protocol_profile_id == effective_protocol_id)
            .cloned()
            .ok_or_else(|| {
                pre_dispatch_failure(format!(
                    "Protocol `{}` is not registered for service entry `{}`.",
                    effective_protocol_id, entry.id
                ))
            })?;
        // Manifest-declared extraction profiles are authoritative; the protocol baseline only
        // applies when the route declares no usage profile (plan §15.4).
        // Manifest 声明的提取 Profile 是权威；仅当路由未声明 usage Profile 时应用协议基线（方案 §15.4）。
        if protocol_route.usage_extraction_profiles.is_empty()
            && let Some(protocol) =
                upstream_protocol_for_profile_id(protocol_route.protocol_profile_id.as_str())
        {
            protocol_route.usage_extraction_profiles =
                vulcan_llm_core::standard_usage_profiles(protocol);
        }
        if !protocol_route.verified
            || !connection
                .verified_protocols
                .contains(effective_protocol_id)
        {
            return Err(pre_dispatch_failure(format!(
                "Protocol `{}` is not verified for connection `{}`.",
                effective_protocol_id, connection.id
            )));
        }
        if !matches!(secondary_route, SecondaryRouteSelection::Automatic)
            && entry.request_projector != RequestProjectorKind::RoutedCompatible
        {
            return Err(pre_dispatch_failure(format!(
                "Provider `{}` does not register secondary routing.",
                provider.id
            )));
        }
        // Route default profiles are cloned before the route moves into the snapshot so the
        // model-level override chain can replace them during selection (plan §R8.4).
        // 在路由移入快照前克隆路由默认 Profile，使选择期可用模型级覆盖链替换它们（方案 §R8.4）。
        let request_projection_profiles = protocol_route.request_projection_profiles.clone();
        let response_projection_profiles = protocol_route.response_projection_profiles.clone();
        Ok(Self {
            provider,
            entry,
            connection,
            protocol_route,
            secondary_route,
            custom_model_projection: None,
            billing_profile: None,
            request_projection_profiles,
            response_projection_profiles,
            upload_channels: Vec::new(),
            manifest_revision: 0,
            manifest_generation: 0,
            route_revision: String::new(),
        })
    }

    /// Returns the exact request URL for this immutable route.
    /// 返回该不可变路由的精确请求 URL。
    ///
    /// # Returns
    /// 返回值
    /// The service base URL joined with the registered endpoint path.
    /// 返回服务基础 URL 与已注册端点路径的拼接结果。
    pub fn request_url(&self, encoded_path: &str) -> String {
        format!(
            "{}{}",
            crate::connection_endpoint::resolved_connection_protocol_base_url(
                &self.connection,
                &self.entry,
                &self.protocol_route,
            )
            .trim_end_matches('/'),
            ensure_leading_slash(encoded_path)
        )
    }

    /// Returns model parameters that a frozen manifest guard conflicts with one request field.
    /// 返回冻结 Manifest 守卫中与一个请求字段冲突的模型参数。
    ///
    /// # Parameters
    /// 参数
    /// `request_field` is the registered field name without the leading `@` marker.
    /// `request_field` 是不带前导 `@` 标记的已注册字段名。
    ///
    /// # Returns
    /// 返回值
    /// Stable parameter identifiers from all-present guards that include the requested field.
    /// 返回包含目标请求字段的全部存在守卫中声明的稳定参数标识。
    pub fn model_parameters_conflicting_with_request_field(
        &self,
        request_field: &str,
    ) -> BTreeSet<String> {
        let expected_request_field = format!("@{request_field}");
        let mut conflicts = BTreeSet::new();
        for profile in &self.request_projection_profiles {
            for step in &profile.steps {
                let crate::provider_manifest::MappingStep::RejectIfAllPresent {
                    conditions, ..
                } = step
                else {
                    continue;
                };
                let guards_request_field = conditions.iter().any(|condition| {
                    condition.source == crate::provider_manifest::MappingStepSource::RequestField
                        && condition.source_path == expected_request_field
                });
                if !guards_request_field {
                    continue;
                }
                for condition in conditions {
                    if condition.source != crate::provider_manifest::MappingStepSource::RequestField
                    {
                        continue;
                    }
                    if let Some(parameter_id) = condition
                        .source_path
                        .strip_prefix("@model_parameters.")
                        .filter(|parameter_id| !parameter_id.is_empty())
                    {
                        conflicts.insert(parameter_id.to_string());
                    }
                }
            }
        }
        conflicts
    }
}

/// Carries one structured provider execution failure.
/// 承载一个结构化供应商执行失败。
#[derive(Debug, Clone)]
pub struct ProviderExecutionFailure {
    /// Normalized public error.
    /// 归一化公共错误。
    pub error: Box<AiExecutionError>,
}

impl std::fmt::Display for ProviderExecutionFailure {
    /// Formats the controlled execution message.
    /// 格式化受控执行消息。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.error.message.as_str())
    }
}

impl std::error::Error for ProviderExecutionFailure {}

/// Carries one validated non-streaming Responses compaction result.
/// 承载一份已验证的非流式 Responses 远端压缩结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsesCompactResult {
    /// Ordered provider-native output items preserved for the next Responses request.
    /// 为下一次 Responses 请求保留的有序供应商原生输出项。
    pub output_items: Vec<Value>,
    /// Optional provider-reported usage object retained without lossy projection.
    /// 可选的供应商用量对象，以无损形式保留。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    /// Optional upstream response identifier.
    /// 可选的上游响应标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
}

/// Stream of normalized public execution events.
/// 归一化公共执行事件流。
pub type ProviderExecutionStream =
    Pin<Box<dyn Stream<Item = Result<AiStreamEvent, ProviderExecutionFailure>> + Send>>;

/// Closed host-side download authority carried with one provider resource URL.
/// 随单个供应商资源 URL 携带的封闭宿主侧下载权限。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolResourceDownloadPolicy {
    /// Reviewed URL encoding, including the exact permitted scheme family.
    /// 已审核的 URL 编码，包含精确允许的 Scheme 范围。
    pub encoding: crate::ProviderToolResourceEncoding,
    /// Reviewed exact hosts or dot-prefixed DNS suffixes accepted from the provider response.
    /// 允许供应商响应返回的已审核精确宿主或点前缀 DNS 后缀。
    pub host_suffixes: Vec<String>,
}

impl ProviderToolResourceDownloadPolicy {
    /// Validates one provider-returned URL against the frozen scheme and host authority.
    /// 根据冻结的 Scheme 与宿主权限校验供应商返回的 URL。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the untrusted provider locator and `label` scopes controlled diagnostics.
    /// `value` 是不受信任的供应商定位符，`label` 用于限定受控诊断信息。
    ///
    /// # Returns
    /// 返回值
    /// A parsed URL only when it remains inside the complete declared download boundary.
    /// 仅当 URL 完整处于声明的下载边界内时返回解析结果。
    pub fn validate_url(&self, value: &str, label: &str) -> Result<reqwest::Url, String> {
        self.validate_url_with_optional_exact_origin(value, label, None)
    }

    /// Validates a provider-returned URL while admitting one frozen exact operation origin.
    /// 校验供应商返回 URL，同时允许一个冻结的精确操作 Origin。
    ///
    /// # Parameters
    /// 参数
    /// `value` and `label` retain the ordinary download contract; `exact_origin` is the
    /// already-validated endpoint that initiated this provider task.
    /// `value` 与 `label` 保留普通下载契约；`exact_origin` 是启动该供应商任务的
    /// 已验证端点。
    ///
    /// # Returns
    /// 返回值
    /// A parsed URL only when a non-default port matches the frozen operation origin exactly.
    /// 仅当非默认端口与冻结操作 Origin 精确一致时返回解析 URL。
    pub(crate) fn validate_url_with_exact_origin(
        &self,
        value: &str,
        label: &str,
        exact_origin: &reqwest::Url,
    ) -> Result<reqwest::Url, String> {
        self.validate_url_with_optional_exact_origin(value, label, Some(exact_origin))
    }

    /// Implements URL validation with an optional host-owned exact-origin port grant.
    /// 使用可选的宿主持有精确 Origin 端口授权实现 URL 校验。
    ///
    /// # Parameters
    /// 参数
    /// `value` is untrusted, `label` scopes diagnostics, and `exact_origin` never comes from the
    /// response being checked.
    /// `value` 不受信任，`label` 限定诊断，`exact_origin` 绝不来自当前被校验响应。
    ///
    /// # Returns
    /// 返回值
    /// The same closed result as `validate_url` with no broadened host or scheme authority.
    /// 返回与 `validate_url` 相同的封闭结果，不扩大宿主或 Scheme 权限。
    fn validate_url_with_optional_exact_origin(
        &self,
        value: &str,
        label: &str,
        exact_origin: Option<&reqwest::Url>,
    ) -> Result<reqwest::Url, String> {
        if self.host_suffixes.is_empty()
            || self.host_suffixes.len() > 16
            || self
                .host_suffixes
                .iter()
                .any(|suffix| !provider_resource_host_suffix_is_safe(suffix))
        {
            return Err(format!(
                "{label} download policy has no valid bounded host authority."
            ));
        }
        let url = reqwest::Url::parse(value).map_err(|_| format!("{label} is not a valid URL."))?;
        let scheme_allowed = match self.encoding {
            crate::ProviderToolResourceEncoding::HttpsUrl => url.scheme() == "https",
            crate::ProviderToolResourceEncoding::PublicHttpUrl => {
                matches!(url.scheme(), "http" | "https")
            }
            _ => false,
        };
        let host = url
            .host_str()
            .ok_or_else(|| format!("{label} must declare an absolute host."))?;
        // portAllowed grants the default scheme port or the one exact initiating operation origin;
        // response data can never introduce a third network authority.
        // portAllowed 只授权 Scheme 默认端口或精确的启动操作 Origin；响应数据绝不能
        // 引入第三个网络权限。
        let port_allowed = url.port().is_none()
            || exact_origin.is_some_and(|origin| provider_resource_urls_share_origin(&url, origin));
        if !scheme_allowed
            || !url.username().is_empty()
            || url.password().is_some()
            || !port_allowed
            || url.fragment().is_some()
            || !provider_resource_host_matches_suffixes(host, &self.host_suffixes)
        {
            return Err(format!(
                "{label} is outside the declared scheme, default port or exact operation origin, and host suffixes."
            ));
        }
        Ok(url)
    }
}

/// Reports whether two parsed URLs select the same scheme, host, and effective port.
/// 报告两个已解析 URL 是否选择相同 Scheme、宿主与有效端口。
///
/// # Parameters
/// 参数
/// `candidate` is provider-returned and `authority` is the frozen initiating endpoint.
/// `candidate` 由供应商返回；`authority` 是冻结的启动端点。
///
/// # Returns
/// 返回值
/// True only for an exact network-origin match.
/// 仅在网络 Origin 精确匹配时返回真。
fn provider_resource_urls_share_origin(candidate: &reqwest::Url, authority: &reqwest::Url) -> bool {
    candidate.scheme() == authority.scheme()
        && candidate.host_str().zip(authority.host_str()).is_some_and(
            |(candidate_host, authority_host)| candidate_host.eq_ignore_ascii_case(authority_host),
        )
        && candidate.port_or_known_default() == authority.port_or_known_default()
}

/// Reports whether one manifest host matcher is a safe exact host or DNS suffix.
/// 报告一个 Manifest 宿主匹配项是否为安全的精确宿主或 DNS 后缀。
///
/// # Parameters
/// 参数
/// `suffix` is an exact IPv4/DNS host or a dot-prefixed DNS subdomain suffix.
/// `suffix` 是精确 IPv4/DNS 宿主或点前缀 DNS 子域后缀。
///
/// # Returns
/// 返回值
/// True only for bounded lowercase host syntax without wildcard or URL components.
/// 仅对不含通配符或 URL 组件的有界小写宿主语法返回真。
pub fn provider_resource_host_suffix_is_safe(suffix: &str) -> bool {
    if suffix.is_empty() || suffix.len() > 253 {
        return false;
    }
    let host = suffix.strip_prefix('.').unwrap_or(suffix);
    if host.is_empty() || host.parse::<std::net::Ipv6Addr>().is_ok() {
        return false;
    }
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return !suffix.starts_with('.');
    }
    if !host.contains('.') || host.ends_with('.') {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label.chars().next().is_some_and(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit()
            })
            && label.chars().last().is_some_and(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit()
            })
            && label.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
    })
}

/// Reports whether one parsed host belongs to a manifest-owned exact/suffix allowlist.
/// 报告一个已解析宿主是否属于 Manifest 持有的精确/后缀允许列表。
///
/// # Parameters
/// 参数
/// `host` is the parsed URL host and `suffixes` contains already validated matchers.
/// `host` 是解析后的 URL 宿主，`suffixes` 包含已经校验的匹配项。
///
/// # Returns
/// 返回值
/// True only for an exact match or a non-root dot-suffix match.
/// 仅在精确匹配或非根域点后缀匹配时返回真。
pub fn provider_resource_host_matches_suffixes(host: &str, suffixes: &[String]) -> bool {
    let host = host.to_ascii_lowercase();
    suffixes.iter().any(|suffix| {
        if suffix.starts_with('.') {
            host.len() > suffix.len() && host.ends_with(suffix)
        } else {
            host == *suffix
        }
    })
}

/// One normalized generated resource returned by a provider tool.
/// 供应商工具返回的一项归一化生成资源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolGeneratedResource {
    /// Stable output identifier scoped to this invocation.
    /// 作用域限定在当前调用内的稳定输出标识。
    pub output_id: String,
    /// Unified semantic resource kind.
    /// 统一语义资源类别。
    pub kind: String,
    /// Exact MIME type.
    /// 精确 MIME 类型。
    pub mime_type: String,
    /// Private provider bytes when returned inline.
    /// 供应商以内联形式返回的私有字节。
    pub bytes: Option<Vec<u8>>,
    /// Public provider download URL when returned by reference.
    /// 供应商以引用形式返回的公开下载 URL。
    pub download_url: Option<String>,
    /// Frozen download authority required exactly when `download_url` is present.
    /// 仅在存在 `download_url` 时必须携带的冻结下载权限。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_policy: Option<ProviderToolResourceDownloadPolicy>,
}

/// One normalized provider web-search result.
/// 一条归一化供应商网页搜索结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolSearchResult {
    /// One-based provider result rank.
    /// 从一开始的供应商结果排名。
    pub rank: usize,
    /// Provider-returned result title.
    /// 供应商返回的结果标题。
    pub title: String,
    /// Validated absolute HTTP(S) result URL.
    /// 已校验的绝对 HTTP(S) 结果地址。
    pub url: String,
    /// Normalized source hostname.
    /// 归一化来源主机名。
    pub source_domain: String,
    /// Provider-returned result excerpt.
    /// 供应商返回的结果摘要。
    pub snippet: String,
}

/// Unified structured provider web-search output.
/// 统一结构化供应商网页搜索输出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolSearchOutput {
    /// Exact submitted query.
    /// 精确提交的查询。
    pub query: String,
    /// Ordered normalized results.
    /// 有序归一化结果。
    pub results: Vec<ProviderToolSearchResult>,
}

/// Closed normalized output returned by provider-native tools.
/// 供应商原生工具返回的封闭归一化输出。
/// One normalized successfully extracted web page.
/// 一条归一化的成功网页提取结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolExtractResult {
    /// Validated absolute HTTPS source URL.
    /// 已验证的绝对 HTTPS 来源地址。
    pub url: String,
    /// Provider-returned Markdown or plain-text content.
    /// 供应商返回的 Markdown 或纯文本内容。
    pub content: String,
    /// Validated absolute HTTPS image URLs returned for this page.
    /// 当前页面返回的已验证绝对 HTTPS 图片地址。
    pub images: Vec<String>,
    /// Optional validated absolute HTTPS favicon URL.
    /// 可选的已验证绝对 HTTPS 站点图标地址。
    pub favicon: Option<String>,
}

/// One normalized provider-reported extraction failure.
/// 一条归一化的供应商报告提取失败。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolExtractFailure {
    /// Validated absolute HTTPS source URL.
    /// 已验证的绝对 HTTPS 来源地址。
    pub url: String,
    /// Controlled provider failure message.
    /// 受控的供应商失败消息。
    pub error: String,
}

/// Unified structured provider web-extraction output.
/// 统一结构化供应商网页提取输出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderToolExtractOutput {
    /// Ordered successful extraction results.
    /// 有序的成功提取结果。
    pub results: Vec<ProviderToolExtractResult>,
    /// Ordered per-URL failures retained for user visibility.
    /// 为用户可见性保留的有序逐 URL 失败。
    pub failures: Vec<ProviderToolExtractFailure>,
}

/// One normalized embedding vector and its source index.
/// 一条归一化向量及其来源索引。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolEmbeddingVector {
    /// Zero-based input index associated with this vector.
    /// 与该向量关联的从零开始输入索引。
    pub index: usize,
    /// Ordered floating-point vector values returned by the provider.
    /// 供应商返回的有序浮点向量值。
    pub values: Vec<f64>,
}

/// Unified embedding output independent of provider wire fields.
/// 与供应商线级字段无关的统一向量输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolEmbeddingOutput {
    /// Optional exact embedding model reported by the provider.
    /// 供应商报告的可选精确向量模型。
    pub model: Option<String>,
    /// Ordered vectors matched to their original input indexes.
    /// 与原始输入索引匹配的有序向量。
    pub vectors: Vec<ProviderToolEmbeddingVector>,
}

/// One normalized reranked candidate.
/// 一条归一化重排候选项。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolRerankResult {
    /// Original zero-based candidate index.
    /// 原始的从零开始候选项索引。
    pub index: usize,
    /// Provider-reported relevance score.
    /// 供应商报告的相关性分数。
    pub score: f64,
    /// Optional provider-returned candidate text.
    /// 供应商可选返回的候选文本。
    pub document: Option<String>,
}

/// Unified rerank output independent of provider wire fields.
/// 与供应商线级字段无关的统一重排输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolRerankOutput {
    /// Optional exact rerank model reported by the provider.
    /// 供应商报告的可选精确重排模型。
    pub model: Option<String>,
    /// Ordered candidates in provider-ranked order.
    /// 按供应商排序顺序排列的候选项。
    pub results: Vec<ProviderToolRerankResult>,
}

/// Flat composite output used when one provider operation returns multiple semantic kinds.
/// 当一个供应商操作返回多种语义类型时使用的扁平复合输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolCompositeOutput {
    /// Ordered text fragments.
    /// 有序文本片段。
    pub text: Vec<String>,
    /// Ordered generated resources.
    /// 有序生成资源。
    pub resources: Vec<ProviderToolGeneratedResource>,
    /// Ordered embedding result groups.
    /// 有序向量结果组。
    pub embeddings: Vec<ProviderToolEmbeddingOutput>,
    /// Ordered rerank result groups.
    /// 有序重排结果组。
    pub reranks: Vec<ProviderToolRerankOutput>,
    /// Ordered search result groups.
    /// 有序搜索结果组。
    pub searches: Vec<ProviderToolSearchOutput>,
    /// Ordered extraction result groups.
    /// 有序提取结果组。
    pub extracts: Vec<ProviderToolExtractOutput>,
}

/// Closed normalized output returned by provider-native tools.
/// 供应商原生工具返回的封闭归一化输出。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ProviderToolNormalizedOutput {
    /// Provider-generated plain text.
    /// 供应商生成的纯文本。
    Text(String),
    /// Ordered generated resources.
    /// 有序生成资源。
    Resources(Vec<ProviderToolGeneratedResource>),
    /// Structured web-search evidence.
    /// 结构化网页搜索证据。
    Search(ProviderToolSearchOutput),
    /// Structured web-extraction content and per-URL failures.
    /// 结构化网页提取内容与逐 URL 失败。
    Extract(ProviderToolExtractOutput),
    /// Embedding vectors with stable source indexes.
    /// 带稳定来源索引的向量结果。
    Embedding(ProviderToolEmbeddingOutput),
    /// Ranked candidates with provider relevance scores.
    /// 带供应商相关性分数的重排候选项。
    Rerank(ProviderToolRerankOutput),
    /// Multiple normalized semantic output kinds from one invocation.
    /// 一次调用返回的多种归一化语义输出。
    Composite(ProviderToolCompositeOutput),
}

/// Unified typed result produced by one provider tool adapter.
/// 一个供应商工具适配器产生的统一强类型结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderToolExecutionResult {
    /// Optional provider trace or task identifier.
    /// 可选供应商追踪或任务标识。
    pub upstream_response_id: Option<String>,
    /// Closed normalized output independent of the provider wire shape.
    /// 与供应商 Wire 结构无关的封闭归一化输出。
    pub output: ProviderToolNormalizedOutput,
}

/// Executes immutable provider routes through public protocol profiles.
/// 通过公共协议 Profile 执行不可变供应商路由。
#[derive(Clone)]
pub struct ProviderExecutionClient {
    /// Shared configured HTTP client.
    /// 共享的已配置 HTTP 客户端。
    pub(crate) http: reqwest::Client,
    /// Core-owned protocol runtime facade.
    /// 由核心拥有的协议运行时门面。
    pub(crate) protocols: Arc<LlmProtocolRuntime>,
    /// Process-local prepared media references shared by preflight and dispatch clones.
    /// 由分派前准备与实际分派副本共享的进程内媒体引用缓存。
    prepared_media: Arc<StdMutex<BTreeMap<String, PreparedMediaCacheEntry>>>,
    /// Credential-, route-, and session-isolated Responses WebSocket connections.
    /// 按凭据、路由与会话隔离的 Responses WebSocket 连接。
    responses_websocket_sessions:
        Arc<TokioMutex<BTreeMap<String, ResponsesWebsocketSessionCacheEntry>>>,
    /// Session-scoped WebSocket circuit breakers activated after transport failure.
    /// 传输失败后激活的会话作用域 WebSocket 熔断器。
    responses_websocket_breakers: Arc<StdMutex<BTreeMap<String, Instant>>>,
}

/// Stores one shared Responses WebSocket connection and its last cache access.
/// 保存一个共享 Responses WebSocket 连接及其最近缓存访问时间。
#[derive(Clone)]
struct ResponsesWebsocketSessionCacheEntry {
    /// Shared non-multiplexed connection handle.
    /// 共享的非多路复用连接句柄。
    session: Arc<TokioMutex<ResponsesWebsocketSession>>,
    /// Monotonic timestamp used for deterministic least-recently-used eviction.
    /// 用于确定性最久未使用淘汰的单调时间戳。
    last_used_at: Instant,
}

/// Stores one temporary provider media reference until the matching request is dispatched.
/// 在匹配请求分派前保存一个临时供应商媒体引用。
#[derive(Clone)]
struct PreparedMediaCacheEntry {
    /// Provider-native URL or URI used by the exact request projector.
    /// 精确请求投影器使用的供应商原生 URL 或 URI。
    url: String,
    /// Expiry prevents an abandoned submission from retaining a reference forever.
    /// 过期时间防止放弃的提交永久保留远端引用。
    expires_at: Instant,
    /// Creation time provides deterministic oldest-first capacity eviction.
    /// 创建时间用于确定性的最旧优先容量淘汰。
    created_at: Instant,
    /// Source metadata prevents a rewritten file from reusing an older provider reference.
    /// 源文件元数据防止被改写的文件复用旧供应商引用。
    source_fingerprint: PreparedMediaSourceFingerprint,
}

/// Carries one prepared media outcome together with the exact verified source revision.
/// 携带一次媒体准备结果及其经过精确验证的源文件修订。
#[derive(Debug)]
pub struct PreparedConversationMedia {
    /// Provider-native reference when the frozen route uses an upload channel.
    /// 冻结路由使用上传通道时产生的供应商原生引用。
    native_reference: Option<String>,
    /// Content-addressed source revision verified before and after materialization.
    /// 在物化前后验证过的内容寻址源文件修订。
    source_fingerprint: PreparedMediaSourceFingerprint,
}

/// Identifies the source-file revision used to create one prepared media reference.
/// 标识创建媒体准备引用时所使用的源文件修订版本。
#[derive(Clone, Debug, Eq, PartialEq)]
struct PreparedMediaSourceFingerprint {
    /// Source byte length at the time the reference was remembered.
    /// 记住引用时的源文件字节长度。
    byte_len: u64,
    /// Last modification time when the host exposes it.
    /// 宿主可以提供时记录源文件最后修改时间。
    modified_at: Option<SystemTime>,
    /// SHA-256 digest of the exact source bytes.
    /// 精确源字节的 SHA-256 摘要。
    content_sha256: [u8; 32],
}

/// Streams and hashes the exact stable source revision used by one media reference.
/// 流式读取并哈希一个媒体引用所使用的精确稳定源文件修订。
///
/// # Parameters
/// 参数
/// `local_path` is the exact local file, `progress` observes monotonic bytes for the optional
/// pre-dispatch read, and `cancellation` cooperatively stops long media hashing.
/// `local_path` 是精确本地文件，`progress` 观察可选分派前读取的单调字节进度，
/// `cancellation` 协作终止长媒体哈希。
///
/// # Returns
/// 返回值
/// A content-addressed fingerprint only when metadata and byte length remain stable for the full
/// read, otherwise a controlled disappearance, mutation, I/O, or cancellation failure.
/// 仅当完整读取期间元数据与字节长度保持稳定时返回内容寻址指纹；否则返回受控的消失、
/// 变更、I/O 或取消失败。
async fn prepared_media_source_fingerprint(
    local_path: &Path,
    progress: Option<MediaUploadProgressCallback>,
    cancellation: Option<&MediaUploadCancellation>,
) -> Result<PreparedMediaSourceFingerprint, String> {
    let metadata = tokio::fs::metadata(local_path)
        .await
        .map_err(|error| format!("Media source metadata could not be read: {error}"))?;
    if !metadata.is_file() {
        return Err(String::from("Media source must be a file."));
    }
    let expected_byte_len = metadata.len();
    if expected_byte_len == 0 {
        return Err(String::from("Media source must be a non-empty file."));
    }
    let modified_at = metadata.modified().ok();
    let file = tokio::fs::File::open(local_path)
        .await
        .map_err(|error| format!("Media source could not be opened: {error}"))?;
    let mut source = tokio_util::io::ReaderStream::new(file);
    let mut content_hasher = Sha256::new();
    let mut processed = 0_u64;
    while let Some(chunk) = source.next().await {
        if cancellation.is_some_and(MediaUploadCancellation::is_cancelled) {
            return Err(String::from("media upload cancelled"));
        }
        let chunk = chunk.map_err(|error| format!("Media source could not be read: {error}"))?;
        processed = processed
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| String::from("Media source byte count overflowed."))?;
        if processed > expected_byte_len {
            return Err(String::from(
                "Media source changed while its content fingerprint was being read.",
            ));
        }
        content_hasher.update(chunk.as_ref());
        if let Some(callback) = progress.as_ref() {
            callback(processed, Some(expected_byte_len));
        }
    }
    if processed != expected_byte_len {
        return Err(String::from(
            "Media source changed while its content fingerprint was being read.",
        ));
    }
    let current_metadata = tokio::fs::metadata(local_path)
        .await
        .map_err(|_| String::from("Media source disappeared during preparation."))?;
    if !current_metadata.is_file()
        || current_metadata.len() != expected_byte_len
        || current_metadata.modified().ok() != modified_at
    {
        return Err(String::from(
            "Media source changed during preparation; please retry the submission.",
        ));
    }
    Ok(PreparedMediaSourceFingerprint {
        byte_len: expected_byte_len,
        modified_at,
        content_sha256: content_hasher.finalize().into(),
    })
}

/// Resolves the declaration that materializes one pre-dispatch conversation resource.
/// 解析物化一个分派前对话资源的声明。
///
/// # Parameters
/// 参数
/// `route` is the frozen route; `kind` and `mime_type` are the exact normalized resource facts.
/// `route` 是冻结路由；`kind` 与 `mime_type` 是精确归一化资源事实。
///
/// # Returns
/// 返回值
/// The selected channel, `None` for inline host materialization, or a closed reference failure.
/// 返回选中通道；宿主内联物化返回 `None`；引用错误返回封闭失败。
fn resolve_conversation_media_upload_channel<'a>(
    route: &'a ProviderExecutionRoute,
    kind: ResourceKind,
    mime_type: &str,
) -> Result<Option<&'a crate::UploadChannelDeclaration>, String> {
    let credential_profile = route
        .entry
        .credential_profiles
        .iter()
        .find(|profile| profile.id == route.connection.credential_profile_id)
        .ok_or_else(|| {
            format!(
                "Provider route credential profile `{}` is absent from service entry `{}`.",
                route.connection.credential_profile_id, route.entry.id
            )
        })?;
    let Some(constraint) =
        route
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| {
                constraint.resource_kind == kind && !constraint.upload_channel_refs.is_empty()
            })
    else {
        return Ok(None);
    };
    // Channel selection depends only on the frozen references, resource kind, and MIME allowlist;
    // byte and path checks remain inside the materializer before network dispatch.
    // 通道选择只依赖冻结引用、资源类别与 MIME 允许列表；字节与路径检查仍由物化器在网络
    // 分派前执行。
    let resource = crate::upload_engine::UploadResource {
        local_path: None,
        inline_bytes: None,
        mime_type: mime_type.to_string(),
        byte_len: 1,
        remote_url: None,
        item_count: 1,
    };
    crate::upload_engine::resolve_upload_channel(
        route.upload_channels.as_slice(),
        constraint.upload_channel_refs.as_slice(),
        &resource,
        credential_profile.credential_role.as_deref(),
    )
    .map(Some)
}

/// Builds the process-local cache key for one exact provider media projection.
/// 构建一个精确供应商媒体投影的进程内缓存键。
fn prepared_media_key(
    route: &ProviderExecutionRoute,
    model_id: &str,
    local_path: &Path,
    kind: ResourceKind,
    mime_type: &str,
) -> Option<String> {
    let credential_profile = route
        .entry
        .credential_profiles
        .iter()
        .find(|profile| profile.id == route.connection.credential_profile_id)?;
    let source = crate::upload_engine::UploadResource {
        local_path: Some(local_path.to_path_buf()),
        inline_bytes: None,
        mime_type: mime_type.to_string(),
        byte_len: 1,
        remote_url: None,
        item_count: 1,
    };
    let constraint = route
        .protocol_route
        .input_resource_constraints
        .iter()
        .find(|constraint| {
            constraint.resource_kind == kind && !constraint.upload_channel_refs.is_empty()
        })?;
    let channel = crate::upload_engine::resolve_upload_channel(
        route.upload_channels.as_slice(),
        constraint.upload_channel_refs.as_slice(),
        &source,
        credential_profile.credential_role.as_deref(),
    )
    .ok()?;
    channel.cache_duration_ms?;
    // material includes account, protocol, manifest generation, complete route revision, and
    // the resolved upload declaration so no hot update can reuse a stale provider reference.
    // material 同时包含账号、协议、Manifest 代际、完整路由修订与解析后的上传声明，确保
    // 热更新绝不会复用过期的供应商引用。
    let material = serde_json::to_vec(&(
        route.provider.id.as_str(),
        route.connection.id.as_str(),
        model_id,
        route.protocol_route.protocol_profile_id.as_str(),
        route.manifest_revision,
        route.manifest_generation,
        route.route_revision.as_str(),
        CONVERSATION_MEDIA_INPUT_OPERATION_ID,
        constraint,
        channel,
        kind,
        mime_type,
        local_path.to_string_lossy(),
    ))
    .ok()?;
    Some(format!("sha256:{:x}", Sha256::digest(material)))
}

/// Removes expired prepared media references and enforces the global process-local capacity.
/// 移除已过期媒体准备引用并强制执行进程内全局容量上限。
fn prune_prepared_media_cache(cache: &mut BTreeMap<String, PreparedMediaCacheEntry>, now: Instant) {
    cache.retain(|_, entry| entry.expires_at > now);
    while cache.len() > PREPARED_MEDIA_CACHE_MAX_ENTRIES {
        let Some(oldest_key) = cache
            .iter()
            .min_by(|(left_key, left), (right_key, right)| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left_key.cmp(right_key))
            })
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        cache.remove(&oldest_key);
    }
}

impl ProviderExecutionClient {
    /// Creates one execution client.
    /// 创建一个执行客户端。
    ///
    /// # Parameters
    /// 参数
    /// `http` is the shared HTTP client and `protocols` is the core-owned protocol runtime.
    /// `http` 是共享 HTTP 客户端，`protocols` 是由核心拥有的协议运行时。
    ///
    /// # Returns
    /// 返回值
    /// A reusable execution client.
    /// 返回可复用执行客户端。
    pub fn new<Source>(http: reqwest::Client, protocols: Source) -> Self
    where
        Source: ProtocolRuntimeSource,
    {
        Self {
            http,
            protocols: protocols.into_protocol_runtime(),
            prepared_media: Arc::new(StdMutex::new(BTreeMap::new())),
            responses_websocket_sessions: Arc::new(TokioMutex::new(BTreeMap::new())),
            responses_websocket_breakers: Arc::new(StdMutex::new(BTreeMap::new())),
        }
    }

    /// Uploads or pre-reads one local media resource before the conversation is materialized.
    /// 在会话物化前上传或预读取一个本地媒体资源。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `model_id`, and `secret` freeze the exact provider account and model contract.
    /// `route`、`model_id` 与 `secret` 固定精确的供应商账号及模型契约。
    /// `local_path` and `mime_type` identify the validated host attachment.
    /// `local_path` 与 `mime_type` 标识已校验的宿主附件。
    /// `progress` receives source-byte progress and `cancellation` owns the submit cancel path.
    /// `progress` 接收源字节进度，`cancellation` 持有提交取消路径。
    ///
    /// # Returns
    /// 返回值
    /// A prepared outcome containing the optional provider-native reference and the exact source
    /// fingerprint verified before and after materialization.
    /// 返回包含可选供应商原生引用的准备结果，以及在物化前后验证过的精确源文件指纹。
    // Keep route, model, credential, media, progress, and cancellation boundaries explicit for callers.
    // 保持路由、模型、凭据、媒体、进度和取消边界显式，便于调用方准确控制生命周期。
    #[allow(clippy::too_many_arguments)]
    pub async fn prepare_local_media(
        &self,
        route: &ProviderExecutionRoute,
        model_id: &str,
        secret: &CredentialExecutionSecret,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        progress: Option<MediaUploadProgressCallback>,
        cancellation: &MediaUploadCancellation,
    ) -> Result<PreparedConversationMedia, String> {
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        // The request must never bind a provider reference to a source that changed during preparation.
        // 请求绝不能把供应商引用绑定到准备期间已经发生变化的源文件。
        let source_fingerprint =
            prepared_media_source_fingerprint(local_path, None, Some(cancellation)).await?;
        let native_reference = if let Some(channel) =
            resolve_conversation_media_upload_channel(route, kind, mime_type)?
        {
            // The frozen route selects a bounded upload channel by resource kind and ordered
            // channel references; provider identity never participates in dispatch.
            // 冻结路由按资源类型与有序通道引用选择有界上传通道；供应商身份不参与分派。
            let resource = crate::upload_engine::UploadResource {
                local_path: Some(local_path.to_path_buf()),
                inline_bytes: None,
                mime_type: mime_type.to_string(),
                byte_len: source_fingerprint.byte_len,
                remote_url: None,
                item_count: 1,
            };
            let credential_profile = route
                .entry
                .credential_profiles
                .iter()
                .find(|profile| profile.id == route.connection.credential_profile_id)
                .ok_or_else(|| {
                    format!(
                        "Provider route credential profile `{}` is absent from service entry `{}`.",
                        route.connection.credential_profile_id, route.entry.id
                    )
                })?;
            Some(
                crate::upload_engine::materialize_upload(
                    crate::upload_engine::UploadExecutionContext {
                        http: &self.http,
                        entry: &route.entry,
                        secret,
                        credential_profile,
                        progress: progress.clone(),
                        cancellation,
                        model_id: Some(model_id),
                    },
                    channel,
                    &resource,
                )
                .await?
                .reference,
            )
        } else {
            None
        };
        if cancellation.is_cancelled() {
            return Err(String::from("media upload cancelled"));
        }
        // Inline materialization reports progress during the final verified read; provider-native
        // channels already reported transfer progress and therefore verify without a second cycle.
        // 内联物化在最终校验读取期间报告进度；供应商原生通道已经报告传输进度，因此后续
        // 校验不再产生第二轮进度。
        let verification_progress = if native_reference.is_none() {
            progress
        } else {
            None
        };
        let current_source_fingerprint = prepared_media_source_fingerprint(
            local_path,
            verification_progress,
            Some(cancellation),
        )
        .await?;
        if current_source_fingerprint != source_fingerprint {
            return Err(String::from(
                "Media source changed during preparation; please retry the submission.",
            ));
        }
        Ok(PreparedConversationMedia {
            native_reference,
            source_fingerprint: current_source_fingerprint,
        })
    }

    /// Resolves the exact non-secret upload-channel identity used for one conversation resource.
    /// 解析一个对话资源使用的精确非秘密上传通道身份。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the immutable execution route; `kind` and `mime_type` identify the exact resource.
    /// `route` 是不可变执行路由；`kind` 与 `mime_type` 标识精确资源。
    ///
    /// # Returns
    /// 返回值
    /// The manifest channel id, `None` for host inline materialization, or a closed route error.
    /// 返回 Manifest 通道标识；宿主内联物化返回 `None`；路由错误返回封闭错误。
    pub fn conversation_media_upload_channel_id(
        &self,
        route: &ProviderExecutionRoute,
        kind: ResourceKind,
        mime_type: &str,
    ) -> Result<Option<String>, String> {
        resolve_conversation_media_upload_channel(route, kind, mime_type)
            .map(|channel| channel.map(|channel| channel.upload_channel_id.clone()))
    }

    /// Remembers one provider-native reference for the exact route and local source.
    /// 为精确路由与本地源记住一个供应商原生引用。
    pub fn remember_prepared_media(
        &self,
        route: &ProviderExecutionRoute,
        model_id: &str,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
        prepared: PreparedConversationMedia,
    ) {
        let PreparedConversationMedia {
            native_reference: Some(url),
            source_fingerprint,
        } = prepared
        else {
            return;
        };
        let resource = crate::upload_engine::UploadResource {
            local_path: Some(local_path.to_path_buf()),
            inline_bytes: None,
            mime_type: mime_type.to_string(),
            byte_len: source_fingerprint.byte_len,
            remote_url: None,
            item_count: 1,
        };
        let Some(credential_profile) = route
            .entry
            .credential_profiles
            .iter()
            .find(|profile| profile.id == route.connection.credential_profile_id)
        else {
            return;
        };
        let credential_role = credential_profile.credential_role.as_deref();
        let cache_duration_ms = route
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| {
                constraint.resource_kind == kind && !constraint.upload_channel_refs.is_empty()
            })
            .and_then(|constraint| {
                crate::upload_engine::resolve_upload_channel(
                    route.upload_channels.as_slice(),
                    constraint.upload_channel_refs.as_slice(),
                    &resource,
                    credential_role,
                )
                .ok()
            })
            .and_then(|channel| channel.cache_duration_ms);
        // An absent cache declaration intentionally disables reuse; the host must not invent
        // a lifetime that can outlive the provider-issued reference.
        // 缺少缓存声明时应有意禁用复用；宿主不得虚构可能超过供应商引用有效期的生命周期。
        let Some(cache_duration_ms) = cache_duration_ms else {
            return;
        };
        let Some(key) = prepared_media_key(route, model_id, local_path, kind, mime_type) else {
            return;
        };
        let now = Instant::now();
        let Some(expires_at) = now.checked_add(Duration::from_millis(cache_duration_ms)) else {
            return;
        };
        if let Ok(mut cache) = self.prepared_media.lock() {
            cache.insert(
                key,
                PreparedMediaCacheEntry {
                    url,
                    expires_at,
                    created_at: now,
                    source_fingerprint,
                },
            );
            prune_prepared_media_cache(&mut cache, now);
        }
    }

    /// Returns a still-valid provider-native reference for the exact local source.
    /// 返回精确本地源仍然有效的供应商原生引用。
    pub async fn prepared_media_url(
        &self,
        route: &ProviderExecutionRoute,
        model_id: &str,
        local_path: &Path,
        kind: ResourceKind,
        mime_type: &str,
    ) -> Option<String> {
        let source_fingerprint = prepared_media_source_fingerprint(local_path, None, None)
            .await
            .ok()?;
        let key = prepared_media_key(route, model_id, local_path, kind, mime_type)?;
        let mut cache = self.prepared_media.lock().ok()?;
        prune_prepared_media_cache(&mut cache, Instant::now());
        let entry = cache.get(&key)?.clone();
        if entry.source_fingerprint != source_fingerprint {
            cache.remove(&key);
            return None;
        }
        Some(entry.url)
    }

    /// Executes one frozen provider-native tool target and normalizes its output.
    /// 执行一个已冻结的供应商原生工具目标并归一化其输出。
    ///
    /// # Parameters
    /// 参数
    /// `entry`, `target`, and `secret` are immutable snapshots resolved before invocation;
    /// `arguments` already match the selected target's exact AI-visible schema.
    /// `entry`、`target` 与 `secret` 是调用前解析的不可变快照；
    /// `arguments` 已符合选定目标的精确 AI 可见 Schema。
    ///
    /// # Returns
    /// 返回值
    /// A unified typed result or an explicit provider/contract failure.
    /// 返回统一强类型结果或明确的供应商与契约错误。
    pub async fn execute_provider_tool(
        &self,
        entry: &ServiceEntry,
        target: &ProviderToolTarget,
        arguments: Value,
        secret: &CredentialExecutionSecret,
    ) -> Result<ProviderToolExecutionResult, String> {
        if target.service_entry_id != entry.id {
            return Err(format!(
                "Provider tool target `{}` does not belong to service entry `{}`.",
                target.target_id, entry.id
            ));
        }
        let arguments = crate::provider_tool_execution::canonicalize_provider_tool_arguments(
            target, arguments,
        )?;
        crate::provider_tool_execution::execute_provider_tool(
            &self.http,
            entry,
            target,
            &[],
            &[],
            arguments,
            secret,
        )
        .await
    }

    /// Executes one provider tool through frozen operation request and response profiles.
    /// 通过冻结的操作请求与响应 Profile 执行一个供应商工具。
    ///
    /// # Parameters
    /// 参数
    /// `entry`, `target`, and `secret` freeze the route; `request_profiles` and
    /// `response_profiles` come from the same manifest generation; `arguments` are canonical
    /// operation inputs after upload materialization.
    /// `entry`、`target` 与 `secret` 冻结路由；`request_profiles` 与 `response_profiles` 来自
    /// 同一清单代际；`arguments` 是上传物化后的规范操作输入。
    ///
    /// # Returns
    /// 返回值
    /// A normalized result or a controlled ownership, profile, transport, or contract failure.
    /// 返回归一化结果，或受控的归属、Profile、传输与契约失败。
    pub(crate) async fn execute_provider_tool_with_profiles(
        &self,
        entry: &ServiceEntry,
        target: &ProviderToolTarget,
        request_profiles: &[crate::provider_manifest::RequestProjectionProfile],
        response_profiles: &[crate::provider_manifest::ResponseProjectionProfile],
        arguments: Value,
        secret: &CredentialExecutionSecret,
    ) -> Result<ProviderToolExecutionResult, String> {
        if target.service_entry_id != entry.id {
            return Err(format!(
                "Provider tool target `{}` does not belong to service entry `{}`.",
                target.target_id, entry.id
            ));
        }
        crate::provider_tool_execution::execute_provider_tool(
            &self.http,
            entry,
            target,
            request_profiles,
            response_profiles,
            arguments,
            secret,
        )
        .await
    }

    /// Encodes one immutable execution request without dispatching it.
    /// 编码一个不可变执行请求，但不进行网络分派。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the frozen provider route and `request` is the provider-neutral request.
    /// `route` 是冻结的供应商路由，`request` 是供应商无关请求。
    ///
    /// # Returns
    /// 返回值
    /// The exact projected wire request or a controlled validation failure.
    /// 返回精确投影的线级请求或受控校验失败。
    pub fn encode_request(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        validate_request_policy(route, request)?;
        validate_request_projection_guards_before_encoding(route, request)?;
        // The extracted LLM core owns the canonical Codex Responses request body.
        // 提取后的 LLM 核心拥有规范 Codex Responses 请求正文的唯一编码权。
        let encoded = encode_codex_responses_request(request).map_err(|error| {
            pre_dispatch_failure(format!("LLM core request encoding failed: {error}"))
        })?;
        let encoded = project_responses_baseline_for_provider(route, request, encoded)?;
        project_provider_request(route, request, encoded)
    }

    /// Encodes one immutable request with credential-bound non-secret identity projection.
    /// 编码一个不可变请求，并投影凭据绑定的非秘密身份信息。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` form the public route projection; `secret` supplies only the
    /// account metadata required in the wire body.
    /// `route` 与 `request` 构成公共路由投影；`secret` 仅提供线级请求体所需的账号元数据。
    ///
    /// # Returns
    /// 返回值
    /// The exact request body used by dispatch and wire preview, without authentication headers.
    /// 返回分派与线级预览共同使用的精确请求体，不包含认证请求头。
    pub fn encode_request_with_credential(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        secret: &CredentialExecutionSecret,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        let encoded = self.encode_request(route, request)?;
        project_credential_request(route, secret, encoded)
    }

    /// Encodes the exact credential-projected wire body selected for this route and session.
    /// 编码由当前路由与会话选中的精确凭据投影 Wire 正文。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the frozen provider route and `request` is the immutable canonical request.
    /// `route` 是冻结的供应商路由，`request` 是不可变规范请求。
    /// `secret` supplies the same credential-bound body projection used by real dispatch.
    /// `secret` 提供与真实分派相同的凭据绑定正文投影。
    ///
    /// # Returns
    /// 返回值
    /// The exact credential-projected request body selected by current transport state.
    /// 返回由当前传输状态选中的精确凭据投影请求体。
    pub async fn encode_effective_request_with_credential(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        secret: &CredentialExecutionSecret,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        let websocket_key = responses_websocket_cache_key(route, request, secret);
        let websocket_allowed = websocket_key.as_ref().is_some_and(|key| {
            route.protocol_route.responses_websocket_enabled()
                && upstream_protocol_for_route(route) == Some(UpstreamProtocol::OpenAiResponses)
                && !self.responses_websocket_disabled(key)
        });
        let effective_request = if websocket_allowed {
            let existing = {
                let mut sessions = self.responses_websocket_sessions.lock().await;
                sessions
                    .get_mut(
                        websocket_key
                            .as_deref()
                            .expect("allowed route has a cache key"),
                    )
                    .map(|entry| {
                        entry.last_used_at = Instant::now();
                        Arc::clone(&entry.session)
                    })
            };
            if let Some(existing) = existing {
                let session = existing.lock().await;
                if session.replacement_required() {
                    request_without_nonpersistent_responses_continuation(route, request)
                } else {
                    responses_websocket_request_for_active_session(
                        request,
                        session.previous_response_id(),
                    )
                }
            } else {
                request_without_nonpersistent_responses_continuation(route, request)
            }
        } else {
            request_without_nonpersistent_responses_continuation(route, request)
        };
        self.encode_request_with_credential(route, &effective_request, secret)
    }

    /// Executes the standalone Codex Responses compaction endpoint without streaming.
    /// 执行独立 Codex Responses 远端压缩端点，且不使用流式传输。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` are the exact immutable provider route and conversation projection.
    /// `route` 与 `request` 是精确不可变供应商路由和会话投影。
    /// `secret` supplies only the credential-bound authentication projection.
    /// `secret` 仅提供凭据绑定的鉴权投影。
    ///
    /// # Returns
    /// 返回值
    /// A bounded result whose ordered output items can be atomically persisted as a route-bound checkpoint.
    /// 一份有界结果；其有序输出项可作为路由绑定检查点原子持久化。
    pub async fn execute_responses_compact(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
    ) -> Result<ResponsesCompactResult, ProviderExecutionFailure> {
        // traceStartedAt measures the complete unary compact attempt, including pre-dispatch validation.
        // traceStartedAt 衡量完整的一元压缩尝试，包括分派前校验。
        let trace_started_at = Instant::now();
        // traceId reuses the non-secret cache scope so concurrent session compactions remain distinguishable.
        // traceId 复用不含密钥的缓存作用域，使并发会话压缩仍可区分。
        let trace_id = request
            .cache_scope_id
            .clone()
            .unwrap_or_else(|| String::from("responses-compact"));
        emit_provider_transport_trace(
            trace_id.as_str(),
            "responses_compact",
            "execution_started",
            &trace_started_at,
        );
        // result keeps every early failure inside the same secret-safe diagnostic boundary.
        // result 将所有提前失败保留在同一个不含密钥的诊断边界内。
        let result = async {
            if route.protocol_route.protocol_profile_id != OPENAI_CODEX_RESPONSES_PROFILE_ID {
                return Err(pre_dispatch_failure(
                    "Standalone remote compaction requires the Codex Responses protocol.",
                ));
            }
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "route_validated",
                &trace_started_at,
            );
            // profile is the exact registered Codex Responses implementation used for both encoding and transport limits.
            // profile 是同时用于编码与传输限制的精确已注册 Codex Responses 实现。
            let transport = self
                .protocols
                .transport_profile(route.protocol_route.protocol_profile_id.as_str())
                .map_err(llm_core_protocol_failure)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "protocol_ready",
                &trace_started_at,
            );
            validate_request_policy(&route, &request)?;
            validate_request_projection_guards_before_encoding(&route, &request)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "request_policy_validated",
                &trace_started_at,
            );
            // encoded preserves the normal request projection order before the compact-only field filter runs.
            // encoded 在压缩专用字段过滤执行前保留普通请求的投影顺序。
            let encoded = encode_codex_responses_request(&request).map_err(|error| {
                pre_dispatch_failure(format!("LLM core request encoding failed: {error}"))
            })?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "protocol_encoded",
                &trace_started_at,
            );
            // encoded applies provider-owned model and request overrides without credential material.
            // encoded 应用供应商持有的模型与请求覆盖，但不包含凭据材料。
            let encoded = project_provider_request(&route, &request, encoded)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "provider_projected",
                &trace_started_at,
            );
            // encoded adds the exact credential projection only after all public request checks pass.
            // encoded 仅在全部公共请求检查通过后加入精确凭据投影。
            let mut encoded = project_credential_request(&route, &secret, encoded)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "credential_projected",
                &trace_started_at,
            );
            if encoded.path != "/responses" {
                return Err(pre_dispatch_failure(format!(
                    "Codex Responses compaction requires the `/responses` endpoint, got `{}`.",
                    encoded.path
                )));
            }
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "endpoint_validated",
                &trace_started_at,
            );
            prepare_responses_compact_body(&mut encoded.body)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "body_prepared",
                &trace_started_at,
            );
            encoded.path = String::from("/responses/compact");
            let url = route.request_url(encoded.path.as_str());
            let headers = execution_headers(&route, &encoded.headers, &secret)?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "headers_ready",
                &trace_started_at,
            );
            // transport freezes the same profile's response limits for the unary HTTP call.
            // transport 为一元 HTTP 调用冻结同一协议实现的响应限制。
            // compactRequestStartedAt and compactRequestTimeout bound both headers and the complete body.
            // compactRequestStartedAt 与 compactRequestTimeout 同时约束响应头和完整正文。
            let compact_request_started_at = Instant::now();
            let compact_request_timeout = Duration::from_millis(
                transport
                    .stream_idle_timeout_ms
                    .saturating_mul(u64::from(RESPONSES_COMPACT_TIMEOUT_IDLE_MULTIPLIER)),
            );
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "transport_ready",
                &trace_started_at,
            );
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "request_dispatched",
                &trace_started_at,
            );
            let response = tokio::time::timeout(
                Duration::from_millis(transport.response_headers_timeout_ms),
                self.http
                    .post(url.as_str())
                    .headers(headers)
                    .json(&encoded.body)
                    .send(),
            )
            .await
            .map_err(|_| {
                transport_failure(format!(
                    "Provider compaction request to `{url}` exceeded the registered {} ms response-header timeout.",
                    transport.response_headers_timeout_ms
                ))
            })?
            .map_err(|error| {
                http_send_transport_failure(
                    &error,
                    format!("Provider compaction request to `{url}` failed: {error}"),
                )
            })?;
            emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "response_headers_received",
                &trace_started_at,
            );
            let status = response.status();
            emit_provider_transport_http_status_trace(
                trace_id.as_str(),
                "responses_compact",
                status.as_u16(),
                &trace_started_at,
            );
            if !status.is_success() {
                let retry_after = parse_retry_after_headers(response.headers(), SystemTime::now());
                let body = read_responses_compact_body_with_timeout(
                    response,
                    transport.max_error_body_bytes,
                    url.as_str(),
                    status.as_u16(),
                    retry_after.clone(),
                    compact_request_started_at,
                    compact_request_timeout,
                )
                .await?;
                let body = project_response_record_text(
                    &route.response_projection_profiles,
                    body.as_str(),
                )
                .map_err(protocol_failure)?;
                return Err(http_failure(status.as_u16(), retry_after, body.as_str()));
            }
            let content_type = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            if !content_type
                .split(';')
                .next()
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
            {
                return Err(compact_protocol_failure(format!(
                    "Provider compaction response from `{url}` must use application/json, got `{content_type}`."
                )));
            }
            let body = read_responses_compact_body_with_timeout(
                response,
                transport.max_frame_bytes,
                url.as_str(),
                status.as_u16(),
                None,
                compact_request_started_at,
                compact_request_timeout,
            )
            .await?;
            let mut value: Value = serde_json::from_str(body.as_str()).map_err(|error| {
                compact_protocol_failure(format!(
                    "Provider compaction response from `{url}` is invalid JSON: {error}"
                ))
            })?;
            crate::provider_manifest::validate_runtime_json_bounds(
                &value,
                "Provider compaction response",
            )
            .map_err(|error| compact_protocol_failure(error.to_string()))?;
            apply_response_projection_profiles(&route.response_projection_profiles, &mut value)
                .map_err(protocol_failure)?;
            emit_responses_compact_response_shape_trace(
                trace_id.as_str(),
                &value,
                &trace_started_at,
            );
            decode_responses_compact_result(&value, url.as_str())
        }
        .await;
        // result crosses the same exact-secret redaction boundary as streaming execution errors.
        // result 与流式执行错误经过同一条精确秘密脱敏边界。
        let result =
            result.map_err(|failure| sanitize_provider_execution_failure(failure, &secret));
        match &result {
            Ok(_) => emit_provider_transport_trace(
                trace_id.as_str(),
                "responses_compact",
                "execution_finished",
                &trace_started_at,
            ),
            Err(failure) => emit_provider_transport_failure_trace(
                trace_id.as_str(),
                "responses_compact",
                failure,
                &trace_started_at,
            ),
        }
        result
    }

    /// Counts one complete Gemini API request with the provider-authoritative tokenizer.
    /// 使用供应商权威 Tokenizer 统计一个完整 Gemini API 请求。
    ///
    /// # Parameters
    /// 参数
    /// `route` must be the verified public Gemini GenerateContent route, `request` is the
    /// immutable provider-neutral request, and `secret` is its exact selected credential.
    /// `route` 必须是已验证的公共 Gemini GenerateContent 路由，`request` 是不可变的
    /// 供应商无关请求，`secret` 是其精确选定凭据。
    ///
    /// # Returns
    /// 返回值
    /// The exact provider count and modality details, or a controlled pre-dispatch,
    /// transport, HTTP, or protocol failure.
    /// 返回精确供应商统计与模态明细，或受控的分派前、传输、HTTP 或协议错误。
    pub async fn count_input_tokens(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        secret: &CredentialExecutionSecret,
    ) -> Result<InputTokenCount, ProviderExecutionFailure> {
        if route.protocol_route.protocol_profile_id
            != vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID
        {
            return Err(pre_dispatch_failure(
                "Provider-authoritative CountTokens is only registered for the public Gemini GenerateContent route.",
            ));
        }
        validate_request_policy(route, request)?;
        let transport = self
            .protocols
            .transport_profile(route.protocol_route.protocol_profile_id.as_str())
            .map_err(llm_core_protocol_failure)?;
        let encoded = encode_gemini_count_tokens_request(request).map_err(protocol_failure)?;
        let url = route.request_url(encoded.path.as_str());
        let headers = execution_headers(route, &encoded.headers, secret)?;
        let response = tokio::time::timeout(
            Duration::from_millis(transport.response_headers_timeout_ms),
            self.http
                .post(url.as_str())
                .headers(headers)
                .json(&encoded.body)
                .send(),
        )
        .await
        .map_err(|_| {
            transport_failure(format!(
                "Provider CountTokens request to `{url}` exceeded the registered {} ms response-header timeout.",
                transport.response_headers_timeout_ms
            ))
        })?
        .map_err(|error| {
            http_send_transport_failure(
                &error,
                format!("Provider CountTokens request to `{url}` failed: {error}"),
            )
        })?;
        let status = response.status();
        if !status.is_success() {
            let retry_after = parse_retry_after_headers(response.headers(), SystemTime::now());
            let body = read_bounded_response_text(
                response,
                transport.max_error_body_bytes,
                url.as_str(),
                status.as_u16(),
                retry_after.clone(),
                Duration::from_millis(transport.stream_idle_timeout_ms),
            )
            .await?;
            return Err(sanitize_provider_execution_failure(
                http_failure(status.as_u16(), retry_after, body.as_str()),
                secret,
            ));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !content_type
            .split(';')
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
        {
            return Err(response_headers_protocol_failure(format!(
                "Provider CountTokens response from `{url}` must use application/json, got `{content_type}`."
            )));
        }
        let body = read_bounded_response_text(
            response,
            transport.max_frame_bytes,
            url.as_str(),
            status.as_u16(),
            None,
            Duration::from_millis(transport.stream_idle_timeout_ms),
        )
        .await?;
        let body = serde_json::from_str::<Value>(&body).map_err(|error| {
            protocol_failure(ProtocolConversionError::new(format!(
                "Gemini CountTokens response JSON is invalid: {error}"
            )))
        })?;
        crate::provider_manifest::validate_runtime_json_bounds(
            &body,
            "Gemini CountTokens response",
        )
        .map_err(|error| protocol_failure(ProtocolConversionError::new(error)))?;
        decode_gemini_count_tokens_response(&body).map_err(protocol_failure)
    }

    /// Starts one immutable execution without protocol or account fallback.
    /// 启动一次不可变执行，不进行协议或账号回退。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the resolved immutable route, `request` is the public request,
    /// `secret` is the exact host-loaded credential, and `execution_id` is stable.
    /// `route` 是解析后的不可变路由，`request` 是公共请求，`secret` 是宿主加载的精确凭据，
    /// `execution_id` 是稳定标识。
    ///
    /// # Returns
    /// 返回值
    /// A normalized event stream.
    /// 返回归一化事件流。
    pub fn execute(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
    ) -> ProviderExecutionStream {
        // diagnostic_secret remains available only to the final stream error boundary after the
        // transport receives its own frozen credential clone.
        // diagnostic_secret 仅供最终流错误边界使用，传输层接收其独立的冻结凭据副本。
        let diagnostic_secret = secret.clone();
        let websocket_key = responses_websocket_cache_key(&route, &request, &secret);
        let websocket_allowed = websocket_key.as_ref().is_some_and(|key| {
            route.protocol_route.responses_websocket_enabled()
                && upstream_protocol_for_route(&route) == Some(UpstreamProtocol::OpenAiResponses)
                && !self.responses_websocket_disabled(key)
        });
        // stream selects exactly one declared transport before the credential leaves this boundary.
        // stream 在凭据离开当前边界前精确选择一个已声明传输。
        let stream = if websocket_allowed {
            self.execute_responses_websocket(
                route,
                request,
                secret,
                execution_id,
                websocket_key.expect("allowed WebSocket route has a cache key"),
            )
        } else {
            self.execute_http(route, request, secret, execution_id)
        };
        // Every asynchronous error crosses one common secret-safe and bounded public boundary.
        // 每个异步错误都经过同一条不含秘密且有界的公共边界。
        Box::pin(stream.map(move |result| {
            result
                .map_err(|failure| sanitize_provider_execution_failure(failure, &diagnostic_secret))
        }))
    }

    /// Executes one explicitly enabled Responses route over a non-multiplexed WebSocket.
    /// 通过不复用并发响应的 WebSocket 执行一条显式启用的 Responses 路由。
    fn execute_responses_websocket(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
        websocket_key: String,
    ) -> ProviderExecutionStream {
        let client = self.clone();
        Box::pin(try_stream! {
            let transport_started_at = Instant::now();
            let dispatched_at_ms = provider_transport_unix_millis()?;
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "execution_started",
                &transport_started_at,
            );
            // Handshake projection must never depend on a non-persistent continuation cursor.
            // 握手投影绝不能依赖非持久化的延续游标。
            let handshake_request = request_without_nonpersistent_responses_continuation(
                &route,
                &request,
            );
            let handshake_encoded = client.encode_request_with_credential(
                &route,
                &handshake_request,
                &secret,
            )?;
            let transport = client
                .protocols
                .transport_profile(route.protocol_route.protocol_profile_id.as_str())
                .map_err(llm_core_protocol_failure)?;
            let url = responses_websocket_url(&route, handshake_encoded.path.as_str())?;
            let mut headers = execution_headers(&route, &handshake_encoded.headers, &secret)?;
            headers.insert(
                HeaderName::from_static("openai-beta"),
                HeaderValue::from_static(CODEX_RESPONSES_WEBSOCKET_BETA),
            );
            let config = ResponsesWebsocketConfig {
                url,
                headers: websocket_headers(&headers)?,
                connect_timeout: Duration::from_millis(transport.response_headers_timeout_ms),
                idle_timeout: Duration::from_millis(transport.stream_idle_timeout_ms),
                max_connection_age: DEFAULT_RESPONSES_WEBSOCKET_MAX_AGE,
                max_frame_bytes: transport.max_frame_bytes,
            };
            let session_handle = match client
                .responses_websocket_session(websocket_key.as_str(), config.clone())
                .await
            {
                Ok(session) => session,
                Err(error) => {
                    client.disable_responses_websocket(websocket_key.as_str());
                    Err(trace_provider_transport_failure(
                        execution_id.as_str(),
                        "responses_websocket",
                        stream_transport_failure_with_websocket_close_code(
                            error.message,
                            false,
                            error.websocket_close_code,
                        ),
                        &transport_started_at,
                    ))?
                }
            };
            let mut session = session_handle.lock().await;
            if session.replacement_required() {
                let _ = session.close().await;
                *session = match ResponsesWebsocketSession::connect(config).await {
                    Ok(session) => session,
                    Err(error) => {
                        client.disable_responses_websocket(websocket_key.as_str());
                        Err(trace_provider_transport_failure(
                            execution_id.as_str(),
                            "responses_websocket",
                            stream_transport_failure_with_websocket_close_code(
                                error.message,
                                false,
                                error.websocket_close_code,
                            ),
                            &transport_started_at,
                        ))?
                    }
                };
            }
            // A store=false cursor is valid only when this exact active socket proves ownership.
            // store=false 游标仅在当前精确活动 Socket 证明其归属时才有效。
            let wire_request = responses_websocket_request_for_active_session(
                &request,
                session.previous_response_id(),
            );
            let encoded = client.encode_request_with_credential(&route, &wire_request, &secret)?;
            if let Err(error) = session.begin_response(encoded.body, None).await {
                client.disable_responses_websocket(websocket_key.as_str());
                Err(trace_provider_transport_failure(
                    execution_id.as_str(),
                    "responses_websocket",
                    stream_transport_failure_with_websocket_close_code(
                        error.message,
                        false,
                        error.websocket_close_code,
                    ),
                    &transport_started_at,
                ))?;
            }
            yield AiStreamEvent::TransportObserved {
                transport: AiTransportKind::ResponsesWebsocket,
                dispatched_at_ms,
                headers_at_ms: None,
                http_status: None,
                provider_request_id: None,
                rate_limit_headers: BTreeMap::new(),
            };
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "request_dispatched",
                &transport_started_at,
            );
            let mut decoder = client
                .protocols
                .create_stream_decoder(
                    route.protocol_route.protocol_profile_id.as_str(),
                    execution_id.clone(),
                    &request,
                )
                .map_err(llm_core_protocol_failure)?;
            let usage_profiles = route.protocol_route.usage_extraction_profiles.clone();
            let response_profiles = route.response_projection_profiles.clone();
            let profiled_usage_enabled = !usage_profiles.is_empty();
            let mut usage_accumulator = UsageAccumulator::default();
            let mut profiled_usage_gate = ProfiledUsageEventGate::default();
            let mut semantic_output_observed = false;
            let mut stream_state = AiStreamStateMachine::default();
            let mut terminal_observed = false;
            loop {
                let mut frame = match session.next_frame().await {
                    Ok(Some(frame)) => frame,
                    Ok(None) => {
                        client.disable_responses_websocket(websocket_key.as_str());
                        Err(trace_provider_transport_failure(
                            execution_id.as_str(),
                            "responses_websocket",
                            stream_transport_failure_with_websocket_close_code(
                                "Responses WebSocket closed without a terminal event.",
                                semantic_output_observed,
                                None,
                            ),
                            &transport_started_at,
                        ))?
                    }
                    Err(error) => {
                        client.disable_responses_websocket(websocket_key.as_str());
                        Err(trace_provider_transport_failure(
                            execution_id.as_str(),
                            "responses_websocket",
                            stream_transport_failure_with_websocket_close_code(
                                error.message,
                                semantic_output_observed,
                                error.websocket_close_code,
                            ),
                            &transport_started_at,
                        ))?
                    }
                };
                apply_response_projection_profiles(&response_profiles, &mut frame.value).map_err(
                    |error| {
                        abandon_responses_websocket_attempt(
                            &mut session,
                            stream_protocol_failure_for_transport(
                                error,
                                semantic_output_observed,
                                AiTransportKind::ResponsesWebsocket,
                            ),
                        )
                    },
                )?;
                if let Some((code, message)) = responses_websocket_retry_control(&frame.value) {
                    Err(responses_websocket_control_failure(
                        code,
                        message,
                        semantic_output_observed,
                    ))?;
                }
                let mut profiled_usage_events = extract_profiled_usage_value_events(
                    &usage_profiles,
                    UsageRecordKind::WebSocketFrame,
                    &frame.value,
                    &mut usage_accumulator,
                )
                .map_err(|error| {
                    abandon_responses_websocket_attempt(
                        &mut session,
                        stream_protocol_failure_for_transport(
                            error,
                            semantic_output_observed,
                            AiTransportKind::ResponsesWebsocket,
                        ),
                    )
                })?;
                for usage_event in &mut profiled_usage_events {
                    apply_local_cost_estimate(&route, usage_event);
                }
                for mut usage_event in profiled_usage_gate
                    .stage(profiled_usage_events)
                    .map_err(|error| {
                        abandon_responses_websocket_attempt(
                            &mut session,
                            stream_protocol_failure_for_transport(
                                error,
                                semantic_output_observed,
                                AiTransportKind::ResponsesWebsocket,
                            ),
                        )
                    })?
                {
                    validate_stream_event(
                        &mut stream_state,
                        &usage_event,
                        semantic_output_observed,
                    )
                    .map_err(|failure| {
                        abandon_responses_websocket_attempt(&mut session, failure)
                    })?;
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut usage_event,
                    );
                    yield usage_event;
                }
                for mut event in decoder
                    .push_json(&frame.value)
                    .map_err(|error| {
                        abandon_responses_websocket_attempt(
                            &mut session,
                            stream_protocol_failure_for_transport(
                                error,
                                semantic_output_observed,
                                AiTransportKind::ResponsesWebsocket,
                            ),
                        )
                    })?
                {
                    if profiled_usage_enabled
                        && matches!(event, AiStreamEvent::UsageUpdated { .. })
                    {
                        continue;
                    }
                    apply_local_cost_estimate(&route, &mut event);
                    let was_semantic_output_observed = semantic_output_observed;
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut event,
                    );
                    if semantic_output_observed && !was_semantic_output_observed {
                        emit_provider_transport_trace(
                            execution_id.as_str(),
                            "responses_websocket",
                            "first_semantic_event",
                            &transport_started_at,
                        );
                    }
                    terminal_observed |= matches!(
                        event,
                        AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
                    );
                    validate_stream_event(&mut stream_state, &event, semantic_output_observed)
                        .map_err(|failure| {
                            abandon_responses_websocket_attempt(&mut session, failure)
                        })?;
                    let released_usage = profiled_usage_gate.observe(&event);
                    yield event;
                    for mut usage_event in released_usage {
                        validate_stream_event(
                            &mut stream_state,
                            &usage_event,
                            semantic_output_observed,
                        )
                        .map_err(|failure| {
                            abandon_responses_websocket_attempt(&mut session, failure)
                        })?;
                        update_semantic_output_observation(
                            &mut semantic_output_observed,
                            &mut usage_event,
                        );
                        yield usage_event;
                    }
                }
                if frame.completed || frame.failed {
                    break;
                }
            }
            for mut event in decoder
                .finish()
                .map_err(|error| {
                    abandon_responses_websocket_attempt(
                        &mut session,
                        stream_completion_failure_for_transport(
                            error,
                            semantic_output_observed,
                            AiTransportKind::ResponsesWebsocket,
                        ),
                    )
                })?
            {
                if profiled_usage_enabled && matches!(event, AiStreamEvent::UsageUpdated { .. }) {
                    continue;
                }
                apply_local_cost_estimate(&route, &mut event);
                update_semantic_output_observation(&mut semantic_output_observed, &mut event);
                terminal_observed |= matches!(
                    event,
                    AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
                );
                validate_stream_event(&mut stream_state, &event, semantic_output_observed).map_err(
                    |failure| abandon_responses_websocket_attempt(&mut session, failure),
                )?;
                let released_usage = profiled_usage_gate.observe(&event);
                yield event;
                for mut usage_event in released_usage {
                    validate_stream_event(
                        &mut stream_state,
                        &usage_event,
                        semantic_output_observed,
                    )
                    .map_err(|failure| {
                        abandon_responses_websocket_attempt(&mut session, failure)
                    })?;
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut usage_event,
                    );
                    yield usage_event;
                }
            }
            profiled_usage_gate
                .ensure_released()
                .map_err(|error| {
                    abandon_responses_websocket_attempt(
                        &mut session,
                        stream_completion_failure_for_transport(
                            error,
                            semantic_output_observed,
                            AiTransportKind::ResponsesWebsocket,
                        ),
                    )
                })?;
            if !terminal_observed || !stream_state.is_terminal() {
                Err(abandon_responses_websocket_attempt(
                    &mut session,
                    stream_completion_failure_for_transport(
                        ProtocolConversionError::new(
                            "Responses WebSocket ended without a normalized terminal event.",
                        ),
                        semantic_output_observed,
                        AiTransportKind::ResponsesWebsocket,
                    ),
                ))?;
            }
            emit_provider_transport_trace(
                execution_id.as_str(),
                "responses_websocket",
                "execution_completed",
                &transport_started_at,
            );
        })
    }

    /// Returns or establishes one session-isolated WebSocket connection.
    /// 返回或建立一个会话隔离的 WebSocket 连接。
    async fn responses_websocket_session(
        &self,
        key: &str,
        config: ResponsesWebsocketConfig,
    ) -> Result<Arc<TokioMutex<ResponsesWebsocketSession>>, vulcan_llm_core::LlmCoreError> {
        {
            let mut sessions = self.responses_websocket_sessions.lock().await;
            if let Some(existing) = sessions.get_mut(key) {
                existing.last_used_at = Instant::now();
                return Ok(Arc::clone(&existing.session));
            }
            if sessions.len() >= RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES
                && oldest_idle_responses_websocket_session_key(&sessions).is_none()
            {
                return Err(LlmCoreError::new(format!(
                    "Responses WebSocket session capacity {} is exhausted by active connections.",
                    RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES
                )));
            }
        }
        let connected = Arc::new(TokioMutex::new(
            ResponsesWebsocketSession::connect(config).await?,
        ));
        let mut sessions = self.responses_websocket_sessions.lock().await;
        if let Some(existing) = sessions.get_mut(key) {
            existing.last_used_at = Instant::now();
            return Ok(Arc::clone(&existing.session));
        }
        if sessions.len() >= RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES {
            let eviction_key = oldest_idle_responses_websocket_session_key(&sessions)
                .ok_or_else(|| {
                    LlmCoreError::new(format!(
                        "Responses WebSocket session capacity {} is exhausted by active connections.",
                        RESPONSES_WEBSOCKET_SESSION_MAX_ENTRIES
                    ))
                })?;
            sessions.remove(eviction_key.as_str());
        }
        sessions.insert(
            key.to_string(),
            ResponsesWebsocketSessionCacheEntry {
                session: Arc::clone(&connected),
                last_used_at: Instant::now(),
            },
        );
        Ok(connected)
    }

    /// Reports whether a session-scoped WebSocket breaker is active and refreshes its LRU age.
    /// 报告会话作用域 WebSocket 熔断器是否生效，并刷新其 LRU 时间。
    ///
    /// # Parameters
    /// 参数
    /// `key` is the opaque credential-, route-, model-, and root-session-bound cache key.
    /// `key` 是绑定凭据、路由、模型与根会话的不透明缓存键。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when this exact cache key has an active process-local breaker.
    /// 仅当该精确缓存键存在活动的进程内熔断器时返回 `true`。
    fn responses_websocket_disabled(&self, key: &str) -> bool {
        let mut breakers = self
            .responses_websocket_breakers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(last_used_at) = breakers.get_mut(key) else {
            return false;
        };
        *last_used_at = Instant::now();
        true
    }

    /// Activates a sticky circuit breaker for one route and root session.
    /// 为一条路由与根会话激活粘性熔断器。
    fn disable_responses_websocket(&self, key: &str) {
        let mut breakers = self
            .responses_websocket_breakers
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        insert_bounded_responses_websocket_breaker(&mut breakers, key, Instant::now());
    }

    /// Starts one immutable HTTP SSE execution without transport fallback.
    /// 启动一次不可变 HTTP SSE 执行，不进行传输回退。
    fn execute_http(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
    ) -> ProviderExecutionStream {
        let client = self.clone();
        Box::pin(try_stream! {
            let transport_started_at = Instant::now();
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "execution_started",
                &transport_started_at,
            );
            let request = request_without_nonpersistent_responses_continuation(&route, &request);
            let encoded = client.encode_request_with_credential(&route, &request, &secret)?;
            let response_profiles = route.response_projection_profiles.clone();
            let transport = client
                .protocols
                .transport_profile(route.protocol_route.protocol_profile_id.as_str())
                .map_err(llm_core_protocol_failure)?;
            let url = route.request_url(encoded.path.as_str());
            let headers = execution_headers(&route, &encoded.headers, &secret)?;
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "request_dispatched",
                &transport_started_at,
            );
            let dispatched_at_ms = provider_transport_unix_millis()?;
            let response = tokio::time::timeout(
                Duration::from_millis(transport.response_headers_timeout_ms),
                client
                    .http
                    .post(url.as_str())
                    .headers(headers)
                    .json(&encoded.body)
                    .send(),
            )
            .await
            .map_err(|_| {
                transport_failure(format!(
                    "Provider request to `{url}` exceeded the registered {} ms response-header timeout.",
                    transport.response_headers_timeout_ms
                ))
            })?
            .map_err(|error| {
                http_send_transport_failure(
                    &error,
                    format!("Provider request to `{url}` failed: {error}"),
                )
            })?;
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "response_headers_received",
                &transport_started_at,
            );
            let status = response.status();
            let headers_at_ms = provider_transport_unix_millis()?;
            let rate_limit_headers = allowlisted_rate_limit_headers(response.headers());
            let provider_request_id = provider_request_id_from_headers(
                response.headers(),
                route.protocol_route.provider_request_id_headers.as_slice(),
            )?;
            yield AiStreamEvent::TransportObserved {
                transport: AiTransportKind::Http,
                dispatched_at_ms,
                headers_at_ms: Some(headers_at_ms),
                http_status: Some(status.as_u16()),
                provider_request_id,
                rate_limit_headers,
            };
            let response = if status.is_success() {
                response
            } else {
                let retry_after =
                    parse_retry_after_headers(response.headers(), SystemTime::now());
                let body = read_bounded_response_text(
                    response,
                    transport.max_error_body_bytes,
                    url.as_str(),
                    status.as_u16(),
                    retry_after.clone(),
                    Duration::from_millis(transport.stream_idle_timeout_ms),
                )
                .await?;
                let body = project_response_record_text(&response_profiles, body.as_str())
                    .map_err(protocol_failure)?;
                Err(http_failure(
                    status.as_u16(),
                    retry_after,
                    body.as_str(),
                ))?
            };
            validate_response_content_type(
                response.headers(),
                &transport,
                url.as_str(),
            )?;
            let mut decoder = client
                .protocols
                .create_stream_decoder(
                    route.protocol_route.protocol_profile_id.as_str(),
                    execution_id.clone(),
                    &request,
                )
                .map_err(llm_core_protocol_failure)?;
            let mut parser = IncrementalFrameParser::new(transport);
            let mut bytes = response.bytes_stream();
            let usage_profiles = route.protocol_route.usage_extraction_profiles.clone();
            let profiled_usage_enabled = !usage_profiles.is_empty();
            let mut usage_accumulator = UsageAccumulator::default();
            let mut profiled_usage_gate = ProfiledUsageEventGate::default();
            let mut semantic_output_observed = false;
            let mut first_chunk_observed = false;
            let mut stream_state = AiStreamStateMachine::default();
            loop {
                let next_chunk = tokio::time::timeout(
                    Duration::from_millis(transport.stream_idle_timeout_ms),
                    bytes.next(),
                )
                .await
                .map_err(|_| {
                    stream_transport_failure(
                        format!(
                            "Provider stream from `{url}` exceeded the registered {} ms idle timeout.",
                            transport.stream_idle_timeout_ms
                        ),
                        semantic_output_observed,
                    )
                })?;
                let Some(chunk) = next_chunk else {
                    break;
                };
                let chunk = chunk.map_err(|error| {
                    stream_transport_failure(
                        format!("Provider stream from `{url}` failed: {error}"),
                        semantic_output_observed,
                    )
                })?;
                if !first_chunk_observed {
                    first_chunk_observed = true;
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "http_sse",
                        "first_chunk_received",
                        &transport_started_at,
                    );
                }
                for frame in parser
                    .push(chunk.as_ref())
                    .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?
                {
                    let projected_data = project_response_record_text(
                        &response_profiles,
                        frame.data.as_str(),
                    )
                    .map_err(|error| {
                        stream_protocol_failure(error, semantic_output_observed)
                    })?;
                    let mut profiled_usage_events = extract_profiled_usage_events(
                        &usage_profiles,
                        UsageRecordKind::SseEvent,
                        projected_data.as_str(),
                        &mut usage_accumulator,
                    )
                    .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?;
                    for usage_event in &mut profiled_usage_events {
                        apply_local_cost_estimate(&route, usage_event);
                    }
                    for mut usage_event in profiled_usage_gate
                        .stage(profiled_usage_events)
                        .map_err(|error| {
                            stream_protocol_failure(error, semantic_output_observed)
                        })?
                    {
                        validate_stream_event(
                            &mut stream_state,
                            &usage_event,
                            semantic_output_observed,
                        )?;
                        update_semantic_output_observation(
                            &mut semantic_output_observed,
                            &mut usage_event,
                        );
                        yield usage_event;
                    }
                    for mut event in decoder
                        .push_sse(frame.event_name.as_deref(), projected_data.as_str())
                        .map_err(|error| {
                            stream_protocol_failure(error, semantic_output_observed)
                        })?
                    {
                        if profiled_usage_enabled
                            && matches!(event, AiStreamEvent::UsageUpdated { .. })
                        {
                            continue;
                        }
                        apply_local_cost_estimate(&route, &mut event);
                        let was_semantic_output_observed = semantic_output_observed;
                        update_semantic_output_observation(
                            &mut semantic_output_observed,
                            &mut event,
                        );
                        if semantic_output_observed && !was_semantic_output_observed {
                            emit_provider_transport_trace(
                                execution_id.as_str(),
                                "http_sse",
                                "first_semantic_event",
                                &transport_started_at,
                            );
                        }
                        if matches!(
                            event,
                            AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
                        ) {
                            emit_provider_transport_trace(
                                execution_id.as_str(),
                                "http_sse",
                                "terminal_event",
                                &transport_started_at,
                            );
                        }
                        validate_stream_event(&mut stream_state, &event, semantic_output_observed)?;
                        let released_usage = profiled_usage_gate.observe(&event);
                        yield event;
                        for mut usage_event in released_usage {
                            validate_stream_event(
                                &mut stream_state,
                                &usage_event,
                                semantic_output_observed,
                            )?;
                            update_semantic_output_observation(
                                &mut semantic_output_observed,
                                &mut usage_event,
                            );
                            yield usage_event;
                        }
                    }
                }
            }
            for frame in parser
                .finish()
                .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?
            {
                let projected_data = project_response_record_text(
                    &response_profiles,
                    frame.data.as_str(),
                )
                .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?;
                let mut profiled_usage_events = extract_profiled_usage_events(
                    &usage_profiles,
                    UsageRecordKind::SseEvent,
                    projected_data.as_str(),
                    &mut usage_accumulator,
                )
                .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?;
                for usage_event in &mut profiled_usage_events {
                    apply_local_cost_estimate(&route, usage_event);
                }
                for mut usage_event in profiled_usage_gate
                    .stage(profiled_usage_events)
                    .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?
                {
                    validate_stream_event(
                        &mut stream_state,
                        &usage_event,
                        semantic_output_observed,
                    )?;
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut usage_event,
                    );
                    yield usage_event;
                }
                for mut event in decoder
                    .push_sse(frame.event_name.as_deref(), projected_data.as_str())
                    .map_err(|error| stream_protocol_failure(error, semantic_output_observed))?
                {
                    if profiled_usage_enabled
                        && matches!(event, AiStreamEvent::UsageUpdated { .. })
                    {
                        continue;
                    }
                    apply_local_cost_estimate(&route, &mut event);
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut event,
                    );
                    validate_stream_event(&mut stream_state, &event, semantic_output_observed)?;
                    let released_usage = profiled_usage_gate.observe(&event);
                    yield event;
                    for mut usage_event in released_usage {
                        validate_stream_event(
                            &mut stream_state,
                            &usage_event,
                            semantic_output_observed,
                        )?;
                        update_semantic_output_observation(
                            &mut semantic_output_observed,
                            &mut usage_event,
                        );
                        yield usage_event;
                    }
                }
            }
            for mut event in decoder
                .finish()
                .map_err(|error| stream_completion_failure(error, semantic_output_observed))?
            {
                if profiled_usage_enabled && matches!(event, AiStreamEvent::UsageUpdated { .. }) {
                    continue;
                }
                apply_local_cost_estimate(&route, &mut event);
                let was_semantic_output_observed = semantic_output_observed;
                update_semantic_output_observation(
                    &mut semantic_output_observed,
                    &mut event,
                );
                if semantic_output_observed && !was_semantic_output_observed {
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "http_sse",
                        "first_semantic_event",
                        &transport_started_at,
                    );
                }
                if matches!(
                    event,
                    AiStreamEvent::ResponseCompleted | AiStreamEvent::ResponseFailed { .. }
                ) {
                    emit_provider_transport_trace(
                        execution_id.as_str(),
                        "http_sse",
                        "terminal_event",
                        &transport_started_at,
                    );
                }
                validate_stream_event(&mut stream_state, &event, semantic_output_observed)?;
                let released_usage = profiled_usage_gate.observe(&event);
                yield event;
                for mut usage_event in released_usage {
                    validate_stream_event(
                        &mut stream_state,
                        &usage_event,
                        semantic_output_observed,
                    )?;
                    update_semantic_output_observation(
                        &mut semantic_output_observed,
                        &mut usage_event,
                    );
                    yield usage_event;
                }
            }
            profiled_usage_gate
                .ensure_released()
                .map_err(|error| stream_completion_failure(error, semantic_output_observed))?;
            emit_provider_transport_trace(
                execution_id.as_str(),
                "http_sse",
                "execution_finished",
                &transport_started_at,
            );
        })
    }
}

/// Maximum profiled usage events retained before the normalized response starts.
/// 规范化响应开始前允许保留的最大 Profile 用量事件数。
const PROFILED_USAGE_GATE_MAX_PENDING_EVENTS: usize = 256;

/// Delays raw profiled usage until the normalized response lifecycle has started.
/// 将原始 Profile 用量延迟到规范化响应生命周期开始之后。
#[derive(Debug, Default)]
struct ProfiledUsageEventGate {
    /// Whether a validated response start event has been observed.
    /// 是否已经观察到通过校验的响应开始事件。
    response_started: bool,
    /// Usage events collected before the response start event.
    /// 在响应开始事件之前采集的用量事件。
    pending: Vec<AiStreamEvent>,
}

impl ProfiledUsageEventGate {
    /// Stages newly extracted usage and returns events that may be published immediately.
    /// 暂存新提取的用量并返回可以立即发布的事件。
    ///
    /// # Parameters
    /// 参数
    /// `events` contains one bounded frame's extracted usage observations.
    /// `events` 包含从一个有界帧提取的用量观测。
    ///
    /// # Returns
    /// 返回值
    /// Immediate events after response start, an empty vector while staging, or a bounded overflow error.
    /// 响应开始后返回立即发布事件；暂存期间返回空向量；超过上限时返回有界溢出错误。
    fn stage(
        &mut self,
        events: Vec<AiStreamEvent>,
    ) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
        if self.response_started {
            return Ok(events);
        }
        if self.pending.len().saturating_add(events.len()) > PROFILED_USAGE_GATE_MAX_PENDING_EVENTS
        {
            return Err(ProtocolConversionError::new(format!(
                "Provider usage before response start exceeds the {PROFILED_USAGE_GATE_MAX_PENDING_EVENTS}-event limit."
            )));
        }
        self.pending.extend(events);
        Ok(Vec::new())
    }

    /// Observes one validated normalized event and releases pending usage after response start.
    /// 观察一个已校验的规范化事件并在响应开始后释放暂存用量。
    fn observe(&mut self, event: &AiStreamEvent) -> Vec<AiStreamEvent> {
        if !matches!(event, AiStreamEvent::ResponseStarted { .. }) {
            return Vec::new();
        }
        self.response_started = true;
        std::mem::take(&mut self.pending)
    }

    /// Rejects a completed transport that collected usage without a normalized response start.
    /// 拒绝已经采集用量却没有规范化响应开始事件的已结束传输。
    fn ensure_released(&self) -> Result<(), ProtocolConversionError> {
        if self.pending.is_empty() {
            return Ok(());
        }
        Err(ProtocolConversionError::new(
            "Provider usage was observed before normalization, but the response start event was never emitted.",
        ))
    }
}

/// Extracts usage from one untouched stream record before protocol normalization.
/// 在协议归一化前从一条未经修改的流记录中提取用量。
fn extract_profiled_usage_events(
    profiles: &[UsageExtractionProfile],
    record_kind: UsageRecordKind,
    data: &str,
    accumulator: &mut UsageAccumulator,
) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
    if profiles.is_empty() || data.trim() == "[DONE]" {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_str(data).map_err(|error| {
        ProtocolConversionError::new(format!(
            "Provider usage record is not valid JSON before normalization: {error}"
        ))
    })?;
    extract_profiled_usage_value_events(profiles, record_kind, &value, accumulator)
}

/// Extracts usage from one already parsed raw provider record.
/// 从一条已解析的原始供应商记录中提取用量。
fn extract_profiled_usage_value_events(
    profiles: &[UsageExtractionProfile],
    record_kind: UsageRecordKind,
    value: &Value,
    accumulator: &mut UsageAccumulator,
) -> Result<Vec<AiStreamEvent>, ProtocolConversionError> {
    let mut events = Vec::new();
    for profile in profiles {
        let Some(observation) = profile
            .extract(record_kind, value)
            .map_err(|error| ProtocolConversionError::new(error.message))?
        else {
            continue;
        };
        let usage = observation.usage.clone();
        if accumulator
            .apply(None, observation)
            .map_err(|error| ProtocolConversionError::new(error.message))?
        {
            events.push(AiStreamEvent::UsageUpdated {
                usage: Box::new(usage),
            });
        }
    }
    Ok(events)
}

/// Applies one frozen local price profile to a normalized usage event without replacing provider cost facts.
/// 将一个冻结本地价格 Profile 应用于规范化用量事件且不替换供应商成本事实。
fn apply_local_cost_estimate(route: &ProviderExecutionRoute, event: &mut AiStreamEvent) {
    let Some(profile) = route.billing_profile.as_ref() else {
        return;
    };
    let AiStreamEvent::UsageUpdated { usage } = event else {
        return;
    };
    if usage.cost.is_some() {
        // reported_currency must remain authoritative whenever provider cost evidence exists.
        // 存在供应商成本证据时 reported_currency 必须保持权威。
        let Some(reported_currency) = usage.cost_currency.as_deref() else {
            usage
                .validation_flags
                .push(String::from("reported_cost_currency_missing"));
            return;
        };
        if reported_currency != profile.currency {
            usage.validation_flags.push(String::from(
                "reported_and_estimated_cost_currency_mismatch",
            ));
            return;
        }
    }
    let breakdown = CostCalculator::calculate(usage, &profile.pricing, profile.multiplier);
    usage.estimated_cost = breakdown.total.map(|value| value.normalize().to_string());
    usage.cost_currency = Some(profile.currency.clone());
    usage.pricing_revision = Some(profile.pricing_revision.clone());
    usage.pricing_profile = Some(profile.snapshot.clone());
}

/// Removes a Responses continuation whenever the wire request is explicitly non-persistent.
/// 在线路请求明确不持久化时移除 Responses 延续游标。
///
/// # Parameters
/// 参数
/// `route` identifies the exact protocol profile and `request` is the immutable canonical request.
/// `route` 标识精确协议配置，`request` 是不可变规范请求。
///
/// # Returns
/// 返回值
/// A request that replays complete context for standard and Codex store=false Responses calls.
/// 返回一项为标准及 Codex store=false Responses 调用重放完整上下文的请求。
fn request_without_nonpersistent_responses_continuation(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
) -> AiExecutionRequest {
    let mut request = request.clone();
    if matches!(
        route.protocol_route.protocol_profile_id.as_str(),
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID
    ) {
        request.continuation = None;
    }
    request
}

/// Selects incremental WebSocket input only when the active socket proves cursor ownership.
/// 仅在活动 Socket 证明游标归属时选择 WebSocket 增量输入。
///
/// # Parameters
/// 参数
/// `request` carries the proven local append boundary and `active_response_id` is owned by the
/// exact cached socket.
/// `request` 携带已证明的本地追加边界，`active_response_id` 由精确缓存 Socket 持有。
///
/// # Returns
/// 返回值
/// The incremental request on an exact identity match, otherwise a complete-history request.
/// 精确身份匹配时返回增量请求，否则返回完整历史请求。
fn responses_websocket_request_for_active_session(
    request: &AiExecutionRequest,
    active_response_id: Option<&str>,
) -> AiExecutionRequest {
    let continuation_matches = request.continuation.as_ref().is_some_and(|continuation| {
        active_response_id == Some(continuation.provider_message_id.as_str())
    });
    let mut request = request.clone();
    if !continuation_matches {
        request.continuation = None;
    }
    request
}

/// Extracts retryable Codex WebSocket control errors from untouched provider frames.
/// 从未经修改的供应商帧中提取可重试的 Codex WebSocket 控制错误。
///
/// # Parameters
/// 参数
/// `value` is the exact parsed WebSocket frame before protocol decoding.
/// `value` 是协议解码前的精确已解析 WebSocket 帧。
///
/// # Returns
/// 返回值
/// The stable code and controlled message for recoverable chain or connection loss.
/// 返回可恢复链或连接丢失对应的稳定代码与受控消息。
fn responses_websocket_retry_control(value: &Value) -> Option<(String, String)> {
    let code = value
        .pointer("/error/code")
        .or_else(|| value.pointer("/response/error/code"))
        .and_then(Value::as_str)?;
    if !matches!(
        code,
        RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE
            | RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE
    ) {
        return None;
    }
    let message = value
        .pointer("/error/message")
        .or_else(|| value.pointer("/response/error/message"))
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or(match code {
            RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE => {
                "Responses WebSocket connection lifetime was exhausted."
            }
            RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE => {
                "Responses WebSocket previous response was not found."
            }
            _ => unreachable!("retry control code was matched above"),
        });
    Some((code.to_string(), message.to_string()))
}

/// Converts a recoverable WebSocket control event into one replay-safe physical-attempt failure.
/// 将可恢复 WebSocket 控制事件转换为一次可安全重放的真实尝试失败。
///
/// # Parameters
/// 参数
/// `code` and `message` preserve bounded upstream diagnostics; `semantic_output_observed` protects
/// callers from replay after visible output.
/// `code` 与 `message` 保留有界上游诊断，`semantic_output_observed` 防止在可见输出后重放。
///
/// # Returns
/// 返回值
/// A structured retryable transport failure whose safety flag remains authoritative.
/// 返回一项安全标志仍具权威性的结构化可重试传输失败。
fn responses_websocket_control_failure(
    code: String,
    message: String,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    let mut failure = stream_transport_failure_with_websocket_close_code(
        message.clone(),
        semantic_output_observed,
        None,
    );
    failure.error.subcategory = Some(String::from("responses_websocket_control"));
    failure.error.upstream_code = Some(code);
    failure.error.upstream_message = Some(message);
    failure
}

/// Invalidates a socket after local decoding or lifecycle validation rejects its active response.
/// 在本地解码或生命周期校验拒绝活动响应后使 Socket 失效。
///
/// # Parameters
/// 参数
/// `session` owns the exact in-flight socket and `failure` preserves the original typed cause.
/// `session` 持有精确的处理中 Socket，`failure` 保留原始强类型原因。
///
/// # Returns
/// 返回值
/// The unchanged failure after synchronously clearing unsafe connection-local state.
/// 同步清除不安全的连接局部状态后返回未经修改的失败。
fn abandon_responses_websocket_attempt(
    session: &mut ResponsesWebsocketSession,
    failure: ProviderExecutionFailure,
) -> ProviderExecutionFailure {
    session.abandon_response();
    failure
}

/// Builds the non-secret isolation key for one Responses WebSocket session.
/// 为一个 Responses WebSocket 会话构建不含密钥的隔离键。
fn responses_websocket_cache_key(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    secret: &CredentialExecutionSecret,
) -> Option<String> {
    let root_session_id = request.cache_scope_id.as_deref()?;
    let material = serde_json::to_vec(&json!({
        "provider_id": route.provider.id,
        "entry": route.entry,
        "connection_id": route.connection.id,
        "credential_revision": secret.revision(),
        "protocol_route": route.protocol_route,
        "secondary_route": route.secondary_route,
        "custom_model_projection": route.custom_model_projection,
        "model_id": request.model_id,
        "root_session_id": root_session_id,
    }))
    .ok()?;
    Some(format!("sha256:{:x}", Sha256::digest(material)))
}

/// Selects the least-recently-used WebSocket session that has no external owner.
/// 选择没有外部持有者且最久未使用的 WebSocket 会话。
///
/// # Parameters
/// 参数
/// `sessions` is the complete bounded process-local WebSocket session table.
/// `sessions` 是完整的有界进程内 WebSocket 会话表。
///
/// # Returns
/// 返回值
/// The deterministic eviction key, or `None` when every cached session is currently referenced.
/// 返回确定性的淘汰键；当每个缓存会话当前都被引用时返回 `None`。
fn oldest_idle_responses_websocket_session_key(
    sessions: &BTreeMap<String, ResponsesWebsocketSessionCacheEntry>,
) -> Option<String> {
    sessions
        .iter()
        .filter(|(_, entry)| Arc::strong_count(&entry.session) == 1)
        .min_by(|(left_key, left), (right_key, right)| {
            left.last_used_at
                .cmp(&right.last_used_at)
                .then_with(|| left_key.cmp(right_key))
        })
        .map(|(key, _)| key.clone())
}

/// Inserts one sticky WebSocket breaker while enforcing a deterministic LRU capacity bound.
/// 在强制确定性 LRU 容量上限的同时插入一个粘性 WebSocket 熔断器。
///
/// # Parameters
/// 参数
/// `breakers` is the mutable process-local breaker table, `key` is the opaque session key, and
/// `observed_at` is the monotonic access timestamp.
/// `breakers` 是可变进程内熔断器表，`key` 是不透明会话键，`observed_at` 是单调访问时间。
///
/// # Returns
/// 返回值
/// No value; an older breaker is evicted before a new key can exceed the hard limit.
/// 不返回值；新键超过硬上限前会先淘汰一个更旧的熔断器。
fn insert_bounded_responses_websocket_breaker(
    breakers: &mut BTreeMap<String, Instant>,
    key: &str,
    observed_at: Instant,
) {
    if !breakers.contains_key(key)
        && breakers.len() >= RESPONSES_WEBSOCKET_BREAKER_MAX_ENTRIES
        && let Some(oldest_key) = breakers
            .iter()
            .min_by(|(left_key, left_at), (right_key, right_at)| {
                left_at.cmp(right_at).then_with(|| left_key.cmp(right_key))
            })
            .map(|(oldest_key, _)| oldest_key.clone())
    {
        breakers.remove(oldest_key.as_str());
    }
    breakers.insert(key.to_string(), observed_at);
}

/// Converts the exact HTTP Responses endpoint to its WebSocket scheme.
/// 将精确 HTTP Responses 端点转换为对应 WebSocket scheme。
fn responses_websocket_url(
    route: &ProviderExecutionRoute,
    encoded_path: &str,
) -> Result<String, ProviderExecutionFailure> {
    let http_url = route.request_url(encoded_path);
    let mut url = reqwest::Url::parse(http_url.as_str()).map_err(|error| {
        pre_dispatch_failure(format!("Responses WebSocket endpoint is invalid: {error}"))
    })?;
    let websocket_scheme = match url.scheme() {
        "https" => "wss",
        "http" => "ws",
        scheme => {
            return Err(pre_dispatch_failure(format!(
                "Responses WebSocket cannot derive from endpoint scheme `{scheme}`."
            )));
        }
    };
    url.set_scheme(websocket_scheme).map_err(|_| {
        pre_dispatch_failure("Responses WebSocket endpoint scheme could not be replaced.")
    })?;
    Ok(url.to_string())
}

/// Copies validated HTTP headers into the secret-safe WebSocket handshake container.
/// 将已校验 HTTP Header 复制到不实现调试输出的 WebSocket 握手容器。
fn websocket_headers(
    headers: &HeaderMap,
) -> Result<BTreeMap<String, String>, ProviderExecutionFailure> {
    headers
        .iter()
        .map(|(name, value)| {
            value
                .to_str()
                .map(|value| (name.as_str().to_string(), value.to_string()))
                .map_err(|error| {
                    pre_dispatch_failure(format!(
                        "Responses WebSocket header `{name}` is not valid UTF-8: {error}"
                    ))
                })
        })
        .collect()
}

/// Emits one bounded provider transport trace event.
/// 发出一条有界供应商传输跟踪事件。
fn emit_provider_transport_trace(
    execution_id: &str,
    transport: &'static str,
    stage: &'static str,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport={transport} stage={stage} elapsed_ms={}",
        started_at.elapsed().as_millis()
    );
}

/// Emits one opt-in HTTP status without response bodies, headers, or credentials.
/// 输出一项可选 HTTP 状态，且不包含响应正文、请求头或凭据。
///
/// # Parameters
/// 参数
/// `execution_id` is the non-secret local correlation id, `transport` is a fixed internal label,
/// `status` is the numeric HTTP response code, and `started_at` is the monotonic attempt start.
/// `execution_id` 是不含密钥的本地关联标识，`transport` 是固定内部标签，`status` 是数字 HTTP
/// 响应码，`started_at` 是尝试开始时的单调时钟。
fn emit_provider_transport_http_status_trace(
    execution_id: &str,
    transport: &'static str,
    status: u16,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport={transport} stage=response_http_status elapsed_ms={} status={status}",
        started_at.elapsed().as_millis()
    );
}

/// Emits one opt-in normalized failure classification without the possibly sensitive error message.
/// 输出一项可选标准化失败分类，且不包含可能敏感的错误消息。
///
/// # Parameters
/// 参数
/// `execution_id` and `transport` identify the physical attempt, `failure` supplies only normalized
/// classification fields, and `started_at` is the monotonic attempt start.
/// `execution_id` 与 `transport` 标识真实尝试，`failure` 仅提供标准化分类字段，`started_at`
/// 是尝试开始时的单调时钟。
fn emit_provider_transport_failure_trace(
    execution_id: &str,
    transport: &'static str,
    failure: &ProviderExecutionFailure,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport={transport} stage=execution_failed elapsed_ms={} category={:?} subcategory={:?} error_stage={:?} http_status={:?} websocket_close_code={:?} dispatch_outcome={:?} retryable={}",
        started_at.elapsed().as_millis(),
        failure.error.category,
        failure.error.subcategory,
        failure.error.stage,
        failure.error.http_status,
        failure.error.websocket_close_code,
        failure.error.dispatch_outcome,
        failure.error.retryable
    );
}

/// Emits a secret-safe failure trace and returns the unchanged typed failure.
/// 输出不含秘密的失败追踪，并原样返回强类型失败。
///
/// # Parameters
/// 参数
/// `execution_id`, `transport`, and `started_at` identify the physical attempt; `failure` is the
/// authoritative structured failure passed back to retry accounting.
/// `execution_id`、`transport` 与 `started_at` 标识真实尝试；`failure` 是返回重试统计的权威
/// 结构化失败。
///
/// # Returns
/// 返回值
/// The same failure after its normalized, non-secret fields have been traced.
/// 在记录标准化且不含秘密的字段后，返回同一失败。
fn trace_provider_transport_failure(
    execution_id: &str,
    transport: &'static str,
    failure: ProviderExecutionFailure,
    started_at: &Instant,
) -> ProviderExecutionFailure {
    emit_provider_transport_failure_trace(execution_id, transport, &failure, started_at);
    failure
}

/// Emits one opt-in Responses compact JSON shape without response values or user content.
/// 输出一项可选 Responses 压缩 JSON 形状，且不包含响应值或用户内容。
///
/// # Parameters
/// 参数
/// `execution_id` is the non-secret local correlation id, `value` is the decoded JSON response,
/// and `started_at` is the monotonic attempt start.
/// `execution_id` 是不含密钥的本地关联标识，`value` 是已解码 JSON 响应，`started_at` 是尝试开始时的单调时钟。
fn emit_responses_compact_response_shape_trace(
    execution_id: &str,
    value: &Value,
    started_at: &Instant,
) {
    if !provider_transport_trace_enabled() {
        return;
    }
    // topLevelKeys lists bounded structural names only and never serializes their values.
    // topLevelKeys 仅列出有界结构名称，绝不序列化对应值。
    let top_level_keys = value
        .as_object()
        .map(|object| {
            object
                .keys()
                .take(32)
                .map(|key| bounded_trace_label(key))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // outputKind distinguishes absent, array, object, and scalar shapes without exposing content.
    // outputKind 在不暴露内容的情况下区分缺失、数组、对象与标量形状。
    let output_kind = match value.get("output") {
        None => "absent",
        Some(Value::Array(_)) => "array",
        Some(Value::Object(_)) => "object",
        Some(_) => "scalar",
    };
    // outputLen reports the exact array length while item details remain bounded below.
    // outputLen 报告精确数组长度，而下方 item 详情仍保持有界。
    let output_len = value
        .get("output")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    // outputItems captures only bounded item type labels and object key names.
    // outputItems 仅捕获有界 item 类型标签与对象键名。
    let output_items = value
        .get("output")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(16)
                .map(|item| {
                    // itemType is a bounded structural discriminator rather than response content.
                    // itemType 是有界结构判别符，而不是响应内容。
                    let item_type = item
                        .get("type")
                        .and_then(Value::as_str)
                        .map(bounded_trace_label)
                        .unwrap_or_else(|| String::from("<missing>"));
                    // itemKeys lists object member names only.
                    // itemKeys 仅列出对象成员名称。
                    let item_keys = item
                        .as_object()
                        .map(|object| {
                            object
                                .keys()
                                .take(32)
                                .map(|key| bounded_trace_label(key))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    format!("{item_type}:{item_keys:?}")
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    eprintln!(
        "[provider-transport-trace] execution_id={execution_id} transport=responses_compact stage=response_json_shape elapsed_ms={} top_level_keys={top_level_keys:?} output_kind={output_kind} output_len={} output_items={output_items:?}",
        started_at.elapsed().as_millis(),
        output_len,
    );
}

/// Normalizes one provider-controlled structural label into bounded ASCII trace text.
/// 把单个供应商控制的结构标签规范化为有界 ASCII 追踪文本。
///
/// # Parameters
/// 参数
/// `value` is a JSON object key or item type discriminator.
/// `value` 是 JSON 对象键或 item 类型判别符。
///
/// # Returns
/// 返回值
/// At most 64 safe ASCII characters with unsupported characters replaced by `_`.
/// 返回最多 64 个安全 ASCII 字符，不支持的字符替换为 `_`。
fn bounded_trace_label(value: &str) -> String {
    value
        .chars()
        .take(64)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// Returns whether secret-safe provider transport tracing is explicitly enabled.
/// 返回是否已经显式启用不含密钥的供应商传输追踪。
fn provider_transport_trace_enabled() -> bool {
    std::env::var(PROVIDER_TRANSPORT_TRACE_ENV).is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Returns whether one normalized event makes transparent replay unsafe.
/// 返回一条归一化事件是否会使透明重放变得不安全。
///
/// # Parameters
/// 参数
/// `event` is the exact normalized event emitted by one physical provider attempt.
/// `event` 是一次真实供应商尝试发出的精确归一化事件。
///
/// # Returns
/// 返回值
/// `true` for user-visible, tool-visible, or resource-side-effect semantic output.
/// 对用户可见、工具可见或资源副作用语义输出返回 `true`。
fn stream_event_has_semantic_output(event: &AiStreamEvent) -> bool {
    matches!(
        event,
        AiStreamEvent::ResponseTextDelta { .. }
            | AiStreamEvent::ReasoningDelta { .. }
            | AiStreamEvent::ToolCallStarted { .. }
            | AiStreamEvent::ImageGenerationPartial { .. }
            | AiStreamEvent::ResourceProgress { .. }
            | AiStreamEvent::UsageUpdated { .. }
    )
}

/// Tracks semantic output and annotates terminal public failures with the observed state.
/// 跟踪语义输出并用已观测状态标注公共终态失败。
fn update_semantic_output_observation(observed: &mut bool, event: &mut AiStreamEvent) {
    if stream_event_has_semantic_output(event) {
        *observed = true;
    }
    if let AiStreamEvent::ResponseFailed { error } = event {
        error.semantic_output_observed = *observed;
    }
}

/// Applies the production stream state machine and preserves the physical request state on failure.
/// 应用生产流状态机，并在失败时保留物理请求状态。
///
/// # Parameters
/// 参数
/// `state` owns lifecycle validation, `event` is the next normalized event, and `observed`
/// records whether semantic output has already escaped the transport boundary.
/// `state` 持有生命周期校验，`event` 是下一条规范化事件，`observed`
/// 记录语义输出是否已经越过传输边界。
///
/// # Returns
/// 返回值
/// `Ok(())` for a valid transition or a structured dispatched protocol failure.
/// 有效转换返回 `Ok(())`，否则返回结构化的已分派协议失败。
fn validate_stream_event(
    state: &mut AiStreamStateMachine,
    event: &AiStreamEvent,
    observed: bool,
) -> Result<(), ProviderExecutionFailure> {
    state
        .apply(event)
        .map_err(|message| stream_protocol_failure(ProtocolConversionError::new(message), observed))
}

/// Reports whether one route's manifest request profiles reference a model parameter.
/// 报告一个路由的 Manifest 请求 Profile 是否引用某模型参数。
///
/// A closed step references a parameter either through a `CopyRequestField` field value or
/// through a request-field source path with the `@model_parameters.<id>` form (plan §R8.5).
/// 封闭步骤通过 `CopyRequestField` 的字段值或 `@model_parameters.<id>` 形式的请求字段
/// 源路径引用参数（方案 §R8.5）。
fn request_projection_profiles_cover_parameter(
    route: &ProviderExecutionRoute,
    parameter_id: &str,
) -> bool {
    let expected_field = format!("model_parameters.{parameter_id}");
    let expected_source = format!("@model_parameters.{parameter_id}");
    route.request_projection_profiles.iter().any(|profile| {
        profile.steps.iter().any(|step| match step {
            crate::provider_manifest::MappingStep::CopyRequestField { field, .. } => {
                *field == expected_field
            }
            crate::provider_manifest::MappingStep::EnumMap { source_path, .. }
            | crate::provider_manifest::MappingStep::ConditionalSet { source_path, .. }
            | crate::provider_manifest::MappingStep::DeleteIfEquals { source_path, .. }
            | crate::provider_manifest::MappingStep::DeleteUnlessEquals { source_path, .. }
            | crate::provider_manifest::MappingStep::RejectIfPresent { source_path, .. } => {
                *source_path == expected_source
            }
            crate::provider_manifest::MappingStep::RejectIfAllPresent { conditions, .. } => {
                conditions
                    .iter()
                    .any(|condition| condition.source_path == expected_source)
            }
            _ => false,
        })
    })
}

/// Applies route-scoped request policy before encoding.
/// 在编码前应用路由作用域请求策略。
fn validate_request_policy(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
) -> Result<(), ProviderExecutionFailure> {
    for parameter_id in request.model_parameters.keys() {
        // Temperature is a protocol-neutral host primitive; provider-specific admissibility and
        // conditional omission are declared by the effective model Profile.
        // Temperature 是协议无关的宿主原语；供应商特定的可用性与条件省略由生效模型
        // Profile 声明。
        let registered_temperature_route = parameter_id == SAMPLING_TEMPERATURE_PARAMETER_ID;
        let registered_custom_parameter = route
            .custom_model_projection
            .as_ref()
            .is_some_and(|projection| projection.model_parameter_paths.contains_key(parameter_id));
        // Manifest request profiles register the model parameters their closed steps reference;
        // the legacy per-provider projector gate is superseded by the profile chain (plan §R8.5).
        // Manifest 请求 Profile 注册其封闭步骤引用的模型参数；旧版逐供应商投影器门槛被
        // Profile 链取代（方案 §R8.5）。
        let registered_by_profile =
            request_projection_profiles_cover_parameter(route, parameter_id);
        if !registered_temperature_route && !registered_custom_parameter && !registered_by_profile {
            return Err(pre_dispatch_failure(format!(
                "Model-specific parameter `{parameter_id}` has no registered route projector."
            )));
        }
    }
    Ok(())
}

/// Evaluates request-field-only manifest guards before the canonical protocol encoder runs.
/// 在规范协议编码器运行前执行仅依赖请求字段的 Manifest 守卫。
///
/// Provider profiles may declare a more specific endpoint restriction than the provider-neutral
/// baseline encoder. Running only pure request-field guards here preserves that configuration
/// authority; guards that depend on the encoded body remain deferred to normal projection.
/// 供应商 Profile 可以声明比供应商无关基线编码器更具体的端点限制。此处仅提前执行纯请求字段
/// 守卫，以保留配置权威；依赖编码正文的守卫仍延后到常规投影阶段。
///
/// # Parameters
/// 参数
/// `route` is the frozen route and `request` is the immutable provider-neutral request.
/// `route` 是冻结路由，`request` 是不可变的供应商无关请求。
///
/// # Returns
/// 返回值
/// Success when every eligible guard accepts the request, otherwise its manifest-owned failure.
/// 所有可提前执行的守卫均接受请求时返回成功，否则返回其 Manifest 自有失败。
fn validate_request_projection_guards_before_encoding(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
) -> Result<(), ProviderExecutionFailure> {
    let mut preflight = EncodedWireRequest {
        path: String::from("/manifest-request-guard-preflight"),
        headers: BTreeMap::new(),
        body: Value::Object(Default::default()),
    };
    for profile in &route.request_projection_profiles {
        for step in &profile.steps {
            let request_field_only = match step {
                crate::provider_manifest::MappingStep::RejectIfPresent { source, .. } => {
                    *source == crate::provider_manifest::MappingStepSource::RequestField
                }
                crate::provider_manifest::MappingStep::RejectIfAllPresent {
                    conditions, ..
                } => conditions.iter().all(|condition| {
                    condition.source == crate::provider_manifest::MappingStepSource::RequestField
                }),
                _ => false,
            };
            if request_field_only {
                apply_mapping_step(request, &mut preflight, step)?;
            }
        }
    }
    Ok(())
}

/// Projects the canonical Responses body into the exact Chat upstream shape.
/// 将规范 Responses 正文投影为精确的 Chat 上游形状。
///
/// The target route still supplies its exact path and fixed headers, while the
/// request body is derived from the same Responses encoder used by the public
/// baseline. This keeps provider conversion independent from the caller-facing
/// protocol selected by a legacy route.
/// 目标路由仍然提供精确路径和固定请求头，但请求正文统一来自公共基线使用的
/// Responses 编码器。这样供应商转换不会依赖旧路由暴露给调用方的协议形状。
fn project_responses_baseline_for_provider(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    mut encoded: EncodedWireRequest,
) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
    let Some(upstream_protocol) = upstream_protocol_for_route(route) else {
        return Ok(encoded);
    };
    if matches!(upstream_protocol, UpstreamProtocol::OpenAiResponses) {
        return Ok(encoded);
    }
    // Preserve only the converted protocol headers; the request body remains Responses-owned.
    // 仅保留转换协议所需的请求头；请求正文仍由 Responses 核心拥有。
    encoded
        .headers
        .extend(converted_protocol_headers(upstream_protocol));

    let mut canonical_request = request.clone();
    // Cross-protocol routes must replay complete history because a target protocol cannot reuse
    // a continuation cursor issued under another protocol identity.
    // 跨协议路由必须重放完整历史，因为目标协议不能复用另一协议身份签发的延续游标。
    canonical_request.continuation = None;
    prepare_target_reasoning_replay_for_responses(&mut canonical_request, upstream_protocol);
    let mut canonical_body = encode_codex_responses_request(&canonical_request)
        .map_err(|error| {
            pre_dispatch_failure(format!("LLM core request encoding failed: {error}"))
        })?
        .body;
    attach_target_tool_replay_to_responses(&mut canonical_body, request, upstream_protocol)?;
    if let Some(output_token_limit) = canonical_request.output_token_limit {
        // Codex omits this optional wire field, while Anthropic conversion requires the caller-resolved limit.
        // Codex 会省略该可选线路字段，而 Anthropic 转换需要调用方已解析出的输出上限。
        canonical_body["max_output_tokens"] = json!(output_token_limit);
    }
    let anthropic_reasoning_mode = if route.entry.request_projector == RequestProjectorKind::Direct
    {
        AnthropicReasoningMode::NativeBudget
    } else {
        AnthropicReasoningMode::ProviderSpecific
    };
    let context = ProviderRouteContext {
        upstream_protocol,
        anthropic_reasoning_mode,
    };
    encoded.body = project_responses_for_provider(canonical_body, &context).map_err(|error| {
        pre_dispatch_failure(format!("LLM provider conversion failed: {error}"))
    })?;
    Ok(encoded)
}

/// Resolves the provider-side protocol handled by the Responses conversion core.
/// 解析 Responses 转换核心处理的供应商侧协议。
fn upstream_protocol_for_route(route: &ProviderExecutionRoute) -> Option<UpstreamProtocol> {
    upstream_protocol_for_profile_id(route.protocol_route.protocol_profile_id.as_str())
}

/// Maps one exact registered protocol profile to the conversion-core wire protocol.
/// 将一个精确注册协议 Profile 映射到转换核心 Wire 协议。
pub(crate) fn upstream_protocol_for_profile_id(profile_id: &str) -> Option<UpstreamProtocol> {
    match profile_id {
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => {
            Some(UpstreamProtocol::OpenAiResponses)
        }
        OPENAI_CHAT_PROFILE_ID => Some(UpstreamProtocol::OpenAiChatCompletions),
        ANTHROPIC_MESSAGES_PROFILE_ID
        | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | CLAUDE_CODE_MESSAGES_PROFILE_ID => Some(UpstreamProtocol::AnthropicMessages),
        GEMINI_GENERATE_CONTENT_PROFILE_ID => Some(UpstreamProtocol::GeminiGenerateContent),
        GEMINI_INTERACTIONS_PROFILE_ID => Some(UpstreamProtocol::GeminiInteractions),
        GOOGLE_CODE_ASSIST_PROFILE_ID => Some(UpstreamProtocol::GoogleCodeAssist),
        _ => None,
    }
}

/// Rewraps same-provider Gemini reasoning signatures for the internal Responses conversion body.
/// 为内部 Responses 转换正文重新封装同供应商 Gemini 推理签名。
///
/// # Parameters
/// 参数
/// `request` is the conversion-only clone and `upstream_protocol` selects whether Gemini replay
/// metadata is meaningful for the frozen target route.
/// `request` 是仅供转换使用的克隆，`upstream_protocol` 决定 Gemini 重放元数据是否适用于冻结目标路由。
fn prepare_target_reasoning_replay_for_responses(
    request: &mut AiExecutionRequest,
    upstream_protocol: UpstreamProtocol,
) {
    if !matches!(
        upstream_protocol,
        UpstreamProtocol::GeminiGenerateContent
            | UpstreamProtocol::GeminiInteractions
            | UpstreamProtocol::GoogleCodeAssist
    ) {
        return;
    }
    for message in &mut request.messages {
        for item in &mut message.content {
            let ContentItem::Reasoning { signature, .. } = item else {
                continue;
            };
            let Some(raw_signature) = signature
                .as_deref()
                .and_then(gemini_replay_signature_payload)
            else {
                continue;
            };
            *signature = wrap_openai_reasoning_signature(raw_signature);
        }
    }
}

/// Attaches same-provider Gemini function-call signatures to internal Responses items.
/// 将同供应商 Gemini 函数调用签名附加到内部 Responses Item。
///
/// # Parameters
/// 参数
/// `canonical_body` is never dispatched directly, `request` owns signed calls, and
/// `upstream_protocol` is the exact target route protocol.
/// `canonical_body` 不会被直接分派，`request` 持有已签名调用，`upstream_protocol` 是精确目标路由协议。
///
/// # Returns
/// 返回值
/// Success after every matching call receives a bounded internal extension, or a controlled
/// conversion failure when the canonical body cannot preserve its call identity.
/// 每个匹配调用收到有界内部扩展后返回成功；若规范正文无法保留调用身份，则返回受控转换失败。
fn attach_target_tool_replay_to_responses(
    canonical_body: &mut Value,
    request: &AiExecutionRequest,
    upstream_protocol: UpstreamProtocol,
) -> Result<(), ProviderExecutionFailure> {
    if !matches!(
        upstream_protocol,
        UpstreamProtocol::GeminiGenerateContent | UpstreamProtocol::GoogleCodeAssist
    ) {
        return Ok(());
    }
    let mut signatures = BTreeMap::new();
    for message in &request.messages {
        for item in &message.content {
            let ContentItem::ToolCall { call } = item else {
                continue;
            };
            if let Some(signature) = call
                .signature
                .as_deref()
                .and_then(gemini_replay_signature_payload)
            {
                signatures.insert(call.id.as_str(), signature);
            }
        }
    }
    if signatures.is_empty() {
        return Ok(());
    }
    let input = canonical_body
        .get_mut("input")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            pre_dispatch_failure(
                "Responses conversion body is missing the input array required for Gemini signed calls.",
            )
        })?;
    for item in input {
        let Some(call_id) = item.get("call_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(signature) = signatures.remove(call_id) else {
            continue;
        };
        item["x_vulcan_provider_signature"] = Value::String(signature.to_string());
    }
    if let Some((call_id, _)) = signatures.into_iter().next() {
        return Err(pre_dispatch_failure(format!(
            "Responses conversion body did not preserve signed Gemini function call `{call_id}`."
        )));
    }
    Ok(())
}

/// Applies provider-only fields after protocol encoding.
/// 在协议编码后应用仅供应商字段。
fn project_provider_request(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    mut encoded: EncodedWireRequest,
) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
    if let Some(endpoint_path) = route.protocol_route.endpoint_path.as_ref() {
        encoded.path = endpoint_path.clone();
    }
    project_sampling_temperature(request, &mut encoded)?;
    // The ordered Manifest Profile chain is the only provider/model-specific request mapper.
    // Empty chains mean that baseline protocol encoding needs no vendor mutation and never
    // reactivate a hidden code-owned provider projector (plan §R8.5/§R10).
    // 有序 Manifest Profile 链是唯一的供应商/模型特定请求映射器。空链表示协议基线无需
    // 供应商变换，绝不会重新启用隐藏的代码持有供应商 projector（方案 §R8.5/§R10）。
    apply_request_projection_profiles(route, request, &mut encoded)?;
    project_secondary_route(route, &mut encoded)?;
    if let Some(projection) = route.custom_model_projection.as_ref() {
        project_custom_model_request(projection, request, &mut encoded)?;
    }
    validate_structured_output_projection(route, request, &encoded)?;
    Ok(encoded)
}

/// Projects an explicitly frozen secondary endpoint through the registered order primitive.
/// 通过已注册的顺序原语投影一条显式冻结的二级端点。
///
/// # Parameters
/// 参数
/// `route` carries the immutable secondary selection and `encoded` is the protocol body.
/// `route` 承载不可变二级选择，`encoded` 是协议请求体。
///
/// # Returns
/// 返回值
/// Success for automatic routing or one non-conflicting pinned endpoint.
/// 自动路由或一条不冲突的固定端点返回成功。
fn project_secondary_route(
    route: &ProviderExecutionRoute,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    let SecondaryRouteSelection::PinnedEndpoint { route_id, .. } = &route.secondary_route else {
        return Ok(());
    };
    let body = request_body_mut(encoded, "Secondary route")?;
    if body.contains_key("provider") {
        return Err(pre_dispatch_failure(
            "Secondary-route projection conflicts with an existing provider field.",
        ));
    }
    body.insert(
        String::from("provider"),
        json!({
            "order": [route_id],
            "allow_fallbacks": false,
        }),
    );
    Ok(())
}

/// Applies the ordered closed request-projection profile chain to one encoded wire request.
/// 把有序封闭请求投影 Profile 链应用到一个编码 Wire 请求。
///
/// Steps run in declaration order over the encoded body; the chain carries frozen profile
/// revisions for telemetry and every step is a registered primitive (plan §6.6/§R8.2).
/// 步骤按声明顺序作用于编码请求体；链携带冻结 Profile 修订用于遥测，每个步骤都是
/// 已注册原语（方案 §6.6/§R8.2）。
fn apply_request_projection_profiles(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    for profile in &route.request_projection_profiles {
        for step in &profile.steps {
            apply_mapping_step(request, encoded, step)?;
        }
    }
    Ok(())
}

/// Executes one compiled request profile over a JSON body for the executable fixture harness.
/// 为可执行 fixture 验证器在 JSON 正文上执行一份已编译请求 Profile。
///
/// # Parameters
/// 参数
/// `profile` is the exact compiled payload; `model_id` supplies the canonical request identity;
/// `body` is the already protocol-encoded JSON body consumed by production mapping steps.
/// `profile` 是精确已编译载荷；`model_id` 提供规范请求身份；`body` 是生产映射步骤消费的
/// 已完成协议编码 JSON 正文。
///
/// # Returns
/// 返回值
/// The projected body, or the same controlled pre-dispatch failure used in production.
/// 返回投影后的正文，或与生产环境相同的受控分发前错误。
pub(crate) fn execute_request_profile_fixture(
    profile: &crate::provider_manifest::RequestProjectionProfile,
    model_id: &str,
    body: Value,
) -> Result<Value, String> {
    project_json_request_profiles(std::slice::from_ref(profile), model_id, body)
}

/// Applies an ordered request-profile chain to a JSON provider-operation body.
/// 将有序请求 Profile 链应用到 JSON 供应商操作正文。
///
/// # Parameters
/// 参数
/// `profiles` are frozen payloads from one manifest generation, `model_id` is the exact
/// provider-tool target identity used by bounded templates, and `body` is the canonical JSON
/// operation body after upload materialization.
/// `profiles` 是同一清单代际冻结的载荷，`model_id` 是受限模板使用的精确供应商工具目标身份，
/// `body` 是上传物化后的规范 JSON 操作正文。
///
/// # Returns
/// 返回值
/// The projected wire body, or the production-equivalent controlled pre-dispatch diagnostic.
/// 返回投影后的 Wire 正文，或与生产一致的受控分派前诊断。
pub(crate) fn project_json_request_profiles(
    profiles: &[crate::provider_manifest::RequestProjectionProfile],
    model_id: &str,
    body: Value,
) -> Result<Value, String> {
    let request = AiExecutionRequest {
        operation: vulcan_ai_contract::AiOperation::ConversationRespond,
        model_id: model_id.to_string(),
        cache_scope_id: None,
        continuation: None,
        messages: Vec::new(),
        tools: Vec::new(),
        native_tools: Vec::new(),
        tool_choice: Default::default(),
        parallel_tool_calls: Default::default(),
        structured_output: Default::default(),
        stop_sequences: Vec::new(),
        prompt_cache: PromptCachePolicy::ProviderDefault,
        reasoning_effort: None,
        reasoning_output: Default::default(),
        model_parameters: BTreeMap::new(),
        resources: Vec::new(),
        output_token_limit: None,
    };
    let mut encoded = EncodedWireRequest {
        path: String::from("/fixture"),
        headers: BTreeMap::new(),
        body,
    };
    for profile in profiles {
        for step in &profile.steps {
            apply_mapping_step(&request, &mut encoded, step)
                .map_err(|failure| failure.error.message)?;
        }
    }
    Ok(encoded.body)
}

/// Applies an ordered response-profile chain to one JSON provider-operation response.
/// 将有序响应 Profile 链应用到一个 JSON 供应商操作响应。
///
/// # Parameters
/// 参数
/// `profiles` are frozen payloads from one manifest generation and `body` is the decoded raw
/// provider response before output-contract normalization.
/// `profiles` 是同一清单代际冻结的载荷，`body` 是输出契约归一化前已解码的原始供应商响应。
///
/// # Returns
/// 返回值
/// The projected response JSON, or a deterministic profile conversion diagnostic.
/// 返回投影后的响应 JSON，或确定性的 Profile 转换诊断。
pub(crate) fn project_json_response_profiles(
    profiles: &[crate::provider_manifest::ResponseProjectionProfile],
    mut body: Value,
) -> Result<Value, String> {
    apply_response_projection_profiles(profiles, &mut body).map_err(|error| error.to_string())?;
    Ok(body)
}

/// Applies every frozen response/error profile to one decoded provider JSON value.
/// 把每一份冻结响应/错误 Profile 应用于一个已解码供应商 JSON 值。
fn apply_response_projection_profiles(
    profiles: &[crate::provider_manifest::ResponseProjectionProfile],
    value: &mut Value,
) -> Result<(), ProtocolConversionError> {
    if profiles.is_empty() {
        return Ok(());
    }
    if !value.is_object() {
        return Err(ProtocolConversionError::new(
            "Manifest response projection requires a JSON object value.",
        ));
    }
    for profile in profiles {
        for step in &profile.steps {
            apply_response_mapping_step(value, step)?;
        }
    }
    Ok(())
}

/// Parses, projects, and re-serializes one stream or HTTP-error JSON record.
/// 解析、投影并重新序列化一条流式或 HTTP 错误 JSON 记录。
pub(crate) fn project_response_record_text(
    profiles: &[crate::provider_manifest::ResponseProjectionProfile],
    text: &str,
) -> Result<String, ProtocolConversionError> {
    if profiles.is_empty() || text.trim() == "[DONE]" {
        return Ok(text.to_string());
    }
    let mut value: Value = serde_json::from_str(text).map_err(|error| {
        ProtocolConversionError::new(format!(
            "Manifest response projection expected JSON but decoding failed: {error}"
        ))
    })?;
    apply_response_projection_profiles(profiles, &mut value)?;
    serde_json::to_string(&value).map_err(|error| {
        ProtocolConversionError::new(format!(
            "Manifest response projection failed to serialize JSON: {error}"
        ))
    })
}

/// Applies one request-independent closed mapping step to a response JSON object.
/// 把一个不依赖请求上下文的封闭映射步骤应用于响应 JSON 对象。
fn apply_response_mapping_step(
    body: &mut Value,
    step: &crate::provider_manifest::MappingStep,
) -> Result<(), ProtocolConversionError> {
    match step {
        crate::provider_manifest::MappingStep::SetLiteral { path, value } => {
            write_response_json_pointer(body, path, value.clone())?;
        }
        crate::provider_manifest::MappingStep::SetDefaultIfMissing { path, value } => {
            if pointer_value(body, path).is_none() {
                write_response_json_pointer(body, path, value.clone())?;
            }
        }
        crate::provider_manifest::MappingStep::CopyJsonPointer { from, to } => {
            if let Some(value) = pointer_value(body, from).cloned() {
                write_response_json_pointer(body, to, value)?;
            }
        }
        crate::provider_manifest::MappingStep::RenameJsonPointer { from, to } => {
            if let Some(value) = pointer_value(body, from).cloned() {
                delete_json_pointer(body, from);
                write_response_json_pointer(body, to, value)?;
            }
        }
        crate::provider_manifest::MappingStep::DeleteJsonPointer { path } => {
            delete_json_pointer(body, path);
        }
        crate::provider_manifest::MappingStep::EnumMap {
            path,
            source,
            source_path,
            mapping,
            unmatched,
            default,
        } => {
            ensure_response_body_source(*source)?;
            let source_value = pointer_value(body, source_path).cloned();
            let mapped = source_value
                .as_ref()
                .and_then(|value| value.as_str().and_then(|text| mapping.get(text).cloned()));
            match (source_value, mapped, unmatched, default) {
                (None, _, _, Some(fallback)) => {
                    write_response_json_pointer(body, path, fallback.clone())?;
                }
                (None, _, _, None) => {}
                (_, Some(mapped), _, _) => write_response_json_pointer(body, path, mapped)?,
                (Some(_), None, crate::provider_manifest::EnumMapUnmatched::Remove, _) => {
                    delete_json_pointer(body, path);
                }
                (Some(_), None, crate::provider_manifest::EnumMapUnmatched::Error, _) => {
                    return Err(ProtocolConversionError::new(format!(
                        "Response enum map source `{source_path}` is not in the declared mapping table."
                    )));
                }
                (Some(_), None, crate::provider_manifest::EnumMapUnmatched::KeepLiteral, _) => {}
            }
        }
        crate::provider_manifest::MappingStep::WrapSingletonArray { path } => {
            if let Some(value) = pointer_value(body, path).cloned() {
                write_response_json_pointer(body, path, Value::Array(vec![value]))?;
            }
        }
        crate::provider_manifest::MappingStep::UnwrapSingletonArray { path } => {
            if let Some(Value::Array(items)) = pointer_value(body, path).cloned()
                && items.len() == 1
            {
                write_response_json_pointer(
                    body,
                    path,
                    items.into_iter().next().unwrap_or(Value::Null),
                )?;
            }
        }
        crate::provider_manifest::MappingStep::ConditionalSet {
            path,
            source,
            source_path,
            equals,
            then,
            otherwise,
        } => {
            ensure_response_body_source(*source)?;
            if let Some(value) = pointer_value(body, source_path) {
                let selected = if value.as_str().is_some_and(|text| text == equals) {
                    then
                } else {
                    otherwise
                };
                write_or_delete_response(body, path, selected)?;
            }
        }
        crate::provider_manifest::MappingStep::DeleteIfEquals {
            path,
            source,
            source_path,
            equals,
        } => {
            ensure_response_body_source(*source)?;
            if pointer_value(body, source_path)
                .and_then(Value::as_str)
                .is_some_and(|text| text == equals)
            {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::DeleteUnlessEquals {
            path,
            source,
            source_path,
            equals,
        } => {
            ensure_response_body_source(*source)?;
            let retains_target = pointer_value(body, source_path)
                .and_then(Value::as_str)
                .is_some_and(|text| text == equals);
            if !retains_target {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::RemoveIfEmpty { path } => {
            let empty = matches!(pointer_value(body, path), Some(Value::Array(items)) if items.is_empty())
                || matches!(pointer_value(body, path), Some(Value::Object(items)) if items.is_empty());
            if empty {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::RejectIfPresent {
            source,
            source_path,
            message,
        } => {
            ensure_response_body_source(*source)?;
            if pointer_value(body, source_path).is_some() {
                return Err(ProtocolConversionError::new(message.clone()));
            }
        }
        crate::provider_manifest::MappingStep::RejectIfAllPresent {
            conditions,
            message,
        } => {
            for condition in conditions {
                ensure_response_body_source(condition.source)?;
            }
            if conditions
                .iter()
                .all(|condition| pointer_value(body, &condition.source_path).is_some())
            {
                return Err(ProtocolConversionError::new(message.clone()));
            }
        }
        crate::provider_manifest::MappingStep::StringTemplate { .. }
        | crate::provider_manifest::MappingStep::CopyRequestField { .. } => {
            return Err(ProtocolConversionError::new(
                "Response projection contains a request-only mapping step.",
            ));
        }
        crate::provider_manifest::MappingStep::RebuildJsonTemplate { template } => {
            let source = body.clone();
            let rendered = render_complete_json_template(template, &source, "")
                .map_err(ProtocolConversionError::new)?;
            if !rendered.is_object() {
                return Err(ProtocolConversionError::new(
                    "Manifest response JSON template must render an object body.",
                ));
            }
            *body = rendered;
        }
    }
    Ok(())
}

/// Enforces that response mapping sources come only from the decoded response body.
/// 强制响应映射源只能来自已解码响应正文。
fn ensure_response_body_source(
    source: crate::provider_manifest::MappingStepSource,
) -> Result<(), ProtocolConversionError> {
    if source == crate::provider_manifest::MappingStepSource::BodyJsonPointer {
        Ok(())
    } else {
        Err(ProtocolConversionError::new(
            "Response projection may not read host request fields.",
        ))
    }
}

/// Safely writes one response JSON pointer without panicking on remote scalar ancestors.
/// 安全写入一个响应 JSON Pointer，远端标量祖先存在时不会触发 panic。
fn write_response_json_pointer(
    body: &mut Value,
    path: &str,
    value: Value,
) -> Result<(), ProtocolConversionError> {
    let segments = pointer_segments(path);
    if segments.is_empty() {
        return Err(ProtocolConversionError::new(
            "Response projection target JSON pointer is empty.",
        ));
    }
    let mut current = body;
    for segment in &segments[..segments.len() - 1] {
        let object = current.as_object_mut().ok_or_else(|| {
            ProtocolConversionError::new(format!(
                "Response projection target `{path}` crosses a non-object ancestor."
            ))
        })?;
        current = object
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    let object = current.as_object_mut().ok_or_else(|| {
        ProtocolConversionError::new(format!(
            "Response projection target `{path}` has a non-object parent."
        ))
    })?;
    let last = segments.last().expect("validated non-empty response path");
    object.insert(last.to_string(), value);
    Ok(())
}

/// Writes a response value or deletes the target when the selected value is null.
/// 写入响应值；所选值为 null 时删除目标。
fn write_or_delete_response(
    body: &mut Value,
    path: &str,
    value: &Value,
) -> Result<(), ProtocolConversionError> {
    if value.is_null() {
        delete_json_pointer(body, path);
        Ok(())
    } else {
        write_response_json_pointer(body, path, value.clone())
    }
}

/// Applies one closed mapping step to the encoded wire request.
/// 把一个封闭映射步骤应用到编码 Wire 请求。
/// Maximum array entries emitted by one declarative JSON-template map expression.
/// 单个声明式 JSON 模板映射表达式允许输出的最大数组项数。
const JSON_TEMPLATE_MAX_ARRAY_ITEMS: usize = 256;

/// Maximum serialized byte size emitted by one declarative JSON template.
/// 单份声明式 JSON 模板允许输出的最大序列化字节数。
const JSON_TEMPLATE_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

/// One internal template rendering outcome, including an explicitly omitted optional value.
/// 一种内部模板渲染结果，包含被显式省略的可选值。
enum JsonTemplateRender {
    /// One concrete rendered JSON value.
    /// 一份具体的已渲染 JSON 值。
    Value(Value),
    /// One absent optional source omitted from its containing object or array.
    /// 一个从所属对象或数组中省略的缺失可选来源。
    Omitted,
}

/// Immutable source context available to one bounded JSON-template expression.
/// 单个有界 JSON 模板表达式可读取的不可变来源上下文。
struct JsonTemplateContext<'a> {
    /// Complete body captured before the rebuild step starts.
    /// 重建步骤开始前捕获的完整正文。
    body: &'a Value,
    /// Current array item while rendering a registered map expression.
    /// 渲染已注册映射表达式时的当前数组项。
    item: Option<&'a Value>,
    /// Exact immutable model or upstream target identifier.
    /// 精确不可变模型或上游目标标识。
    model_id: &'a str,
}

/// Renders one complete body from the closed declarative JSON-template language.
/// 使用封闭的声明式 JSON 模板语言渲染一份完整正文。
///
/// # Parameters
/// 参数
/// `template` is the manifest-owned template, `source` is the exact original body, and
/// `model_id` is the already frozen route identifier.
/// `template` 是 Manifest 持有的模板，`source` 是精确原始正文，`model_id` 是已冻结路由标识。
///
/// # Returns
/// 返回值
/// A bounded JSON value or one controlled validation/rendering error.
/// 返回有界 JSON 值或一条受控校验/渲染错误。
fn render_complete_json_template(
    template: &Value,
    source: &Value,
    model_id: &str,
) -> Result<Value, String> {
    let context = JsonTemplateContext {
        body: source,
        item: None,
        model_id,
    };
    let rendered = match render_json_template_value(template, &context, 0)? {
        JsonTemplateRender::Value(value) => value,
        JsonTemplateRender::Omitted => {
            return Err(String::from(
                "A complete JSON template cannot resolve to an omitted optional value.",
            ));
        }
    };
    let serialized = serde_json::to_vec(&rendered)
        .map_err(|error| format!("Rendered JSON template could not be serialized: {error}"))?;
    if serialized.len() > JSON_TEMPLATE_MAX_OUTPUT_BYTES {
        return Err(format!(
            "Rendered JSON template exceeds the {JSON_TEMPLATE_MAX_OUTPUT_BYTES} byte limit."
        ));
    }
    Ok(rendered)
}

/// Renders one recursive declarative JSON-template node under fixed depth and array limits.
/// 在固定深度与数组限制下渲染一个递归声明式 JSON 模板节点。
fn render_json_template_value(
    template: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    if depth > crate::provider_manifest::MAX_JSON_DEPTH {
        return Err(format!(
            "JSON template exceeds the {} level depth limit.",
            crate::provider_manifest::MAX_JSON_DEPTH
        ));
    }
    match template {
        Value::Object(object) => {
            if let Some((tag, specification)) = json_template_registered_form(object)? {
                return render_json_template_form(tag, specification, context, depth + 1);
            }
            let mut rendered = serde_json::Map::new();
            for (key, value) in object {
                match render_json_template_value(value, context, depth + 1)? {
                    JsonTemplateRender::Value(value) => {
                        rendered.insert(key.clone(), value);
                    }
                    JsonTemplateRender::Omitted => {}
                }
            }
            Ok(JsonTemplateRender::Value(Value::Object(rendered)))
        }
        Value::Array(values) => {
            if values.len() > JSON_TEMPLATE_MAX_ARRAY_ITEMS {
                return Err(format!(
                    "JSON template literal array exceeds the {JSON_TEMPLATE_MAX_ARRAY_ITEMS} item limit."
                ));
            }
            let mut rendered = Vec::with_capacity(values.len());
            for value in values {
                if let JsonTemplateRender::Value(value) =
                    render_json_template_value(value, context, depth + 1)?
                {
                    rendered.push(value);
                }
            }
            Ok(JsonTemplateRender::Value(Value::Array(rendered)))
        }
        scalar => Ok(JsonTemplateRender::Value(scalar.clone())),
    }
}

/// Detects one exact registered template form and rejects ambiguous reserved-key objects.
/// 识别一个精确已注册模板形式，并拒绝含糊的保留键对象。
fn json_template_registered_form(
    object: &serde_json::Map<String, Value>,
) -> Result<Option<(&str, &Value)>, String> {
    let reserved = object
        .iter()
        .filter(|(key, _)| key.starts_with("$vulcan_"))
        .collect::<Vec<_>>();
    if reserved.is_empty() {
        return Ok(None);
    }
    if object.len() != 1 || reserved.len() != 1 {
        return Err(String::from(
            "A registered JSON template form must be the object's only property.",
        ));
    }
    let (tag, specification) = reserved[0];
    match tag.as_str() {
        "$vulcan_ref"
        | "$vulcan_model_id"
        | "$vulcan_map"
        | "$vulcan_enum"
        | "$vulcan_if_equals"
        | "$vulcan_if_present"
        | "$vulcan_concat"
        | "$vulcan_if_array_length"
        | "$vulcan_array_concat"
        | "$vulcan_coalesce"
        | "$vulcan_trim" => Ok(Some((tag.as_str(), specification))),
        _ => Err(format!("JSON template form `{tag}` is not registered.")),
    }
}

/// Executes one exact registered template form without dynamic code or reflective field lookup.
/// 在不执行动态代码或反射字段查找的情况下执行一个精确已注册模板形式。
fn render_json_template_form(
    tag: &str,
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    match tag {
        "$vulcan_ref" => render_json_template_reference(specification, context),
        "$vulcan_model_id" => {
            if specification != &Value::Bool(true) {
                return Err(String::from(
                    "$vulcan_model_id must carry the literal boolean true.",
                ));
            }
            Ok(JsonTemplateRender::Value(Value::String(
                context.model_id.to_string(),
            )))
        }
        "$vulcan_map" => render_json_template_map(specification, context, depth),
        "$vulcan_enum" => render_json_template_enum(specification, context),
        "$vulcan_if_equals" => render_json_template_condition(specification, context, depth),
        "$vulcan_if_present" => {
            render_json_template_presence_condition(specification, context, depth)
        }
        "$vulcan_if_array_length" => {
            render_json_template_array_length_condition(specification, context, depth)
        }
        "$vulcan_concat" => render_json_template_concat(specification, context, depth),
        "$vulcan_array_concat" => render_json_template_array_concat(specification, context, depth),
        "$vulcan_coalesce" => render_json_template_coalesce(specification, context, depth),
        "$vulcan_trim" => render_json_template_trimmed_string(specification, context),
        _ => Err(format!("JSON template form `{tag}` is not registered.")),
    }
}

/// Resolves one body or current-item JSON Pointer declared by a template form.
/// 解析模板形式声明的一个正文或当前项 JSON Pointer。
fn json_template_source<'a>(
    specification: &'a serde_json::Map<String, Value>,
    context: &'a JsonTemplateContext<'a>,
) -> Result<Option<&'a Value>, String> {
    let scope = specification
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("body");
    let path = specification
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("JSON template source requires a string `path`."))?;
    if !path.is_empty() && !path.starts_with('/') {
        return Err(String::from(
            "JSON template source path must be empty or an absolute JSON Pointer.",
        ));
    }
    let source = match scope {
        "body" => context.body,
        "item" => context.item.ok_or_else(|| {
            String::from("JSON template item scope is only available inside $vulcan_map.")
        })?,
        _ => {
            return Err(format!(
                "JSON template source scope `{scope}` is not registered."
            ));
        }
    };
    Ok(if path.is_empty() {
        Some(source)
    } else {
        source.pointer(path)
    })
}

/// Renders one required or optional JSON Pointer reference.
/// 渲染一个必需或可选的 JSON Pointer 引用。
fn render_json_template_reference(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_ref requires an object specification."))?;
    let optional = object
        .get("optional")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let omit_if_empty = object
        .get("omit_if_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match json_template_source(object, context)? {
        Some(value)
            if !value.is_null()
                && !(omit_if_empty
                    && (matches!(value, Value::String(text) if text.is_empty())
                        || matches!(value, Value::Array(items) if items.is_empty())
                        || matches!(value, Value::Object(items) if items.is_empty()))) =>
        {
            Ok(JsonTemplateRender::Value(value.clone()))
        }
        _ if optional => Ok(JsonTemplateRender::Omitted),
        _ => Err(String::from(
            "$vulcan_ref could not resolve its required source value.",
        )),
    }
}

/// Resolves and trims one bounded string source without changing any other value type.
/// 解析并裁剪一个有界字符串源，不改变任何其他值类型。
///
/// # Parameters
/// 参数
/// `specification` declares the exact source pointer and optionality; `context` owns the frozen
/// body and current-item roots.
/// `specification` 声明精确源 Pointer 与可选性；`context` 持有冻结正文与当前条目根。
///
/// # Returns
/// 返回值
/// A non-empty trimmed string, an omitted optional value, or a deterministic type/presence error.
/// 返回非空裁剪字符串、省略的可选值，或确定性的类型与存在性错误。
fn render_json_template_trimmed_string(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_trim requires an object specification."))?;
    let optional = object
        .get("optional")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(value) = json_template_source(object, context)? else {
        return if optional {
            Ok(JsonTemplateRender::Omitted)
        } else {
            Err(String::from(
                "$vulcan_trim could not resolve its required source value.",
            ))
        };
    };
    let trimmed = value
        .as_str()
        .ok_or_else(|| String::from("$vulcan_trim source must be a string."))?
        .trim();
    if trimmed.is_empty() {
        return if optional {
            Ok(JsonTemplateRender::Omitted)
        } else {
            Err(String::from(
                "$vulcan_trim source must contain non-whitespace text.",
            ))
        };
    }
    Ok(JsonTemplateRender::Value(Value::String(
        trimmed.to_string(),
    )))
}

/// Renders one bounded source array through a nested template for each item.
/// 使用嵌套模板逐项渲染一个有界来源数组。
fn render_json_template_map(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_map requires an object specification."))?;
    let template = object
        .get("template")
        .ok_or_else(|| String::from("$vulcan_map requires a `template`."))?;
    let optional = object
        .get("optional")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let flatten = object
        .get("flatten")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(source) = json_template_source(object, context)? else {
        return if optional {
            Ok(JsonTemplateRender::Omitted)
        } else {
            Err(String::from(
                "$vulcan_map could not resolve its source array.",
            ))
        };
    };
    let items = source
        .as_array()
        .ok_or_else(|| String::from("$vulcan_map source must be an array."))?;
    if items.len() > JSON_TEMPLATE_MAX_ARRAY_ITEMS {
        return Err(format!(
            "$vulcan_map source exceeds the {JSON_TEMPLATE_MAX_ARRAY_ITEMS} item limit."
        ));
    }
    let mut rendered = Vec::with_capacity(items.len());
    for item in items {
        let item_context = JsonTemplateContext {
            body: context.body,
            item: Some(item),
            model_id: context.model_id,
        };
        if let JsonTemplateRender::Value(value) =
            render_json_template_value(template, &item_context, depth + 1)?
        {
            if flatten {
                let values = value.as_array().ok_or_else(|| {
                    String::from("$vulcan_map with flatten=true must render arrays.")
                })?;
                if rendered.len().saturating_add(values.len()) > JSON_TEMPLATE_MAX_ARRAY_ITEMS {
                    return Err(format!(
                        "$vulcan_map flattened output exceeds the {JSON_TEMPLATE_MAX_ARRAY_ITEMS} item limit."
                    ));
                }
                rendered.extend(values.iter().cloned());
            } else {
                rendered.push(value);
            }
        }
    }
    Ok(JsonTemplateRender::Value(Value::Array(rendered)))
}

/// Maps one exact string source through a bounded literal table.
/// 通过有界字面量表映射一个精确字符串来源。
fn render_json_template_enum(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_enum requires an object specification."))?;
    let mapping = object
        .get("mapping")
        .and_then(Value::as_object)
        .ok_or_else(|| String::from("$vulcan_enum requires an object `mapping`."))?;
    let optional = object
        .get("optional")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let Some(source) = json_template_source(object, context)? else {
        return match object.get("default") {
            Some(value) => Ok(JsonTemplateRender::Value(value.clone())),
            None if optional => Ok(JsonTemplateRender::Omitted),
            None => Err(String::from("$vulcan_enum could not resolve its source.")),
        };
    };
    let source = source
        .as_str()
        .ok_or_else(|| String::from("$vulcan_enum source must be a string."))?;
    match mapping.get(source).or_else(|| object.get("default")) {
        Some(value) => Ok(JsonTemplateRender::Value(value.clone())),
        None if optional => Ok(JsonTemplateRender::Omitted),
        None => Err(format!(
            "$vulcan_enum source value `{source}` is not in its mapping table."
        )),
    }
}

/// Selects and renders one of two templates using exact JSON equality.
/// 使用精确 JSON 相等判断选择并渲染两个模板之一。
fn render_json_template_condition(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_if_equals requires an object specification."))?;
    let expected = object
        .get("equals")
        .ok_or_else(|| String::from("$vulcan_if_equals requires `equals`."))?;
    let selected = if json_template_source(object, context)? == Some(expected) {
        object.get("then")
    } else {
        object.get("otherwise")
    };
    match selected {
        Some(template) => render_json_template_value(template, context, depth + 1),
        None => Ok(JsonTemplateRender::Omitted),
    }
}

/// Renders one nested template only when its exact JSON Pointer resolves to a non-null value.
/// 仅当精确 JSON Pointer 解析为非 null 值时渲染一个嵌套模板。
///
/// # Parameters
/// 参数
/// `specification` declares the source and the required `then` template; `context` is immutable.
/// `specification` 声明来源与必需的 `then` 模板；`context` 保持不可变。
///
/// # Returns
/// 返回值
/// The rendered branch when present, otherwise an explicit omission marker.
/// 当来源存在时返回已渲染分支，否则返回显式省略标记。
fn render_json_template_presence_condition(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_if_present requires an object specification."))?;
    if !object.contains_key("then") && !object.contains_key("otherwise") {
        return Err(String::from(
            "$vulcan_if_present requires `then` or `otherwise`.",
        ));
    }
    let non_empty = object
        .get("non_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let selected = match json_template_source(object, context)? {
        Some(value)
            if !value.is_null()
                && !(non_empty
                    && (matches!(value, Value::String(text) if text.is_empty())
                        || matches!(value, Value::Array(items) if items.is_empty())
                        || matches!(value, Value::Object(items) if items.is_empty()))) =>
        {
            object.get("then")
        }
        _ => object.get("otherwise"),
    };
    match selected {
        Some(template) => render_json_template_value(template, context, depth + 1),
        None => Ok(JsonTemplateRender::Omitted),
    }
}

/// Selects one template branch by the exact length of a resolved JSON array.
/// 按已解析 JSON 数组的精确长度选择一个模板分支。
///
/// # Parameters
/// 参数
/// `specification` declares the source, expected length, and optional branches.
/// `specification` 声明来源、期望长度与可选分支。
///
/// # Returns
/// 返回值
/// The selected rendered branch, or omission when the selected branch is absent.
/// 返回所选已渲染分支；所选分支缺失时返回省略标记。
fn render_json_template_array_length_condition(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let object = specification
        .as_object()
        .ok_or_else(|| String::from("$vulcan_if_array_length requires an object specification."))?;
    let expected = object
        .get("equals")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value <= JSON_TEMPLATE_MAX_ARRAY_ITEMS)
        .ok_or_else(|| {
            format!(
                "$vulcan_if_array_length equals must be an integer from 0 through {JSON_TEMPLATE_MAX_ARRAY_ITEMS}."
            )
        })?;
    let actual = json_template_source(object, context)?
        .and_then(Value::as_array)
        .map(Vec::len);
    let selected = if actual == Some(expected) {
        object.get("then")
    } else {
        object.get("otherwise")
    };
    match selected {
        Some(template) => render_json_template_value(template, context, depth + 1),
        None => Ok(JsonTemplateRender::Omitted),
    }
}

/// Concatenates rendered scalar template values into one bounded string.
/// 把已渲染的标量模板值拼接为一个有界字符串。
fn render_json_template_concat(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let values = specification
        .as_array()
        .ok_or_else(|| String::from("$vulcan_concat requires an array specification."))?;
    if values.len() > JSON_TEMPLATE_MAX_ARRAY_ITEMS {
        return Err(format!(
            "$vulcan_concat exceeds the {JSON_TEMPLATE_MAX_ARRAY_ITEMS} item limit."
        ));
    }
    let mut output = String::new();
    for value in values {
        let rendered = match render_json_template_value(value, context, depth + 1)? {
            JsonTemplateRender::Value(value) => value,
            JsonTemplateRender::Omitted => continue,
        };
        match rendered {
            Value::String(value) => output.push_str(value.as_str()),
            Value::Number(value) => output.push_str(value.to_string().as_str()),
            Value::Bool(value) => output.push_str(if value { "true" } else { "false" }),
            _ => {
                return Err(String::from(
                    "$vulcan_concat items must render to strings, numbers, or booleans.",
                ));
            }
        }
        if output.len() > JSON_TEMPLATE_MAX_OUTPUT_BYTES {
            return Err(format!(
                "$vulcan_concat exceeds the {JSON_TEMPLATE_MAX_OUTPUT_BYTES} byte limit."
            ));
        }
    }
    Ok(JsonTemplateRender::Value(Value::String(output)))
}

/// Concatenates rendered array templates into one bounded ordered array.
/// 把已渲染的数组模板拼接为一个有界有序数组。
///
/// # Parameters
/// 参数
/// `specification` is an ordered bounded template list and `context` is the immutable source.
/// `specification` 是有序有界模板列表，`context` 是不可变来源。
///
/// # Returns
/// 返回值
/// One flattened array whose total item count never exceeds the shared template ceiling.
/// 返回一个扁平数组，其总元素数量永不超过共享模板上限。
fn render_json_template_array_concat(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    let templates = specification
        .as_array()
        .filter(|templates| {
            !templates.is_empty() && templates.len() <= JSON_TEMPLATE_MAX_ARRAY_ITEMS
        })
        .ok_or_else(|| {
            format!(
                "$vulcan_array_concat requires between 1 and {JSON_TEMPLATE_MAX_ARRAY_ITEMS} templates."
            )
        })?;
    let mut output = Vec::new();
    for template in templates {
        let rendered = match render_json_template_value(template, context, depth + 1)? {
            JsonTemplateRender::Value(value) => value,
            JsonTemplateRender::Omitted => continue,
        };
        let values = rendered
            .as_array()
            .ok_or_else(|| String::from("$vulcan_array_concat templates must render to arrays."))?;
        if output.len().saturating_add(values.len()) > JSON_TEMPLATE_MAX_ARRAY_ITEMS {
            return Err(format!(
                "$vulcan_array_concat exceeds the {JSON_TEMPLATE_MAX_ARRAY_ITEMS} item limit."
            ));
        }
        output.extend(values.iter().cloned());
    }
    Ok(JsonTemplateRender::Value(Value::Array(output)))
}

/// Selects the first non-omitted value from an ordered bounded template list.
/// 从一份有序有界模板列表中选择首个未省略值。
///
/// # Parameters
/// 参数
/// `specification` is the closed ordered template array and `context` is the immutable source.
/// `specification` 是封闭有序模板数组，`context` 是不可变来源。
///
/// # Returns
/// 返回值
/// The first rendered value, or `Omitted` when every candidate is explicitly optional and absent.
/// 返回首个已渲染值；当每个候选均显式可选且缺失时返回 `Omitted`。
fn render_json_template_coalesce(
    specification: &Value,
    context: &JsonTemplateContext<'_>,
    depth: usize,
) -> Result<JsonTemplateRender, String> {
    // Candidate count is bounded by the same collection ceiling as literal arrays and maps.
    // 候选数量与字面数组、映射使用相同集合上限。
    let candidates = specification
        .as_array()
        .filter(|candidates| {
            !candidates.is_empty() && candidates.len() <= JSON_TEMPLATE_MAX_ARRAY_ITEMS
        })
        .ok_or_else(|| {
            format!(
                "$vulcan_coalesce requires between 1 and {JSON_TEMPLATE_MAX_ARRAY_ITEMS} candidates."
            )
        })?;
    for candidate in candidates {
        match render_json_template_value(candidate, context, depth + 1)? {
            JsonTemplateRender::Value(value) => {
                return Ok(JsonTemplateRender::Value(value));
            }
            JsonTemplateRender::Omitted => {}
        }
    }
    Ok(JsonTemplateRender::Omitted)
}

fn apply_mapping_step(
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
    step: &crate::provider_manifest::MappingStep,
) -> Result<(), ProviderExecutionFailure> {
    if !encoded.body.is_object() {
        return Err(pre_dispatch_failure(
            "Manifest request projection requires a JSON object body.",
        ));
    }
    let body = &mut encoded.body;
    match step {
        crate::provider_manifest::MappingStep::SetLiteral { path, value } => {
            write_json_pointer(body, path, value.clone())?;
        }
        crate::provider_manifest::MappingStep::SetDefaultIfMissing { path, value } => {
            if pointer_value(body, path).is_none() {
                write_json_pointer(body, path, value.clone())?;
            }
        }
        crate::provider_manifest::MappingStep::CopyJsonPointer { from, to } => {
            if let Some(value) = pointer_value(body, from).cloned() {
                write_json_pointer(body, to, value)?;
            }
        }
        crate::provider_manifest::MappingStep::RenameJsonPointer { from, to } => {
            if let Some(value) = pointer_value(body, from).cloned() {
                delete_json_pointer(body, from);
                write_json_pointer(body, to, value)?;
            }
        }
        crate::provider_manifest::MappingStep::DeleteJsonPointer { path } => {
            delete_json_pointer(body, path);
        }
        crate::provider_manifest::MappingStep::EnumMap {
            path,
            source,
            source_path,
            mapping,
            unmatched,
            default,
        } => {
            let value = projection_source_value(*source, source_path, request, body);
            let mapped = value
                .as_ref()
                .and_then(|value| value.as_str().and_then(|text| mapping.get(text).cloned()));
            match (value, mapped, unmatched, default) {
                (None, _, _, Some(fallback)) => {
                    write_json_pointer(body, path, fallback.clone())?;
                }
                (None, _, _, None) => {}
                (_, Some(mapped), _, _) => {
                    write_json_pointer(body, path, mapped)?;
                }
                (Some(_), None, _, _) => match unmatched {
                    crate::provider_manifest::EnumMapUnmatched::Remove => {
                        delete_json_pointer(body, path);
                    }
                    crate::provider_manifest::EnumMapUnmatched::Error => {
                        return Err(pre_dispatch_failure(format!(
                            "Enum map source `{source_path}` value is not in the declared mapping table."
                        )));
                    }
                    crate::provider_manifest::EnumMapUnmatched::KeepLiteral => {}
                },
            }
        }
        crate::provider_manifest::MappingStep::WrapSingletonArray { path } => {
            if let Some(value) = pointer_value(body, path).cloned() {
                write_json_pointer(body, path, Value::Array(vec![value]))?;
            }
        }
        crate::provider_manifest::MappingStep::UnwrapSingletonArray { path } => {
            if let Some(Value::Array(items)) = pointer_value(body, path).cloned()
                && items.len() == 1
            {
                write_json_pointer(body, path, items.into_iter().next().unwrap_or(Value::Null))?;
            }
        }
        crate::provider_manifest::MappingStep::StringTemplate { path, template } => {
            let rendered = template.replace("{model_id}", &request.model_id);
            write_json_pointer(body, path, Value::String(rendered))?;
        }
        crate::provider_manifest::MappingStep::CopyRequestField { path, field } => {
            if let Some(value) = request_field_value(field, request) {
                write_json_pointer(body, path, value)?;
            }
        }
        crate::provider_manifest::MappingStep::ConditionalSet {
            path,
            source,
            source_path,
            equals,
            then,
            otherwise,
        } => {
            let value = projection_source_value(*source, source_path, request, body);
            let matched = value
                .as_ref()
                .and_then(|value| value.as_str())
                .is_some_and(|text| text == equals);
            match (value, matched) {
                (None, _) => {}
                (_, true) => write_or_delete(body, path, then)?,
                (_, false) => write_or_delete(body, path, otherwise)?,
            }
        }
        crate::provider_manifest::MappingStep::DeleteIfEquals {
            path,
            source,
            source_path,
            equals,
        } => {
            let value = projection_source_value(*source, source_path, request, body);
            if value
                .as_ref()
                .and_then(|value| value.as_str())
                .is_some_and(|text| text == equals)
            {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::DeleteUnlessEquals {
            path,
            source,
            source_path,
            equals,
        } => {
            let retains_target = projection_source_value(*source, source_path, request, body)
                .as_ref()
                .and_then(Value::as_str)
                .is_some_and(|text| text == equals);
            if !retains_target {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::RemoveIfEmpty { path } => {
            let empty = matches!(
                pointer_value(body, path),
                Some(Value::Array(items)) if items.is_empty()
            ) || matches!(
                pointer_value(body, path),
                Some(Value::Object(items)) if items.is_empty()
            );
            if empty {
                delete_json_pointer(body, path);
            }
        }
        crate::provider_manifest::MappingStep::RejectIfPresent {
            source,
            source_path,
            message,
        } => {
            if projection_source_value(*source, source_path, request, body).is_some() {
                return Err(pre_dispatch_failure(message.clone()));
            }
        }
        crate::provider_manifest::MappingStep::RejectIfAllPresent {
            conditions,
            message,
        } => {
            if conditions.iter().all(|condition| {
                projection_source_value(condition.source, &condition.source_path, request, body)
                    .is_some()
            }) {
                return Err(pre_dispatch_failure(message.clone()));
            }
        }
        crate::provider_manifest::MappingStep::RebuildJsonTemplate { template } => {
            let source = body.clone();
            let rendered = render_complete_json_template(template, &source, &request.model_id)
                .map_err(pre_dispatch_failure)?;
            if !rendered.is_object() {
                return Err(pre_dispatch_failure(
                    "Manifest request JSON template must render an object body.",
                ));
            }
            *body = rendered;
        }
    }
    Ok(())
}

/// Safely writes one request JSON pointer while creating missing object ancestors.
/// 安全写入一个请求 JSON Pointer，并创建缺失的对象祖先。
///
/// # Parameters
/// 参数
/// `body` is the protocol-encoded request object, `path` is the validated manifest pointer, and
/// `value` is the exact projected value.
/// `body` 是协议编码后的请求对象，`path` 是已校验的 Manifest 指针，`value` 是精确投影值。
///
/// # Returns
/// 返回值
/// Success after insertion, or a controlled pre-dispatch failure when runtime data places a
/// scalar at an ancestor that the manifest expects to be an object.
/// 插入成功时返回成功；若运行时数据在 Manifest 期望为对象的祖先位置放置标量，则返回受控的
/// 分发前失败。
fn write_json_pointer(
    body: &mut Value,
    path: &str,
    value: Value,
) -> Result<(), ProviderExecutionFailure> {
    let segments = pointer_segments(path);
    if segments.is_empty() {
        return Err(pre_dispatch_failure(
            "Request projection target JSON pointer is empty.",
        ));
    }
    let mut current: &mut Value = body;
    for segment in &segments[..segments.len() - 1] {
        let object = current.as_object_mut().ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Request projection target `{path}` crosses a non-object ancestor."
            ))
        })?;
        let next = object
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        current = next;
    }
    let object = current.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure(format!(
            "Request projection target `{path}` has a non-object parent."
        ))
    })?;
    let Some(last) = segments.last() else {
        return Err(pre_dispatch_failure(
            "Request projection target JSON pointer is empty.",
        ));
    };
    object.insert(last.to_string(), value);
    Ok(())
}

/// Deletes one JSON pointer path, pruning now-empty ancestors.
/// 删除一个 JSON Pointer 路径并修剪变空的祖先。
fn delete_json_pointer(body: &mut Value, path: &str) {
    let segments = pointer_segments(path);
    if segments.is_empty() {
        return;
    }
    delete_pointer_recursive(body, &segments);
}

fn delete_pointer_recursive(value: &mut Value, segments: &[String]) {
    let Some((first, rest)) = segments.split_first() else {
        return;
    };
    if rest.is_empty() {
        match value {
            Value::Object(object) => {
                object.remove(first);
            }
            Value::Array(items) => {
                if let Some(index) = first.parse::<usize>().ok()
                    && index < items.len()
                {
                    items.remove(index);
                }
            }
            _ => {}
        }
        return;
    }
    match value {
        Value::Object(object) => {
            let Some(child) = object.get_mut(first) else {
                return;
            };
            delete_pointer_recursive(child, rest);
            if matches!(child, Value::Object(nested) if nested.is_empty()) {
                object.remove(first);
            }
        }
        Value::Array(items) => {
            let Some(index) = first.parse::<usize>().ok() else {
                return;
            };
            if index < items.len() {
                let child = &mut items[index];
                delete_pointer_recursive(child, rest);
                if matches!(child, Value::Object(nested) if nested.is_empty()) {
                    items.remove(index);
                }
            }
        }
        _ => {}
    }
}

/// Reads one JSON pointer value from a request body map.
/// 从请求体映射读取一个 JSON Pointer 值。
fn pointer_value<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    value.pointer(path)
}

/// Splits one JSON pointer into unescaped segments.
/// 把一个 JSON Pointer 拆分为未转义分段。
fn pointer_segments(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect()
}

/// Writes one request value or deletes the target when the value is null.
/// 写入一个请求值；值为 null 时删除目标。
///
/// # Parameters
/// 参数
/// `body`, `path`, and `value` carry the same runtime projection boundary as
/// `write_json_pointer`.
/// `body`、`path` 与 `value` 承载与 `write_json_pointer` 相同的运行时投影边界。
///
/// # Returns
/// 返回值
/// Success after deletion/insertion, or the controlled pointer-write failure.
/// 删除或插入后返回成功，或返回受控的指针写入失败。
fn write_or_delete(
    body: &mut Value,
    path: &str,
    value: &Value,
) -> Result<(), ProviderExecutionFailure> {
    if value.is_null() {
        delete_json_pointer(body, path);
        Ok(())
    } else {
        write_json_pointer(body, path, value.clone())
    }
}

/// Resolves one projection source value from a body pointer or a validated host field.
/// 从请求体指针或已校验宿主字段解析一个投影源值。
fn projection_source_value(
    source: crate::provider_manifest::MappingStepSource,
    source_path: &str,
    request: &AiExecutionRequest,
    body: &Value,
) -> Option<Value> {
    match source {
        crate::provider_manifest::MappingStepSource::BodyJsonPointer => {
            body.pointer(source_path).cloned()
        }
        crate::provider_manifest::MappingStepSource::RequestField => {
            if let Some(parameter_id) = source_path.strip_prefix("@model_parameters.") {
                request
                    .model_parameters
                    .get(parameter_id)
                    .and_then(model_parameter_to_value)
            } else {
                request_field_value(source_path, request)
            }
        }
    }
}

/// Resolves one bounded host request field value.
/// 解析一个有界宿主请求字段值。
fn request_field_value(field: &str, request: &AiExecutionRequest) -> Option<Value> {
    // The optional `@` marker distinguishes request fields from body pointers; both call
    // sites (CopyRequestField and projection_source_value) pass the raw path, so the marker
    // is stripped here once.
    // 可选 `@` 标记用于区分请求字段与请求体指针；两个调用点（CopyRequestField 与
    // projection_source_value）都传原始路径，因此在此统一剥离标记。
    let field = field.strip_prefix('@').unwrap_or(field);
    match field {
        "reasoning_effort" => request
            .reasoning_effort
            .map(|effort| Value::String(effort.as_str().to_string())),
        "model_id" => Some(Value::String(request.model_id.clone())),
        "output_token_limit" => request
            .output_token_limit
            .map(|limit| Value::Number(limit.into())),
        "cache_scope_id" => request
            .cache_scope_id
            .as_ref()
            .map(|scope| Value::String(scope.clone())),
        "prompt_cache" => (!matches!(request.prompt_cache, PromptCachePolicy::ProviderDefault))
            .then_some(Value::Bool(true)),
        _ => field
            .strip_prefix("model_parameters.")
            .and_then(|parameter_id| {
                request
                    .model_parameters
                    .get(parameter_id)
                    .and_then(model_parameter_to_value)
            }),
    }
}

/// Converts one strongly typed model parameter value into a wire JSON value.
/// 把一个强类型模型参数值转换为 Wire JSON 值。
fn model_parameter_to_value(value: &ModelParameterValue) -> Option<Value> {
    match value {
        ModelParameterValue::Boolean(value) => Some(Value::Bool(*value)),
        ModelParameterValue::Integer(value) => Some(Value::Number((*value).into())),
        ModelParameterValue::Decimal(value) => Some(Value::String(value.clone())),
        ModelParameterValue::String(value) => Some(Value::String(value.clone())),
        ModelParameterValue::Array(values) => Some(Value::Array(
            values.iter().filter_map(model_parameter_to_value).collect(),
        )),
        ModelParameterValue::Object(values) => Some(Value::Object(
            values
                .iter()
                .filter_map(|(key, value)| {
                    model_parameter_to_value(value).map(|value| (key.clone(), value))
                })
                .collect(),
        )),
    }
}

/// 验证协议原生 JSON Schema 字段在供应商专属请求投影后仍然存在。
///
/// # Parameters
/// 参数
/// `route` identifies the exact protocol, `request` carries the requested schema, and `encoded`
/// is the final request after all provider and custom projections.
/// `route` 标识精确协议，`request` 携带请求 Schema，`encoded` 是完成全部供应商与自定义投影后的最终请求。
///
/// # Returns
/// 返回值
/// Success when the protocol carries the exact requested schema, or a pre-dispatch failure when
/// a supported Chat/Responses/Messages projection dropped or changed it.
/// 当协议携带精确请求 Schema 时返回成功；支持的 Chat、Responses、Messages 投影丢失或改写 Schema 时返回分派前失败。
fn validate_structured_output_projection(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    encoded: &EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    let StructuredOutputPolicy::JsonSchema { schema, .. } = &request.structured_output else {
        return Ok(());
    };
    let protocol_id = route.protocol_route.protocol_profile_id.as_str();
    let (projected_type, projected_schema) = match protocol_id {
        ANTHROPIC_MESSAGES_PROFILE_ID
        | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | CLAUDE_CODE_MESSAGES_PROFILE_ID => (
            encoded
                .body
                .pointer("/output_config/format/type")
                .and_then(Value::as_str),
            encoded.body.pointer("/output_config/format/schema"),
        ),
        OPENAI_CHAT_PROFILE_ID => (
            encoded
                .body
                .pointer("/response_format/type")
                .and_then(Value::as_str),
            encoded.body.pointer("/response_format/json_schema/schema"),
        ),
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => (
            encoded
                .body
                .pointer("/text/format/type")
                .and_then(Value::as_str),
            encoded.body.pointer("/text/format/schema"),
        ),
        GEMINI_GENERATE_CONTENT_PROFILE_ID => (
            encoded
                .body
                .pointer("/generationConfig/responseMimeType")
                .and_then(Value::as_str),
            encoded.body.pointer("/generationConfig/responseJsonSchema"),
        ),
        GOOGLE_CODE_ASSIST_PROFILE_ID => (
            encoded
                .body
                .pointer("/request/generationConfig/responseMimeType")
                .and_then(Value::as_str),
            encoded
                .body
                .pointer("/request/generationConfig/responseJsonSchema"),
        ),
        _ => return Ok(()),
    };
    if (projected_type == Some("json_schema") && projected_schema == Some(schema))
        || (projected_type == Some("application/json") && projected_schema == Some(schema))
    {
        return Ok(());
    }
    Err(pre_dispatch_failure(format!(
        "Protocol `{protocol_id}` dropped or changed the requested structured-output JSON Schema.",
    )))
}

/// Applies one validated custom model projection after standard protocol encoding.
/// 在标准协议编码后应用一个已校验的自定义模型投影。
fn project_custom_model_request(
    projection: &CustomModelRequestProjection,
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    for (path, value) in &projection.defaults {
        set_json_path(&mut encoded.body, path, value.clone(), false)?;
    }
    for (parameter_id, path) in &projection.model_parameter_paths {
        if let Some(value) = request.model_parameters.get(parameter_id) {
            let value = model_parameter_json_value(value)?;
            set_json_path(&mut encoded.body, path, value, true)?;
        }
    }
    if let Some(reasoning_effort) = request.reasoning_effort
        && let Some(mutation) = projection.reasoning.get(&reasoning_effort)
    {
        for path in &mutation.delete {
            delete_json_path(&mut encoded.body, path)?;
        }
        for (path, value) in &mutation.set {
            set_json_path(&mut encoded.body, path, value.clone(), true)?;
        }
    }
    for path in &projection.delete {
        delete_json_path(&mut encoded.body, path)?;
    }
    for (path, value) in &projection.overrides {
        set_json_path(&mut encoded.body, path, value.clone(), true)?;
    }
    Ok(())
}

/// Converts one strongly typed model value into its provider JSON value.
/// 将一个强类型模型值转换为供应商 JSON 值。
fn model_parameter_json_value(
    value: &ModelParameterValue,
) -> Result<Value, ProviderExecutionFailure> {
    match value {
        ModelParameterValue::Boolean(value) => Ok(Value::Bool(*value)),
        ModelParameterValue::Integer(value) => Ok(Value::Number((*value).into())),
        ModelParameterValue::Decimal(value) => serde_json::from_str(value).map_err(|error| {
            pre_dispatch_failure(format!(
                "Custom decimal model parameter `{value}` is invalid JSON: {error}"
            ))
        }),
        ModelParameterValue::String(value) => Ok(Value::String(value.clone())),
        ModelParameterValue::Array(values) => values
            .iter()
            .map(model_parameter_json_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        ModelParameterValue::Object(values) => values
            .iter()
            .map(|(key, value)| Ok((key.clone(), model_parameter_json_value(value)?)))
            .collect::<Result<serde_json::Map<_, _>, ProviderExecutionFailure>>()
            .map(Value::Object),
    }
}

/// Sets one validated dot-separated object path.
/// 设置一条已校验的点分隔对象路径。
fn set_json_path(
    root: &mut Value,
    path: &str,
    value: Value,
    replace: bool,
) -> Result<(), ProviderExecutionFailure> {
    let segments = path.split('.').collect::<Vec<_>>();
    let (leaf, parents) = segments
        .split_last()
        .ok_or_else(|| pre_dispatch_failure("Custom request path is empty."))?;
    let mut current = root;
    for segment in parents {
        let object = current.as_object_mut().ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Custom request path `{path}` crosses a non-object value."
            ))
        })?;
        current = object
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    let object = current.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure(format!(
            "Custom request path `{path}` targets a non-object parent."
        ))
    })?;
    if object.contains_key(*leaf) && !replace {
        return Ok(());
    }
    object.insert((*leaf).to_string(), value);
    Ok(())
}

/// Deletes one validated dot-separated object path when present.
/// 在存在时删除一条已校验的点分隔对象路径。
fn delete_json_path(root: &mut Value, path: &str) -> Result<(), ProviderExecutionFailure> {
    let segments = path.split('.').collect::<Vec<_>>();
    let (leaf, parents) = segments
        .split_last()
        .ok_or_else(|| pre_dispatch_failure("Custom request path is empty."))?;
    let mut current = root;
    for segment in parents {
        let Some(object) = current.as_object_mut() else {
            return Ok(());
        };
        let Some(next) = object.get_mut(*segment) else {
            return Ok(());
        };
        current = next;
    }
    if let Some(object) = current.as_object_mut() {
        object.remove(*leaf);
    }
    Ok(())
}

/// Projects the shared sampling-temperature parameter into one registered provider body.
/// 将共享采样温度参数投影到一个已注册供应商请求体。
///
/// # Parameters
/// 参数
/// `request` contains the typed parameter and `encoded` is the mutable wire request.
/// `request` 包含强类型参数，`encoded` 是可变 Wire 请求。
///
/// # Returns
/// 返回值
/// Success when absent or projected without conflict.
/// 缺失或无冲突投影时返回成功。
fn project_sampling_temperature(
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    let Some(value) = request
        .model_parameters
        .get(SAMPLING_TEMPERATURE_PARAMETER_ID)
    else {
        return Ok(());
    };
    let ModelParameterValue::Decimal(decimal) = value else {
        return Err(pre_dispatch_failure(
            "The registered sampling temperature value must be a decimal string.",
        ));
    };
    let temperature = serde_json::from_str::<Value>(decimal).map_err(|error| {
        pre_dispatch_failure(format!(
            "Sampling temperature could not be projected as JSON: {error}"
        ))
    })?;
    let body = request_body_mut(encoded, "Protocol")?;
    if body.contains_key("temperature") {
        return Err(pre_dispatch_failure(
            "Sampling temperature projection conflicts with an existing protocol field.",
        ));
    }
    body.insert(String::from("temperature"), temperature);
    Ok(())
}

/// Borrows one encoded request body as a JSON object with a provider-specific diagnostic.
/// 以供应商专属诊断把编码请求体借用为 JSON 对象。
///
/// # Parameters
/// 参数
/// `encoded` is the mutable wire request and `label` identifies its projector.
/// `encoded` 是可变 Wire 请求，`label` 标识其投影器。
///
/// # Returns
/// 返回值
/// The mutable JSON object, or a controlled pre-dispatch failure.
/// 返回可变 JSON 对象，或受控分派前失败。
fn request_body_mut<'a>(
    encoded: &'a mut EncodedWireRequest,
    label: &str,
) -> Result<&'a mut serde_json::Map<String, Value>, ProviderExecutionFailure> {
    encoded
        .body
        .as_object_mut()
        .ok_or_else(|| pre_dispatch_failure(format!("{label} request body must be a JSON object.")))
}

/// Applies credential-bound non-secret identity fields immediately before dispatch.
/// 在分派前立即应用凭据绑定的非秘密身份字段。
///
/// # Parameters
/// 参数
/// `route` fixes the exact protocol, `secret` carries validated account metadata, and
/// `encoded` is the provider-projected request.
/// `route` 固定精确协议，`secret` 承载已验证账号元数据，`encoded` 是供应商投影请求。
///
/// # Returns
/// 返回值
/// A request containing required credential identity without exposing access material in JSON.
/// 返回包含所需凭据身份且不在 JSON 中暴露访问材料的请求。
fn project_credential_request(
    route: &ProviderExecutionRoute,
    secret: &CredentialExecutionSecret,
    mut encoded: EncodedWireRequest,
) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
    if route.protocol_route.protocol_profile_id
        != vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID
    {
        return Ok(encoded);
    }
    let project_id = secret.project_id().ok_or_else(|| {
        pre_dispatch_failure(
            "Google Code Assist execution requires a validated credential project id.",
        )
    })?;
    let body = encoded.body.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure("Google Code Assist request body must be a JSON object.")
    })?;
    if body.contains_key("project") {
        return Err(pre_dispatch_failure(
            "Google Code Assist project projection conflicts with a protocol field.",
        ));
    }
    body.insert(
        String::from("project"),
        Value::String(project_id.to_string()),
    );
    Ok(encoded)
}

/// Builds provider authentication and public application headers.
/// 构建供应商鉴权与公共应用请求头。
///
/// # Parameters
/// 参数
/// `route` identifies the exact provider, `protocol_headers` contains non-secret Profile
/// requirements, and `secret` is the selected credential material.
/// `route` 标识精确供应商，`protocol_headers` 包含非秘密 Profile 要求，
/// `secret` 是选中的凭据材料。
///
/// # Returns
/// 返回值
/// A validated merged header map or a controlled pre-dispatch failure.
/// 返回已校验的合并请求头，或受控的分派前失败。
pub(crate) fn execution_headers(
    route: &ProviderExecutionRoute,
    protocol_headers: &std::collections::BTreeMap<String, String>,
    secret: &CredentialExecutionSecret,
) -> Result<HeaderMap, ProviderExecutionFailure> {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    // Streaming requests require byte-transparent SSE framing; compressed error responses
    // are still decoded by the configured reqwest content decoders if an upstream ignores this.
    // 流式请求要求 SSE 分帧保持字节透明；若上游忽略该设置，已配置的 reqwest 内容
    // 解码器仍会解码压缩错误响应。
    headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        HeaderValue::from_static("identity"),
    );
    for (name, value) in protocol_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol profile produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
        {
            return Err(pre_dispatch_failure(format!(
                "Protocol profile must not provide credential header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol profile produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    for (name, value) in &route.entry.request_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Service entry produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
            || name == HeaderName::from_static("chatgpt-account-id")
            || name == HeaderName::from_static("x-goog-user-project")
            || name == reqwest::header::ACCEPT_ENCODING
            || name == reqwest::header::CONTENT_TYPE
        {
            return Err(pre_dispatch_failure(format!(
                "Service entry must not override reserved header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Service entry produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    for (name, value) in &route.protocol_route.request_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol route produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
            || name == HeaderName::from_static("chatgpt-account-id")
            || name == HeaderName::from_static("x-goog-user-project")
            || name == reqwest::header::ACCEPT_ENCODING
            || name == reqwest::header::CONTENT_TYPE
        {
            return Err(pre_dispatch_failure(format!(
                "Protocol route must not override reserved header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol route produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    let credential_profile = route
        .entry
        .credential_profile(route.connection.credential_profile_id.as_str())
        .ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Credential profile `{}` is not registered for service entry `{}`.",
                route.connection.credential_profile_id, route.entry.id
            ))
        })?;
    let authentication = route.protocol_route.resolved_authentication(
        route.entry.authentication,
        credential_profile.authentication,
    );
    match authentication {
        AuthenticationProjection::XApiKey => {
            headers.insert(
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_str(secret.expose_value()).map_err(|_| {
                    pre_dispatch_failure("API key contains invalid header characters.")
                })?,
            );
        }
        AuthenticationProjection::BearerAuthorization
        | AuthenticationProjection::CodingDeviceBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Bearer credential contains invalid header characters.",
                        )
                    })?,
            );
        }
        AuthenticationProjection::OpenAiCodexBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Codex bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            let account_id = secret.account_id().ok_or_else(|| {
                pre_dispatch_failure("Codex credential does not contain an account identity.")
            })?;
            headers.insert(
                HeaderName::from_static("chatgpt-account-id"),
                HeaderValue::from_str(account_id).map_err(|_| {
                    pre_dispatch_failure(
                        "Codex account identity contains invalid header characters.",
                    )
                })?,
            );
        }
        AuthenticationProjection::GoogleCodeAssistBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Google bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            if let Some(quota_project_id) = secret.quota_project_id() {
                headers.insert(
                    HeaderName::from_static("x-goog-user-project"),
                    HeaderValue::from_str(quota_project_id).map_err(|_| {
                        pre_dispatch_failure(
                            "Google quota project contains invalid header characters.",
                        )
                    })?,
                );
            }
        }
        AuthenticationProjection::GoogleGeminiBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Google Gemini bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            let quota_project_id = secret.quota_project_id().ok_or_else(|| {
                pre_dispatch_failure(
                    "Public Gemini OAuth credential does not contain a Google Cloud project id.",
                )
            })?;
            headers.insert(
                HeaderName::from_static("x-goog-user-project"),
                HeaderValue::from_str(quota_project_id).map_err(|_| {
                    pre_dispatch_failure(
                        "Google Gemini quota project contains invalid header characters.",
                    )
                })?,
            );
        }
        AuthenticationProjection::GoogleApiKey => {
            headers.insert(
                HeaderName::from_static("x-goog-api-key"),
                HeaderValue::from_str(secret.expose_value()).map_err(|_| {
                    pre_dispatch_failure("Google API key contains invalid header characters.")
                })?,
            );
        }
    }
    if authentication == AuthenticationProjection::CodingDeviceBearer {
        let device_name = std::env::var("COMPUTERNAME")
            .or_else(|_| std::env::var("HOSTNAME"))
            .unwrap_or_else(|_| String::from("unknown"));
        let device_model = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
        for (name, value) in [
            ("user-agent", "CLIProxyAPI/dev"),
            ("x-msh-platform", "CLIProxyAPI"),
            ("x-msh-version", "dev"),
            ("x-msh-device-name", device_name.as_str()),
            ("x-msh-device-model", device_model.as_str()),
            (
                "x-msh-device-id",
                secret
                    .provider_device_id()
                    .unwrap_or("cli-proxy-api-device"),
            ),
        ] {
            headers.insert(
                HeaderName::from_static(name),
                HeaderValue::from_str(value).map_err(|_| {
                    pre_dispatch_failure("Coding-device execution identity is invalid.")
                })?,
            );
        }
    }
    Ok(headers)
}

/// One parsed transport frame and its optional SSE event name.
/// 一条解析后的传输帧及其可选 SSE 事件名称。
#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamFrame {
    /// Optional event name.
    /// 可选事件名称。
    event_name: Option<String>,
    /// Complete frame data.
    /// 完整帧数据。
    data: String,
}

/// Incrementally frames arbitrary chunks with bounded retained memory.
/// 使用有界保留内存对任意分片进行增量分帧。
#[derive(Debug)]
struct IncrementalFrameParser {
    /// Registered framing dialect.
    /// 已注册的分帧方言。
    framing: ProtocolStreamFraming,
    /// Maximum complete frame size.
    /// 完整帧最大尺寸。
    max_frame_bytes: usize,
    /// Maximum unconsumed buffer size.
    /// 未消费缓冲区最大尺寸。
    max_buffer_bytes: usize,
    /// Unprocessed bytes retained across transport chunks.
    /// 在传输分片间保留的未处理字节。
    buffer: Vec<u8>,
    /// First unconsumed byte.
    /// 首个未消费字节位置。
    consumed: usize,
}

impl IncrementalFrameParser {
    /// Creates a bounded parser from one immutable transport profile.
    /// 根据一个不可变传输 Profile 创建有界解析器。
    ///
    /// # Parameters
    /// 参数
    /// `profile` provides the exact framing and byte limits.
    /// `profile` 提供精确分帧方式与字节上限。
    ///
    /// # Returns
    /// 返回值
    /// A new empty parser.
    /// 返回一个新的空解析器。
    fn new(profile: ProtocolTransportProfile) -> Self {
        Self {
            framing: profile.framing,
            max_frame_bytes: profile.max_frame_bytes,
            max_buffer_bytes: profile.max_buffer_bytes,
            buffer: Vec::new(),
            consumed: 0,
        }
    }

    /// Pushes arbitrary transport bytes.
    /// 推入任意传输字节。
    ///
    /// # Parameters
    /// 参数
    /// `chunk` is one arbitrary network byte chunk.
    /// `chunk` 是一个任意网络字节分片。
    ///
    /// # Returns
    /// 返回值
    /// Complete frames, or a deterministic framing limit/encoding error.
    /// 返回完整帧，或确定性的分帧上限/编码错误。
    fn push(&mut self, mut chunk: &[u8]) -> Result<Vec<StreamFrame>, ProtocolConversionError> {
        let mut frames = Vec::new();
        while !chunk.is_empty() {
            self.compact_if_worthwhile();
            let retained = self.retained_len();
            if retained >= self.max_buffer_bytes {
                return Err(self.buffer_limit_error());
            }
            let capacity = self.max_buffer_bytes - retained;
            let take = capacity.min(chunk.len());
            self.buffer.extend_from_slice(&chunk[..take]);
            chunk = &chunk[take..];
            frames.extend(self.drain_frames(false)?);
            if self.retained_len() >= self.max_buffer_bytes && !chunk.is_empty() {
                return Err(self.buffer_limit_error());
            }
        }
        Ok(frames)
    }

    /// Finishes framing and includes a final non-empty record.
    /// 完成分帧并包含最后一条非空记录。
    ///
    /// # Returns
    /// 返回值
    /// Final complete frames, or a deterministic truncation/encoding error.
    /// 返回最终完整帧，或确定性的截断/编码错误。
    fn finish(&mut self) -> Result<Vec<StreamFrame>, ProtocolConversionError> {
        self.drain_frames(true)
    }

    /// Drains complete records from the internal byte buffer.
    /// 从内部字节缓冲区提取完整记录。
    ///
    /// # Parameters
    /// 参数
    /// `include_tail` controls whether the final unterminated record is accepted.
    /// `include_tail` 控制是否接受最后一条未带分隔符的记录。
    ///
    /// # Returns
    /// 返回值
    /// Complete frames in transport order.
    /// 返回按传输顺序排列的完整帧。
    fn drain_frames(
        &mut self,
        include_tail: bool,
    ) -> Result<Vec<StreamFrame>, ProtocolConversionError> {
        let mut frames = Vec::new();
        while let Some((record_end, delimiter_len)) = find_record_end(self.framing, self.retained())
        {
            self.validate_frame_size(record_end)?;
            let start = self.consumed;
            let end = start + record_end;
            let record = &self.buffer[start..end];
            if let Some(frame) = parse_stream_record(self.framing, record)? {
                frames.push(frame);
            }
            self.consumed = end + delimiter_len;
        }
        if include_tail && self.retained_len() > 0 {
            let record_len = self.retained_len();
            self.validate_frame_size(record_len)?;
            let record = self.retained();
            if let Some(frame) = parse_stream_record(self.framing, record)? {
                frames.push(frame);
            }
            self.consumed = self.buffer.len();
        }
        self.compact_if_worthwhile();
        Ok(frames)
    }

    /// Returns the unconsumed suffix.
    /// 返回未消费的后缀。
    ///
    /// # Returns
    /// 返回值
    /// Borrowed pending bytes.
    /// 返回借用的待处理字节。
    fn retained(&self) -> &[u8] {
        &self.buffer[self.consumed..]
    }

    /// Returns the number of unconsumed bytes.
    /// 返回未消费字节数。
    ///
    /// # Returns
    /// 返回值
    /// Pending byte count.
    /// 返回待处理字节数。
    fn retained_len(&self) -> usize {
        self.buffer.len().saturating_sub(self.consumed)
    }

    /// Compacts only after consumed storage is materially large.
    /// 仅在已消费存储达到显著规模后执行压缩。
    fn compact_if_worthwhile(&mut self) {
        if self.consumed == self.buffer.len() {
            self.buffer.clear();
            self.consumed = 0;
        } else if self.consumed >= 64 * 1024 && self.consumed >= self.buffer.len() / 2 {
            self.buffer.copy_within(self.consumed.., 0);
            self.buffer.truncate(self.buffer.len() - self.consumed);
            self.consumed = 0;
        }
    }

    /// Rejects one oversized complete or pending frame.
    /// 拒绝一个过大的完整帧或待处理帧。
    ///
    /// # Parameters
    /// 参数
    /// `frame_bytes` is the exact candidate frame size.
    /// `frame_bytes` 是候选帧的精确尺寸。
    ///
    /// # Returns
    /// 返回值
    /// Success within the profile limit, otherwise a controlled error.
    /// 在 Profile 上限内返回成功，否则返回受控错误。
    fn validate_frame_size(&self, frame_bytes: usize) -> Result<(), ProtocolConversionError> {
        if frame_bytes <= self.max_frame_bytes {
            return Ok(());
        }
        Err(ProtocolConversionError::new(format!(
            "Provider stream frame exceeded the registered {} byte limit.",
            self.max_frame_bytes
        )))
    }

    /// Builds one controlled pending-buffer limit error.
    /// 构建一个受控的待处理缓冲区上限错误。
    ///
    /// # Returns
    /// 返回值
    /// A deterministic protocol conversion failure.
    /// 返回确定性的协议转换失败。
    fn buffer_limit_error(&self) -> ProtocolConversionError {
        ProtocolConversionError::new(format!(
            "Provider stream buffer exceeded the registered {} byte limit.",
            self.max_buffer_bytes
        ))
    }
}

/// Finds the first complete delimiter for one registered framing dialect.
/// 查找一种已注册分帧方言的首个完整分隔符。
///
/// # Parameters
/// 参数
/// `framing` is the exact dialect and `buffer` is the unconsumed suffix.
/// `framing` 是精确方言，`buffer` 是未消费后缀。
///
/// # Returns
/// 返回值
/// Record-end offset and delimiter length, or `None`.
/// 返回记录结束偏移与分隔符长度，否则返回 `None`。
fn find_record_end(framing: ProtocolStreamFraming, buffer: &[u8]) -> Option<(usize, usize)> {
    match framing {
        ProtocolStreamFraming::Sse => {
            for index in 0..buffer.len() {
                let Some(first_len) = sse_line_ending_len(buffer, index) else {
                    continue;
                };
                let second_start = index + first_len;
                if let Some(second_len) = sse_line_ending_len(buffer, second_start) {
                    return Some((index, first_len + second_len));
                }
            }
            None
        }
        ProtocolStreamFraming::JsonLines | ProtocolStreamFraming::Ndjson => buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| (index, 1)),
        ProtocolStreamFraming::SingleJson => None,
    }
}

/// Returns the byte length of one SSE line ending at an exact offset.
/// 返回精确偏移处一项 SSE 行结束符的字节长度。
///
/// # Parameters
/// 参数
/// `buffer` is the pending transport data and `index` is the candidate line-ending offset.
/// `buffer` 是待处理传输数据，`index` 是候选行结束符偏移。
///
/// # Returns
/// 返回值
/// Two bytes for CRLF, one byte for CR or LF, otherwise `None`.
/// CRLF 返回两个字节，CR 或 LF 返回一个字节，否则返回 `None`。
fn sse_line_ending_len(buffer: &[u8], index: usize) -> Option<usize> {
    match buffer.get(index) {
        Some(b'\r') if buffer.get(index + 1) == Some(&b'\n') => Some(2),
        Some(b'\r' | b'\n') => Some(1),
        _ => None,
    }
}

/// Parses one complete record according to its registered framing.
/// 根据已注册分帧方式解析一条完整记录。
///
/// # Parameters
/// 参数
/// `framing` is the exact dialect and `record` contains one complete frame.
/// `framing` 是精确方言，`record` 包含一个完整帧。
///
/// # Returns
/// 返回值
/// A decoded frame, no frame for heartbeat/blank records, or a controlled error.
/// 返回解码帧；心跳/空记录返回空；编码错误返回受控错误。
fn parse_stream_record(
    framing: ProtocolStreamFraming,
    record: &[u8],
) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    match framing {
        ProtocolStreamFraming::Sse => parse_sse_record(record),
        ProtocolStreamFraming::JsonLines | ProtocolStreamFraming::Ndjson => {
            parse_json_line_record(record)
        }
        ProtocolStreamFraming::SingleJson => parse_single_json_record(record),
    }
}

/// Parses one complete SSE record.
/// 解析一条完整 SSE 记录。
///
/// # Parameters
/// 参数
/// `record` contains one complete SSE record without its blank-line delimiter.
/// `record` 包含一条不带空行分隔符的完整 SSE 记录。
///
/// # Returns
/// 返回值
/// A semantic data frame, no frame for comments, or a controlled UTF-8 error.
/// 返回语义数据帧；注释返回空；UTF-8 错误返回受控错误。
fn parse_sse_record(record: &[u8]) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    let text = std::str::from_utf8(record).map_err(|error| {
        ProtocolConversionError::new(format!("Provider SSE record is not valid UTF-8: {error}"))
    })?;
    let mut event_name = None;
    let mut data_lines = Vec::new();
    // SSE permits CR, LF, and CRLF independently, so normalize all three before field parsing.
    // SSE 独立允许 CR、LF 与 CRLF，因此在字段解析前统一三种换行。
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for line in normalized.lines() {
        if line.starts_with(':') || line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event_name = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data_lines.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    if data_lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(StreamFrame {
        event_name,
        data: data_lines.join("\n"),
    }))
}

/// Parses one JSONL or NDJSON record.
/// 解析一条 JSONL 或 NDJSON 记录。
///
/// # Parameters
/// 参数
/// `record` is one line without its LF delimiter.
/// `record` 是不带 LF 分隔符的一行。
///
/// # Returns
/// 返回值
/// A trimmed JSON frame, no frame for a blank line, or a controlled UTF-8 error.
/// 返回去除首尾空白的 JSON 帧；空行返回空；UTF-8 错误返回受控错误。
fn parse_json_line_record(record: &[u8]) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    let text = std::str::from_utf8(record).map_err(|error| {
        ProtocolConversionError::new(format!("Provider JSON line is not valid UTF-8: {error}"))
    })?;
    let data = text.trim_end_matches('\r').trim();
    if data.is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<Value>(data).map_err(|error| {
        ProtocolConversionError::new(format!("Provider JSON line is invalid: {error}"))
    })?;
    Ok(Some(StreamFrame {
        event_name: None,
        data: data.to_string(),
    }))
}

/// Parses one complete single-JSON response.
/// 解析一份完整的单 JSON 响应。
///
/// # Parameters
/// 参数
/// `record` is the complete response body.
/// `record` 是完整响应体。
///
/// # Returns
/// 返回值
/// A validated JSON frame, no frame for an empty body, or a controlled error.
/// 返回已校验 JSON 帧；空响应体返回空；错误返回受控结果。
fn parse_single_json_record(record: &[u8]) -> Result<Option<StreamFrame>, ProtocolConversionError> {
    let text = std::str::from_utf8(record).map_err(|error| {
        ProtocolConversionError::new(format!(
            "Provider JSON response is not valid UTF-8: {error}"
        ))
    })?;
    let data = text.trim();
    if data.is_empty() {
        return Ok(None);
    }
    serde_json::from_str::<Value>(data).map_err(|error| {
        ProtocolConversionError::new(format!("Provider JSON response is invalid: {error}"))
    })?;
    Ok(Some(StreamFrame {
        event_name: None,
        data: data.to_string(),
    }))
}

/// Ensures one endpoint path begins with a slash.
/// 确保一个端点路径以斜杠开头。
fn ensure_leading_slash(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

/// Creates a structured pre-dispatch failure.
/// 创建结构化分派前失败。
pub(crate) fn pre_dispatch_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("route_validation")),
            stage: String::from("prepare"),
            http_status: None,
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
            message: message.into(),
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Converts a deterministic protocol failure.
/// 转换确定性协议失败。
fn protocol_failure(error: ProtocolConversionError) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("protocol"),
            http_status: None,
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
            message: error.message,
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Converts an LLM-core protocol-runtime failure without restoring adapter-side registry logic.
/// 将 LLM 核心协议运行时失败转换为适配层错误，但不恢复适配层注册表逻辑。
fn llm_core_protocol_failure(error: LlmCoreError) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("protocol"),
            http_status: None,
            transport: None,
            websocket_close_code: error.websocket_close_code,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: error.message,
            dispatch_outcome: DispatchOutcome::NotDispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Restricts one encoded Codex compaction body to the official `CompactionInput` fields.
/// 将编码后的 Codex 压缩请求体限制为官方 `CompactionInput` 字段集合。
///
/// # Parameters
/// 参数
/// `body` is the protocol-encoded Responses request body before the compact endpoint rewrite.
/// `body` 是改写到 compact 端点前的协议编码 Responses 请求体。
///
/// # Returns
/// 返回值
/// Success after removing non-compact fields while preserving the encoded parallel-call policy, or
/// a pre-dispatch failure when the encoder did not produce an object body.
/// 成功时移除非压缩字段并保留已编码的并行调用策略；编码器未产生对象体时返回分派前失败。
fn prepare_responses_compact_body(body: &mut Value) -> Result<(), ProviderExecutionFailure> {
    let object = body.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure("Codex Responses compaction request body must be a JSON object.")
    })?;
    object.retain(|key, _| {
        matches!(
            key.as_str(),
            "model"
                | "input"
                | "instructions"
                | "tools"
                | "parallel_tool_calls"
                | "reasoning"
                | "service_tier"
                | "prompt_cache_key"
                | "text"
        )
    });
    if object
        .get("instructions")
        .and_then(Value::as_str)
        .is_some_and(str::is_empty)
    {
        object.remove("instructions");
    }
    Ok(())
}

/// Decodes and validates one standalone Responses compact response.
/// 解码并校验一份独立 Responses compact 响应。
///
/// # Parameters
/// 参数
/// `value` is the bounded JSON response body and `url` identifies the exact endpoint in errors.
/// `value` 是已受大小限制的 JSON 响应体，`url` 用于错误中标识精确端点。
///
/// # Returns
/// 返回值
/// A result containing the ordered non-empty `compaction` output items.
/// 返回按原顺序保留的非空 `compaction` 输出项。
fn decode_responses_compact_result(
    value: &Value,
    url: &str,
) -> Result<ResponsesCompactResult, ProviderExecutionFailure> {
    let mut output_items = value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            compact_protocol_failure(format!(
                "Provider compaction response from `{url}` does not contain an output array."
            ))
        })?
        .clone();
    canonicalize_codex_responses_replay_items(&mut output_items);
    let contains_compaction = output_items
        .iter()
        .any(|item| item.get("type").and_then(Value::as_str) == Some("compaction"));
    if output_items.is_empty()
        || !output_items.iter().all(is_replayable_codex_responses_item)
        || !contains_compaction
    {
        return Err(compact_protocol_failure(format!(
            "Provider compaction response from `{url}` must contain ordered replayable output items and a compaction item."
        )));
    }
    Ok(ResponsesCompactResult {
        output_items,
        usage: value.get("usage").cloned(),
        response_id: value.get("id").and_then(Value::as_str).map(str::to_string),
    })
}

/// Builds one dispatched protocol failure from the standalone Responses compact response.
/// 根据独立 Responses 压缩响应构建一项已分派协议失败。
fn compact_protocol_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("response_compact"),
            http_status: Some(200),
            transport: Some(AiTransportKind::Http),
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Builds one dispatched transport failure from the standalone Responses compact response.
/// 根据独立 Responses 压缩响应构建一项已分派传输失败。
///
/// # Parameters
/// 参数
/// `message` is the controlled transport diagnostic produced after response dispatch.
/// `message` 是响应分派后产生的受控传输诊断。
///
/// # Returns
/// 返回值
/// A retryable failure that allows the compaction coordinator to use its local safe fallback.
/// 返回允许压缩协调器使用本地安全回退的可重试失败。
fn compact_transport_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("transport")),
            stage: String::from("response_compact"),
            http_status: None,
            transport: Some(AiTransportKind::Http),
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: true,
            retry_after: None,
        }),
    }
}

/// Converts one protocol failure observed after the upstream request was dispatched.
/// 转换一项在上游请求已经分派后观察到的协议失败。
///
/// # Parameters
/// 参数
/// `error` is the decoder or lifecycle failure and `semantic_output_observed` records whether
/// user-visible or tool-visible semantic output was already emitted.
/// `error` 是解码器或生命周期失败，`semantic_output_observed` 记录是否已经发出
/// 用户可见或工具可见的语义输出。
///
/// # Returns
/// 返回值
/// A terminal, non-retryable protocol failure with exact dispatch state.
/// 返回携带精确分派状态的终态、不可重试协议失败。
fn stream_protocol_failure(
    error: ProtocolConversionError,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    stream_protocol_failure_for_transport(error, semantic_output_observed, AiTransportKind::Http)
}

/// Converts one malformed stream into a terminal failure for the exact transport.
/// 将一个畸形流转换为精确传输对应的终态失败。
///
/// # Parameters
/// 参数
/// `error` is the decoder failure, `semantic_output_observed` protects replay, and `transport` is exact.
/// `error` 是解码失败，`semantic_output_observed` 保护重放，`transport` 是精确传输。
///
/// # Returns
/// 返回值
/// A non-retryable dispatched protocol failure retaining the observed transport.
/// 返回保留已观察传输的不可重试已分派协议失败。
fn stream_protocol_failure_for_transport(
    error: ProtocolConversionError,
    semantic_output_observed: bool,
    transport: AiTransportKind,
) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("protocol_conversion")),
            stage: String::from("response_stream"),
            http_status: None,
            transport: Some(transport),
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: error.message,
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Converts end-of-stream validation into a retry-safe transport failure or a protocol failure.
/// 将流结束校验转换为可安全重试的传输失败或协议失败。
///
/// # Parameters
/// 参数
/// `error` is the typed decoder completion failure and `semantic_output_observed` records whether
/// replay would duplicate user-visible or tool-visible semantics.
/// `error` 是强类型解码器结束失败；`semantic_output_observed` 记录重放是否会重复用户或工具可见语义。
///
/// # Returns
/// 返回值
/// A retryable transport failure only for a pre-semantic truncated stream; malformed protocol
/// payloads and post-semantic truncation remain terminal.
/// 仅语义输出前的截断流返回可重试传输失败；畸形协议载荷与语义输出后的截断仍保持终态。
fn stream_completion_failure(
    error: ProtocolConversionError,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    stream_completion_failure_for_transport(error, semantic_output_observed, AiTransportKind::Http)
}

/// Converts end-of-stream validation for one exact transport.
/// 转换一个精确传输的流结束校验结果。
///
/// # Parameters
/// 参数
/// `error` is the typed completion failure, `semantic_output_observed` protects replay, and `transport` is exact.
/// `error` 是强类型结束失败，`semantic_output_observed` 保护重放，`transport` 是精确传输。
///
/// # Returns
/// 返回值
/// A retryable truncation or terminal protocol failure retaining the exact transport.
/// 返回保留精确传输的可重试截断或终态协议失败。
fn stream_completion_failure_for_transport(
    error: ProtocolConversionError,
    semantic_output_observed: bool,
    transport: AiTransportKind,
) -> ProviderExecutionFailure {
    if error.is_truncated_stream() {
        return dispatched_stream_transport_failure(
            error.message,
            semantic_output_observed,
            transport,
            None,
        );
    }
    stream_protocol_failure_for_transport(error, semantic_output_observed, transport)
}

/// Creates a structured transport failure.
/// 创建结构化传输失败。
pub(crate) fn transport_failure(message: impl Into<String>) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("transport")),
            stage: String::from("transport"),
            http_status: None,
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
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Unknown,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: true,
            retry_after: None,
        }),
    }
}

/// Classifies one HTTP request-send failure without guessing whether an established request arrived upstream.
/// 对一次 HTTP 请求发送失败进行分类，且不猜测已建立连接的请求是否到达上游。
///
/// # Parameters
/// 参数
/// `error` is the exact reqwest send error and `message` is the controlled diagnostic.
/// `error` 是精确的 reqwest 发送错误，`message` 是受控诊断信息。
///
/// # Returns
/// 返回值
/// A retryable transport failure marked not dispatched only when reqwest proves connection
/// establishment failed; every other send failure keeps the conservative unknown outcome.
/// 仅当 reqwest 证明连接建立失败时返回标记为未分派的可重试传输失败；其他发送失败均保守保持未知结果。
pub(crate) fn http_send_transport_failure(
    error: &reqwest::Error,
    message: impl Into<String>,
) -> ProviderExecutionFailure {
    // failure starts with the conservative unknown dispatch outcome used for ambiguous send errors.
    // failure 初始采用用于模糊发送错误的保守未知分派结果。
    let mut failure = transport_failure(message);
    failure.error.transport = Some(AiTransportKind::Http);
    if error.is_connect() {
        // A DNS, TCP, or TLS connection-establishment failure occurs before an HTTP request can be dispatched.
        // DNS、TCP 或 TLS 连接建立失败发生在 HTTP 请求可被分派之前。
        failure.error.dispatch_outcome = DispatchOutcome::NotDispatched;
    }
    failure
}

/// Creates one transport failure after response dispatch while preserving semantic-output safety.
/// 创建一项响应分派后的传输失败，并保留语义输出安全状态。
///
/// # Parameters
/// 参数
/// `message` is the controlled transport diagnostic and `semantic_output_observed` records
/// whether transparent replay would duplicate visible semantics.
/// `message` 是受控传输诊断，`semantic_output_observed` 记录透明重放是否会重复可见语义。
///
/// # Returns
/// 返回值
/// A structured transport failure retryable only before semantic output.
/// 返回仅在语义输出前可重试的结构化传输失败。
fn stream_transport_failure(
    message: impl Into<String>,
    semantic_output_observed: bool,
) -> ProviderExecutionFailure {
    dispatched_stream_transport_failure(
        message,
        semantic_output_observed,
        AiTransportKind::Http,
        None,
    )
}

/// Creates one dispatched transport failure with an optional exact WebSocket close code.
/// 创建一项已分派传输失败，并可携带精确 WebSocket 关闭码。
fn stream_transport_failure_with_websocket_close_code(
    message: impl Into<String>,
    semantic_output_observed: bool,
    websocket_close_code: Option<u16>,
) -> ProviderExecutionFailure {
    dispatched_stream_transport_failure(
        message,
        semantic_output_observed,
        AiTransportKind::ResponsesWebsocket,
        websocket_close_code,
    )
}

/// Creates one dispatched stream failure for the exact observed transport.
/// 为精确观察到的传输创建一项已分派流失败。
///
/// # Parameters
/// 参数
/// `message` is controlled text, `semantic_output_observed` protects replay, `transport` is exact, and `websocket_close_code` is optional WS evidence.
/// `message` 是受控文本，`semantic_output_observed` 保护重放，`transport` 是精确传输，`websocket_close_code` 是可选 WS 证据。
///
/// # Returns
/// 返回值
/// A typed terminal transport failure retaining exact replay and transport facts.
/// 返回保留精确重放与传输事实的强类型终态传输失败。
fn dispatched_stream_transport_failure(
    message: impl Into<String>,
    semantic_output_observed: bool,
    transport: AiTransportKind,
    websocket_close_code: Option<u16>,
) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::ProviderUnavailable,
            subcategory: Some(String::from("transport")),
            stage: String::from("response_stream"),
            http_status: None,
            transport: Some(transport),
            websocket_close_code,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed,
            side_effect_observed: false,
            terminal: true,
            retryable: !semantic_output_observed,
            retry_after: None,
        }),
    }
}

/// Returns the current host Unix timestamp in milliseconds for transport telemetry.
/// 返回用于传输遥测的当前宿主 Unix 毫秒时间戳。
fn provider_transport_unix_millis() -> Result<i64, ProviderExecutionFailure> {
    let millis = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| transport_failure(format!("Host clock precedes Unix epoch: {error}")))?
        .as_millis();
    i64::try_from(millis)
        .map_err(|_| transport_failure("Host Unix timestamp exceeds SQLite INTEGER."))
}

/// Copies only the fixed non-secret rate-limit response-header allowlist with bounded values.
/// 仅复制固定且不含秘密、值有界的限流响应头允许列表。
fn allowlisted_rate_limit_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    const NAMES: &[&str] = &[
        "retry-after",
        "ratelimit-limit",
        "ratelimit-policy",
        "ratelimit-remaining",
        "ratelimit-reset",
        "x-ratelimit-limit-requests",
        "x-ratelimit-limit-tokens",
        "x-ratelimit-remaining-requests",
        "x-ratelimit-remaining-tokens",
        "x-ratelimit-reset-requests",
        "x-ratelimit-reset-tokens",
    ];
    NAMES
        .iter()
        .filter_map(|name| {
            headers
                .get(*name)
                .and_then(|value| value.to_str().ok())
                .map(|value| {
                    (
                        (*name).to_string(),
                        bounded_provider_diagnostic_header_value(value),
                    )
                })
        })
        .collect()
}

/// Copies one provider-controlled diagnostic header value into a fixed byte budget.
/// 将单个供应商控制的诊断响应头值复制到固定字节预算内。
///
/// # Parameters
/// 参数
/// `value` is already decoded response-header text and may still be adversarially large.
/// `value` 是已解码的响应头文本，但仍可能被对端恶意放大。
///
/// # Returns
/// 返回值
/// The original value when bounded, otherwise a UTF-8-safe prefix plus an explicit marker whose
/// total length never exceeds `PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES`.
/// 值已受限时原样返回，否则返回 UTF-8 安全前缀与显式标记，且总长度绝不超过
/// `PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES`。
fn bounded_provider_diagnostic_header_value(value: &str) -> String {
    if value.len() <= PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES {
        return value.to_string();
    }
    // prefix_budget reserves the marker inside the same hard byte ceiling.
    // prefix_budget 在相同硬字节上限内为标记预留空间。
    let prefix_budget = PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES
        .saturating_sub(PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX.len());
    // boundary walks backward only when the byte budget splits one UTF-8 scalar.
    // boundary 仅在字节预算切开 UTF-8 标量时向前回退。
    let mut boundary = prefix_budget.min(value.len());
    while !value.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    // bounded owns the exact retained prefix and explicit truncation evidence.
    // bounded 持有精确保留前缀与显式截断证据。
    let mut bounded = String::with_capacity(PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES);
    bounded.push_str(&value[..boundary]);
    bounded.push_str(PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX);
    bounded
}

/// Extracts the upstream request identifier using only the route-declared ordered headers.
/// 仅使用路由声明的有序响应头提取上游请求标识。
///
/// # Parameters
/// 参数
/// `headers` are the exact upstream response headers and `declared_names` is the route-owned order.
/// `headers` 是精确上游响应头，`declared_names` 是路由自有的查找顺序。
///
/// # Returns
/// 返回值
/// The first declared non-empty bounded identifier, absence when none is present, or a protocol failure for invalid text.
/// 返回首个已声明、非空且有界的标识；均不存在时返回空；文本无效时返回协议失败。
fn provider_request_id_from_headers(
    headers: &HeaderMap,
    declared_names: &[String],
) -> Result<Option<String>, ProviderExecutionFailure> {
    for name in declared_names {
        let Some(value) = headers.get(name) else {
            continue;
        };
        let value = value.to_str().map_err(|error| {
            response_headers_protocol_failure(format!(
                "Provider request-id header `{name}` is not valid text: {error}"
            ))
        })?;
        if !value.is_empty() {
            return Ok(Some(bounded_provider_diagnostic_header_value(value)));
        }
    }
    Ok(None)
}

/// Validates a successful response content type against the immutable protocol profile.
/// 根据不可变协议 Profile 校验成功响应的内容类型。
///
/// # Parameters
/// 参数
/// `headers` are the exact upstream headers, `profile` is the registered transport declaration,
/// and `url` is used only in the controlled diagnostic.
/// `headers` 是精确上游响应头，`profile` 是已注册传输声明，`url` 仅用于受控诊断。
///
/// # Returns
/// 返回值
/// Success for an exact accepted media type, otherwise a dispatched protocol failure.
/// 精确匹配允许媒体类型时返回成功，否则返回已分派协议失败。
fn validate_response_content_type(
    headers: &HeaderMap,
    profile: &ProtocolTransportProfile,
    url: &str,
) -> Result<(), ProviderExecutionFailure> {
    let value = headers
        .get(reqwest::header::CONTENT_TYPE)
        .ok_or_else(|| {
            response_headers_protocol_failure(format!(
                "Provider response from `{url}` omitted the required Content-Type header."
            ))
        })?
        .to_str()
        .map_err(|_| {
            response_headers_protocol_failure(format!(
                "Provider response from `{url}` contained a non-ASCII Content-Type header."
            ))
        })?;
    let media_type = value.split(';').next().map(str::trim).unwrap_or_default();
    if profile
        .accepted_response_content_types
        .iter()
        .any(|accepted| media_type.eq_ignore_ascii_case(accepted))
    {
        return Ok(());
    }
    Err(response_headers_protocol_failure(format!(
        "Provider response from `{url}` used unregistered Content-Type `{media_type}`; expected one of [{}].",
        profile.accepted_response_content_types.join(", ")
    )))
}

/// Creates a dispatched protocol failure for invalid successful response headers.
/// 为无效的成功响应头创建已分派协议失败。
///
/// # Parameters
/// 参数
/// `message` is the controlled diagnostic.
/// `message` 是受控诊断。
///
/// # Returns
/// 返回值
/// A terminal non-retryable protocol failure.
/// 返回终结且不可重试的协议失败。
pub(crate) fn response_headers_protocol_failure(
    message: impl Into<String>,
) -> ProviderExecutionFailure {
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category: AiErrorCategory::InvalidRequest,
            subcategory: Some(String::from("response_content_type")),
            stage: String::from("response_headers"),
            http_status: Some(200),
            transport: Some(AiTransportKind::Http),
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: message.into(),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }),
    }
}

/// Reads one standalone Responses compact body within the official complete-request deadline.
/// 在官方完整请求截止时间内读取一份独立 Responses 压缩正文。
///
/// # Parameters
/// 参数
/// `response`, `max_bytes`, `url`, `status`, and `retry_after` retain the normal bounded body
/// contract; `request_started_at` and `request_timeout` cover the entire dispatched unary request.
/// `response`、`max_bytes`、`url`、`status` 与 `retry_after` 保留常规有界正文契约；
/// `request_started_at` 与 `request_timeout` 覆盖完整的已分派一元请求。
///
/// # Returns
/// 返回值
/// The complete bounded body, or a structured transport timeout/failure.
/// 返回完整的有界正文，或结构化传输超时/失败。
async fn read_responses_compact_body_with_timeout(
    response: reqwest::Response,
    max_bytes: usize,
    url: &str,
    status: u16,
    retry_after: Option<RetryAfterHint>,
    request_started_at: Instant,
    request_timeout: Duration,
) -> Result<String, ProviderExecutionFailure> {
    // remaining prevents a successful header response from resetting the official unary deadline.
    // remaining 避免成功收到响应头后重新计算官方一元请求截止时间。
    let remaining = request_timeout.saturating_sub(request_started_at.elapsed());
    // result separates a complete body, a bounded body-read failure, and the complete deadline.
    // result 区分完整正文、有界正文读取失败与完整请求截止超时。
    let result = tokio::time::timeout(
        remaining,
        read_bounded_response_text(
            response,
            max_bytes,
            url,
            status,
            retry_after,
            request_timeout,
        ),
    )
    .await;
    match result {
        Ok(Ok(body)) => Ok(body),
        Ok(Err(mut failure)) => {
            if failure.error.category == AiErrorCategory::ProviderUnavailable {
                failure.error.stage = String::from("response_compact");
                failure.error.dispatch_outcome = DispatchOutcome::Dispatched;
            }
            Err(failure)
        }
        Err(_) => Err(compact_transport_failure(format!(
            "Provider compaction request to `{url}` exceeded the registered {} ms complete-response timeout.",
            request_timeout.as_millis()
        ))),
    }
}

/// Reads a non-success response body without exceeding the registered memory limit.
/// 在不超过已注册内存上限的情况下读取非成功响应体。
///
/// # Parameters
/// 参数
/// `response` is the exact response, `max_bytes` is the profile limit, `url` is diagnostic
/// context, and `status` plus `retry_after` preserve typed retry semantics on overflow.
/// `response` 是精确响应，`max_bytes` 是 Profile 上限，`url` 是诊断上下文，
/// `status` 与 `retry_after` 在溢出时保留强类型重试语义。
///
/// # Returns
/// 返回值
/// The complete UTF-8-lossy body within the limit, or a structured failure.
/// 返回上限内完整的 UTF-8 宽容解码正文，或结构化失败。
pub(crate) async fn read_bounded_response_text(
    response: reqwest::Response,
    max_bytes: usize,
    url: &str,
    status: u16,
    retry_after: Option<RetryAfterHint>,
    idle_timeout: Duration,
) -> Result<String, ProviderExecutionFailure> {
    let mut bytes = response.bytes_stream();
    let mut body = Vec::new();
    loop {
        let next_chunk = tokio::time::timeout(idle_timeout, bytes.next())
            .await
            .map_err(|_| {
                stream_transport_failure(
                    format!(
                        "Provider response body from `{url}` exceeded the registered {} ms idle timeout.",
                        idle_timeout.as_millis()
                    ),
                    false,
                )
            })?;
        let Some(chunk) = next_chunk else {
            break;
        };
        let chunk = chunk.map_err(|error| {
            stream_transport_failure(
                format!("Provider error response from `{url}` could not be read: {error}"),
                false,
            )
        })?;
        if body.len().saturating_add(chunk.len()) > max_bytes {
            let mut failure = http_failure(status, retry_after, "");
            failure.error.subcategory = Some(String::from("error_body_too_large"));
            failure.error.stage = String::from("error_response_body");
            failure.error.message = format!(
                "Provider error response from `{url}` exceeded the registered {max_bytes} byte limit."
            );
            return Err(failure);
        }
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(String::from_utf8_lossy(body.as_slice()).into_owned())
}

/// Parses the standard Retry-After header without discarding fractional precision or provenance.
/// 解析标准 Retry-After 响应头，且不丢弃小数精度或来源。
///
/// # Parameters
/// 参数
/// `headers` is the exact upstream response header map and `now` is the comparison instant for
/// HTTP-date values.
/// `headers` 是精确上游响应头集合，`now` 是 HTTP 日期值使用的比较时刻。
///
/// # Returns
/// 返回值
/// A normalized retry hint for valid seconds, fractional seconds, or HTTP dates.
/// 对合法秒数、小数秒或 HTTP 日期返回归一化重试提示。
pub(crate) fn parse_retry_after_headers(
    headers: &HeaderMap,
    now: SystemTime,
) -> Option<RetryAfterHint> {
    let value = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();
    if let Some(delay_milliseconds) = parse_decimal_seconds_milliseconds(value) {
        return Some(RetryAfterHint {
            delay_milliseconds,
            source: RetryAfterSource::HeaderSeconds,
        });
    }
    let retry_at = httpdate::parse_http_date(value).ok()?;
    let delay_milliseconds = retry_at
        .duration_since(now)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    Some(RetryAfterHint {
        delay_milliseconds,
        source: RetryAfterSource::HeaderHttpDate,
    })
}

/// Converts a non-negative decimal seconds string into milliseconds without floating-point rounding.
/// 将非负十进制秒数字符串转换为毫秒，且不进行浮点舍入。
///
/// # Parameters
/// 参数
/// `value` is the trimmed Retry-After header value.
/// `value` 是去除首尾空白后的 Retry-After 响应头值。
///
/// # Returns
/// 返回值
/// Exact milliseconds rounded up to the next millisecond for sub-millisecond fractions.
/// 对亚毫秒小数向上取整后的精确毫秒数。
fn parse_decimal_seconds_milliseconds(value: &str) -> Option<u64> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let whole_milliseconds = whole.parse::<u64>().ok()?.checked_mul(1_000)?;
    if fraction.is_empty() {
        return Some(whole_milliseconds);
    }
    let mut digits = fraction.bytes();
    let hundreds = u64::from(digits.next().unwrap_or(b'0') - b'0');
    let tens = u64::from(digits.next().unwrap_or(b'0') - b'0');
    let ones = u64::from(digits.next().unwrap_or(b'0') - b'0');
    let rounded_fraction =
        hundreds * 100 + tens * 10 + ones + u64::from(digits.any(|digit| digit != b'0'));
    whole_milliseconds.checked_add(rounded_fraction)
}

/// Recognizes the official Codex overload message in either JSON or plain-text responses.
/// 识别 JSON 或纯文本响应中的官方 Codex 过载消息。
fn is_known_overload_message(message: &str) -> bool {
    message
        .to_ascii_lowercase()
        .contains("our servers are currently overloaded. please try again later")
}

/// Converts one non-success HTTP response.
/// 转换一个非成功 HTTP 响应。
pub(crate) fn http_failure(
    status: u16,
    retry_after: Option<RetryAfterHint>,
    body: &str,
) -> ProviderExecutionFailure {
    let parsed = serde_json::from_str::<Value>(body).ok().filter(|value| {
        crate::provider_manifest::validate_runtime_json_bounds(value, "Provider error response")
            .is_ok()
    });
    let error = parsed
        .as_ref()
        .map(|value| value.get("error").unwrap_or(value));
    let upstream_code = error.and_then(|value| {
        value
            .get("code")
            .and_then(|code| match code {
                Value::String(code) => Some(code.clone()),
                Value::Number(code) => Some(code.to_string()),
                _ => None,
            })
            .or_else(|| {
                value
                    .get("error_type")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .or_else(|| {
                value
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
    });
    let error_type = error
        .and_then(|value| value.get("error_type").and_then(Value::as_str))
        .or_else(|| error.and_then(|value| value.get("type").and_then(Value::as_str)));
    // upstream_message keeps plain-text provider bodies available to the shared retry classifier.
    // upstream_message 保留纯文本供应商响应，供共享重试分类器继续识别。
    let upstream_message = error
        .and_then(|value| value.get("message").and_then(Value::as_str))
        .map(str::to_string)
        .or_else(|| (!body.trim().is_empty()).then(|| body.trim().to_string()));
    // Official Codex overload codes remain authoritative when a proxy reports a generic error type.
    // 当代理返回通用错误类型时，官方 Codex 过载代码仍作为权威分类依据。
    let known_overload = matches!(
        upstream_code.as_deref(),
        Some("server_is_overloaded" | "slow_down")
    ) || upstream_message
        .as_deref()
        .is_some_and(is_known_overload_message);
    // knownContextOverflow uses only provider-owned stable codes or types, never display-message guessing.
    // knownContextOverflow 仅使用供应商自有稳定代码或类型，绝不猜测展示消息。
    let known_context_overflow = matches!(
        upstream_code.as_deref().or(error_type),
        Some("context_length_exceeded" | "max_tokens_exceeded" | "token_limit_exceeded")
    );
    let category = if known_overload {
        AiErrorCategory::ProviderOverloaded
    } else if known_context_overflow {
        AiErrorCategory::ContextLengthExceeded
    } else {
        match error_type {
            Some("authentication" | "authentication_error") => AiErrorCategory::Authentication,
            Some("permission_denied" | "permission_error") => AiErrorCategory::PermissionDenied,
            Some("payment_required") => AiErrorCategory::PaymentRequired,
            Some("rate_limit_exceeded" | "rate_limit_error") => AiErrorCategory::RateLimited,
            Some("provider_overloaded" | "overloaded_error") => AiErrorCategory::ProviderOverloaded,
            Some("provider_unavailable" | "server" | "api_error") => {
                AiErrorCategory::ProviderUnavailable
            }
            Some("timeout") => AiErrorCategory::Timeout,
            Some("content_policy_violation") => AiErrorCategory::ContentPolicy,
            Some("invalid_request" | "invalid_request_error" | "string_too_long") => {
                AiErrorCategory::InvalidRequest
            }
            _ => match status {
                401 => AiErrorCategory::Authentication,
                403 => AiErrorCategory::PermissionDenied,
                402 => AiErrorCategory::PaymentRequired,
                408 => AiErrorCategory::Timeout,
                429 => AiErrorCategory::RateLimited,
                500..=599 => AiErrorCategory::ProviderUnavailable,
                _ => AiErrorCategory::InvalidRequest,
            },
        }
    };
    // public_field sanitizes provider-owned scalars only after raw values have completed typed
    // classification, so redaction cannot alter retry or terminal semantics.
    // public_field 仅在原始值完成强类型分类后清理供应商字段，确保脱敏不会改变重试或终止语义。
    let public_field = |value: String| {
        crate::provider_tool_execution::sanitize_provider_diagnostic_text(value.as_str())
    };
    let public_upstream_message = upstream_message.clone().map(&public_field);
    ProviderExecutionFailure {
        error: Box::new(AiExecutionError {
            category,
            subcategory: error_type
                .map(str::to_string)
                .map(&public_field)
                .or_else(|| Some(String::from("http"))),
            stage: String::from("response_headers"),
            http_status: Some(status),
            transport: Some(AiTransportKind::Http),
            websocket_close_code: None,
            upstream_code: upstream_code.map(&public_field),
            upstream_type: error_type.map(str::to_string).map(&public_field),
            upstream_parameter: error
                .and_then(|value| value.get("param").and_then(Value::as_str))
                .map(str::to_string)
                .map(&public_field),
            upstream_message: public_upstream_message.clone(),
            request_id: parsed
                .as_ref()
                .and_then(|value| {
                    value
                        .get("request_id")
                        .and_then(Value::as_str)
                        .or_else(|| value.pointer("/error/request_id").and_then(Value::as_str))
                        .map(str::to_string)
                })
                .map(&public_field),
            provider_id: None,
            entry_id: None,
            model_id: parsed
                .as_ref()
                .and_then(|value| {
                    value
                        .get("model")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .map(&public_field),
            message: public_upstream_message.unwrap_or_else(|| {
                if body.is_empty() {
                    format!("Provider returned HTTP {status} with an empty error response.")
                } else {
                    public_field(body.trim().to_string())
                }
            }),
            dispatch_outcome: DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: matches!(
                category,
                AiErrorCategory::RateLimited
                    | AiErrorCategory::ProviderOverloaded
                    | AiErrorCategory::ProviderUnavailable
                    | AiErrorCategory::Timeout
            ),
            retry_after,
        }),
    }
}

/// Redacts the exact execution credential and bounds every public failure string.
/// 脱敏精确执行凭据并限制每个公共错误字符串。
///
/// # Parameters
/// 参数
/// `failure` is one typed local or provider failure and `secret` is the exact credential used by
/// its physical request.
/// `failure` 是一个强类型本地或供应商错误，`secret` 是其物理请求使用的精确凭据。
///
/// # Returns
/// 返回值
/// The same typed failure semantics with secret-safe bounded diagnostic fields.
/// 返回错误语义不变、诊断字段不含秘密且有界的同一强类型错误。
pub(crate) fn sanitize_provider_execution_failure(
    mut failure: ProviderExecutionFailure,
    secret: &CredentialExecutionSecret,
) -> ProviderExecutionFailure {
    // sanitize replaces exact credential occurrences before the generic provider-controlled
    // URL and credential-shape filter executes.
    // sanitize 先替换精确凭据，再执行通用供应商 URL 与凭据形态过滤。
    let sanitize = |value: String| {
        let sensitive_value = secret.expose_value();
        let redacted = if sensitive_value.is_empty() {
            value
        } else {
            value.replace(sensitive_value, "<redacted>")
        };
        crate::provider_tool_execution::sanitize_provider_diagnostic_text(redacted.as_str())
    };
    failure.error.subcategory = failure.error.subcategory.take().map(&sanitize);
    failure.error.stage = sanitize(std::mem::take(&mut failure.error.stage));
    failure.error.upstream_code = failure.error.upstream_code.take().map(&sanitize);
    failure.error.upstream_type = failure.error.upstream_type.take().map(&sanitize);
    failure.error.upstream_parameter = failure.error.upstream_parameter.take().map(&sanitize);
    failure.error.upstream_message = failure.error.upstream_message.take().map(&sanitize);
    failure.error.request_id = failure.error.request_id.take().map(&sanitize);
    failure.error.provider_id = failure.error.provider_id.take().map(&sanitize);
    failure.error.entry_id = failure.error.entry_id.take().map(&sanitize);
    failure.error.model_id = failure.error.model_id.take().map(&sanitize);
    failure.error.message = sanitize(std::mem::take(&mut failure.error.message));
    failure
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CredentialMode, MINIMAX_REASONING_SPLIT_PARAMETER_ID, ProviderRegistry,
        ResponsesRemoteCompactionPolicy, ResponsesRouteFeatures, ResponsesWebsocketPolicy,
    };
    use futures_util::SinkExt;
    use serde_json::json;
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};
    use tokio_tungstenite::{
        accept_async,
        tungstenite::{
            Message,
            protocol::{CloseFrame, frame::coding::CloseCode},
        },
    };
    use vulcan_ai_contract::ReasoningEffort;
    use vulcan_ai_contract::{
        AiContinuation, AiMessage, AiOperation, ContentItem, MessageRole, StructuredOutputPolicy,
    };
    use vulcan_ai_protocols::{
        ANTHROPIC_MESSAGES_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
        ProtocolRegistry,
    };

    impl ProtocolRuntimeSource for Arc<ProtocolRegistry> {
        /// Converts the legacy registry only for existing unit-test fixtures.
        /// 仅为既有单元测试夹具转换旧注册表。
        fn into_protocol_runtime(self) -> Arc<LlmProtocolRuntime> {
            Arc::new(LlmProtocolRuntime::new(self))
        }
    }

    /// Verifies every executable built-in provider protocol is owned by the Responses conversion core.
    /// 验证每个可执行的内置供应商协议都由 Responses 转换核心负责。
    #[test]
    fn every_builtin_provider_protocol_routes_through_responses_core() {
        let registry = ProviderRegistry::first_batch().expect("provider registry should build");
        let unmapped = registry
            .providers()
            .into_iter()
            .flat_map(|provider| {
                provider.service_entries.iter().flat_map(move |entry| {
                    entry
                        .protocol_routes
                        .iter()
                        .filter(|route| {
                            upstream_protocol_for_profile_id(route.protocol_profile_id.as_str())
                                .is_none()
                        })
                        .map(move |route| {
                            format!("{}/{}/{}", provider.id, entry.id, route.protocol_profile_id)
                        })
                })
            })
            .collect::<Vec<_>>();

        assert!(
            unmapped.is_empty(),
            "built-in provider protocols bypass Responses core: {unmapped:?}"
        );
    }

    /// Builds one exact execution route from a bundled manifest provider.
    /// 从内置清单供应商构建一条精确执行路由。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` selects the bundled manifest, `entry_id` the service entry, and `protocol_id`
    /// the frozen protocol profile.
    /// `provider_id` 选择内置清单，`entry_id` 选择服务入口，`protocol_id` 选择冻结协议 Profile。
    ///
    /// # Returns
    /// 返回值
    /// A resolved route whose usage profiles come from the manifest declarations.
    /// 返回已解析路由，其 usage Profile 来自清单声明。
    fn bundled_manifest_route(
        provider_id: &str,
        entry_id: &str,
        protocol_id: &str,
    ) -> ProviderExecutionRoute {
        let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
        let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
            .unwrap_or_else(|_| panic!("missing bundled manifest `{provider_id}`"));
        let manifest = crate::provider_manifest::compile_manifest_text(
            &text,
            crate::provider_manifest::ManifestOwnership::System,
        )
        .expect("bundled manifest must compile");
        let provider = manifest.provider().clone();
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == entry_id)
            .expect("manifest entry must exist")
            .clone();
        // The first registered credential profile is the deterministic test identity; entries
        // declare different profile ids per product (e.g. MiniMax payg/token-plan).
        // 首个注册凭据配置档是确定性测试身份；入口按产品声明不同配置档（如 MiniMax）。
        let credential_profile = entry.credential_profiles.first().cloned();
        let credential_profile_id = credential_profile
            .as_ref()
            .map(|profile| profile.id.clone())
            .unwrap_or_else(|| String::from("api-key"));
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: provider.id.clone(),
            service_entry_id: entry.id.clone(),
            credential_profile_id,
            credential_mode: credential_profile
                .map(|profile| profile.credential_mode)
                .unwrap_or(CredentialMode::Token),
            credential_id: String::from("credential"),
            display_name: String::from("Primary"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            service_origin_override: None,
            verified_protocols: BTreeSet::from([protocol_id.to_string()]),
            selected_protocol: protocol_id.to_string(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        let effective_protocol_id = connection.selected_protocol.clone();
        let mut route = ProviderExecutionRoute::resolve(
            provider,
            entry,
            connection,
            SecondaryRouteSelection::Automatic,
            effective_protocol_id.as_str(),
        )
        .expect("bundled manifest route should resolve");
        route.upload_channels = manifest.upload_channels.clone();
        route
    }

    /// Verifies conversation media upload selection is frozen entirely from bundled manifests.
    /// 验证对话媒体上传选择完全由内置 Manifest 冻结。
    #[test]
    fn bundled_media_upload_routes_are_manifest_driven() {
        let kimi =
            bundled_manifest_route("kimi", "coding-plan", ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID);
        let kimi_video = kimi
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| constraint.resource_kind == ResourceKind::Video)
            .expect("Kimi video route must declare its upload binding");
        assert_eq!(
            kimi_video.upload_channel_refs,
            [String::from("coding-plan.video-file")]
        );
        assert!(kimi.upload_channels.iter().any(|channel| {
            channel.upload_channel_id == "coding-plan.video-file"
                && channel.primitive == Some(crate::UploadPrimitiveKind::MultipartFile)
        }));

        let minimax = bundled_manifest_route("minimax", "cn", OPENAI_CHAT_PROFILE_ID);
        let minimax_video = minimax
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| constraint.resource_kind == ResourceKind::Video)
            .expect("MiniMax video route must declare its upload binding");
        assert_eq!(
            minimax_video.upload_channel_refs,
            [String::from("cn.video-file")]
        );

        let alibaba =
            bundled_manifest_route("alibaba", "model-studio-cn", OPENAI_RESPONSES_PROFILE_ID);
        let alibaba_image = alibaba
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| constraint.resource_kind == ResourceKind::Image)
            .expect("Alibaba image route must declare its upload binding");
        assert_eq!(
            alibaba_image.upload_channel_refs,
            [String::from("cn.temporary-oss")]
        );
        assert_eq!(
            alibaba
                .protocol_route
                .request_headers
                .get("x-dashscope-ossresource-resolve")
                .map(String::as_str),
            Some("enable")
        );
        assert!(alibaba.upload_channels.iter().any(|channel| {
            channel.upload_channel_id == "cn.temporary-oss"
                && channel.primitive == Some(crate::UploadPrimitiveKind::SignedObjectUpload)
                && channel.signing_primitive
                    == Some(crate::UploadSigningPrimitiveKind::TemporaryOssPolicy)
        }));
    }

    /// Verifies prepared media keys isolate account, protocol, and manifest route generations.
    /// 验证媒体准备缓存键会隔离账号、协议与 Manifest 路由代际。
    #[test]
    fn prepared_media_key_isolates_frozen_route_identity() {
        // route is a cacheable Kimi video route with an explicit frozen generation identity.
        // route 是带显式冻结代际身份且允许缓存的 Kimi 视频路由。
        let mut route =
            bundled_manifest_route("kimi", "coding-plan", ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID);
        route.manifest_revision = 10;
        route.manifest_generation = 20;
        route.route_revision = String::from("sha256:route-a");
        // first is the baseline key for one exact account and route generation.
        // first 是一个精确账号与路由代际的基准缓存键。
        let first = prepared_media_key(
            &route,
            "kimi-for-coding",
            Path::new("C:/media/video.mp4"),
            ResourceKind::Video,
            "video/mp4",
        )
        .expect("cacheable upload route should produce a key");
        // replaced_manifest models a hot replacement that keeps provider, account, and model stable.
        // replaced_manifest 模拟保持供应商、账号与模型不变的热替换。
        let mut replaced_manifest = route.clone();
        replaced_manifest.manifest_revision = 11;
        replaced_manifest.manifest_generation = 21;
        replaced_manifest.route_revision = String::from("sha256:route-b");
        // replacement_key must not reuse a reference produced by the previous manifest route.
        // replacement_key 不得复用上一 Manifest 路由生成的引用。
        let replacement_key = prepared_media_key(
            &replaced_manifest,
            "kimi-for-coding",
            Path::new("C:/media/video.mp4"),
            ResourceKind::Video,
            "video/mp4",
        )
        .expect("replacement route should produce a key");
        assert_ne!(first, replacement_key);
        // other_account proves credential connections never share provider-native references.
        // other_account 证明不同凭据连接绝不会共享供应商原生引用。
        let mut other_account = route.clone();
        other_account.connection.id = String::from("secondary-account");
        // account_key is scoped to the second exact credential connection.
        // account_key 限定到第二条精确凭据连接。
        let account_key = prepared_media_key(
            &other_account,
            "kimi-for-coding",
            Path::new("C:/media/video.mp4"),
            ResourceKind::Video,
            "video/mp4",
        )
        .expect("second account route should produce a key");
        assert_ne!(first, account_key);
    }

    /// Verifies prepared media cache pruning expires entries and enforces global capacity.
    /// 验证媒体准备缓存裁剪会清除过期项并强制全局容量上限。
    #[test]
    fn prepared_media_cache_pruning_is_bounded() {
        // now is the deterministic pruning boundary shared by every fixture entry.
        // now 是全部测试条目共享的确定性裁剪边界。
        let now = Instant::now();
        // cache intentionally exceeds capacity and includes one already expired entry.
        // cache 故意超过容量，并包含一条已经过期的记录。
        let mut cache = BTreeMap::new();
        for index in 0..(PREPARED_MEDIA_CACHE_MAX_ENTRIES + 8) {
            // created_at gives every entry a strict oldest-first order.
            // created_at 为每条记录提供严格的最旧优先顺序。
            let created_at = now + Duration::from_millis(index as u64 + 1);
            cache.insert(
                format!("key-{index:04}"),
                PreparedMediaCacheEntry {
                    url: format!("https://example.invalid/{index}"),
                    expires_at: now + Duration::from_secs(60),
                    created_at,
                    source_fingerprint: PreparedMediaSourceFingerprint {
                        byte_len: 1,
                        modified_at: None,
                        content_sha256: [0; 32],
                    },
                },
            );
        }
        cache.insert(
            String::from("expired"),
            PreparedMediaCacheEntry {
                url: String::from("https://example.invalid/expired"),
                expires_at: now,
                created_at: now,
                source_fingerprint: PreparedMediaSourceFingerprint {
                    byte_len: 1,
                    modified_at: None,
                    content_sha256: [u8::MAX; 32],
                },
            },
        );
        prune_prepared_media_cache(&mut cache, now);
        assert_eq!(cache.len(), PREPARED_MEDIA_CACHE_MAX_ENTRIES);
        assert!(!cache.contains_key("expired"));
        assert!(!cache.contains_key("key-0000"));
        assert!(cache.contains_key(&format!("key-{:04}", PREPARED_MEDIA_CACHE_MAX_ENTRIES + 7)));
    }

    /// Verifies equal-length media replacements receive distinct content-addressed fingerprints.
    /// 验证同长度媒体替换会获得不同的内容寻址指纹。
    #[tokio::test]
    async fn prepared_media_fingerprint_detects_equal_length_content_replacement() {
        // path is unique per test process and never overlaps a user-owned media source.
        // path 在每个测试进程中唯一，绝不会与用户媒体源重叠。
        let path = std::env::temp_dir().join(format!(
            "vulcan-prepared-media-fingerprint-{}.bin",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::write(path.as_path(), b"source-a")
            .await
            .expect("first media fixture should be written");
        let first = prepared_media_source_fingerprint(path.as_path(), None, None)
            .await
            .expect("first media fingerprint should be readable");

        tokio::fs::write(path.as_path(), b"source-b")
            .await
            .expect("replacement media fixture should be written");
        let second = prepared_media_source_fingerprint(path.as_path(), None, None)
            .await
            .expect("replacement media fingerprint should be readable");

        assert_eq!(first.byte_len, second.byte_len);
        assert_ne!(first.content_sha256, second.content_sha256);
        tokio::fs::remove_file(path.as_path())
            .await
            .expect("media fixture should be removed");
    }

    /// Verifies provider-specialized dynamic usage paths are frozen by manifest declarations.
    /// 验证供应商专属动态用量路径由清单声明冻结（方案 §15.4）。
    #[test]
    fn exact_provider_routes_install_specialized_usage_profiles() {
        // The bundled manifest is the single fact source for the Alibaba Responses billing details.
        // 内置清单是 Alibaba Responses 计费明细的唯一事实来源。
        let alibaba_route =
            bundled_manifest_route("alibaba", "model-studio-cn", OPENAI_RESPONSES_PROFILE_ID);
        // alibaba_route is the immutable execution snapshot inspected below.
        // alibaba_route 是下方检查的不可变执行快照。
        assert!(
            alibaba_route
                .protocol_route
                .usage_extraction_profiles
                .iter()
                .all(|profile| profile.profile_id.starts_with("alibaba.openai.responses."))
        );
        assert!(
            alibaba_route
                .protocol_route
                .usage_extraction_profiles
                .iter()
                .all(|profile| profile.bindings.iter().any(|binding| {
                    binding.field == vulcan_llm_core::UsageField::BillingDetails
                        && binding.encoding
                            == vulcan_llm_core::UsageValueEncoding::AlibabaResponsesBillingDetails
                }))
        );

        // deepseek_route is resolved from the bundled manifest Chat route.
        // deepseek_route 由内置清单 Chat 路由解析。
        let deepseek_route =
            bundled_manifest_route("deepseek", "official-api", OPENAI_CHAT_PROFILE_ID);
        assert!(
            deepseek_route
                .protocol_route
                .usage_extraction_profiles
                .iter()
                .all(|profile| profile.profile_id.starts_with("deepseek.chat."))
        );
        assert!(
            deepseek_route
                .protocol_route
                .usage_extraction_profiles
                .iter()
                .all(|profile| profile.bindings.iter().any(|binding| {
                    binding.field == vulcan_llm_core::UsageField::UncachedInputTokens
                        && binding.pointers == [String::from("/usage/prompt_cache_miss_tokens")]
                }))
        );

        // openrouter_route proves the manifest binds the usage profile to the exact Chat route.
        // openrouter_route 证明清单把用量 Profile 绑定到精确 Chat 路由。
        let openrouter_route =
            bundled_manifest_route("openrouter", "public-api", OPENAI_CHAT_PROFILE_ID);
        assert!(
            openrouter_route
                .protocol_route
                .usage_extraction_profiles
                .iter()
                .all(|profile| profile.profile_id.starts_with("openrouter.chat."))
        );
        assert!(
            openrouter_route
                .protocol_route
                .usage_extraction_profiles
                .iter()
                .all(|profile| profile.bindings.iter().any(|binding| {
                    binding.field == vulcan_llm_core::UsageField::ReportedCostUsd
                        && binding.pointers == [String::from("/usage/cost")]
                }))
        );
    }

    /// Verifies local estimates never assign their currency to incompatible reported cost evidence.
    /// 验证本地估算绝不会把自身币种赋给不兼容的供应商上报成本证据。
    #[test]
    fn local_cost_estimate_rejects_reported_currency_conflicts() {
        // route owns the frozen USD estimate that intentionally conflicts with reported EUR.
        // route 持有与供应商上报 EUR 故意冲突的冻结 USD 估算。
        let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        route.billing_profile = Some(ResolvedBillingProfile {
            pricing_revision: String::from("pricing-revision"),
            currency: String::from("USD"),
            pricing: ModelPricing::from_optional_strings(
                Some("1"),
                Some("2"),
                Some("0"),
                Some("0"),
            )
            .expect("pricing should parse"),
            multiplier: Decimal::ONE,
            snapshot: vulcan_ai_contract::PricingProfileSnapshot {
                pricing_revision: String::from("pricing-revision"),
                provider_id: String::from("openrouter"),
                service_entry_id: Some(String::from("public")),
                model_id: String::from("openai/gpt-5"),
                currency: String::from("USD"),
                input_per_million: Some(String::from("1")),
                output_per_million: Some(String::from("2")),
                cache_read_per_million: Some(String::from("0")),
                cache_write_per_million: Some(String::from("0")),
                source_kind: String::from("test"),
                source_id: String::from("test"),
                content_hash: String::from("content-hash"),
            },
        });
        // event carries exact provider evidence before the local estimate is attempted.
        // event 在尝试本地估算前承载精确供应商证据。
        let mut event = AiStreamEvent::UsageUpdated {
            usage: Box::new(vulcan_ai_contract::RequestUsage {
                uncached_input_tokens: vulcan_ai_contract::UsageValue::Known {
                    value: 1_000_000,
                    source: vulcan_ai_contract::UsageValueSource::Reported,
                },
                output_tokens: vulcan_ai_contract::UsageValue::Known {
                    value: 500_000,
                    source: vulcan_ai_contract::UsageValueSource::Reported,
                },
                cost: Some(String::from("0.5")),
                cost_currency: Some(String::from("EUR")),
                ..vulcan_ai_contract::RequestUsage::default()
            }),
        };

        apply_local_cost_estimate(&route, &mut event);

        // usage is the post-projection evidence inspected for accidental currency overwrite.
        // usage 是投影后用于检查意外币种覆盖的证据。
        let AiStreamEvent::UsageUpdated { usage } = event else {
            panic!("usage event should remain a usage event");
        };
        assert_eq!(usage.cost.as_deref(), Some("0.5"));
        assert_eq!(usage.cost_currency.as_deref(), Some("EUR"));
        assert!(usage.estimated_cost.is_none());
        assert!(usage.pricing_revision.is_none());
        assert!(usage.pricing_profile.is_none());
        assert_eq!(
            usage.validation_flags,
            [String::from(
                "reported_and_estimated_cost_currency_mismatch"
            )]
        );
    }

    /// Verifies raw usage is retained but cannot precede the normalized response start.
    /// 验证原始用量会被保留但不能先于规范化响应开始事件。
    #[test]
    fn profiled_usage_gate_releases_usage_after_response_start() {
        let mut gate = ProfiledUsageEventGate::default();
        let usage_event = AiStreamEvent::UsageUpdated {
            usage: Box::new(vulcan_ai_contract::RequestUsage::default()),
        };
        assert!(
            gate.stage(vec![usage_event])
                .expect("one pending usage event should remain within the bound")
                .is_empty()
        );

        let start = AiStreamEvent::ResponseStarted {
            execution_id: String::from("execution"),
        };
        let mut state = AiStreamStateMachine::default();
        state.apply(&start).expect("start should be valid");
        let released = gate.observe(&start);
        assert_eq!(released.len(), 1);
        state
            .apply(&released[0])
            .expect("released usage should follow response start");
        gate.ensure_released()
            .expect("all pending usage should be released");
    }

    /// Verifies a transport cannot silently discard usage when normalization never starts.
    /// 验证规范化从未开始时传输不能静默丢弃用量。
    #[test]
    fn profiled_usage_gate_rejects_missing_response_start() {
        let mut gate = ProfiledUsageEventGate::default();
        let usage_event = AiStreamEvent::UsageUpdated {
            usage: Box::new(vulcan_ai_contract::RequestUsage::default()),
        };
        assert!(
            gate.stage(vec![usage_event])
                .expect("one pending usage event should remain within the bound")
                .is_empty()
        );
        assert!(gate.ensure_released().is_err());
    }

    /// Verifies pre-start usage staging fails closed at its explicit event ceiling.
    /// 验证响应开始前的用量暂存会在显式事件上限处封闭失败。
    #[test]
    fn profiled_usage_gate_rejects_unbounded_pre_start_events() {
        // gate starts before normalization and therefore retains extracted usage temporarily.
        // gate 在规范化前启动，因此会临时保留提取的用量。
        let mut gate = ProfiledUsageEventGate::default();
        // pending_events fills the complete reviewed staging capacity in one bounded batch.
        // pending_events 在一个有界批次中填满全部已审核暂存容量。
        let pending_events = (0..PROFILED_USAGE_GATE_MAX_PENDING_EVENTS)
            .map(|_| AiStreamEvent::UsageUpdated {
                usage: Box::new(vulcan_ai_contract::RequestUsage::default()),
            })
            .collect::<Vec<_>>();
        assert!(
            gate.stage(pending_events)
                .expect("the exact pending usage ceiling should be accepted")
                .is_empty()
        );
        // overflow is the first event that would make the retained vector unbounded.
        // overflow 是首个会使保留向量越过上限的事件。
        let overflow = AiStreamEvent::UsageUpdated {
            usage: Box::new(vulcan_ai_contract::RequestUsage::default()),
        };

        // error proves the gate rejects overflow instead of dropping evidence or growing memory.
        // error 证明门控会拒绝溢出，而不是丢弃证据或继续增长内存。
        let error = gate
            .stage(vec![overflow])
            .expect_err("usage staging beyond the ceiling must fail closed");
        assert!(error.message.contains("256-event limit"));
    }

    /// Verifies an emitted usage fact closes the transparent-replay safety window.
    /// 验证已发出的用量事实会关闭透明重放安全窗口。
    #[test]
    fn usage_output_marks_subsequent_transport_failure_retry_unsafe() {
        let mut observed = false;
        let mut usage = AiStreamEvent::UsageUpdated {
            usage: Box::new(vulcan_ai_contract::RequestUsage::default()),
        };
        update_semantic_output_observation(&mut observed, &mut usage);
        assert!(observed);

        let failure = stream_transport_failure("stream lost", observed);
        assert!(failure.error.semantic_output_observed);
        assert!(!failure.error.retryable);
    }

    /// Verifies an exact upstream context code is distinct from generic invalid requests.
    /// 验证精确上游上下文代码与通用无效请求具有不同类别。
    #[test]
    fn http_context_length_code_is_typed() {
        let failure = http_failure(
            400,
            None,
            r#"{"error":{"type":"invalid_request_error","code":"context_length_exceeded","message":"too large"}}"#,
        );

        assert_eq!(
            failure.error.category,
            AiErrorCategory::ContextLengthExceeded
        );
        assert_eq!(
            failure.error.upstream_code.as_deref(),
            Some("context_length_exceeded")
        );
    }

    /// Verifies every public execution diagnostic is bounded and cannot retain the exact secret.
    /// 验证每个公共执行诊断均有界且无法保留精确秘密。
    #[test]
    fn execution_failure_boundary_redacts_exact_secret_and_provider_urls() {
        // secret models an opaque value whose shape alone would not trigger generic redaction.
        // secret 模拟一个仅凭形态不会触发通用脱敏的不透明值。
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from(
            "opaque-execution-credential-value",
        ))
        .expect("test credential should validate")
        .execution_secret()
        .expect("test execution secret should resolve");
        // failure carries the secret in a provider message and a signed URL in another public field.
        // failure 在供应商消息中携带秘密，并在另一公共字段中携带签名 URL。
        let mut failure = http_failure(
            401,
            None,
            r#"{"error":{"type":"authentication_error","message":"echo opaque-execution-credential-value"}}"#,
        );
        failure.error.model_id = Some(String::from(
            "https://provider.example/result?signature=private",
        ));
        failure.error.request_id = Some("r".repeat(8_192));

        // sanitized preserves typed semantics while closing every public diagnostic field.
        // sanitized 在封闭每个公共诊断字段的同时保留强类型语义。
        let sanitized = sanitize_provider_execution_failure(failure, &secret);
        assert_eq!(sanitized.error.category, AiErrorCategory::Authentication);
        assert!(!sanitized.error.message.contains(secret.expose_value()));
        assert!(
            !sanitized
                .error
                .upstream_message
                .as_deref()
                .unwrap_or_default()
                .contains(secret.expose_value())
        );
        assert_eq!(sanitized.error.model_id.as_deref(), Some("<redacted-url>"));
        assert!(
            sanitized
                .error
                .request_id
                .as_deref()
                .is_some_and(|value| value.chars().count() <= 2_048)
        );
    }

    /// Verifies provider request identifiers follow the route-declared header order exactly.
    /// 验证供应商请求标识严格遵循路由声明的响应头顺序。
    #[test]
    fn provider_request_id_uses_declared_order() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", "primary".parse().expect("valid header"));
        headers.insert(
            "x-oai-request-id",
            "secondary".parse().expect("valid header"),
        );
        let declared_names = vec![
            String::from("x-oai-request-id"),
            String::from("x-request-id"),
        ];

        assert_eq!(
            provider_request_id_from_headers(&headers, declared_names.as_slice())
                .expect("declared request-id headers should decode"),
            Some(String::from("secondary"))
        );
        assert_eq!(
            provider_request_id_from_headers(&headers, &[])
                .expect("an undeclared route should not inspect headers"),
            None
        );
    }

    /// Verifies provider-controlled diagnostic header values remain explicitly bounded.
    /// 验证供应商控制的诊断响应头值保持显式有界。
    #[test]
    fn provider_diagnostic_header_values_are_bounded() {
        // oversized_value exceeds the complete retained telemetry budget.
        // oversized_value 超过遥测完整保留预算。
        let oversized_value = "x".repeat(PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES + 128);
        // headers carries the same adversarial value through both diagnostic extraction paths.
        // headers 通过两条诊断提取路径携带同一恶意超长值。
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-request-id",
            HeaderValue::from_bytes(oversized_value.as_bytes())
                .expect("oversized ASCII header must parse"),
        );
        headers.insert(
            "retry-after",
            HeaderValue::from_bytes(oversized_value.as_bytes())
                .expect("oversized ASCII header must parse"),
        );
        // request_id is the bounded route-declared diagnostic value.
        // request_id 是有界的路由声明诊断值。
        let request_id =
            provider_request_id_from_headers(&headers, &[String::from("x-request-id")])
                .expect("valid diagnostic text must decode")
                .expect("declared response header must be present");
        assert_eq!(request_id.len(), PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES);
        assert!(request_id.ends_with(PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX));

        // rate_limit_headers contains only the fixed allowlist and the same bounded representation.
        // rate_limit_headers 仅包含固定允许列表与相同的有界表示。
        let rate_limit_headers = allowlisted_rate_limit_headers(&headers);
        let retry_after = rate_limit_headers
            .get("retry-after")
            .expect("allowlisted rate-limit header must be retained");
        assert_eq!(
            retry_after.len(),
            PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES
        );
        assert!(retry_after.ends_with(PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX));

        // unicode_value proves the shared truncator never splits a UTF-8 scalar.
        // unicode_value 证明共享截断器绝不会切开 UTF-8 标量。
        let unicode_value = "测".repeat(PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES);
        let bounded_unicode = bounded_provider_diagnostic_header_value(&unicode_value);
        assert!(bounded_unicode.len() <= PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES);
        assert!(bounded_unicode.ends_with(PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX));
    }

    /// Verifies only a proven connection-establishment failure is safe to replay.
    /// 验证只有已证明的连接建立失败才可安全重放。
    #[tokio::test]
    async fn http_send_failure_uses_reqwest_connection_evidence() {
        // listener reserves one loopback port so the subsequent closed-port connection fails deterministically.
        // listener 预留一个回环端口，使随后对已关闭端口的连接确定性失败。
        let listener =
            std::net::TcpListener::bind(("127.0.0.1", 0)).expect("loopback listener should bind");
        // port is the exact operating-system-assigned loopback port.
        // port 是操作系统精确分配的回环端口。
        let port = listener
            .local_addr()
            .expect("loopback address should resolve")
            .port();
        drop(listener);

        // connect_error is produced before an HTTP connection can be established.
        // connect_error 产生于 HTTP 连接建立之前。
        let connect_error = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/responses"))
            .send()
            .await
            .expect_err("closed loopback port should reject the connection");
        assert!(connect_error.is_connect());
        // connect_failure is therefore explicitly safe for structured retry.
        // connect_failure 因而可明确安全地进入结构化重试。
        let connect_failure = http_send_transport_failure(&connect_error, "connect failed");
        assert_eq!(
            connect_failure.error.dispatch_outcome,
            DispatchOutcome::NotDispatched
        );
        assert_eq!(connect_failure.error.transport, Some(AiTransportKind::Http));
        assert!(connect_failure.error.retryable);

        // builder_error proves that unrelated request-send failures retain the conservative outcome.
        // builder_error 证明无关的请求发送失败会保留保守结果。
        let builder_error = reqwest::Client::new()
            .post("not a valid provider URL")
            .send()
            .await
            .expect_err("invalid URL should fail before transport");
        assert!(!builder_error.is_connect());
        // builder_failure must not be promoted to replay-safe without connection evidence.
        // builder_failure 在缺少连接证据时不得被提升为可安全重放。
        let builder_failure = http_send_transport_failure(&builder_error, "builder failed");
        assert_eq!(
            builder_failure.error.dispatch_outcome,
            DispatchOutcome::Unknown
        );
        assert_eq!(builder_failure.error.transport, Some(AiTransportKind::Http));
    }

    /// Verifies HTTP stream failures cannot be mislabeled as Responses WebSocket failures.
    /// 验证 HTTP 流失败不会被错误标记为 Responses WebSocket 失败。
    #[test]
    fn http_stream_failure_retains_http_transport() {
        let failure = stream_transport_failure("stream ended", false);

        assert_eq!(failure.error.transport, Some(AiTransportKind::Http));
        assert_eq!(failure.error.websocket_close_code, None);
    }

    /// Verifies WebSocket decoder failures retain the WebSocket transport classification.
    /// 验证 WebSocket 解码失败保留 WebSocket 传输分类。
    #[test]
    fn websocket_protocol_failure_retains_websocket_transport() {
        let failure = stream_protocol_failure_for_transport(
            ProtocolConversionError::new("bad frame"),
            false,
            AiTransportKind::ResponsesWebsocket,
        );

        assert_eq!(
            failure.error.transport,
            Some(AiTransportKind::ResponsesWebsocket)
        );
    }

    /// Verifies an empty upstream error body retains the exact HTTP status.
    /// 验证空上游错误正文仍会保留精确 HTTP 状态。
    #[test]
    fn empty_http_error_response_reports_status() {
        let failure = http_failure(404, None, "");

        assert_eq!(failure.error.http_status, Some(404));
        assert_eq!(
            failure.error.message,
            "Provider returned HTTP 404 with an empty error response."
        );
    }

    /// Verifies the official Codex overload code overrides a generic HTTP proxy error type.
    /// 验证官方 Codex 过载代码会覆盖 HTTP 代理返回的通用错误类型。
    #[test]
    fn http_server_overloaded_code_is_retryable() {
        // failure models the structured 503 body used by official Codex and compatible proxies.
        // failure 模拟官方 Codex 与兼容代理使用的结构化 503 正文。
        let failure = http_failure(
            503,
            None,
            r#"{"error":{"type":"server","code":"server_is_overloaded","message":"Our servers are currently overloaded. Please try again later."}}"#,
        );

        assert_eq!(failure.error.category, AiErrorCategory::ProviderOverloaded);
        assert!(failure.error.retryable);
        assert_eq!(
            failure.error.upstream_code.as_deref(),
            Some("server_is_overloaded")
        );
    }

    /// Verifies the official overload message remains retryable when a proxy returns plain text.
    /// 验证代理返回纯文本时，官方过载消息仍会被归入可重试类别。
    #[test]
    fn http_server_overloaded_plain_text_is_retryable() {
        let failure = http_failure(
            400,
            None,
            "Our servers are currently overloaded. Please try again later.",
        );

        assert_eq!(failure.error.category, AiErrorCategory::ProviderOverloaded);
        assert!(failure.error.retryable);
        assert_eq!(
            failure.error.upstream_message.as_deref(),
            Some("Our servers are currently overloaded. Please try again later.")
        );
    }

    /// Verifies provider-controlled shape labels are bounded and cannot inject trace syntax.
    /// 验证供应商控制的形状标签有界且无法注入追踪语法。
    #[test]
    fn compact_shape_trace_labels_are_bounded_and_sanitized() {
        // normalized removes whitespace and punctuation while preserving safe structural characters.
        // normalized 移除空白与标点，同时保留安全结构字符。
        let normalized = bounded_trace_label("compaction\r\nsecret:value/field");
        assert_eq!(normalized, "compaction__secret_value_field");

        // bounded caps provider-controlled labels at the documented maximum length.
        // bounded 把供应商控制的标签限制在文档规定的最大长度内。
        let bounded = bounded_trace_label("a".repeat(80).as_str());
        assert_eq!(bounded.len(), 64);
    }

    /// Builds one minimal provider-neutral conversation request.
    /// 构建一个最小供应商无关对话请求。
    fn minimal_request() -> AiExecutionRequest {
        AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: String::from("openai/gpt-4.1"),
            cache_scope_id: None,
            continuation: None,
            messages: vec![AiMessage {
                role: MessageRole::User,
                content: vec![ContentItem::Text {
                    text: String::from("hello"),
                }],
                provider_context_items: Vec::new(),
            }],
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: PromptCachePolicy::ProviderDefault,
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: BTreeMap::new(),
            resources: Vec::new(),
            output_token_limit: Some(512),
        }
    }

    /// Verifies WebSocket reuse keys are opaque and change with the exact endpoint snapshot.
    /// 验证 WebSocket 复用键不透明，并会随精确 endpoint 快照变化。
    #[test]
    fn responses_websocket_cache_key_is_bounded_and_endpoint_bound() {
        let mut route = deepseek_route(OPENAI_CHAT_PROFILE_ID);
        let mut request = minimal_request();
        request.cache_scope_id = Some(String::from("session-sensitive-value"));
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("test-secret"))
            .expect("test secret envelope should validate")
            .execution_secret()
            .expect("test execution secret should resolve");
        let first = responses_websocket_cache_key(&route, &request, &secret)
            .expect("session-bound request should produce a key");
        assert_eq!(first.len(), "sha256:".len() + 64);
        assert!(!first.contains("session-sensitive-value"));
        assert!(!first.contains(route.entry.base_url.as_str()));

        route.entry.base_url = String::from("https://changed.example.invalid/v1");
        let changed = responses_websocket_cache_key(&route, &request, &secret)
            .expect("changed endpoint should still produce a key");
        assert_ne!(first, changed);
    }

    /// Verifies sticky WebSocket breakers retain a hard LRU bound and refresh existing keys.
    /// 验证粘性 WebSocket 熔断器保持硬性 LRU 上限，并刷新既有键。
    #[test]
    fn responses_websocket_breakers_are_bounded_and_lru() {
        let base = Instant::now();
        let mut breakers = BTreeMap::new();
        for index in 0..RESPONSES_WEBSOCKET_BREAKER_MAX_ENTRIES {
            insert_bounded_responses_websocket_breaker(
                &mut breakers,
                format!("key-{index:04}").as_str(),
                base + Duration::from_millis(index as u64),
            );
        }
        insert_bounded_responses_websocket_breaker(
            &mut breakers,
            "key-0000",
            base + Duration::from_secs(10),
        );
        insert_bounded_responses_websocket_breaker(
            &mut breakers,
            "key-new",
            base + Duration::from_secs(11),
        );

        assert_eq!(breakers.len(), RESPONSES_WEBSOCKET_BREAKER_MAX_ENTRIES);
        assert!(breakers.contains_key("key-0000"));
        assert!(!breakers.contains_key("key-0001"));
        assert!(breakers.contains_key("key-new"));
    }

    /// Verifies executable Gemini Interactions routes pass through the Responses conversion core.
    /// 验证可执行 Gemini Interactions 路由会经过 Responses 转换核心。
    #[test]
    fn gemini_interactions_route_uses_responses_baseline_and_preserves_replay() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        route.protocol_route.protocol_profile_id = String::from(GEMINI_INTERACTIONS_PROFILE_ID);
        route.protocol_route.endpoint_path = Some(String::from("/interactions"));
        route.connection.selected_protocol = String::from(GEMINI_INTERACTIONS_PROFILE_ID);
        route.connection.verified_protocols =
            BTreeSet::from([String::from(GEMINI_INTERACTIONS_PROFILE_ID)]);
        let mut request = minimal_request();
        request.messages.push(vulcan_ai_contract::AiMessage {
            role: vulcan_ai_contract::MessageRole::Assistant,
            content: vec![
                ContentItem::Reasoning {
                    text: String::from("signed reasoning"),
                    signature: Some(String::from("gemini#reasoning-signature")),
                },
                ContentItem::ToolCall {
                    call: vulcan_ai_contract::ToolCall {
                        id: String::from("call-1"),
                        name: String::from("lookup"),
                        arguments: json!({"q":"x"}),
                        signature: Some(String::from("gemini#call-signature")),
                    },
                },
            ],
            provider_context_items: Vec::new(),
        });
        request.tools.push(vulcan_ai_contract::ToolDefinition {
            name: String::from("lookup"),
            description: String::from("Lookup"),
            input_schema: json!({"type":"object"}),
            strict: false,
        });

        let encoded = client
            .encode_request(&route, &request)
            .expect("Interactions route should project from Responses");

        assert_eq!(encoded.path, "/interactions");
        assert!(encoded.body.get("contents").is_none());
        assert_eq!(encoded.body["input"][1]["type"], "thought");
        assert_eq!(encoded.body["input"][1]["signature"], "reasoning-signature");
        assert_eq!(encoded.body["input"][2]["type"], "function_call");
        assert!(
            encoded.body["input"][2]
                .get("x_vulcan_provider_signature")
                .is_none()
        );
    }

    /// Adds one exact Codex continuation boundary to a minimal test request.
    /// 为最小测试请求增加一项精确 Codex 延续边界。
    fn request_with_codex_continuation() -> AiExecutionRequest {
        let mut request = minimal_request();
        request.continuation = Some(AiContinuation {
            provider_id: String::from("openrouter"),
            protocol_profile_id: String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID),
            model_id: request.model_id.clone(),
            provider_message_id: String::from("response-1"),
            covered_local_message_id: String::from("message-1"),
            covered_message_count: 1,
        });
        request
    }

    /// Reads one bounded local HTTP request including its declared body.
    /// 读取一项有界本地 HTTP 请求及其声明正文。
    ///
    /// # Parameters
    /// 参数
    /// `stream` is the accepted loopback connection owned by one test request.
    /// `stream` 是一项测试请求持有的已接受环回连接。
    ///
    /// # Returns
    /// 返回值
    /// The complete UTF-8 request used only for transport assertions.
    /// 返回仅用于传输断言的完整 UTF-8 请求。
    async fn read_test_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 4_096];
        let expected_length = loop {
            let read = stream.read(&mut chunk).await.expect("request should read");
            assert!(read > 0, "request headers must not end early");
            bytes.extend_from_slice(&chunk[..read]);
            assert!(
                bytes.len() <= 128 * 1_024,
                "test request must remain bounded"
            );
            let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8(bytes[..header_end].to_vec())
                .expect("request headers should be UTF-8");
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .expect("request should declare content length");
            break header_end + 4 + content_length;
        };
        while bytes.len() < expected_length {
            let read = stream.read(&mut chunk).await.expect("body should read");
            assert!(read > 0, "request body must not end early");
            bytes.extend_from_slice(&chunk[..read]);
            assert!(
                bytes.len() <= 128 * 1_024,
                "test request must remain bounded"
            );
        }
        String::from_utf8(bytes).expect("request should be UTF-8")
    }

    /// Verifies store=false HTTP never emits a cursor and WS requires exact socket ownership.
    /// 验证 store=false HTTP 永不发送游标且 WS 要求精确 Socket 归属。
    #[tokio::test]
    async fn nonpersistent_responses_continuation_is_scoped_to_the_active_websocket() {
        let request = request_with_codex_continuation();
        let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
        route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);

        let http_request = request_without_nonpersistent_responses_continuation(&route, &request);
        assert!(http_request.continuation.is_none());

        let fresh_socket = responses_websocket_request_for_active_session(&request, None);
        assert!(fresh_socket.continuation.is_none());
        let wrong_socket =
            responses_websocket_request_for_active_session(&request, Some("response-other"));
        assert!(wrong_socket.continuation.is_none());
        let matching_socket =
            responses_websocket_request_for_active_session(&request, Some("response-1"));
        assert_eq!(
            matching_socket
                .continuation
                .as_ref()
                .map(|continuation| continuation.provider_message_id.as_str()),
            Some("response-1")
        );

        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
            .expect("test key should validate")
            .execution_secret()
            .expect("test key should resolve");
        let effective = client
            .encode_effective_request_with_credential(&route, &request, &secret)
            .await
            .expect("HTTP preview should encode");
        assert!(effective.body.get("previous_response_id").is_none());
    }

    /// Verifies only official recoverable WebSocket controls become replay-safe failures.
    /// 验证仅官方可恢复 WebSocket 控制错误会转换为可安全重放失败。
    #[test]
    fn websocket_retry_controls_preserve_code_and_semantic_safety() {
        for code in [
            RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE,
            RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE,
        ] {
            let (actual_code, message) = responses_websocket_retry_control(&json!({
                "type": "error",
                "error": {"code": code, "message": "retry the request"}
            }))
            .expect("official control should be recognized");
            let failure = responses_websocket_control_failure(actual_code, message, false);
            assert!(failure.error.retryable);
            assert_eq!(
                failure.error.transport,
                Some(AiTransportKind::ResponsesWebsocket)
            );
            assert_eq!(failure.error.upstream_code.as_deref(), Some(code));
            assert!(!failure.error.semantic_output_observed);
        }

        assert!(
            responses_websocket_retry_control(&json!({
                "type": "error",
                "error": {"code": "invalid_prompt", "message": "bad prompt"}
            }))
            .is_none()
        );
        let failure = responses_websocket_control_failure(
            String::from(RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE),
            String::from("retry"),
            true,
        );
        assert!(!failure.error.retryable);
        assert!(failure.error.semantic_output_observed);
    }

    /// Verifies a 1013 pre-semantic WS failure activates session-scoped HTTP fallback.
    /// 验证语义输出前的 1013 WS 失败会激活会话作用域 HTTP 回退。
    #[tokio::test]
    async fn websocket_1013_failure_falls_back_to_full_history_http_attempt() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("address should resolve");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("WS client should connect");
            let mut websocket = accept_async(stream)
                .await
                .expect("WS upgrade should succeed");
            let _ = websocket.next().await.expect("WS request should arrive");
            websocket
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::Again,
                    reason: "try HTTP".into(),
                })))
                .await
                .expect("1013 close should send");
            drop(websocket);

            let (mut stream, _) = listener.accept().await.expect("HTTP client should connect");
            let request = read_test_http_request(&mut stream).await;
            assert!(
                !request.contains("previous_response_id"),
                "store=false HTTP fallback must replay complete history"
            );
            let body = [
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"response-http\"}}\n\n",
                "data: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"item_id\":\"message-http\",\"delta\":\"OK\"}\n\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"response-http\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
            ]
            .concat();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("HTTP response should send");
        });

        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
        route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.protocol_route.base_url_override = Some(format!("http://{address}"));
        route.protocol_route.responses_features = Some(ResponsesRouteFeatures {
            remote_compaction: ResponsesRemoteCompactionPolicy::Disabled,
            websocket: ResponsesWebsocketPolicy::Verified,
        });
        route.connection.selected_protocol = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.verified_protocols =
            BTreeSet::from([String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID)]);
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
            .expect("test key should validate")
            .execution_secret()
            .expect("test key should resolve");
        let mut request = request_with_codex_continuation();
        request.cache_scope_id = Some(String::from("session-1"));

        let mut websocket_attempt = client.execute(
            route.clone(),
            request.clone(),
            secret.clone(),
            String::from("execution-ws"),
        );
        let mut websocket_observed = false;
        let failure = loop {
            match websocket_attempt.next().await {
                Some(Ok(AiStreamEvent::TransportObserved { transport, .. })) => {
                    websocket_observed |= transport == AiTransportKind::ResponsesWebsocket;
                }
                Some(Ok(_)) => {}
                Some(Err(failure)) => break failure,
                None => panic!("WS attempt must fail"),
            }
        };
        assert!(websocket_observed);
        assert_eq!(failure.error.websocket_close_code, Some(1013));
        assert!(!failure.error.semantic_output_observed);
        drop(websocket_attempt);

        let mut http_attempt =
            client.execute(route, request, secret, String::from("execution-http"));
        let mut http_observed = false;
        let mut completed = false;
        while let Some(event) = http_attempt.next().await {
            match event.expect("HTTP fallback should succeed") {
                AiStreamEvent::TransportObserved { transport, .. } => {
                    http_observed |= transport == AiTransportKind::Http;
                }
                AiStreamEvent::ResponseCompleted => completed = true,
                _ => {}
            }
        }
        assert!(http_observed);
        assert!(completed);
        server.await.expect("server should finish");
    }

    /// Verifies a missing socket-local response retries full history on the same live connection.
    /// 验证 Socket 局部响应缺失后会在同一活动连接上用完整历史重试。
    #[tokio::test]
    async fn websocket_previous_response_not_found_retries_without_stale_cursor() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("address should resolve");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("WS client should connect");
            let mut websocket = accept_async(stream)
                .await
                .expect("WS upgrade should succeed");

            let first = websocket
                .next()
                .await
                .expect("first request should arrive")
                .expect("first request should read");
            let Message::Text(first) = first else {
                panic!("first request must be text");
            };
            let first: Value = serde_json::from_str(&first).expect("first request should be JSON");
            assert!(first.get("previous_response_id").is_none());
            websocket
                .send(Message::Text(
                    json!({"type":"response.completed","response":{"id":"response-1"}})
                        .to_string()
                        .into(),
                ))
                .await
                .expect("first completion should send");

            let second = websocket
                .next()
                .await
                .expect("incremental request should arrive")
                .expect("incremental request should read");
            let Message::Text(second) = second else {
                panic!("incremental request must be text");
            };
            let second: Value =
                serde_json::from_str(&second).expect("incremental request should be JSON");
            assert_eq!(second["previous_response_id"], "response-1");
            websocket
                .send(Message::Text(
                    json!({
                        "type":"error",
                        "error":{
                            "code":RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE,
                            "message":"chain expired"
                        }
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .expect("missing-chain control should send");

            let third = websocket
                .next()
                .await
                .expect("full retry should arrive")
                .expect("full retry should read");
            let Message::Text(third) = third else {
                panic!("full retry must be text");
            };
            let third: Value = serde_json::from_str(&third).expect("full retry should be JSON");
            assert!(third.get("previous_response_id").is_none());
            websocket
                .send(Message::Text(
                    json!({"type":"response.completed","response":{"id":"response-2"}})
                        .to_string()
                        .into(),
                ))
                .await
                .expect("retry completion should send");
        });

        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
        route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.protocol_route.base_url_override = Some(format!("http://{address}"));
        route.protocol_route.responses_features = Some(ResponsesRouteFeatures {
            remote_compaction: ResponsesRemoteCompactionPolicy::Disabled,
            websocket: ResponsesWebsocketPolicy::Verified,
        });
        route.connection.selected_protocol = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.verified_protocols =
            BTreeSet::from([String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID)]);
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
            .expect("test key should validate")
            .execution_secret()
            .expect("test key should resolve");
        let mut request = request_with_codex_continuation();
        request.cache_scope_id = Some(String::from("session-previous-response"));

        let mut first = client.execute(
            route.clone(),
            request.clone(),
            secret.clone(),
            String::from("execution-first"),
        );
        let mut first_completed = false;
        while let Some(event) = first.next().await {
            first_completed |= matches!(
                event.expect("first request should succeed"),
                AiStreamEvent::ResponseCompleted
            );
        }
        assert!(first_completed);

        let mut incremental = client.execute(
            route.clone(),
            request.clone(),
            secret.clone(),
            String::from("execution-incremental"),
        );
        let failure = loop {
            match incremental.next().await {
                Some(Ok(_)) => {}
                Some(Err(failure)) => break failure,
                None => panic!("missing-chain request must fail"),
            }
        };
        assert_eq!(
            failure.error.upstream_code.as_deref(),
            Some(RESPONSES_WEBSOCKET_PREVIOUS_RESPONSE_NOT_FOUND_CODE)
        );
        assert!(failure.error.retryable);
        drop(incremental);

        let mut retry = client.execute(route, request, secret, String::from("execution-retry"));
        let mut retry_completed = false;
        while let Some(event) = retry.next().await {
            retry_completed |= matches!(
                event.expect("full-history retry should succeed"),
                AiStreamEvent::ResponseCompleted
            );
        }
        assert!(retry_completed);
        server.await.expect("server should finish");
    }

    /// Verifies the compact request body follows the official Codex `CompactionInput` shape.
    /// 验证 compact 请求体遵循官方 Codex `CompactionInput` 结构。
    #[test]
    fn codex_compact_body_removes_stream_only_and_unknown_fields() {
        let mut body = json!({
            "model": "gpt-codex",
            "input": [],
            "instructions": "",
            "stream": true,
            "store": false,
            "include": ["reasoning.encrypted_content"],
            "previous_response_id": "resp-old",
            "unsupported_vendor_field": true,
            "parallel_tool_calls": true,
            "tools": [{
                "type": "function",
                "name": "inspect_workspace",
                "parameters": {"type": "object", "properties": {}}
            }],
        });

        prepare_responses_compact_body(&mut body).expect("object compact body should normalize");

        assert_eq!(body["model"], "gpt-codex");
        assert!(body["input"].is_array());
        assert!(body.get("instructions").is_none());
        assert_eq!(body["parallel_tool_calls"], true);
        assert_eq!(body["tools"][0]["name"], "inspect_workspace");
        assert!(body.get("stream").is_none());
        assert!(body.get("store").is_none());
        assert!(body.get("include").is_none());
        assert!(body.get("previous_response_id").is_none());
        assert!(body.get("unsupported_vendor_field").is_none());
    }

    /// Verifies compact requests retain non-empty ordinary-turn base instructions.
    /// 验证压缩请求会保留非空的普通回合基础指令。
    #[test]
    fn codex_compact_body_preserves_nonempty_base_instructions() {
        let mut body = json!({
            "model": "gpt-codex",
            "input": [],
            "instructions": "ordinary Codex base instructions",
            "stream": true,
        });

        prepare_responses_compact_body(&mut body).expect("object compact body should normalize");

        assert_eq!(
            body["instructions"],
            Value::String(String::from("ordinary Codex base instructions"))
        );
        assert!(body.get("stream").is_none());
    }

    /// Verifies compact responses preserve metadata and reject output without an opaque checkpoint.
    /// 验证 compact 响应保留元数据，并拒绝不含不透明检查点的输出。
    #[test]
    fn codex_compact_response_requires_a_compaction_item() {
        let valid = decode_responses_compact_result(
            &json!({
                "id": "resp-compact",
                "usage": {"input_tokens": 4},
                "output": [{"type": "compaction_summary", "encrypted_content": "opaque"}]
            }),
            "https://example.test/responses/compact",
        )
        .expect("compaction output should decode");
        assert_eq!(valid.response_id.as_deref(), Some("resp-compact"));
        assert_eq!(valid.usage, Some(json!({"input_tokens": 4})));
        assert_eq!(valid.output_items[0]["type"], "compaction");

        let failure = decode_responses_compact_result(
            &json!({"output": [{"type": "message", "content": []}]}),
            "https://example.test/responses/compact",
        )
        .expect_err("message-only output must not become a checkpoint");
        assert!(failure.error.message.contains("replayable output items"));
        assert_eq!(failure.error.dispatch_outcome, DispatchOutcome::Dispatched);

        // empty_checkpoint proves a typed compaction marker still needs a usable opaque payload.
        // empty_checkpoint 证明带类型的压缩标记仍然必须拥有可用的不透明载荷。
        let empty_checkpoint = decode_responses_compact_result(
            &json!({"output": [{"type": "compaction", "encrypted_content": ""}]}),
            "https://example.test/responses/compact",
        )
        .expect_err("empty opaque checkpoints must be rejected before persistence");
        assert!(
            empty_checkpoint
                .error
                .message
                .contains("replayable output items")
        );

        let mixed = decode_responses_compact_result(
            &json!({
                "output": [
                    {"type": "compaction", "encrypted_content": "opaque"},
                    {"type": "message", "content": []}
                ]
            }),
            "https://example.test/responses/compact",
        )
        .expect("ordered replayable output items should be preserved");
        assert_eq!(mixed.output_items.len(), 2);
        let unknown_failure = decode_responses_compact_result(
            &json!({
                "output": [
                    {"type": "compaction", "encrypted_content": "opaque"},
                    {"type": "provider_private_item"}
                ]
            }),
            "https://example.test/responses/compact",
        )
        .expect_err("unknown output items must not become a checkpoint");
        assert!(
            unknown_failure
                .error
                .message
                .contains("replayable output items")
        );
    }

    /// Verifies the protocol encoder can project a cursor proven by the active WebSocket layer.
    /// 验证协议编码器可投影由活动 WebSocket 层证明的游标。
    #[test]
    fn codex_protocol_projection_preserves_socket_proven_previous_response_id() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
        route.protocol_route.protocol_profile_id = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.selected_protocol = String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID);
        route.connection.verified_protocols =
            BTreeSet::from([String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID)]);
        let mut request = minimal_request();
        request.continuation = Some(AiContinuation {
            provider_id: route.provider.id.clone(),
            protocol_profile_id: String::from(OPENAI_CODEX_RESPONSES_PROFILE_ID),
            model_id: request.model_id.clone(),
            provider_message_id: String::from("resp-previous"),
            covered_local_message_id: String::from("local-boundary"),
            covered_message_count: 0,
        });

        let encoded = client
            .encode_request(&route, &request)
            .expect("Responses protocol should retain the socket-proven cursor");

        assert_eq!(encoded.body["previous_response_id"], "resp-previous");
        assert_eq!(
            encoded.body["input"]
                .as_array()
                .expect("Responses input should remain an array")
                .len(),
            1
        );
    }

    /// Verifies a compact body transport failure preserves its proven dispatched boundary.
    /// 验证压缩正文传输失败会保留已经证明的已分派边界。
    #[test]
    fn responses_compact_transport_failure_is_dispatched_and_retryable() {
        let failure = compact_transport_failure("compact body timed out");

        assert_eq!(failure.error.stage, "response_compact");
        assert_eq!(failure.error.dispatch_outcome, DispatchOutcome::Dispatched);
        assert!(failure.error.retryable);
        assert!(!failure.error.semantic_output_observed);
        assert!(!failure.error.side_effect_observed);
    }

    /// Verifies HTTP decoder completion distinguishes transient truncation from malformed payloads.
    /// 验证 HTTP 解码器结束阶段会区分瞬态截断与畸形载荷。
    #[test]
    fn responses_http_completion_failure_preserves_retry_safety() {
        let truncated = stream_completion_failure(
            ProtocolConversionError::truncated_stream("stream ended before terminal"),
            false,
        );
        assert_eq!(
            truncated.error.category,
            AiErrorCategory::ProviderUnavailable
        );
        assert_eq!(
            truncated.error.dispatch_outcome,
            DispatchOutcome::Dispatched
        );
        assert!(truncated.error.retryable);

        let post_semantic_truncation = stream_completion_failure(
            ProtocolConversionError::truncated_stream("stream ended after output"),
            true,
        );
        assert!(!post_semantic_truncation.error.retryable);
        assert!(post_semantic_truncation.error.semantic_output_observed);

        let malformed = stream_completion_failure(
            ProtocolConversionError::new("invalid Responses payload"),
            false,
        );
        assert_eq!(malformed.error.category, AiErrorCategory::InvalidRequest);
        assert_eq!(
            malformed.error.subcategory.as_deref(),
            Some("protocol_conversion")
        );
        assert!(!malformed.error.retryable);
    }

    /// Resolves one pinned OpenRouter route for an exact verified protocol.
    /// 为一个精确的已验证协议解析一条固定二级节点的 OpenRouter 路由。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_id` is one exact protocol profile registered by OpenRouter.
    /// `protocol_id` 是 OpenRouter 注册的一个精确协议 Profile。
    ///
    /// # Returns
    /// 返回值
    /// A frozen route with fallback disabled.
    /// 返回禁用回退的冻结路由。
    fn pinned_openrouter_route(protocol_id: &str) -> ProviderExecutionRoute {
        let mut route = bundled_manifest_route("openrouter", "public-api", protocol_id);
        route.secondary_route = SecondaryRouteSelection::PinnedEndpoint {
            route_id: String::from("anthropic"),
            provider_display_name: String::from("Anthropic"),
            endpoint_display_name: String::from("Anthropic"),
        };
        route
    }

    /// Resolves one MiniMax route for an exact compatible protocol.
    /// 为一个精确兼容协议解析 MiniMax 路由。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_id` selects the credential-bound Anthropic or OpenAI route.
    /// `protocol_id` 选择凭据绑定的 Anthropic 或 OpenAI 路由。
    ///
    /// # Returns
    /// 返回值
    /// A frozen MiniMax international API route.
    /// 返回冻结的 MiniMax 国际 API 路由。
    fn minimax_route(protocol_id: &str) -> ProviderExecutionRoute {
        bundled_manifest_route("minimax", "global", protocol_id)
    }

    /// Resolves one DeepSeek official API route for an exact compatible protocol.
    /// 为一个精确兼容协议解析 DeepSeek 官方 API 路由。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_id` selects the credential-bound Anthropic or OpenAI route.
    /// `protocol_id` 选择凭据绑定的 Anthropic 或 OpenAI 路由。
    ///
    /// # Returns
    /// 返回值
    /// A frozen DeepSeek official API route.
    /// 返回冻结的 DeepSeek 官方 API 路由。
    fn deepseek_route(protocol_id: &str) -> ProviderExecutionRoute {
        bundled_manifest_route("deepseek", "official-api", protocol_id)
    }

    /// Resolves one exact Kimi Coding Plan route.
    /// 解析一条精确 Kimi Coding Plan 路由。
    fn kimi_coding_route() -> ProviderExecutionRoute {
        bundled_manifest_route("kimi", "coding-plan", OPENAI_CHAT_PROFILE_ID)
    }

    /// Resolves one Alibaba Model Studio route with a credential-scoped workspace origin.
    /// 使用凭据作用域工作空间源地址解析一条阿里云百炼路由。
    ///
    /// # Parameters
    /// 参数
    /// `protocol_id` selects the exact OpenAI or Anthropic compatibility path.
    /// `protocol_id` 选择精确 OpenAI 或 Anthropic 兼容路径。
    ///
    /// # Returns
    /// 返回值
    /// A frozen Singapore route whose public origin is replaced by the credential setting.
    /// 返回公共源地址已被凭据设置替换的冻结新加坡路由。
    fn alibaba_workspace_route(protocol_id: &str) -> ProviderExecutionRoute {
        let mut route = bundled_manifest_route("alibaba", "model-studio-singapore", protocol_id);
        route.connection.service_origin_override = Some(String::from(
            "https://ws-example.ap-southeast-1.maas.aliyuncs.com",
        ));
        route
    }

    /// Verifies one workspace origin derives the correct protocol-specific request roots.
    /// 验证同一个工作空间源地址派生正确的协议专属请求根地址。
    #[test]
    fn alibaba_workspace_origin_is_protocol_scoped() {
        let chat = alibaba_workspace_route(OPENAI_CHAT_PROFILE_ID);
        assert_eq!(
            chat.request_url("/chat/completions"),
            "https://ws-example.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1/chat/completions"
        );
        let anthropic = alibaba_workspace_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        assert_eq!(
            anthropic.request_url("/v1/messages"),
            "https://ws-example.ap-southeast-1.maas.aliyuncs.com/apps/anthropic/v1/messages"
        );
    }

    /// Verifies SSE framing survives arbitrary UTF-8 and record chunk boundaries.
    /// 验证 SSE 分帧可以跨越任意 UTF-8 与记录分片边界。
    #[test]
    fn sse_parser_handles_arbitrary_chunks() {
        let bytes = "event: delta\r\ndata: {\"text\":\"你好\"}\r\n\r\n".as_bytes();
        let split = bytes
            .windows(3)
            .position(|window| window == "你".as_bytes())
            .expect("UTF-8 text must exist")
            + 1;
        let mut parser = IncrementalFrameParser::new(ProtocolTransportProfile::bounded_sse());
        assert!(
            parser
                .push(&bytes[..split])
                .expect("first chunk should buffer")
                .is_empty()
        );
        let frames = parser
            .push(&bytes[split..])
            .expect("second chunk should complete");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].event_name.as_deref(), Some("delta"));
        assert!(frames[0].data.contains("你好"));
    }

    /// Verifies SSE framing accepts CR-only and mixed line endings permitted by the event-stream grammar.
    /// 验证 SSE 分帧接受事件流语法允许的单 CR 与混合行结束符。
    #[test]
    fn sse_parser_accepts_cr_only_and_mixed_line_endings() {
        for bytes in [
            b"event: delta\rdata: {\"one\":1}\r\r".as_slice(),
            b"event: delta\r\ndata: {\"two\":2}\r\n\n".as_slice(),
            b"event: delta\ndata: {\"three\":3}\n\r\n".as_slice(),
        ] {
            let mut parser = IncrementalFrameParser::new(ProtocolTransportProfile::bounded_sse());
            let split = bytes.len() - 1;
            let mut frames = parser
                .push(&bytes[..split])
                .expect("first mixed-ending chunk should parse or buffer");
            frames.extend(
                parser
                    .push(&bytes[split..])
                    .expect("second mixed-ending chunk should parse"),
            );
            frames.extend(parser.finish().expect("mixed-ending stream should finish"));
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].event_name.as_deref(), Some("delta"));
            assert!(frames[0].data.starts_with('{'));
        }
    }

    /// Verifies every registered non-SSE framing handles arbitrary chunk boundaries.
    /// 验证每种已注册非 SSE 分帧方式都能处理任意分片边界。
    #[test]
    fn bounded_parser_supports_json_lines_ndjson_and_single_json() {
        for framing in [
            ProtocolStreamFraming::JsonLines,
            ProtocolStreamFraming::Ndjson,
        ] {
            let mut profile = ProtocolTransportProfile::bounded_sse();
            profile.framing = framing;
            let mut parser = IncrementalFrameParser::new(profile);
            let mut frames = parser
                .push(b"{\"one\":1}\n\n{\"two\"")
                .expect("first lines should parse");
            frames.extend(parser.push(b":2}\r\n").expect("split line should complete"));
            frames.extend(parser.finish().expect("line parser should finish"));
            assert_eq!(
                frames
                    .iter()
                    .map(|frame| frame.data.as_str())
                    .collect::<Vec<_>>(),
                vec![r#"{"one":1}"#, r#"{"two":2}"#]
            );
        }

        let mut profile = ProtocolTransportProfile::bounded_sse();
        profile.framing = ProtocolStreamFraming::SingleJson;
        let mut parser = IncrementalFrameParser::new(profile);
        assert!(
            parser
                .push(b"{\"complete\"")
                .expect("single JSON should buffer")
                .is_empty()
        );
        assert!(
            parser
                .push(b":true}")
                .expect("single JSON should remain buffered")
                .is_empty()
        );
        assert_eq!(
            parser.finish().expect("single JSON should finish")[0].data,
            r#"{"complete":true}"#
        );
    }

    /// Verifies frame and pending-buffer limits fail deterministically.
    /// 验证帧与待处理缓冲区上限会确定性失败。
    #[test]
    fn bounded_parser_rejects_oversized_frames_and_buffers() {
        let mut frame_profile = ProtocolTransportProfile::bounded_sse();
        frame_profile.max_frame_bytes = 4;
        frame_profile.max_buffer_bytes = 32;
        let mut frame_parser = IncrementalFrameParser::new(frame_profile);
        let frame_error = frame_parser
            .push(b"data: {}\n\n")
            .expect_err("oversized frame should fail");
        assert!(frame_error.message.contains("4 byte limit"));

        let mut buffer_profile = ProtocolTransportProfile::bounded_sse();
        buffer_profile.max_frame_bytes = 32;
        buffer_profile.max_buffer_bytes = 8;
        let mut buffer_parser = IncrementalFrameParser::new(buffer_profile);
        let buffer_error = buffer_parser
            .push(b"123456789")
            .expect_err("unterminated buffer should fail");
        assert!(buffer_error.message.contains("8 byte limit"));
    }

    /// Verifies successful responses require one exact registered media type.
    /// 验证成功响应必须使用一个精确注册的媒体类型。
    #[test]
    fn response_content_type_is_strict_and_parameter_tolerant() {
        let profile = ProtocolTransportProfile::bounded_sse();
        let mut valid = HeaderMap::new();
        valid.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        validate_response_content_type(&valid, &profile, "https://example.test")
            .expect("registered media type should pass");

        let mut invalid = HeaderMap::new();
        invalid.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let failure = validate_response_content_type(&invalid, &profile, "https://example.test")
            .expect_err("JSON must not enter the SSE decoder");
        assert_eq!(
            failure.error.subcategory.as_deref(),
            Some("response_content_type")
        );
        assert_eq!(failure.error.dispatch_outcome, DispatchOutcome::Dispatched);
    }

    /// Verifies numeric Retry-After values retain fractional-second precision.
    /// 验证数值型 Retry-After 保留小数秒精度。
    #[test]
    fn retry_after_numeric_values_preserve_milliseconds() {
        assert_eq!(parse_decimal_seconds_milliseconds("5"), Some(5_000));
        assert_eq!(parse_decimal_seconds_milliseconds("2.5"), Some(2_500));
        assert_eq!(parse_decimal_seconds_milliseconds("0.0001"), Some(1));
        assert_eq!(parse_decimal_seconds_milliseconds("1.0019"), Some(1_002));
        assert_eq!(parse_decimal_seconds_milliseconds("-1"), None);
        assert_eq!(parse_decimal_seconds_milliseconds("1.2.3"), None);
    }

    /// Verifies Retry-After HTTP dates preserve their source and exact relative delay.
    /// 验证 Retry-After HTTP 日期保留来源与精确相对延迟。
    #[test]
    fn retry_after_http_date_preserves_source() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let retry_at = now + Duration::from_secs(17);
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            HeaderValue::from_str(httpdate::fmt_http_date(retry_at).as_str())
                .expect("valid HTTP date"),
        );

        let hint = parse_retry_after_headers(&headers, now).expect("valid retry hint");
        assert_eq!(hint.delay_milliseconds, 17_000);
        assert_eq!(hint.source, RetryAfterSource::HeaderHttpDate);
    }

    /// Verifies malformed Retry-After values are rejected instead of guessed.
    /// 验证畸形 Retry-After 值会被拒绝而不是被猜测。
    #[test]
    fn retry_after_malformed_value_is_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            HeaderValue::from_static("later"),
        );
        assert!(parse_retry_after_headers(&headers, SystemTime::now()).is_none());
    }

    /// Verifies capture encoding includes provider-specific secondary routing.
    /// 验证捕获编码包含供应商特有的二级路由字段。
    #[test]
    fn encode_request_projects_pinned_openrouter_route_for_every_protocol() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        for (protocol_id, expected_path) in [
            (OPENAI_CHAT_PROFILE_ID, "/chat/completions"),
            (OPENAI_RESPONSES_PROFILE_ID, "/responses"),
            (ANTHROPIC_MESSAGES_PROFILE_ID, "/messages"),
        ] {
            let encoded = client
                .encode_request(&pinned_openrouter_route(protocol_id), &minimal_request())
                .expect("request should encode");

            assert_eq!(encoded.path, expected_path);
            assert_eq!(encoded.body["model"], "openai/gpt-4.1");
            assert_eq!(encoded.body["provider"]["order"], json!(["anthropic"]));
            assert_eq!(encoded.body["provider"]["allow_fallbacks"], false);
        }
    }

    /// Verifies Chat, Responses, and Anthropic routes retain one exact structured-output schema.
    /// 验证 Chat、Responses 与 Anthropic 路由都会保留同一份精确结构化输出 Schema。
    #[test]
    fn encode_request_preserves_structured_output_schema_for_supported_protocols() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let schema = json!({
            "type": "object",
            "properties": { "accepted": { "type": "boolean" } },
            "required": ["accepted"],
            "additionalProperties": false,
        });

        for protocol_id in [
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
        ] {
            let mut request = minimal_request();
            request.structured_output = StructuredOutputPolicy::JsonSchema {
                name: String::from("result"),
                description: Some(String::from("Structured result")),
                schema: schema.clone(),
                strict: true,
            };
            let encoded = client
                .encode_request(&pinned_openrouter_route(protocol_id), &request)
                .expect("supported protocol should preserve structured output schema");
            let projected_schema = match protocol_id {
                OPENAI_CHAT_PROFILE_ID => {
                    encoded.body.pointer("/response_format/json_schema/schema")
                }
                OPENAI_RESPONSES_PROFILE_ID => encoded.body.pointer("/text/format/schema"),
                ANTHROPIC_MESSAGES_PROFILE_ID => {
                    encoded.body.pointer("/output_config/format/schema")
                }
                _ => unreachable!("the loop only contains supported protocol profiles"),
            };
            assert_eq!(projected_schema, Some(&schema));
        }
    }

    /// Verifies a custom projector cannot silently remove the required Anthropic schema.
    /// 验证自定义投影器不能静默删除 Anthropic 所需的 Schema。
    #[test]
    fn encode_request_rejects_structured_schema_loss_after_custom_projection() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        route.custom_model_projection = Some(CustomModelRequestProjection {
            delete: vec![String::from("output_config")],
            ..Default::default()
        });
        let mut request = minimal_request();
        request.structured_output = StructuredOutputPolicy::JsonSchema {
            name: String::from("result"),
            description: None,
            schema: json!({
                "type": "object",
                "properties": { "accepted": { "type": "boolean" } },
                "required": ["accepted"],
                "additionalProperties": false,
            }),
            strict: true,
        };

        let error = client
            .encode_request(&route, &request)
            .expect_err("schema loss must fail before dispatch");
        assert!(error.error.message.contains("dropped or changed"));
    }

    /// Verifies Kimi Coding projection uses thinking.type and the OAuth-bound device identity.
    /// 验证 Kimi Coding 投影使用 thinking.type 与 OAuth 绑定设备身份。
    #[test]
    fn kimi_coding_projection_uses_thinking_and_device_identity() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("kimi-for-coding");
        request.reasoning_effort = Some(ReasoningEffort::High);
        let route = kimi_coding_route();
        let encoded = client
            .encode_request(&route, &request)
            .expect("Kimi request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "enabled");
        assert!(encoded.body.get("reasoning_effort").is_none());
        let secret = crate::CredentialSecretEnvelope::new(
            crate::CredentialSecretMaterial::KimiCodingPlanOAuth {
                tokens: crate::OAuthTokenSet {
                    access_token: String::from("access"),
                    refresh_token: String::from("refresh"),
                    issued_at_unix_ms: 1,
                    expires_at_unix_ms: 2,
                    scopes: BTreeSet::new(),
                    account_subject: None,
                    email: None,
                },
                device_id: String::from("123e4567-e89b-42d3-a456-426614174000"),
            },
        )
        .expect("Kimi material")
        .execution_secret()
        .expect("Kimi execution secret");
        let headers = execution_headers(&route, &encoded.headers, &secret).expect("Kimi headers");
        assert_eq!(
            headers
                .get("x-msh-device-id")
                .and_then(|value| value.to_str().ok()),
            Some("123e4567-e89b-42d3-a456-426614174000")
        );
        assert_eq!(
            headers
                .get("x-msh-platform")
                .and_then(|value| value.to_str().ok()),
            Some("CLIProxyAPI")
        );
    }

    /// Verifies MiniMax Chat projects adaptive thinking separately from reasoning output splitting.
    /// 验证 MiniMax Chat 将自适应思考与思考输出拆分分别投影。
    #[test]
    fn minimax_openai_chat_projects_adaptive_thinking() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3");
        request.reasoning_effort = Some(ReasoningEffort::Adaptive);
        request.model_parameters.insert(
            MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
            ModelParameterValue::Boolean(true),
        );

        let encoded = client
            .encode_request(&minimax_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect("MiniMax Chat request should encode");

        assert_eq!(encoded.body["reasoning_split"], true);
        assert_eq!(encoded.body["stream_options"]["include_usage"], true);
        assert_eq!(encoded.body["thinking"]["type"], "adaptive");
        assert!(encoded.body.get("reasoning_effort").is_none());
    }

    /// Verifies MiniMax Anthropic projects the documented disabled thinking shape.
    /// 验证 MiniMax Anthropic 投影文档化的关闭思考结构。
    #[test]
    fn minimax_anthropic_projects_disabled_thinking() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3");
        request.reasoning_effort = Some(ReasoningEffort::None);

        let encoded = client
            .encode_request(&minimax_route(ANTHROPIC_MESSAGES_PROFILE_ID), &request)
            .expect("MiniMax Anthropic request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "disabled");
        assert!(encoded.body.get("output_config").is_none());
    }

    /// Verifies MiniMax thinking projection preserves Anthropic native JSON Schema output format.
    /// 验证 MiniMax 思考参数投影会保留 Anthropic 原生 JSON Schema 输出格式。
    #[test]
    fn minimax_anthropic_preserves_structured_output_format() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3");
        request.reasoning_effort = Some(ReasoningEffort::None);
        request.structured_output = StructuredOutputPolicy::JsonSchema {
            name: String::from("vmm_turn_analysis"),
            description: Some(String::from("VMM turn analysis")),
            schema: json!({
                "type": "object",
                "properties": { "turn_id": { "type": "integer" } },
                "required": ["turn_id"],
                "additionalProperties": false,
            }),
            strict: true,
        };

        let encoded = client
            .encode_request(&minimax_route(ANTHROPIC_MESSAGES_PROFILE_ID), &request)
            .expect("MiniMax Anthropic structured request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "disabled");
        assert_eq!(
            encoded.body["output_config"]["format"]["type"],
            "json_schema"
        );
        assert_eq!(
            encoded.body["output_config"]["format"]["schema"]["required"][0],
            "turn_id"
        );
        assert!(encoded.body["output_config"].get("effort").is_none());
    }

    /// Verifies MiniMax rejects undocumented strength controls before dispatch.
    /// 验证 MiniMax 在分派前拒绝未文档化的强度控制。
    #[test]
    fn minimax_rejects_undocumented_reasoning_effort() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3");
        request.reasoning_effort = Some(ReasoningEffort::High);

        let failure = client
            .encode_request(&minimax_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect_err("MiniMax high effort is not documented");
        assert!(
            failure
                .error
                .message
                .contains("not in the declared mapping table")
        );
    }

    /// Verifies DeepSeek Chat projects the documented disabled thinking shape.
    /// 验证 DeepSeek Chat 投影文档化的关闭思考结构。
    #[test]
    fn deepseek_openai_chat_projects_disabled_thinking() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-v4-pro");
        request.reasoning_effort = Some(ReasoningEffort::None);
        request.model_parameters.insert(
            String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
            ModelParameterValue::Decimal(String::from("0.2")),
        );

        let encoded = client
            .encode_request(&deepseek_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect("DeepSeek Chat request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "disabled");
        assert_eq!(encoded.body["temperature"], 0.2);
        assert!(encoded.body.get("reasoning_effort").is_none());
    }

    /// Verifies DeepSeek thinking requests omit temperature because the provider ignores it.
    /// 验证 DeepSeek 思考请求省略供应商不会生效的温度参数。
    #[test]
    fn deepseek_thinking_omits_ineffective_temperature() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-v4-flash");
        request.reasoning_effort = Some(ReasoningEffort::High);
        request.model_parameters.insert(
            String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
            ModelParameterValue::Decimal(String::from("0.2")),
        );

        let encoded = client
            .encode_request(&deepseek_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect("DeepSeek thinking request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "enabled");
        assert!(encoded.body.get("temperature").is_none());
    }

    /// Verifies DeepSeek Anthropic projects both disabled and documented effort controls.
    /// 验证 DeepSeek Anthropic 同时投影关闭思考与文档化档位控制。
    #[test]
    fn deepseek_anthropic_projects_disabled_and_enabled_thinking() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let route = deepseek_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        let mut disabled = minimal_request();
        disabled.model_id = String::from("deepseek-v4-pro");
        disabled.reasoning_effort = Some(ReasoningEffort::None);
        disabled.model_parameters.insert(
            String::from(SAMPLING_TEMPERATURE_PARAMETER_ID),
            ModelParameterValue::Decimal(String::from("1.3")),
        );
        let disabled = client
            .encode_request(&route, &disabled)
            .expect("DeepSeek Anthropic disabled request should encode");
        assert_eq!(disabled.body["thinking"]["type"], "disabled");
        assert_eq!(disabled.body["temperature"], 1.3);
        assert!(disabled.body.get("output_config").is_none());

        let mut enabled = minimal_request();
        enabled.model_id = String::from("deepseek-v4-pro");
        enabled.reasoning_effort = Some(ReasoningEffort::Max);
        let enabled = client
            .encode_request(&route, &enabled)
            .expect("DeepSeek Anthropic enabled request should encode");
        assert_eq!(enabled.body["thinking"]["type"], "enabled");
        assert_eq!(enabled.body["output_config"]["effort"], "max");
    }

    /// Verifies DeepSeek Anthropic conversion keeps native web search instead of rejecting it.
    /// 验证 DeepSeek Anthropic 转换保留原生 web search，而不是拒绝该工具。
    #[test]
    fn deepseek_anthropic_preserves_native_web_search() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-v4-flash");
        request.reasoning_effort = Some(ReasoningEffort::Max);
        request.native_tools = vec![vulcan_ai_contract::NativeToolKind::WebSearch];
        let encoded = client
            .encode_request(&deepseek_route(ANTHROPIC_MESSAGES_PROFILE_ID), &request)
            .expect("DeepSeek Anthropic request should encode");
        assert_eq!(encoded.body["tools"][0]["type"], "web_search_20250305");
        assert_eq!(encoded.body["tools"][0]["name"], "web_search");
        assert_eq!(encoded.body["thinking"]["type"], "enabled");
    }

    /// Verifies DeepSeek Responses projects native web search and removes unsupported fields.
    /// 验证 DeepSeek Responses 投影原生网页搜索并移除不支持字段。
    #[test]
    fn deepseek_responses_projects_native_web_search() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-v4-flash");
        request.reasoning_effort = Some(ReasoningEffort::Max);
        request.native_tools = vec![vulcan_ai_contract::NativeToolKind::WebSearch];
        let encoded = client
            .encode_request(&deepseek_route(OPENAI_RESPONSES_PROFILE_ID), &request)
            .expect("DeepSeek Responses request should encode");
        assert_eq!(encoded.path, "/responses");
        assert_eq!(encoded.body["tools"][0]["type"], "web_search");
        assert!(encoded.body.get("store").is_none());
        assert!(encoded.body.get("include").is_none());
        assert_eq!(encoded.body["reasoning"]["effort"], "max");
    }

    /// Verifies every OpenRouter protocol route converges on the unified reasoning object.
    /// 验证每条 OpenRouter 协议路由都会收敛到统一 reasoning 对象。
    #[test]
    fn encode_request_normalizes_openrouter_reasoning_for_every_protocol() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        for protocol_id in [
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
        ] {
            let mut request = minimal_request();
            request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::Max);
            let encoded = client
                .encode_request(&pinned_openrouter_route(protocol_id), &request)
                .expect("reasoning request should encode");
            assert_eq!(encoded.body["reasoning"]["effort"], "max");
            assert!(encoded.body.get("reasoning_effort").is_none());
            if protocol_id == ANTHROPIC_MESSAGES_PROFILE_ID {
                assert_eq!(encoded.body["output_config"]["effort"], "max");
            } else {
                assert!(encoded.body.get("output_config").is_none());
            }
        }
    }

    /// Verifies OpenRouter provider-adaptive reasoning uses its unified enable switch.
    /// 验证 OpenRouter 供应商自适应推理使用其统一启用开关。
    #[test]
    fn encode_request_projects_openrouter_adaptive_reasoning_as_enabled() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        for protocol_id in [
            OPENAI_CHAT_PROFILE_ID,
            OPENAI_RESPONSES_PROFILE_ID,
            ANTHROPIC_MESSAGES_PROFILE_ID,
        ] {
            let mut request = minimal_request();
            request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::Adaptive);

            let encoded = client
                .encode_request(&pinned_openrouter_route(protocol_id), &request)
                .expect("adaptive reasoning request should encode");
            assert_eq!(encoded.body["reasoning"]["enabled"], true);
            assert!(encoded.body["reasoning"].get("effort").is_none());
        }
    }

    /// Verifies OpenRouter rejects simultaneous effort and token-budget reasoning controls.
    /// 验证 OpenRouter 会拒绝同时设置推理档位和 Token 预算。
    #[test]
    fn encode_request_rejects_conflicting_openrouter_reasoning_controls() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        assert_eq!(
            route.model_parameters_conflicting_with_request_field("reasoning_effort"),
            BTreeSet::from([String::from("reasoning.max_tokens")])
        );
        let mut request = minimal_request();
        request.reasoning_effort = Some(vulcan_ai_contract::ReasoningEffort::High);
        request.model_parameters.insert(
            String::from("reasoning.max_tokens"),
            ModelParameterValue::Integer(2048),
        );
        let error = client
            .encode_request(&route, &request)
            .expect_err("conflicting reasoning controls should fail");
        assert!(error.error.message.contains("mutually exclusive"));
    }

    /// Verifies an endpoint-specific prompt-cache restriction is enforced by the frozen manifest.
    /// 验证端点特定的提示词缓存限制由冻结 Manifest 执行。
    #[test]
    fn encode_request_rejects_openrouter_explicit_prompt_cache_from_manifest() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut request = minimal_request();
        request.prompt_cache = PromptCachePolicy::ManagedExplicit {
            ttl: vulcan_ai_contract::PromptCacheTtlSelection::ProfileDefault,
        };

        let error = client
            .encode_request(&pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID), &request)
            .expect_err("manifest-declared prompt-cache restriction should fail");

        assert!(
            error
                .error
                .message
                .contains("verified model-endpoint capability")
        );
    }

    /// Verifies protocol-required headers are merged while credential headers remain adapter-owned.
    /// 验证协议必需请求头会被合并，同时凭据请求头仍由适配器独占管理。
    #[test]
    fn execution_headers_merge_protocol_headers_and_reject_credential_overrides() {
        let route = pinned_openrouter_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let encoded = client
            .encode_request(&route, &minimal_request())
            .expect("Anthropic request should encode");
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
            .expect("test key should validate")
            .execution_secret()
            .expect("test key should resolve");
        let headers = execution_headers(&route, &encoded.headers, &secret)
            .expect("non-secret protocol headers should merge");
        assert_eq!(
            headers
                .get("anthropic-version")
                .and_then(|value| value.to_str().ok()),
            Some("2023-06-01")
        );
        assert_eq!(
            headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer secret")
        );
        assert_eq!(
            headers
                .get(reqwest::header::ACCEPT_ENCODING)
                .and_then(|value| value.to_str().ok()),
            Some("identity")
        );

        let forbidden =
            BTreeMap::from([(String::from("authorization"), String::from("Bearer other"))]);
        assert!(execution_headers(&route, &forbidden, &secret).is_err());
    }

    /// Verifies one MiniMax credential selects protocol-specific paths and authentication.
    /// 验证同一个 MiniMax 凭据会选择协议专属路径与鉴权方式。
    #[test]
    fn minimax_protocol_selection_controls_wire_route_and_authentication() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
            .expect("test key should validate")
            .execution_secret()
            .expect("test key should resolve");

        let anthropic_route = minimax_route(ANTHROPIC_MESSAGES_PROFILE_ID);
        let anthropic_request = minimal_request();
        let anthropic = client
            .encode_request(&anthropic_route, &anthropic_request)
            .expect("Anthropic request should encode");
        let anthropic_headers = execution_headers(&anthropic_route, &anthropic.headers, &secret)
            .expect("Anthropic headers should project");
        assert_eq!(anthropic.path, "/anthropic/v1/messages");
        assert_eq!(
            anthropic_headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("secret")
        );
        assert!(!anthropic_headers.contains_key(reqwest::header::AUTHORIZATION));

        let openai_route = minimax_route(OPENAI_CHAT_PROFILE_ID);
        let mut openai_request = minimal_request();
        openai_request.model_parameters.insert(
            MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
            ModelParameterValue::Boolean(true),
        );
        let openai = client
            .encode_request(&openai_route, &openai_request)
            .expect("OpenAI request should encode");
        let openai_headers = execution_headers(&openai_route, &openai.headers, &secret)
            .expect("OpenAI headers should project");
        assert_eq!(openai.path, "/v1/chat/completions");
        assert_eq!(openai.body["reasoning_split"], true);
        assert_eq!(
            openai_headers
                .get(reqwest::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer secret")
        );
        assert!(!openai_headers.contains_key("x-api-key"));
    }

    /// Verifies custom model projections apply typed parameters, reasoning, deletion, and overrides deterministically.
    /// 验证自定义模型投影会确定性地应用强类型参数、推理、删除与强制覆盖。
    #[test]
    fn custom_model_projection_applies_registered_request_mutations() {
        let mut request = minimal_request();
        request.reasoning_effort = Some(ReasoningEffort::High);
        request.model_parameters.insert(
            String::from("vendor.temperature"),
            ModelParameterValue::Decimal(String::from("0.3")),
        );
        let projection = CustomModelRequestProjection {
            reasoning: BTreeMap::from([(
                ReasoningEffort::High,
                crate::CustomRequestMutation {
                    set: BTreeMap::from([(
                        String::from("thinking.mode"),
                        Value::String(String::from("enabled")),
                    )]),
                    delete: Vec::new(),
                },
            )]),
            model_parameter_paths: BTreeMap::from([(
                String::from("vendor.temperature"),
                String::from("sampling.temperature"),
            )]),
            defaults: BTreeMap::from([(String::from("vendor.keep"), Value::Bool(true))]),
            overrides: BTreeMap::from([(
                String::from("vendor.forced"),
                Value::String(String::from("yes")),
            )]),
            delete: vec![String::from("remove_me")],
        };
        let mut encoded = EncodedWireRequest {
            path: String::from("/v1/chat/completions"),
            headers: BTreeMap::new(),
            body: json!({"model": "custom-model", "messages": [], "remove_me": true}),
        };

        project_custom_model_request(&projection, &request, &mut encoded)
            .expect("custom projection should succeed");

        assert_eq!(encoded.body["thinking"]["mode"], "enabled");
        assert_eq!(encoded.body["sampling"]["temperature"], json!(0.3));
        assert_eq!(encoded.body["vendor"]["keep"], true);
        assert_eq!(encoded.body["vendor"]["forced"], "yes");
        assert!(encoded.body.get("remove_me").is_none());
    }

    /// Verifies a configured reasoning projection reaches the final encoded request body.
    /// 验证已配置的思考投影会抵达最终编码请求体，而不是只停留在编辑器状态。
    #[test]
    fn encode_request_applies_custom_reasoning_projection_to_final_wire_body() {
        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_CHAT_PROFILE_ID);
        route.custom_model_projection = Some(CustomModelRequestProjection {
            reasoning: BTreeMap::from([(
                ReasoningEffort::High,
                crate::CustomRequestMutation {
                    set: BTreeMap::from([(
                        String::from("thinking.type"),
                        Value::String(String::from("enabled")),
                    )]),
                    delete: vec![String::from("reasoning_effort")],
                },
            )]),
            ..Default::default()
        });
        let mut request = minimal_request();
        request.reasoning_effort = Some(ReasoningEffort::High);

        let encoded = client
            .encode_request(&route, &request)
            .expect("custom reasoning projection should encode");

        assert_eq!(encoded.body["thinking"]["type"], "enabled");
        assert!(encoded.body.get("reasoning_effort").is_none());
    }

    /// Verifies a terminal Responses failure never touches the Chat or Anthropic endpoints:
    /// every physical request stays on the bound protocol (plan §13.5.5).
    /// 验证 Responses 终态失败绝不触达 Chat 或 Anthropic 端点：每个真实请求都停留在绑定协议上（方案 §13.5.5）。
    #[tokio::test]
    async fn responses_failure_never_touches_other_protocol_endpoints() {
        use std::sync::Mutex as StdMutex;
        use std::time::Duration;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("address should resolve");
        let observed_paths = std::sync::Arc::new(StdMutex::new(Vec::<String>::new()));
        let server_paths = std::sync::Arc::clone(&observed_paths);
        let server = tokio::spawn(async move {
            // Accept up to four requests to detect any cross-protocol attempt; each returns a
            // terminal 400 so the client must never follow up on another protocol endpoint.
            // 最多接受四个请求以探测任何跨协议尝试；每个都返回终态 400，
            // 客户端绝不能在另一协议端点上继续跟进。
            for _ in 0..4 {
                let accepted =
                    tokio::time::timeout(Duration::from_secs(5), listener.accept()).await;
                let Ok(accept_result) = accepted else {
                    break;
                };
                let Ok((mut stream, _)) = accept_result else {
                    break;
                };
                let request = read_test_http_request(&mut stream).await;
                let path = request
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                server_paths.lock().expect("path lock").push(path);
                let response = "HTTP/1.1 400 Bad Request
content-type: application/json
content-length: 2
connection: close

{}";
                stream
                    .write_all(response.as_bytes())
                    .await
                    .expect("400 response should send");
            }
        });

        let client = ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        );
        let mut route = pinned_openrouter_route(OPENAI_RESPONSES_PROFILE_ID);
        route.protocol_route.base_url_override = Some(format!("http://{address}"));
        let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("secret"))
            .expect("test key should validate")
            .execution_secret()
            .expect("test key should resolve");

        let mut stream = client.execute(
            route,
            minimal_request(),
            secret,
            String::from("execution-responses-400"),
        );
        let mut terminal_seen = false;
        while let Some(event) = stream.next().await {
            if event.is_err() {
                terminal_seen = true;
                break;
            }
        }
        drop(stream);
        server.await.expect("mock server should finish");

        assert!(
            terminal_seen,
            "the 400 must terminate the execution attempt"
        );
        let paths = observed_paths.lock().expect("path lock").clone();
        assert!(
            !paths.is_empty(),
            "the bound protocol endpoint must have been hit"
        );
        assert!(
            paths.iter().all(|path| path.starts_with("/responses")),
            "no request may leave the bound responses protocol: {paths:?}"
        );
    }

    /// Verifies bundled manifest usage profiles equal the legacy provider functions (plan §15.4).
    /// 验证内置清单 usage Profile 与旧版供应商函数等价（方案 §15.4）。
    #[test]
    fn bundled_usage_profiles_match_legacy_provider_functions() {
        for (provider_id, expected) in [
            (
                "alibaba",
                vulcan_llm_core::alibaba_responses_usage_profiles(),
            ),
            ("deepseek", vulcan_llm_core::deepseek_chat_usage_profiles()),
            (
                "openrouter",
                vulcan_llm_core::openrouter_chat_usage_profiles(),
            ),
        ] {
            let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
            let text = std::fs::read_to_string(directory.join(format!("{provider_id}.json")))
                .unwrap_or_else(|_| panic!("missing bundled manifest `{provider_id}`"));
            let manifest = crate::provider_manifest::compile_manifest_text(
                &text,
                crate::provider_manifest::ManifestOwnership::System,
            )
            .expect("bundled manifest must compile");
            // manifest_profiles keeps the exact declared order across every route, deduplicated
            // by profile id because multiple routes may reference the same declaration.
            // manifest_profiles 跨全部路由保持精确声明顺序，并按 Profile 标识去重
            // （多条路由可能引用同一声明）。
            let mut seen = BTreeSet::new();
            let manifest_profiles = manifest
                .provider()
                .service_entries
                .iter()
                .flat_map(|entry| entry.protocol_routes.iter())
                .flat_map(|route| route.usage_extraction_profiles.iter())
                .filter(|profile| profile.profile_id.starts_with(provider_id))
                .filter(|profile| seen.insert(profile.profile_id.clone()))
                .collect::<Vec<_>>();
            assert!(
                !manifest_profiles.is_empty(),
                "{provider_id} manifest must declare its usage profiles"
            );
            assert_eq!(
                manifest_profiles.len(),
                expected.len(),
                "{provider_id} profile count must match the legacy function"
            );
            for (actual, legacy) in manifest_profiles.iter().zip(expected.iter()) {
                assert_eq!(
                    **actual, *legacy,
                    "{provider_id} profile `{}` must equal the legacy declaration",
                    actual.profile_id
                );
            }
        }
    }

    /// Builds one execution client used by the projection equivalence tests.
    /// 构建投影等价测试使用的执行客户端。
    fn projection_test_client() -> ProviderExecutionClient {
        ProviderExecutionClient::new(
            reqwest::Client::new(),
            Arc::new(ProtocolRegistry::first_batch().expect("profiles should register")),
        )
    }

    /// Verifies the bundled DeepSeek request profiles produce the legacy-equivalent wire.
    /// 验证内置 DeepSeek 请求 Profile 产生与旧版等价的 Wire 请求。
    #[test]
    fn deepseek_manifest_profiles_produce_legacy_equivalent_wire() {
        let client = projection_test_client();
        let chat_route = bundled_manifest_route("deepseek", "official-api", OPENAI_CHAT_PROFILE_ID);
        let anthropic_route =
            bundled_manifest_route("deepseek", "official-api", ANTHROPIC_MESSAGES_PROFILE_ID);
        let responses_route =
            bundled_manifest_route("deepseek", "official-api", OPENAI_RESPONSES_PROFILE_ID);
        assert!(
            !chat_route.request_projection_profiles.is_empty()
                && !anthropic_route.request_projection_profiles.is_empty()
                && !responses_route.request_projection_profiles.is_empty(),
            "bundled DeepSeek routes must install manifest request profiles"
        );

        // Chat: none -> thinking disabled, effort removed.
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-chat");
        request.reasoning_effort = Some(ReasoningEffort::None);
        let encoded = client
            .encode_request(&chat_route, &request)
            .expect("DeepSeek Chat request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "disabled");
        assert!(encoded.body.get("reasoning_effort").is_none());

        // Chat: high -> thinking enabled.
        request.reasoning_effort = Some(ReasoningEffort::High);
        let encoded = client
            .encode_request(&chat_route, &request)
            .expect("DeepSeek Chat high request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "enabled");

        // Anthropic: high -> thinking enabled + output_config.effort.
        let encoded = client
            .encode_request(&anthropic_route, &request)
            .expect("DeepSeek Anthropic request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "enabled");
        assert_eq!(encoded.body["output_config"]["effort"], "high");

        // Anthropic: none -> thinking disabled, no output_config.
        request.reasoning_effort = Some(ReasoningEffort::None);
        let encoded = client
            .encode_request(&anthropic_route, &request)
            .expect("DeepSeek Anthropic none request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "disabled");
        assert!(encoded.body.get("output_config").is_none());

        // Responses: documented-unsupported fields removed.
        request.reasoning_effort = None;
        let mut request = minimal_request();
        request.model_id = String::from("deepseek-chat");
        let encoded = client
            .encode_request(&responses_route, &request)
            .expect("DeepSeek Responses request should encode");
        assert!(encoded.body.get("store").is_none());
        assert!(encoded.body.get("include").is_none());
    }

    /// Verifies the bundled Alibaba request profiles produce the legacy-equivalent wire.
    /// 验证内置 Alibaba 请求 Profile 产生与旧版等价的 Wire 请求。
    #[test]
    fn alibaba_manifest_profiles_produce_legacy_equivalent_wire() {
        let client = projection_test_client();
        let chat_route =
            bundled_manifest_route("alibaba", "model-studio-cn", OPENAI_CHAT_PROFILE_ID);
        let anthropic_route =
            bundled_manifest_route("alibaba", "model-studio-cn", ANTHROPIC_MESSAGES_PROFILE_ID);
        let mut request = minimal_request();
        request.model_id = String::from("qwen3.7-max");

        request.reasoning_effort = Some(ReasoningEffort::Adaptive);
        let encoded = client
            .encode_request(&chat_route, &request)
            .expect("Alibaba Chat request should encode");
        assert_eq!(encoded.body["enable_thinking"], true);
        assert!(encoded.body.get("reasoning_effort").is_none());

        request.reasoning_effort = Some(ReasoningEffort::None);
        let encoded = client
            .encode_request(&chat_route, &request)
            .expect("Alibaba Chat none request should encode");
        assert_eq!(encoded.body["enable_thinking"], false);

        request.reasoning_effort = Some(ReasoningEffort::None);
        let encoded = client
            .encode_request(&anthropic_route, &request)
            .expect("Alibaba Anthropic request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "disabled");
        assert!(encoded.body.get("output_config").is_none());

        request.reasoning_effort = Some(ReasoningEffort::Adaptive);
        let encoded = client
            .encode_request(&anthropic_route, &request)
            .expect("Alibaba Anthropic adaptive request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "enabled");
    }

    /// Verifies the bundled OpenRouter request profiles produce the legacy-equivalent wire.
    /// 验证内置 OpenRouter 请求 Profile 产生与旧版等价的 Wire 请求。
    #[test]
    fn openrouter_manifest_profiles_produce_legacy_equivalent_wire() {
        let client = projection_test_client();
        let chat_route = bundled_manifest_route("openrouter", "public-api", OPENAI_CHAT_PROFILE_ID);
        let anthropic_route =
            bundled_manifest_route("openrouter", "public-api", ANTHROPIC_MESSAGES_PROFILE_ID);
        let responses_route =
            bundled_manifest_route("openrouter", "public-api", OPENAI_RESPONSES_PROFILE_ID);
        let mut request = minimal_request();
        request.model_id = String::from("openai/gpt-5.6-sol");

        // Adaptive Chat: reasoning.enabled, no effort, no reasoning_effort.
        request.reasoning_effort = Some(ReasoningEffort::Adaptive);
        let encoded = client
            .encode_request(&chat_route, &request)
            .expect("OpenRouter Chat adaptive request should encode");
        assert_eq!(encoded.body["reasoning"]["enabled"], true);
        assert!(encoded.body["reasoning"].get("effort").is_none());
        assert!(encoded.body.get("reasoning_effort").is_none());

        // Non-adaptive Anthropic: output_config.effort + reasoning.effort.
        request.reasoning_effort = Some(ReasoningEffort::Low);
        let encoded = client
            .encode_request(&anthropic_route, &request)
            .expect("OpenRouter Anthropic request should encode");
        assert_eq!(encoded.body["output_config"]["effort"], "low");
        assert_eq!(encoded.body["reasoning"]["effort"], "low");
        assert!(encoded.body["reasoning"].get("enabled").is_none());

        // Responses max effort: reasoning.effort without the protocol effort field removal.
        request.reasoning_effort = Some(ReasoningEffort::Max);
        let encoded = client
            .encode_request(&responses_route, &request)
            .expect("OpenRouter Responses request should encode");
        assert_eq!(encoded.body["reasoning"]["effort"], "max");
    }

    /// Verifies the OpenRouter reasoning token budget parameter projects via the manifest profile.
    /// 验证 OpenRouter 推理 Token 预算参数经 Manifest Profile 投影。
    #[test]
    fn openrouter_reasoning_max_tokens_projects_via_profile() {
        let client = projection_test_client();
        let chat_route = bundled_manifest_route("openrouter", "public-api", OPENAI_CHAT_PROFILE_ID);
        let mut request = minimal_request();
        request.model_id = String::from("openai/gpt-5.6-sol");
        request.reasoning_effort = None;
        request.model_parameters.insert(
            String::from("reasoning.max_tokens"),
            ModelParameterValue::Integer(16384),
        );
        let encoded = client
            .encode_request(&chat_route, &request)
            .expect("OpenRouter max tokens request should encode");
        assert_eq!(encoded.body["reasoning"]["max_tokens"], 16384);
    }

    /// Verifies the bundled Kimi default profile matches the non-coding-model legacy behavior.
    /// 验证内置 Kimi 默认 Profile 与非编码模型旧版行为一致。
    #[test]
    fn kimi_manifest_default_profile_matches_legacy_wire() {
        let client = projection_test_client();
        let chat_route = bundled_manifest_route("kimi", "platform-global", OPENAI_CHAT_PROFILE_ID);
        assert!(
            !chat_route.request_projection_profiles.is_empty(),
            "bundled Kimi chat route must install the default request profile"
        );
        let mut request = minimal_request();
        request.model_id = String::from("moonshot-v1-8k");

        request.reasoning_effort = Some(ReasoningEffort::High);
        let encoded = client
            .encode_request(&chat_route, &request)
            .expect("Kimi Chat reasoning request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "enabled");
        assert!(encoded.body.get("reasoning_effort").is_none());

        request.reasoning_effort = Some(ReasoningEffort::None);
        let encoded = client
            .encode_request(&chat_route, &request)
            .expect("Kimi Chat none request should encode");
        assert!(encoded.body.get("thinking").is_none());
    }

    /// Verifies the bundled MiniMax profiles produce the legacy-equivalent wire.
    /// 验证内置 MiniMax Profile 产生与旧版等价的 Wire 请求。
    #[test]
    fn minimax_manifest_request_profiles_produce_legacy_equivalent_wire() {
        let client = projection_test_client();
        let chat_route = bundled_manifest_route("minimax", "global", OPENAI_CHAT_PROFILE_ID);
        let anthropic_route =
            bundled_manifest_route("minimax", "global", ANTHROPIC_MESSAGES_PROFILE_ID);
        assert!(
            !chat_route.request_projection_profiles.is_empty()
                && !anthropic_route.request_projection_profiles.is_empty(),
            "bundled MiniMax routes must install manifest request profiles"
        );
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3");

        // Chat adaptive + reasoning split.
        request.reasoning_effort = Some(ReasoningEffort::Adaptive);
        request.model_parameters.insert(
            MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
            ModelParameterValue::Boolean(true),
        );
        let encoded = client
            .encode_request(&chat_route, &request)
            .expect("MiniMax Chat request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "adaptive");
        assert_eq!(encoded.body["reasoning_split"], true);
        assert!(encoded.body.get("reasoning_effort").is_none());

        // Anthropic disabled thinking.
        request.reasoning_effort = Some(ReasoningEffort::None);
        request
            .model_parameters
            .remove(MINIMAX_REASONING_SPLIT_PARAMETER_ID);
        let encoded = client
            .encode_request(&anthropic_route, &request)
            .expect("MiniMax Anthropic request should encode");
        assert_eq!(encoded.body["thinking"]["type"], "disabled");
        assert!(encoded.body.get("output_config").is_none());

        // Anthropic rejects reasoning_split with the controlled legacy message.
        request.model_parameters.insert(
            MINIMAX_REASONING_SPLIT_PARAMETER_ID.to_string(),
            ModelParameterValue::Boolean(true),
        );
        let error = client
            .encode_request(&anthropic_route, &request)
            .expect_err("MiniMax Anthropic must reject reasoning_split");
        assert!(
            error
                .error
                .message
                .contains("registered only for OpenAI Chat"),
            "unexpected error: {}",
            error.error.message
        );
    }

    /// Verifies the projection executor rejects an unsupported MiniMax effort like the legacy fn.
    /// 验证投影执行器像旧版函数一样拒绝不支持的 MiniMax 推理档位。
    #[test]
    fn minimax_profile_rejects_unsupported_effort() {
        let client = projection_test_client();
        let chat_route = bundled_manifest_route("minimax", "global", OPENAI_CHAT_PROFILE_ID);
        let mut request = minimal_request();
        request.model_id = String::from("MiniMax-M3");
        request.reasoning_effort = Some(ReasoningEffort::High);
        let error = client
            .encode_request(&chat_route, &request)
            .expect_err("MiniMax must reject unsupported effort");
        assert!(
            error
                .error
                .message
                .contains("not in the declared mapping table"),
            "unexpected error: {}",
            error.error.message
        );
    }

    /// Verifies a response profile rewrites vendor JSON before protocol decoding.
    /// 验证响应 Profile 会在协议解码前改写供应商 JSON。
    #[test]
    fn response_profile_projects_stream_record_before_decode() {
        let profile = crate::provider_manifest::ResponseProjectionProfile {
            profile_id: String::from("vendor-response.v1"),
            revision: 1,
            steps: vec![crate::provider_manifest::MappingStep::RenameJsonPointer {
                from: String::from("/vendor_usage"),
                to: String::from("/usage"),
            }],
        };

        let projected = project_response_record_text(
            &[profile],
            r#"{"type":"response.completed","vendor_usage":{"input_tokens":7}}"#,
        )
        .expect("response profile should project");
        let value: Value = serde_json::from_str(&projected).expect("projected JSON should parse");
        assert_eq!(value["usage"]["input_tokens"], 7);
        assert!(value.get("vendor_usage").is_none());
    }

    /// Verifies an error profile normalizes vendor error JSON before protocol decoding.
    /// 验证错误 Profile 会在协议解码前规范化供应商错误 JSON。
    /// Verifies vendor error envelopes are normalized before protocol decoding.
    /// 验证供应商错误信封会在协议解码前完成规范化。
    #[test]
    fn error_profile_projects_vendor_error_before_decode() {
        let profile = crate::provider_manifest::ResponseProjectionProfile {
            profile_id: String::from("vendor-error.v1"),
            revision: 1,
            steps: vec![crate::provider_manifest::MappingStep::RenameJsonPointer {
                from: String::from("/vendor_error"),
                to: String::from("/error"),
            }],
        };

        let projected = project_response_record_text(
            &[profile],
            r#"{"vendor_error":{"code":"quota_exceeded","message":"limit"}}"#,
        )
        .expect("error profile should project");
        let value: Value = serde_json::from_str(&projected).expect("projected JSON should parse");
        assert_eq!(value["error"]["code"], "quota_exceeded");
        assert!(value.get("vendor_error").is_none());
    }

    /// Verifies remote scalar ancestors produce a controlled response-projection failure.
    /// 验证远端标量祖先会产生受控响应投影失败。
    #[test]
    fn response_profile_rejects_non_object_target_ancestor_without_panic() {
        let profile = crate::provider_manifest::ResponseProjectionProfile {
            profile_id: String::from("vendor-response.v1"),
            revision: 1,
            steps: vec![crate::provider_manifest::MappingStep::SetLiteral {
                path: String::from("/error/code"),
                value: Value::String(String::from("mapped")),
            }],
        };

        let error = project_response_record_text(&[profile], r#"{"error":"plain"}"#)
            .expect_err("scalar ancestor must fail closed");
        assert!(error.message.contains("non-object"), "{error:?}");
    }

    /// Verifies runtime request scalars cannot turn a valid profile into a process panic.
    /// 验证运行时请求标量不会把有效 Profile 变成进程 panic。
    #[test]
    fn request_profile_rejects_non_object_target_ancestor_without_panic() {
        // profile expects the protocol body to expose an object-valued reasoning envelope.
        // profile 期望协议正文提供对象类型的 reasoning 信封。
        let profile = crate::provider_manifest::RequestProjectionProfile {
            profile_id: String::from("vendor-request.v1"),
            revision: 1,
            steps: vec![crate::provider_manifest::MappingStep::SetLiteral {
                path: String::from("/reasoning/budget_tokens"),
                value: json!(1024),
            }],
        };

        // error proves a runtime scalar is rejected at the pre-dispatch boundary without panic.
        // error 证明运行时标量会在分发前边界被拒绝且不会触发 panic。
        let error = project_json_request_profiles(
            &[profile],
            "vendor-model",
            json!({"reasoning": "provider-default"}),
        )
        .expect_err("scalar ancestor must fail closed");
        assert!(error.contains("non-object"), "{error}");
    }

    /// Verifies the closed template language renders every registered reusable form together.
    /// 验证封闭模板语言可以组合渲染所有已注册的可复用形式。
    #[test]
    fn bounded_json_template_renders_nested_registered_forms() {
        // The source exercises body references, optional omission, array item scope, and enum lookup.
        // 来源同时覆盖正文引用、可选省略、数组项作用域与枚举查找。
        let source = json!({
            "prompt": "draw",
            "format": "jpg",
            "enabled": true,
            "rows": [
                {"name": "first", "url": "https://example.com/first"},
                {"name": "second", "url": "https://example.com/second"}
            ]
        });
        // The template combines model identity, map, enum, condition, concat, and optional reference forms.
        // 模板组合模型标识、数组映射、枚举、条件、拼接与可选引用形式。
        let template = json!({
            "model": {"$vulcan_model_id": true},
            "missing": {"$vulcan_ref": {"path": "/missing", "optional": true}},
            "mime": {
                "$vulcan_enum": {
                    "path": "/format",
                    "mapping": {"jpg": "image/jpeg", "png": "image/png"}
                }
            },
            "mode": {
                "$vulcan_if_equals": {
                    "path": "/enabled",
                    "equals": true,
                    "then": "active",
                    "otherwise": "inactive"
                }
            },
            "label": {
                "$vulcan_concat": [
                    "prefix:",
                    {"$vulcan_ref": {"path": "/prompt"}}
                ]
            },
            "fallback": {
                "$vulcan_coalesce": [
                    {"$vulcan_ref": {"path": "/missing", "optional": true}},
                    {"$vulcan_ref": {"path": "/prompt"}}
                ]
            },
            "present_only": {
                "$vulcan_if_present": {
                    "path": "/prompt",
                    "then": {"value": {"$vulcan_ref": {"path": "/prompt"}}}
                }
            },
            "absent_only": {
                "$vulcan_if_present": {
                    "path": "/missing",
                    "then": "must-not-render"
                }
            },
            "items": {
                "$vulcan_map": {
                    "path": "/rows",
                    "template": {
                        "title": {"$vulcan_ref": {"scope": "item", "path": "/name"}},
                        "url": {"$vulcan_ref": {"scope": "item", "path": "/url"}}
                    }
                }
            },
            "combined_items": {
                "$vulcan_array_concat": [
                    {"$vulcan_map": {"path": "/rows", "template": {"$vulcan_ref": {"scope": "item", "path": "/name"}}}},
                    ["last"]
                ]
            }
        });

        // The rendered value is the exact provider-neutral wire shape.
        // 渲染值是精确的供应商无关 Wire 结构。
        let rendered = render_complete_json_template(&template, &source, "image-model")
            .expect("registered template forms should render");

        assert_eq!(
            rendered,
            json!({
                "model": "image-model",
                "mime": "image/jpeg",
                "mode": "active",
                "label": "prefix:draw",
                "fallback": "draw",
                "present_only": {"value": "draw"},
                "items": [
                    {"title": "first", "url": "https://example.com/first"},
                    {"title": "second", "url": "https://example.com/second"}
                ],
                "combined_items": ["first", "second", "last"]
            })
        );
    }

    /// Verifies template rendering fails closed on ambiguous reserved forms and excessive arrays.
    /// 验证模板渲染会对含糊保留形式与超限数组保持失败关闭。
    #[test]
    fn bounded_json_template_rejects_ambiguous_forms_and_array_overflow() {
        // The ambiguous object mixes a registered form with an ordinary property.
        // 含糊对象把已注册形式与普通属性混在一起。
        let ambiguous = json!({
            "$vulcan_ref": {"path": "/value"},
            "extra": true
        });
        // The ambiguity error proves manifests cannot smuggle unvalidated sibling behavior.
        // 含糊错误证明 Manifest 无法夹带未经校验的同级行为。
        let ambiguity_error =
            render_complete_json_template(&ambiguous, &json!({"value": "ok"}), "model")
                .expect_err("ambiguous reserved form must fail");
        assert!(
            ambiguity_error.contains("only property"),
            "{ambiguity_error}"
        );

        // The source exceeds the fixed collection ceiling by exactly one item.
        // 来源数组恰好比固定集合上限多一个元素。
        let oversized_source = Value::Array(vec![Value::Null; JSON_TEMPLATE_MAX_ARRAY_ITEMS + 1]);
        // The map template is intentionally minimal so only the collection ceiling is exercised.
        // 映射模板刻意保持最小，以便只验证集合上限。
        let map_template = json!({
            "$vulcan_map": {
                "path": "",
                "template": {"$vulcan_ref": {"scope": "item", "path": ""}}
            }
        });
        // The overflow error is deterministic and occurs before allocating provider output.
        // 超限错误是确定性的，并在分配供应商输出前发生。
        let overflow_error =
            render_complete_json_template(&map_template, &oversized_source, "model")
                .expect_err("oversized source array must fail");
        assert!(overflow_error.contains("item limit"), "{overflow_error}");
    }

    /// Verifies template recursion and serialized output are bounded independently.
    /// 验证模板递归深度与序列化输出大小分别受到硬限制。
    #[test]
    fn bounded_json_template_rejects_depth_and_output_overflow() {
        // The nested template starts from one scalar leaf.
        // 嵌套模板从一个标量叶节点开始。
        let mut deep_template = Value::Null;
        for _ in 0..=crate::provider_manifest::MAX_JSON_DEPTH {
            deep_template = Value::Array(vec![deep_template]);
        }
        // The depth error proves rendering cannot recurse beyond the manifest engine ceiling.
        // 深度错误证明渲染不会递归超过 Manifest 引擎上限。
        let depth_error = render_complete_json_template(&deep_template, &Value::Null, "model")
            .expect_err("deep template must fail");
        assert!(depth_error.contains("depth limit"), "{depth_error}");

        // The source string exceeds the final serialized-output ceiling after JSON encoding.
        // 来源字符串在 JSON 编码后超过最终序列化输出上限。
        let oversized_text = "x".repeat(JSON_TEMPLATE_MAX_OUTPUT_BYTES + 1);
        // The direct reference isolates the output-byte guard from collection and depth guards.
        // 直接引用将输出字节守卫与集合、深度守卫隔离。
        let output_template = json!({"$vulcan_ref": {"path": "/text"}});
        // The output error proves a small template cannot amplify an unbounded source value.
        // 输出错误证明小模板不能放大无界来源值。
        let output_error = render_complete_json_template(
            &output_template,
            &json!({"text": oversized_text}),
            "model",
        )
        .expect_err("oversized rendered output must fail");
        assert!(output_error.contains("byte limit"), "{output_error}");
    }
}
