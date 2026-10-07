//! OpenAI Responses WebSocket session transport adapted from the Codex connection model.
//! 基于 Codex 连接模型适配的 OpenAI Responses WebSocket 会话传输。

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderName, HeaderValue},
    },
};

use crate::LlmCoreError;

/// Default maximum lifetime kept below the provider's documented sixty-minute limit.
/// 默认最大生命周期低于供应商文档规定的六十分钟限制。
pub const DEFAULT_RESPONSES_WEBSOCKET_MAX_AGE: Duration = Duration::from_secs(59 * 60);

/// Official control error requiring a fresh Responses WebSocket connection.
/// 要求新建 Responses WebSocket 连接的官方控制错误。
pub const RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE: &str = "websocket_connection_limit_reached";

/// Carries one credential-scoped Responses WebSocket handshake configuration.
/// 承载一个凭据作用域的 Responses WebSocket 握手配置。
#[derive(Clone)]
pub struct ResponsesWebsocketConfig {
    /// Exact `ws` or `wss` Responses endpoint.
    /// 精确的 `ws` 或 `wss` Responses 端点。
    pub url: String,
    /// Exact handshake headers, including the already-resolved credential projection.
    /// 精确握手 Header，包括已解析的凭据投影。
    pub headers: BTreeMap<String, String>,
    /// Maximum handshake duration.
    /// 最大握手时长。
    pub connect_timeout: Duration,
    /// Maximum idle time between response frames.
    /// 响应帧之间的最大空闲时长。
    pub idle_timeout: Duration,
    /// Maximum connection lifetime before proactive replacement.
    /// 主动换代前的最大连接生命周期。
    pub max_connection_age: Duration,
    /// Maximum accepted UTF-8 JSON frame size.
    /// 可接受的 UTF-8 JSON 帧最大尺寸。
    pub max_frame_bytes: usize,
}

/// Carries one parsed server frame and its terminal classification.
/// 承载一条已解析服务端帧及其终态分类。
#[derive(Debug, Clone, PartialEq)]
pub struct ResponsesWebsocketFrame {
    /// Exact decoded WebSocket application-frame payload observed before JSON parsing.
    /// 在 JSON 解析前观察到的精确 WebSocket 应用帧载荷。
    pub raw: ResponsesWebsocketRawFrame,
    /// Untouched parsed provider event.
    /// 未经修改的已解析供应商事件。
    pub value: Value,
    /// Whether the event completed the current response successfully.
    /// 当前事件是否成功完成本次响应。
    pub completed: bool,
    /// Whether the event terminated the current response with failure.
    /// 当前事件是否以失败终止本次响应。
    pub failed: bool,
}

/// Carries one decoded WebSocket frame payload before local protocol interpretation.
/// 承载一条在本地协议解释前的已解码 WebSocket 帧载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponsesWebsocketRawFrame {
    /// Stable decoded frame kind such as `text`, `binary`, or `close`.
    /// 稳定的已解码帧类型，例如 `text`、`binary` 或 `close`。
    pub kind: String,
    /// Exact application payload bytes supplied by Tungstenite.
    /// Tungstenite 提供的精确应用载荷字节。
    pub payload: Vec<u8>,
}

/// Owns one non-multiplexed Responses WebSocket connection.
/// 持有一个不进行多路复用的 Responses WebSocket 连接。
pub struct ResponsesWebsocketSession {
    /// Immutable handshake and transport limits.
    /// 不可变握手与传输限制。
    config: ResponsesWebsocketConfig,
    /// Active provider connection.
    /// 活动供应商连接。
    connection: WebSocketStream<MaybeTlsStream<TcpStream>>,
    /// Connection establishment time used for proactive replacement.
    /// 用于主动换代的连接建立时间。
    connected_at: Instant,
    /// Whether one response is currently in flight.
    /// 当前是否有一条响应正在处理中。
    in_flight: bool,
    /// Most recent response identity cached by this active socket.
    /// 当前活动 Socket 缓存的最新响应标识。
    previous_response_id: Option<String>,
    /// Whether a provider control event requires replacement before the next response.
    /// 供应商控制事件是否要求在下一次响应前换代连接。
    replace_before_next_request: bool,
    /// Most recent application frame retained when local validation returns an error.
    /// 当本地校验返回错误时保留的最近一条应用帧。
    last_raw_frame: Option<ResponsesWebsocketRawFrame>,
}

impl ResponsesWebsocketSession {
    /// Establishes one isolated credential-scoped Responses WebSocket connection.
    /// 建立一个隔离的凭据作用域 Responses WebSocket 连接。
    ///
    /// # Parameters
    /// 参数
    /// `config` contains the exact endpoint, handshake headers, and bounded timeouts.
    /// `config` 包含精确端点、握手 Header 与有界超时。
    ///
    /// # Returns
    /// 返回值
    /// A connected session, or a controlled handshake failure.
    /// 返回已连接会话，或受控握手失败。
    pub async fn connect(config: ResponsesWebsocketConfig) -> Result<Self, LlmCoreError> {
        validate_config(&config)?;
        let mut request = config.url.as_str().into_client_request().map_err(|error| {
            LlmCoreError::new(format!("Responses WebSocket URL is invalid: {error}"))
        })?;
        for (name, value) in &config.headers {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
                LlmCoreError::new(format!(
                    "Responses WebSocket header name is invalid: {error}"
                ))
            })?;
            let value = HeaderValue::from_str(value).map_err(|error| {
                LlmCoreError::new(format!(
                    "Responses WebSocket header value is invalid: {error}"
                ))
            })?;
            request.headers_mut().insert(name, value);
        }
        let (connection, _) = tokio::time::timeout(config.connect_timeout, connect_async(request))
            .await
            .map_err(|_| LlmCoreError::new("Responses WebSocket handshake timed out."))?
            .map_err(|error| {
                LlmCoreError::new(format!("Responses WebSocket handshake failed: {error}"))
            })?;
        Ok(Self {
            config,
            connection,
            connected_at: Instant::now(),
            in_flight: false,
            previous_response_id: None,
            replace_before_next_request: false,
            last_raw_frame: None,
        })
    }

    /// Reports whether this connection must be replaced before another request.
    /// 报告当前连接是否必须在下一次请求前换代。
    pub fn replacement_required(&self) -> bool {
        self.replace_before_next_request
            || self.connected_at.elapsed() >= self.config.max_connection_age
            || matches!(self.connection.get_ref(), MaybeTlsStream::Plain(stream) if stream.peer_addr().is_err())
    }

    /// Reports whether one response currently owns this connection.
    /// 报告当前是否有一条响应占用此连接。
    pub fn in_flight(&self) -> bool {
        self.in_flight
    }

    /// Returns the latest response identity cached by this active socket.
    /// 返回当前活动 Socket 缓存的最新响应标识。
    pub fn previous_response_id(&self) -> Option<&str> {
        self.previous_response_id.as_deref()
    }

    /// Takes the exact frame payload retained by the most recent failed receive operation.
    /// 取出最近一次失败接收操作保留的精确帧载荷。
    ///
    /// # Returns
    /// 返回值
    /// The retained frame when the failure followed an application frame, otherwise `None`.
    /// 当失败发生在收到应用帧之后时返回该帧，否则返回 `None`。
    pub fn take_last_raw_frame(&mut self) -> Option<ResponsesWebsocketRawFrame> {
        self.last_raw_frame.take()
    }

    /// Abandons one locally undecodable response and requires a fresh connection.
    /// 放弃一次本地无法解码的响应并要求新建连接。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; in-flight and continuation state are cleared synchronously.
    /// 不返回值；同步清除处理中状态与延续状态。
    pub fn abandon_response(&mut self) {
        self.in_flight = false;
        self.previous_response_id = None;
        self.replace_before_next_request = true;
    }

    /// Sends one `response.create` request after removing HTTP-only controls.
    /// 移除 HTTP 专属控制字段后发送一条 `response.create` 请求。
    ///
    /// # Parameters
    /// 参数
    /// `responses_body` is the exact canonical Responses create body and `generate` controls prewarm.
    /// `responses_body` 是精确的规范 Responses create 正文，`generate` 控制预热。
    pub async fn begin_response(
        &mut self,
        mut responses_body: Value,
        generate: Option<bool>,
    ) -> Result<(), LlmCoreError> {
        if self.in_flight {
            return Err(LlmCoreError::new(
                "Responses WebSocket does not allow multiplexed in-flight responses.",
            ));
        }
        self.last_raw_frame = None;
        let body = responses_body.as_object_mut().ok_or_else(|| {
            LlmCoreError::new("Responses WebSocket request body must be a JSON object.")
        })?;
        body.remove("background");
        if let Some(generate) = generate {
            body.insert(String::from("generate"), Value::Bool(generate));
        }
        body.insert(
            String::from("type"),
            Value::String(String::from("response.create")),
        );
        let payload = serde_json::to_string(&responses_body).map_err(|error| {
            LlmCoreError::new(format!(
                "Responses WebSocket request could not be serialized: {error}"
            ))
        })?;
        self.connection
            .send(Message::Text(payload.into()))
            .await
            .map_err(|error| {
                LlmCoreError::new(format!("Responses WebSocket send failed: {error}"))
            })?;
        self.in_flight = true;
        Ok(())
    }

    /// Receives one complete provider frame and updates continuation state.
    /// 接收一条完整供应商帧并更新延续状态。
    ///
    /// # Returns
    /// 返回值
    /// The next JSON event, `None` after a clean close, or a controlled transport failure.
    /// 返回下一条 JSON 事件；正常关闭后返回 `None`；失败时返回受控传输错误。
    pub async fn next_frame(&mut self) -> Result<Option<ResponsesWebsocketFrame>, LlmCoreError> {
        if !self.in_flight {
            return Err(LlmCoreError::new(
                "Responses WebSocket has no in-flight response.",
            ));
        }
        self.last_raw_frame = None;
        loop {
            let message = tokio::time::timeout(self.config.idle_timeout, self.connection.next())
                .await
                .map_err(|_| LlmCoreError::new("Responses WebSocket frame idle timeout."))?;
            let Some(message) = message else {
                self.in_flight = false;
                self.previous_response_id = None;
                return Ok(None);
            };
            let message = message.map_err(|error| {
                self.in_flight = false;
                self.previous_response_id = None;
                LlmCoreError::new(format!("Responses WebSocket receive failed: {error}"))
            })?;
            match message {
                Message::Text(text) => {
                    let raw = ResponsesWebsocketRawFrame {
                        kind: String::from("text"),
                        payload: text.as_bytes().to_vec(),
                    };
                    self.last_raw_frame = Some(raw);
                    if text.len() > self.config.max_frame_bytes {
                        self.in_flight = false;
                        self.previous_response_id = None;
                        return Err(LlmCoreError::new(format!(
                            "Responses WebSocket frame exceeded {} bytes.",
                            self.config.max_frame_bytes
                        )));
                    }
                    let value: Value = serde_json::from_str(&text).map_err(|error| {
                        self.in_flight = false;
                        self.previous_response_id = None;
                        LlmCoreError::new(format!(
                            "Responses WebSocket frame is not valid JSON: {error}"
                        ))
                    })?;
                    let event_type = value.get("type").and_then(Value::as_str);
                    let completed = event_type == Some("response.completed");
                    let failed = matches!(
                        event_type,
                        Some("response.failed" | "response.incomplete" | "error")
                    );
                    let error_code = value
                        .pointer("/error/code")
                        .or_else(|| value.pointer("/response/error/code"))
                        .and_then(Value::as_str);
                    if completed {
                        self.previous_response_id = value
                            .pointer("/response/id")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        self.in_flight = false;
                    } else if failed {
                        self.previous_response_id = None;
                        self.in_flight = false;
                        self.replace_before_next_request |=
                            error_code == Some(RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE);
                    }
                    return Ok(Some(ResponsesWebsocketFrame {
                        raw: self
                            .last_raw_frame
                            .take()
                            .expect("text frame capture must exist before JSON parsing"),
                        value,
                        completed,
                        failed,
                    }));
                }
                Message::Ping(payload) => {
                    self.connection
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|error| {
                            LlmCoreError::new(format!("Responses WebSocket pong failed: {error}"))
                        })?;
                }
                Message::Pong(_) => {}
                Message::Close(frame) => {
                    self.last_raw_frame = Some(ResponsesWebsocketRawFrame {
                        kind: String::from("close"),
                        payload: frame
                            .as_ref()
                            .map(|frame| frame.reason.as_bytes().to_vec())
                            .unwrap_or_default(),
                    });
                    self.in_flight = false;
                    self.previous_response_id = None;
                    self.replace_before_next_request = true;
                    let close_code = frame.as_ref().map(|frame| u16::from(frame.code));
                    let error = LlmCoreError::new(format!(
                        "Responses WebSocket closed before terminal response: {}",
                        frame
                            .map(|frame| format!("{} {}", frame.code, frame.reason))
                            .unwrap_or_else(|| String::from("no close frame"))
                    ));
                    return Err(match close_code {
                        Some(close_code) => error.with_websocket_close_code(close_code),
                        None => error,
                    });
                }
                Message::Binary(payload) => {
                    self.last_raw_frame = Some(ResponsesWebsocketRawFrame {
                        kind: String::from("binary"),
                        payload: payload.to_vec(),
                    });
                    self.in_flight = false;
                    self.previous_response_id = None;
                    self.replace_before_next_request = true;
                    return Err(LlmCoreError::new(
                        "Responses WebSocket received an unsupported non-JSON frame.",
                    ));
                }
                Message::Frame(frame) => {
                    self.last_raw_frame = Some(ResponsesWebsocketRawFrame {
                        kind: String::from("raw"),
                        payload: frame.payload().to_vec(),
                    });
                    self.in_flight = false;
                    self.previous_response_id = None;
                    self.replace_before_next_request = true;
                    return Err(LlmCoreError::new(
                        "Responses WebSocket received an unsupported non-JSON frame.",
                    ));
                }
            }
        }
    }

    /// Closes the active connection and clears all continuation state.
    /// 关闭活动连接并清除全部延续状态。
    pub async fn close(&mut self) -> Result<(), LlmCoreError> {
        self.abandon_response();
        self.connection.close(None).await.map_err(|error| {
            LlmCoreError::new(format!("Responses WebSocket close failed: {error}"))
        })
    }
}

/// Validates bounded transport configuration before any network access.
/// 在进行任何网络访问前校验有界传输配置。
fn validate_config(config: &ResponsesWebsocketConfig) -> Result<(), LlmCoreError> {
    if !(config.url.starts_with("ws://") || config.url.starts_with("wss://")) {
        return Err(LlmCoreError::new(
            "Responses WebSocket URL must use `ws` or `wss`.",
        ));
    }
    if config.connect_timeout.is_zero()
        || config.idle_timeout.is_zero()
        || config.max_connection_age.is_zero()
        || config.max_frame_bytes == 0
    {
        return Err(LlmCoreError::new(
            "Responses WebSocket timeouts and frame limits must be positive.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio_tungstenite::{
        accept_async,
        tungstenite::protocol::{CloseFrame, frame::coding::CloseCode},
    };

    /// Creates a bounded local test configuration.
    /// 创建一个有界本地测试配置。
    fn test_config(url: String) -> ResponsesWebsocketConfig {
        ResponsesWebsocketConfig {
            url,
            headers: BTreeMap::new(),
            connect_timeout: Duration::from_secs(2),
            idle_timeout: Duration::from_secs(2),
            max_connection_age: Duration::from_secs(60),
            max_frame_bytes: 64 * 1024,
        }
    }

    #[tokio::test]
    /// Preserves the official streaming flag while removing unsupported background mode.
    /// 保留官方流式标记，同时移除 WebSocket 不支持的后台模式。
    async fn websocket_request_preserves_stream_and_removes_background() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("address should resolve");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("client should connect");
            let mut websocket = accept_async(stream).await.expect("upgrade should succeed");
            let request = websocket
                .next()
                .await
                .expect("request should arrive")
                .expect("request should be readable");
            let Message::Text(request) = request else {
                panic!("request must be text");
            };
            let request: Value = serde_json::from_str(&request).expect("request should be JSON");
            assert_eq!(request["type"], "response.create");
            assert_eq!(request["stream"], true);
            assert_eq!(request["stream_options"]["include_obfuscation"], false);
            assert!(request.get("background").is_none());
            websocket
                .send(Message::Text(
                    json!({"type":"response.completed","response":{"id":"response-1"}})
                        .to_string()
                        .into(),
                ))
                .await
                .expect("completion should send");
        });

        let mut session =
            ResponsesWebsocketSession::connect(test_config(format!("ws://{address}/v1/responses")))
                .await
                .expect("session should connect");
        session
            .begin_response(
                json!({
                    "model":"gpt-5",
                    "input":[],
                    "stream":true,
                    "stream_options":{"include_obfuscation":false},
                    "background":false
                }),
                None,
            )
            .await
            .expect("request should send");
        let frame = session
            .next_frame()
            .await
            .expect("frame should read")
            .expect("completion should exist");
        assert!(frame.completed);
        assert_eq!(session.previous_response_id(), Some("response-1"));
        assert!(!session.in_flight());
        server.await.expect("server should finish");
    }

    #[tokio::test]
    async fn websocket_rejects_multiplexed_requests() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("address should resolve");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("client should connect");
            let mut websocket = accept_async(stream).await.expect("upgrade should succeed");
            let _ = websocket.next().await;
        });
        let mut session =
            ResponsesWebsocketSession::connect(test_config(format!("ws://{address}/v1/responses")))
                .await
                .expect("session should connect");
        session
            .begin_response(json!({"model":"gpt-5","input":[]}), None)
            .await
            .expect("first request should send");
        let error = session
            .begin_response(json!({"model":"gpt-5","input":[]}), None)
            .await
            .expect_err("second in-flight request must fail");
        assert!(error.message.contains("multiplexed"));
        drop(session);
        server.await.expect("server should finish");
    }

    /// Verifies prewarm and one subsequent create reuse the same socket-owned response chain.
    /// 验证预热与后续一次创建会复用同一 Socket 持有的响应链。
    #[tokio::test]
    async fn websocket_prewarm_response_can_seed_one_incremental_create() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("address should resolve");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("client should connect");
            let mut websocket = accept_async(stream).await.expect("upgrade should succeed");
            let warmup = websocket
                .next()
                .await
                .expect("warmup should arrive")
                .expect("warmup should be readable");
            let Message::Text(warmup) = warmup else {
                panic!("warmup must be text");
            };
            let warmup: Value = serde_json::from_str(&warmup).expect("warmup should be JSON");
            assert_eq!(warmup["generate"], false);
            websocket
                .send(Message::Text(
                    json!({"type":"response.completed","response":{"id":"warm-1"}})
                        .to_string()
                        .into(),
                ))
                .await
                .expect("warmup completion should send");

            let create = websocket
                .next()
                .await
                .expect("create should arrive")
                .expect("create should be readable");
            let Message::Text(create) = create else {
                panic!("create must be text");
            };
            let create: Value = serde_json::from_str(&create).expect("create should be JSON");
            assert_eq!(create["previous_response_id"], "warm-1");
            assert!(create.get("generate").is_none());
            websocket
                .send(Message::Text(
                    json!({"type":"response.completed","response":{"id":"response-2"}})
                        .to_string()
                        .into(),
                ))
                .await
                .expect("completion should send");
        });

        let mut session =
            ResponsesWebsocketSession::connect(test_config(format!("ws://{address}/v1/responses")))
                .await
                .expect("session should connect");
        session
            .begin_response(json!({"model":"gpt-5","input":[]}), Some(false))
            .await
            .expect("warmup should send");
        assert!(
            session
                .next_frame()
                .await
                .expect("warmup frame should read")
                .expect("warmup completion should exist")
                .completed
        );
        assert_eq!(session.previous_response_id(), Some("warm-1"));
        session
            .begin_response(
                json!({"model":"gpt-5","input":[],"previous_response_id":"warm-1"}),
                None,
            )
            .await
            .expect("incremental create should send");
        assert!(
            session
                .next_frame()
                .await
                .expect("completion frame should read")
                .expect("completion should exist")
                .completed
        );
        assert_eq!(session.previous_response_id(), Some("response-2"));
        server.await.expect("server should finish");
    }

    /// Verifies the configured maximum age proactively retires a still-open connection.
    /// 验证配置的最大连接年龄会主动淘汰仍保持打开的连接。
    #[tokio::test]
    async fn websocket_requires_replacement_after_maximum_age() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("address should resolve");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("client should connect");
            let mut websocket = accept_async(stream).await.expect("upgrade should succeed");
            let _ = websocket.next().await;
        });
        let mut config = test_config(format!("ws://{address}/v1/responses"));
        config.max_connection_age = Duration::from_millis(1);
        let session = ResponsesWebsocketSession::connect(config)
            .await
            .expect("session should connect");
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert!(session.replacement_required());
        drop(session);
        server.await.expect("server should finish");
    }

    /// Verifies close code 1013 is preserved and invalidates all socket continuation state.
    /// 验证关闭码 1013 会被保留并使全部 Socket 延续状态失效。
    #[tokio::test]
    async fn websocket_preserves_try_again_later_close_code() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let address = listener.local_addr().expect("address should resolve");
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("client should connect");
            let mut websocket = accept_async(stream).await.expect("upgrade should succeed");
            let _ = websocket.next().await;
            websocket
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::Again,
                    reason: "try later".into(),
                })))
                .await
                .expect("close should send");
        });
        let mut session =
            ResponsesWebsocketSession::connect(test_config(format!("ws://{address}/v1/responses")))
                .await
                .expect("session should connect");
        session
            .begin_response(json!({"model":"gpt-5","input":[]}), None)
            .await
            .expect("request should send");
        let error = session
            .next_frame()
            .await
            .expect_err("1013 close must fail the response");
        assert_eq!(error.websocket_close_code, Some(1013));
        assert!(session.replacement_required());
        assert_eq!(session.previous_response_id(), None);
        server.await.expect("server should finish");
    }

    /// Verifies connection-limit and missing-chain controls have distinct replacement semantics.
    /// 验证连接时限与链缺失控制错误具有不同的连接换代语义。
    #[tokio::test]
    async fn websocket_control_errors_invalidate_chain_and_only_limit_replaces_socket() {
        for (code, replacement_required) in [
            (RESPONSES_WEBSOCKET_CONNECTION_LIMIT_CODE, true),
            ("previous_response_not_found", false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0")
                .await
                .expect("listener should bind");
            let address = listener.local_addr().expect("address should resolve");
            let server = tokio::spawn(async move {
                let (stream, _) = listener.accept().await.expect("client should connect");
                let mut websocket = accept_async(stream).await.expect("upgrade should succeed");
                let _ = websocket.next().await;
                websocket
                    .send(Message::Text(
                        json!({"type":"error","error":{"code":code,"message":"retry"}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .expect("control error should send");
            });
            let mut session = ResponsesWebsocketSession::connect(test_config(format!(
                "ws://{address}/v1/responses"
            )))
            .await
            .expect("session should connect");
            session
                .begin_response(json!({"model":"gpt-5","input":[]}), None)
                .await
                .expect("request should send");
            let frame = session
                .next_frame()
                .await
                .expect("control frame should read")
                .expect("control frame should exist");
            assert!(frame.failed);
            assert_eq!(session.previous_response_id(), None);
            assert_eq!(session.replacement_required(), replacement_required);
            server.await.expect("server should finish");
        }
    }
}
