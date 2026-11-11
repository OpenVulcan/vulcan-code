//! Bounded secret-safe request and response captures for physical provider dispatch.
//! 真实供应商分派的有界无秘密请求与响应捕获。

use super::ProviderExecutionFailure;
use super::failure::TransportFailureEvidence;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;
use vulcan_ai_contract::AiStreamEvent;
use vulcan_llm_core::{EncodedWireRequest, ResponsesWebsocketRawFrame};

/// Carries the secret-safe transport request captured immediately before dispatch.
/// 承载在分派前立即捕获且不含秘密值的传输请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderWireRequestCapture {
    /// Exact transport selected for this physical request.
    /// 当前真实请求选中的精确传输类型。
    pub transport: String,
    /// Exact transport operation used to submit the payload.
    /// 提交载荷所使用的精确传输操作。
    pub method: String,
    /// Protocol-relative endpoint path selected by the encoder.
    /// 编码器选中的协议相对端点路径。
    pub path: String,
    /// Fully resolved endpoint used by the selected transport.
    /// 所选传输实际使用的完整解析端点。
    pub resolved_url: String,
    /// Complete final header projection with every credential-shaped value redacted.
    /// 完整的最终请求头投影，其中所有凭据形态的值均已脱敏。
    pub headers: BTreeMap<String, String>,
    /// Exact JSON payload handed to the selected transport.
    /// 交给所选传输的精确 JSON 载荷。
    pub body: Value,
}

/// Couples one exact request capture with a best-effort persistence acknowledgement observed before dispatch.
/// 将精确请求捕获与网络分派前观察的尽力持久化确认绑定。
pub struct ProviderWireRequestTraceDispatch {
    /// Secret-safe request waiting at the transport boundary.
    /// 在传输边界等待的无秘密请求。
    pub request: ProviderWireRequestCapture,
    /// One-shot result sent by the owning trace writer after its publication attempt.
    /// 所属追踪写入器完成发布尝试后发送的一次性结果。
    pub persistence_result: oneshot::Sender<Result<(), String>>,
}

/// Selects fire-and-forget observation or acknowledged best-effort tracing for one wire request.
/// 为一份 Wire 请求选择即发即忘观察或带确认的尽力追踪。
pub(super) enum ProviderWireRequestCaptureTarget {
    /// Publishes the request without delaying transport dispatch.
    /// 发布请求但不延迟传输分派。
    Observe(oneshot::Sender<ProviderWireRequestCapture>),
    /// Observes the trace writer acknowledgement before dispatch without promoting trace failures.
    /// 在分派前观察追踪写入器确认，但不会将追踪失败提升为业务失败。
    Trace(oneshot::Sender<ProviderWireRequestTraceDispatch>),
}

/// Owns the optional request and response capture endpoints for one physical execution.
/// 持有一次真实执行可选的请求与响应捕获端点。
pub(super) struct ProviderWireCaptureTargets {
    /// Optional exact request observer or acknowledged best-effort trace target.
    /// 可选的精确请求观察器或带确认的尽力追踪目标。
    pub(super) request: Option<ProviderWireRequestCaptureTarget>,
    /// Optional raw response receiver endpoint.
    /// 可选的原始响应接收端点。
    pub(super) response: Option<oneshot::Sender<ProviderWireResponseCapture>>,
}

/// Carries the raw HTTP response bytes observed before SSE framing and protocol decoding.
/// 承载在 SSE 分帧与协议解码之前观察到的原始 HTTP 响应字节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireResponseCapture {
    /// Exact transport selected for this physical request.
    /// 当前真实请求选中的精确传输类型。
    pub transport: String,
    /// Upstream HTTP status when response headers were received.
    /// 收到响应头时的上游 HTTP 状态码。
    pub http_status: Option<u16>,
    /// Upstream content type when it was valid visible header text.
    /// 上游内容类型；仅在响应头是合法可见文本时存在。
    pub content_type: Option<String>,
    /// Upstream content encoding when it was valid visible header text.
    /// 上游内容编码；仅在响应头是合法可见文本时存在。
    pub content_encoding: Option<String>,
    /// Raw response prefix retained before any local parser consumes it.
    /// 在任何本地解析器消费之前保留的原始响应前缀。
    pub body: Vec<u8>,
    /// Exact transport chunk records in receive order.
    /// 按接收顺序记录的精确传输 chunk 记录。
    pub chunks: Vec<ProviderWireResponseChunkCapture>,
    /// Decoded WebSocket application-frame boundaries retained in receive order.
    /// 按接收顺序保留的已解码 WebSocket 应用帧边界。
    pub frames: Vec<ProviderWireResponseFrameCapture>,
    /// Total bytes received even when the retained body is bounded.
    /// 实际收到的总字节数，即使保留正文受到容量限制也会继续累计。
    pub received_bytes: u64,
    /// Whether the raw body exceeded the bounded diagnostic capacity.
    /// 原始正文是否超过有界诊断容量。
    pub body_truncated: bool,
    /// Whether the chunk boundary index exceeded its bounded capacity.
    /// chunk 边界索引是否超过其有界容量。
    pub chunk_index_truncated: bool,
    /// Whether the WebSocket frame boundary index exceeded its bounded capacity.
    /// WebSocket 帧边界索引是否超过其有界容量。
    pub frame_index_truncated: bool,
    /// Capture-side terminal classification independent from Provider retry semantics.
    /// 独立于 Provider 重试语义的捕获侧终态分类。
    pub outcome: String,
    /// First normalized tool-call abort diagnostic observed before the host rejected the turn.
    /// 宿主拒绝回合前观察到的首个规范化工具调用中止诊断。
    pub normalized_tool_call_abort: Option<String>,
    /// Exact local processing stage that failed after response bytes were observed.
    /// 观察到响应字节后发生失败的精确本地处理阶段。
    pub failure_stage: Option<String>,
    /// Controlled failure diagnostic paired with `failure_stage`.
    /// 与 `failure_stage` 配对的受控失败诊断。
    pub failure_message: Option<String>,
    /// Stable phase label recorded only when the transport attempt stopped before its first byte.
    /// 仅在传输尝试在其首字节之前中断时记录的稳定阶段标签。
    pub transport_failure_phase: Option<String>,
    /// Bounded ordered transport cause chain recorded only for a stopped transport attempt.
    /// 仅在传输尝试中断时记录的有序受限传输原因链。
    pub transport_failure_causes: Option<Vec<String>>,
    /// Registered bound in milliseconds recorded only when that bound ended the attempt.
    /// 仅在已登记时限结束本次尝试时记录的时限毫秒数。
    pub transport_failure_timeout_ms: Option<u64>,
    /// SHA-256 of every raw payload byte received, including bytes beyond the retained body cap.
    /// 所有已接收原始载荷字节的 SHA-256，包括超出正文保留上限的字节。
    pub body_sha256: String,
}

/// Describes one raw HTTP transport chunk before local framing or decoding.
/// 描述本地分帧或解码前的一块原始 HTTP 传输 chunk。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireResponseChunkCapture {
    /// Original response-stream byte offset before this chunk.
    /// 当前 chunk 之前的原始响应流字节偏移。
    pub offset: u64,
    /// Exact chunk byte length.
    /// chunk 的精确字节长度。
    pub length: u64,
    /// Wall-clock observation time in Unix milliseconds.
    /// 以 Unix 毫秒表示的观察时间。
    pub observed_at_unix_ms: u128,
    /// SHA-256 of the exact chunk bytes.
    /// 精确 chunk 字节的 SHA-256。
    pub sha256: String,
}

/// Describes one decoded WebSocket application frame inside the retained response body.
/// 描述保留响应正文中的一条已解码 WebSocket 应用帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderWireResponseFrameCapture {
    /// Stable decoded frame kind supplied by the WebSocket transport.
    /// WebSocket 传输层提供的稳定已解码帧类型。
    pub kind: String,
    /// Original response-stream byte offset before this frame payload.
    /// 当前帧载荷之前的原始响应流字节偏移。
    pub offset: u64,
    /// Exact application payload length before any local parsing.
    /// 任何本地解析前的精确应用载荷长度。
    pub length: u64,
    /// Wall-clock observation time in Unix milliseconds.
    /// 以 Unix 毫秒表示的观察时间。
    pub observed_at_unix_ms: u128,
    /// SHA-256 of the exact application payload.
    /// 精确应用载荷的 SHA-256。
    pub sha256: String,
}

/// Maximum raw response bytes retained for one opted-in physical API call.
/// 单次显式启用追踪的真实 API 调用允许保留的最大原始响应字节数。
const PROVIDER_WIRE_RESPONSE_CAPTURE_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Maximum transport chunk boundaries retained for one physical API call.
/// 单次真实 API 调用允许保留的最大传输 chunk 边界数量。
const PROVIDER_WIRE_RESPONSE_CAPTURE_MAX_CHUNKS: usize = 65_536;

/// Publishes one partial or complete HTTP response capture when its stream scope ends.
/// 在 HTTP 响应流作用域结束时发布一份部分或完整响应捕获。
pub(super) struct ProviderWireResponseCaptureGuard {
    /// One-shot destination owned by the opted-in host trace writer.
    /// 由显式启用的宿主追踪写入器持有的一次性目标。
    sender: Option<oneshot::Sender<ProviderWireResponseCapture>>,
    /// Mutable bounded capture sent unchanged when the guard is dropped.
    /// 守卫析构时原样发送的可变有界捕获。
    capture: Option<ProviderWireResponseCapture>,
    /// Incremental digest over all received payload bytes, including unretained suffixes.
    /// 覆盖所有已接收载荷字节（包括未保留后缀）的增量摘要。
    body_hasher: Option<Sha256>,
}

impl ProviderWireResponseCaptureGuard {
    /// Creates one response guard whose default outcome denotes an interrupted or failed stream.
    /// 创建响应守卫；其默认终态表示流被中断或失败。
    ///
    /// # Parameters
    /// 参数
    /// `transport` is the exact physical transport and `sender` exists only when tracing is enabled.
    /// `transport` 是精确真实传输；`sender` 仅在启用追踪时存在。
    ///
    /// # Returns
    /// 返回值
    /// A no-op guard when no sender exists, otherwise a bounded capture owner.
    /// 未提供发送器时返回空操作守卫，否则返回有界捕获所有者。
    pub(super) fn new(
        transport: &str,
        sender: Option<oneshot::Sender<ProviderWireResponseCapture>>,
    ) -> Self {
        let capture = sender.as_ref().map(|_| ProviderWireResponseCapture {
            transport: transport.to_string(),
            http_status: None,
            content_type: None,
            content_encoding: None,
            body: Vec::new(),
            chunks: Vec::new(),
            frames: Vec::new(),
            received_bytes: 0,
            body_truncated: false,
            chunk_index_truncated: false,
            frame_index_truncated: false,
            outcome: String::from("stream_failed_or_cancelled"),
            normalized_tool_call_abort: None,
            failure_stage: None,
            failure_message: None,
            transport_failure_phase: None,
            transport_failure_causes: None,
            transport_failure_timeout_ms: None,
            body_sha256: String::new(),
        });
        let body_hasher = capture.as_ref().map(|_| Sha256::new());
        Self {
            sender,
            capture,
            body_hasher,
        }
    }

    /// Records response metadata before any body reader starts.
    /// 在任何正文读取器启动前记录响应元数据。
    ///
    /// # Parameters
    /// 参数
    /// `status` and `headers` are the untouched upstream response head.
    /// `status` 与 `headers` 是未经修改的上游响应头。
    pub(super) fn observe_http_headers(&mut self, status: u16, headers: &HeaderMap) {
        let Some(capture) = self.capture.as_mut() else {
            return;
        };
        capture.http_status = Some(status);
        capture.content_type = headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        capture.content_encoding = headers
            .get(reqwest::header::CONTENT_ENCODING)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
    }

    /// Records one raw transport chunk before framing or protocol decoding.
    /// 在分帧或协议解码之前记录一块原始传输 chunk。
    ///
    /// # Parameters
    /// 参数
    /// `chunk` is the exact byte slice returned by `reqwest::Response::bytes_stream`.
    /// `chunk` 是 `reqwest::Response::bytes_stream` 返回的精确字节切片。
    pub(super) fn observe_chunk(&mut self, chunk: &[u8]) {
        if let Some(hasher) = self.body_hasher.as_mut() {
            hasher.update(chunk);
        }
        let Some(capture) = self.capture.as_mut() else {
            return;
        };
        if capture.chunks.len() < PROVIDER_WIRE_RESPONSE_CAPTURE_MAX_CHUNKS {
            capture.chunks.push(ProviderWireResponseChunkCapture {
                offset: capture.received_bytes,
                length: u64::try_from(chunk.len()).unwrap_or(u64::MAX),
                observed_at_unix_ms: provider_wire_capture_timestamp_ms(),
                sha256: hex::encode(Sha256::digest(chunk)),
            });
        } else {
            capture.chunk_index_truncated = true;
        }
        Self::observe_body_bytes(capture, chunk);
    }

    /// Records one decoded WebSocket application frame before JSON and protocol parsing.
    /// 在 JSON 与协议解析前记录一条已解码 WebSocket 应用帧。
    ///
    /// # Parameters
    /// 参数
    /// `frame` contains the exact application payload and its stable decoded frame kind.
    /// `frame` 包含精确应用载荷及其稳定的已解码帧类型。
    pub(super) fn observe_websocket_frame(&mut self, frame: ResponsesWebsocketRawFrame) {
        if let Some(hasher) = self.body_hasher.as_mut() {
            hasher.update(frame.payload.as_slice());
        }
        let Some(capture) = self.capture.as_mut() else {
            return;
        };
        if capture.frames.len() < PROVIDER_WIRE_RESPONSE_CAPTURE_MAX_CHUNKS {
            capture.frames.push(ProviderWireResponseFrameCapture {
                kind: frame.kind,
                offset: capture.received_bytes,
                length: u64::try_from(frame.payload.len()).unwrap_or(u64::MAX),
                observed_at_unix_ms: provider_wire_capture_timestamp_ms(),
                sha256: hex::encode(Sha256::digest(frame.payload.as_slice())),
            });
        } else {
            capture.frame_index_truncated = true;
        }
        Self::observe_body_bytes(capture, frame.payload.as_slice());
    }

    /// Appends one raw payload to the bounded body while retaining the unbounded byte count.
    /// 将一段原始载荷追加到有界正文，同时保留不受界限影响的总字节计数。
    ///
    /// # Parameters
    /// 参数
    /// `capture` owns the aggregate state and `payload` is untouched transport data.
    /// `capture` 持有聚合状态，`payload` 是未经修改的传输数据。
    fn observe_body_bytes(capture: &mut ProviderWireResponseCapture, payload: &[u8]) {
        capture.received_bytes = capture
            .received_bytes
            .saturating_add(u64::try_from(payload.len()).unwrap_or(u64::MAX));
        let remaining = PROVIDER_WIRE_RESPONSE_CAPTURE_MAX_BYTES.saturating_sub(capture.body.len());
        let retained = remaining.min(payload.len());
        capture.body.extend_from_slice(&payload[..retained]);
        capture.body_truncated |= retained < payload.len();
    }

    /// Replaces the default interrupted outcome with the exact known capture terminal state.
    /// 使用精确已知的捕获终态替换默认中断结果。
    ///
    /// # Parameters
    /// 参数
    /// `outcome` is a stable English diagnostic code.
    /// `outcome` 是稳定的英文诊断代码。
    pub(super) fn set_outcome(&mut self, outcome: &str) {
        if let Some(capture) = self.capture.as_mut() {
            capture.outcome = outcome.to_string();
        }
    }

    /// Records normalized failure and terminal evidence before the consumer can stop polling.
    /// 在调用方可能停止拉流之前记录规范化失败与终态证据。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the validated normalized event about to be yielded to the consumer.
    /// `event` 是即将交付调用方且已经通过校验的规范化事件。
    pub(super) fn observe_normalized_event(&mut self, event: &AiStreamEvent) {
        if let AiStreamEvent::ToolCallAborted { message, .. } = event
            && let Some(capture) = self.capture.as_mut()
        {
            if capture.normalized_tool_call_abort.is_none() {
                capture.normalized_tool_call_abort = Some(message.clone());
            }
            if capture.failure_stage.is_none() {
                capture.failure_stage = Some(String::from("tool_arguments_json"));
                capture.failure_message = Some(message.clone());
            }
        }
        match event {
            AiStreamEvent::ResponseCompleted => self.set_outcome("response_completed"),
            AiStreamEvent::ResponseFailed { .. } => self.set_outcome("response_failed"),
            _ => {}
        }
    }

    /// Records the first exact local processing failure without changing Provider retry semantics.
    /// 记录首个精确本地处理失败，且不改变 Provider 重试语义。
    ///
    /// # Parameters
    /// 参数
    /// `stage` is the stable processing boundary and `failure` owns the controlled diagnostic.
    /// `stage` 是稳定处理边界，`failure` 持有受控诊断。
    pub(super) fn observe_failure(&mut self, stage: &str, failure: &ProviderExecutionFailure) {
        if let Some(capture) = self.capture.as_mut()
            && capture.failure_stage.is_none()
        {
            capture.failure_stage = Some(stage.to_string());
            capture.failure_message = Some(failure.error.message.clone());
        }
    }

    /// Records the first bounded transport-failure evidence without changing Provider retry semantics.
    /// 记录首个受限传输失败证据，且不改变 Provider 重试语义。
    ///
    /// # Parameters
    /// 参数
    /// `evidence` is the secret-safe phase, cause chain, and bound collected from the stopped attempt.
    /// `evidence` 是从中断尝试中收集的无秘密阶段、原因链与时限。
    pub(super) fn observe_transport_failure(&mut self, evidence: &TransportFailureEvidence) {
        if let Some(capture) = self.capture.as_mut()
            && capture.transport_failure_phase.is_none()
        {
            capture.transport_failure_phase = Some(evidence.phase.as_str().to_string());
            capture.transport_failure_causes = Some(evidence.causes.clone());
            capture.transport_failure_timeout_ms = evidence.timeout_ms;
        }
    }

    /// Marks transport completion only when no more specific terminal state was observed.
    /// 仅在尚未观察到更具体终态时标记传输完成。
    ///
    /// # Parameters
    /// 参数
    /// `outcome` is the fallback completion code for a fully drained stream.
    /// `outcome` 是完整拉取流时使用的后备完成代码。
    pub(super) fn set_outcome_if_unresolved(&mut self, outcome: &str) {
        if let Some(capture) = self.capture.as_mut()
            && capture.outcome == "stream_failed_or_cancelled"
        {
            capture.outcome = outcome.to_string();
        }
    }
}

impl Drop for ProviderWireResponseCaptureGuard {
    /// Publishes all bytes already received even when parsing, cancellation, or transport fails.
    /// 即使解析、取消或传输失败，也会发布已经收到的全部字节。
    fn drop(&mut self) {
        let (Some(sender), Some(mut capture)) = (self.sender.take(), self.capture.take()) else {
            return;
        };
        if capture.outcome == "stream_failed_or_cancelled" && capture.failure_stage.is_none() {
            capture.failure_stage = Some(String::from("cancelled"));
            capture.failure_message = Some(String::from(
                "Response capture ended before a terminal transport or protocol outcome was observed.",
            ));
        }
        if let Some(hasher) = self.body_hasher.take() {
            capture.body_sha256 = hex::encode(hasher.finalize());
        }
        let _ = sender.send(capture);
    }
}

/// Returns a best-effort wall-clock timestamp for diagnostic receive records.
/// 返回用于诊断接收记录的尽力墙钟时间戳。
fn provider_wire_capture_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

/// Publishes a wire request and observes the optional durable trace barrier without blocking business dispatch on trace failures.
/// 发布 Wire 请求并观察可选的可靠追踪屏障，但追踪失败不得阻止业务分派。
pub(super) async fn publish_wire_request_capture(
    target: ProviderWireRequestCaptureTarget,
    request: ProviderWireRequestCapture,
) -> Result<(), ProviderExecutionFailure> {
    match target {
        ProviderWireRequestCaptureTarget::Observe(sender) => {
            let _ = sender.send(request);
            Ok(())
        }
        ProviderWireRequestCaptureTarget::Trace(sender) => {
            let (result_sender, result_receiver) = oneshot::channel();
            if sender
                .send(ProviderWireRequestTraceDispatch {
                    request,
                    persistence_result: result_sender,
                })
                .is_err()
            {
                eprintln!(
                    "[provider-api-trace] trace_write_failed=writer closed before request persistence acknowledgement"
                );
                return Ok(());
            }
            match result_receiver.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => eprintln!(
                    "[provider-api-trace] trace_write_failed=request persistence failed: {error}"
                ),
                Err(_) => eprintln!(
                    "[provider-api-trace] trace_write_failed=writer closed without request persistence acknowledgement"
                ),
            }
            Ok(())
        }
    }
}

/// Builds one complete transport request capture without retaining credential values.
/// 构建一份完整的传输请求捕获，同时不保留任何凭据值。
///
/// # Parameters
/// 参数
/// `transport`, `method`, and `resolved_url` identify the selected physical operation;
/// `encoded` supplies the exact endpoint path and body; `headers` are the final transport headers.
/// `transport`、`method` 与 `resolved_url` 标识选中的真实操作；`encoded` 提供精确端点路径
/// 与正文；`headers` 是最终传输请求头。
///
/// # Returns
/// 返回值
/// A capture whose body is exact and whose credential-shaped header values are redacted.
/// 返回正文精确且凭据形态请求头值均已脱敏的捕获结果。
pub(crate) fn provider_wire_request_capture(
    transport: &str,
    method: &str,
    resolved_url: &str,
    encoded: &EncodedWireRequest,
    headers: &HeaderMap,
) -> ProviderWireRequestCapture {
    ProviderWireRequestCapture {
        transport: transport.to_string(),
        method: method.to_string(),
        path: encoded.path.clone(),
        resolved_url: resolved_url.to_string(),
        headers: provider_wire_request_capture_headers(headers),
        body: encoded.body.clone(),
    }
}

/// Projects final transport headers into a deterministic secret-safe diagnostic map.
/// 将最终传输请求头投影为确定性的无秘密诊断映射。
///
/// # Parameters
/// 参数
/// `headers` contains the exact header values about to be sent by the transport.
/// `headers` 包含传输层即将发送的精确请求头值。
///
/// # Returns
/// 返回值
/// A lexical header map that preserves every name while redacting sensitive values.
/// 返回保留全部字段名并脱敏敏感值的字典序请求头映射。
fn provider_wire_request_capture_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .map(|(name, value)| {
            // Use the header's sensitivity flag and known credential headers so ordinary protocol fields stay readable.
            // 使用请求头的敏感标记与已知凭据头，保留普通协议字段供排查。
            let sensitive = value.is_sensitive()
                || matches!(
                    name.as_str(),
                    "authorization"
                        | "proxy-authorization"
                        | "x-api-key"
                        | "api-key"
                        | "x-goog-api-key"
                        | "anthropic-api-key"
                        | "x-auth-token"
                        | "x-session-token"
                        | "cookie"
                        | "set-cookie"
                );
            // Preserve the authentication scheme for diagnosing the wire format.
            // 保留认证方式，便于排查线路格式。
            let value = String::from_utf8_lossy(value.as_bytes());
            // Only recognized authentication schemes can appear before the placeholder.
            // 只有已知的认证方式可以保留在占位符之前。
            let projected = if sensitive {
                match value.split_once(' ') {
                    Some((scheme, _))
                        if matches!(name.as_str(), "authorization" | "proxy-authorization")
                            && ["Bearer", "Basic", "Auth"]
                                .iter()
                                .any(|known| scheme.eq_ignore_ascii_case(known)) =>
                    {
                        format!("{scheme} {{APIKEY}}")
                    }
                    _ => String::from("{APIKEY}"),
                }
            } else {
                value.into_owned()
            };
            (name.as_str().to_string(), projected)
        })
        .collect()
}

#[cfg(test)]
mod tests;
