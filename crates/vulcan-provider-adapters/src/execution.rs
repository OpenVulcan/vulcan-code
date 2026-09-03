//! Shared provider execution client, stable exports, and declared transport dispatch.
//! 共用供应商执行客户端、稳定导出与已声明传输分派。

mod failure;
mod framing;
mod http_transport;
mod json_template;
mod prepared_media;
mod projection_profiles;
mod request_encoding;
mod request_headers;
mod resource_download;
mod response_body;
mod response_metadata;
mod responses_compact;
mod responses_websocket;
mod route;
mod stream_state;
mod tool_output;
mod transport_diagnostics;
mod usage;
mod wire_capture;

#[cfg(test)]
mod test_support;

pub use failure::ProviderExecutionFailure;
pub(crate) use failure::{
    http_failure, http_send_transport_failure, pre_dispatch_failure,
    response_headers_protocol_failure, sanitize_provider_execution_failure, transport_failure,
};
pub use prepared_media::{
    AdmittedLocalMediaSource, CONVERSATION_MEDIA_INPUT_OPERATION_ID,
    ConversationMediaUploadContract, ConversationPreparedMediaRetention, LocalMediaMaterialization,
    PreparedConversationMedia, PreparedMediaReferenceSnapshot, PreparedMediaSourceIdentity,
};
use prepared_media::{PreparedMediaCacheEntry, PreparedMediaFlightRegistry};
pub(crate) use prepared_media::{PreparedMediaFlightOutcome, PreparedMediaFlightRegistration};
pub(crate) use projection_profiles::{
    execute_request_profile_fixture, project_json_request_profiles, project_json_response_profiles,
    project_response_record_text,
};
pub(crate) use request_headers::{execution_headers, insert_coding_device_identity_headers};
pub use resource_download::{
    ProviderToolResourceDownloadPolicy, provider_resource_host_matches_suffixes,
    provider_resource_host_suffix_is_safe,
};
pub(crate) use response_body::read_bounded_response_text;
pub(crate) use response_metadata::parse_retry_after_headers;
pub use responses_compact::ResponsesCompactResult;
use responses_websocket::{ResponsesWebsocketSessionCacheEntry, responses_websocket_cache_key};
pub use route::ProviderExecutionRoute;
pub(crate) use route::upstream_protocol_for_profile_id;
use route::upstream_protocol_for_route;
pub use tool_output::{
    ProviderToolCompositeOutput, ProviderToolEmbeddingOutput, ProviderToolEmbeddingVector,
    ProviderToolExecutionResult, ProviderToolExtractFailure, ProviderToolExtractOutput,
    ProviderToolExtractResult, ProviderToolGeneratedResource, ProviderToolNormalizedOutput,
    ProviderToolRerankOutput, ProviderToolRerankResult, ProviderToolSearchOutput,
    ProviderToolSearchResult,
};
use wire_capture::{ProviderWireCaptureTargets, ProviderWireRequestCaptureTarget};
pub use wire_capture::{
    ProviderWireRequestCapture, ProviderWireRequestTraceDispatch, ProviderWireResponseCapture,
    ProviderWireResponseChunkCapture, ProviderWireResponseFrameCapture,
};

use crate::{CredentialExecutionSecret, ProviderToolTarget, ServiceEntry};
use futures_util::{Stream, StreamExt};
use serde_json::Value;
use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Instant;
use vulcan_ai_contract::{AiExecutionRequest, AiStreamEvent};
use vulcan_llm_core::{LlmProtocolRuntime, UpstreamProtocol};

use tokio::sync::{Mutex as TokioMutex, oneshot};

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

/// Maximum process-local reuse lifetime for one context-free model-extension video upload; durable conversations do not use this limit.
/// 无上下文模型扩展视频上传的进程级最大复用生命周期；持久普通会话不使用此限制。
pub(crate) const PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS: u64 = 86_400_000;

/// Stream of normalized public execution events.
/// 归一化公共执行事件流。
pub type ProviderExecutionStream =
    Pin<Box<dyn Stream<Item = Result<AiStreamEvent, ProviderExecutionFailure>> + Send>>;

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
    /// Same-key physical media preparations shared until one owner publishes a terminal result.
    /// 同键真实媒体准备在唯一所有者发布终态结果前共享。
    prepared_media_flights: Arc<StdMutex<PreparedMediaFlightRegistry>>,
    /// Credential-, route-, and session-isolated Responses WebSocket connections.
    /// 按凭据、路由与会话隔离的 Responses WebSocket 连接。
    responses_websocket_sessions:
        Arc<TokioMutex<BTreeMap<String, ResponsesWebsocketSessionCacheEntry>>>,
    /// Session-scoped WebSocket circuit breakers activated after transport failure.
    /// 传输失败后激活的会话作用域 WebSocket 熔断器。
    responses_websocket_breakers: Arc<StdMutex<BTreeMap<String, Instant>>>,
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
            prepared_media_flights: Arc::new(StdMutex::new(BTreeMap::new())),
            responses_websocket_sessions: Arc::new(TokioMutex::new(BTreeMap::new())),
            responses_websocket_breakers: Arc::new(StdMutex::new(BTreeMap::new())),
        }
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
        self.execute_with_optional_wire_captures(
            route,
            request,
            secret,
            execution_id,
            ProviderWireCaptureTargets {
                request: None,
                response: None,
            },
        )
    }

    /// Starts one immutable execution and captures the exact encoded request used by its transport.
    /// 启动一次不可变执行，并捕获其传输层实际使用的精确编码请求。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the resolved immutable route, `request` is the public request,
    /// `secret` is the exact host-loaded credential, `execution_id` is stable, and
    /// `wire_request_capture` receives the encoded request object immediately before dispatch.
    /// `route` 是解析后的不可变路由，`request` 是公共请求，`secret` 是宿主加载的精确凭据，
    /// `execution_id` 是稳定标识，`wire_request_capture` 在分派前接收编码后的请求对象。
    ///
    /// # Returns
    /// 返回值
    /// A normalized event stream whose transport sends the same encoded request body as the capture.
    /// 返回归一化事件流，其传输层发送与捕获内容相同的编码请求正文。
    pub fn execute_with_wire_request_capture(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
        wire_request_capture: oneshot::Sender<ProviderWireRequestCapture>,
    ) -> ProviderExecutionStream {
        self.execute_with_optional_wire_captures(
            route,
            request,
            secret,
            execution_id,
            ProviderWireCaptureTargets {
                request: Some(ProviderWireRequestCaptureTarget::Observe(
                    wire_request_capture,
                )),
                response: None,
            },
        )
    }

    /// Starts one immutable execution and captures both dispatched request and raw HTTP response.
    /// 启动一次不可变执行，并同时捕获已分派请求与原始 HTTP 响应。
    ///
    /// # Parameters
    /// 参数
    /// Request arguments freeze the physical execution; the two one-shot senders publish the
    /// exact request object and the bounded raw response observed before local parsing.
    /// 请求参数冻结真实执行；两个一次性发送器分别发布精确请求对象，以及本地解析前观察到的
    /// 有界原始响应。
    ///
    /// # Returns
    /// 返回值
    /// A normalized event stream with tracing kept outside model-visible semantics.
    /// 返回规范化事件流，追踪数据不会进入模型可见语义。
    pub fn execute_with_wire_api_trace_capture(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
        wire_request_capture: oneshot::Sender<ProviderWireRequestTraceDispatch>,
        wire_response_capture: oneshot::Sender<ProviderWireResponseCapture>,
    ) -> ProviderExecutionStream {
        self.execute_with_optional_wire_captures(
            route,
            request,
            secret,
            execution_id,
            ProviderWireCaptureTargets {
                request: Some(ProviderWireRequestCaptureTarget::Trace(
                    wire_request_capture,
                )),
                response: Some(wire_response_capture),
            },
        )
    }

    /// Starts one immutable execution with optional exact wire request and response captures.
    /// 启动一次不可变执行，并可选启用精确 Wire 请求与响应捕获。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `request`, `secret`, and `execution_id` freeze the physical execution; capture
    /// senders exist only when the host enabled paired request-and-response tracing.
    /// `route`、`request`、`secret` 与 `execution_id` 冻结物理执行；仅当宿主启用成对请求与响应
    /// 追踪时，捕获发送器才存在。
    ///
    /// # Returns
    /// 返回值
    /// A normalized event stream using exactly one declared transport.
    /// 返回仅使用一个已声明传输的归一化事件流。
    fn execute_with_optional_wire_captures(
        &self,
        route: ProviderExecutionRoute,
        request: AiExecutionRequest,
        secret: CredentialExecutionSecret,
        execution_id: String,
        wire_captures: ProviderWireCaptureTargets,
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
                wire_captures,
            )
        } else {
            self.execute_http(route, request, secret, execution_id, wire_captures)
        };
        // Every asynchronous error crosses one common secret-safe and bounded public boundary.
        // 每个异步错误都经过同一条不含秘密且有界的公共边界。
        Box::pin(stream.map(move |result| {
            result
                .map_err(|failure| sanitize_provider_execution_failure(failure, &diagnostic_secret))
        }))
    }
}

#[cfg(test)]
mod tests;
