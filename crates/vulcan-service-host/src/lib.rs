//! HTTP and WebSocket host for the Vulcan Core Service.
//! Vulcan Core Service 的 HTTP 与 WebSocket 宿主层。

use std::{
    fs,
    future::Future,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    body::Body as AxumBody,
    extract::{
        OriginalUri as AxumOriginalUri, Path as AxumPath, State as AxumState,
        ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap as AxumHeaderMap, Method as AxumMethod, StatusCode as AxumStatusCode},
    response::{IntoResponse, Response as AxumResponse},
    routing::{get, post},
};
use tokio::{net::TcpListener, sync::broadcast};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use vulcan_service_api::{
    ServiceEvent, ServiceEventSink, ServiceMediaResponse, ServiceStaticResponse,
    WebuiServiceDiscoverySnapshot, WebuiServiceRpcHttpRequest, build_service_media_file_response,
    build_webui_static_response, service_media_error_response,
    service_media_path_from_encoded_route_segment, webui_service_discovery_snapshot_path,
    webui_service_http_bind_ports_from_override, webui_service_origin_text_is_allowed,
    webui_service_request_token_values_are_valid, webui_service_required_origin_text_is_allowed,
    webui_service_rpc_error_envelope, webui_service_rpc_success_envelope,
};

/// Boxed asynchronous result returned by a browser-facing RPC dispatcher.
/// 浏览器侧 RPC 分发器返回的装箱异步结果。
pub type WebuiServiceRpcFuture =
    Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send + 'static>>;

/// Transport-independent browser-facing RPC dispatcher.
/// 传输无关的浏览器侧 RPC 分发器。
pub type WebuiServiceRpcDispatcher =
    Arc<dyn Fn(String, Option<serde_json::Value>) -> WebuiServiceRpcFuture + Send + Sync + 'static>;

/// Shared service discovery snapshot used by health checks while readiness changes.
/// 健康检查在就绪状态变化时使用的共享服务发现快照。
pub type SharedWebuiServiceDiscoverySnapshot = Arc<Mutex<WebuiServiceDiscoverySnapshot>>;

/// Service event sink that forwards events into the browser WebSocket bus.
/// 将服务事件转发到浏览器 WebSocket 总线的事件接收器。
struct WebuiServiceEventBusSink {
    /// Broadcast sender shared by all browser event subscribers.
    /// 所有浏览器事件订阅者共享的广播发送端。
    sender: broadcast::Sender<ServiceEvent>,
}

impl ServiceEventSink for WebuiServiceEventBusSink {
    /// Emits one event to every connected WebSocket subscriber.
    /// 将单个事件发送给所有已连接的 WebSocket 订阅者。
    fn emit_service_event(&self, event: ServiceEvent) {
        let _ = self.sender.send(event);
    }
}

/// Builds a service event sink backed by one browser WebSocket broadcast bus.
/// 基于单个浏览器 WebSocket 广播总线构建服务事件接收器。
///
/// # Parameters
/// 参数
/// `sender` is the broadcast sender that owns the browser event stream.
/// `sender` 是承载浏览器事件流的广播发送端。
///
/// # Returns
/// 返回值
/// A cloneable sink object that can be attached to a `ServiceEventEmitter`.
/// 返回可挂载到 `ServiceEventEmitter` 的可克隆 sink 对象。
pub fn webui_service_event_bus_sink(
    sender: broadcast::Sender<ServiceEvent>,
) -> Arc<dyn ServiceEventSink> {
    Arc::new(WebuiServiceEventBusSink { sender })
}

/// Binds the browser-facing service to the first available loopback port.
/// 将浏览器侧服务绑定到首个可用回环端口。
///
/// # Parameters
/// 参数
/// `port_override` is an optional environment override such as `VULCAN_WEBUI_SERVICE_PORT`.
/// `port_override` 是可选环境覆盖值，例如 `VULCAN_WEBUI_SERVICE_PORT`。
///
/// # Returns
/// 返回值
/// The bound listener and its resolved socket address, or a combined bind diagnostic.
/// 返回已绑定监听器及其解析后的套接字地址，或组合后的绑定诊断信息。
pub async fn bind_webui_service_http_listener(
    port_override: Option<&str>,
) -> Result<(TcpListener, SocketAddr), String> {
    let mut errors = Vec::new();
    for port in webui_service_http_bind_ports_from_override(port_override) {
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        match TcpListener::bind(address).await {
            Ok(listener) => {
                let local_address = listener
                    .local_addr()
                    .map_err(|error| format!("failed to read bound service address: {error}"))?;
                return Ok((listener, local_address));
            }
            Err(error) => {
                errors.push(format!("{address}: {error}"));
            }
        }
    }
    Err(errors.join("; "))
}

/// Persists the currently bound browser service endpoint for ordinary browser hosts.
/// 为普通浏览器宿主持久化当前已绑定的浏览器服务端点。
///
/// # Parameters
/// 参数
/// `snapshot` is the service discovery document, while `home_dir` and `temp_dir` resolve the file path.
/// `snapshot` 是服务发现文档，`home_dir` 与 `temp_dir` 用于解析文件路径。
///
/// # Returns
/// 返回值
/// `Ok(())` when the discovery file is written, otherwise a user-facing diagnostic.
/// 成功写入发现文件时返回 `Ok(())`，否则返回面向用户的诊断信息。
pub fn write_webui_service_discovery_snapshot(
    snapshot: &WebuiServiceDiscoverySnapshot,
    home_dir: Option<PathBuf>,
    temp_dir: PathBuf,
) -> Result<(), String> {
    let path = webui_service_discovery_snapshot_path(home_dir, temp_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create service discovery directory: {error}"))?;
    }
    let body = serde_json::to_string_pretty(snapshot)
        .map_err(|error| format!("failed to encode service discovery snapshot: {error}"))?;
    fs::write(&path, body).map_err(|error| format!("failed to write {}: {error}", path.display()))
}

/// Configuration required to host the browser-facing WebUI service.
/// 承载浏览器侧 WebUI 服务所需的配置。
#[derive(Clone)]
pub struct WebuiServiceHttpConfig {
    /// Discovery snapshot returned by `/health`.
    /// `/health` 返回的服务发现快照。
    pub discovery: SharedWebuiServiceDiscoverySnapshot,
    /// Instance access token required by RPC, events, and asset routes.
    /// RPC、事件和资源路由要求携带的实例访问令牌。
    pub service_token: String,
    /// Broadcast sender used by WebSocket event subscribers.
    /// WebSocket 事件订阅者使用的广播发送端。
    pub event_bus: broadcast::Sender<ServiceEvent>,
    /// Optional static bundle root used to serve the WebUI.
    /// 用于提供 WebUI 的可选静态包根目录。
    pub static_root: Option<PathBuf>,
    /// Business RPC dispatcher supplied by the service owner.
    /// 服务所有者提供的业务 RPC 分发器。
    pub rpc_dispatcher: WebuiServiceRpcDispatcher,
}

/// State captured by the browser-facing HTTP/WebSocket service host.
/// 浏览器侧 HTTP/WebSocket 服务宿主捕获的状态。
#[derive(Clone)]
struct WebuiServiceHttpState {
    /// Discovery snapshot returned by health checks and dev discovery files.
    /// 由健康检查和开发发现文件返回的服务发现快照。
    discovery: SharedWebuiServiceDiscoverySnapshot,
    /// Instance access token required by browser RPC, events, and asset routes.
    /// 浏览器 RPC、事件和资源路由要求携带的实例访问令牌。
    service_token: String,
    /// Broadcast sender used to attach WebSocket event subscribers.
    /// 用于挂载 WebSocket 事件订阅者的广播发送端。
    event_bus: broadcast::Sender<ServiceEvent>,
    /// Optional static bundle root used to serve the WebUI.
    /// 用于提供 WebUI 的可选静态包根目录。
    static_root: Option<PathBuf>,
    /// Business RPC dispatcher supplied by the service owner.
    /// 服务所有者提供的业务 RPC 分发器。
    rpc_dispatcher: WebuiServiceRpcDispatcher,
}

/// Serves one browser-facing WebUI service listener until it stops.
/// 持续服务一个浏览器侧 WebUI 服务监听器直到停止。
///
/// # Parameters
/// 参数
/// `listener` is the bound TCP listener, and `config` provides transport state and callbacks.
/// `listener` 是已绑定的 TCP 监听器，`config` 提供传输状态与回调。
///
/// # Returns
/// 返回值
/// `Ok(())` when the server exits cleanly, or the underlying listener error.
/// 服务正常退出时返回 `Ok(())`，否则返回底层监听错误。
pub async fn serve_webui_service_http_listener(
    listener: TcpListener,
    config: WebuiServiceHttpConfig,
) -> Result<(), std::io::Error> {
    axum::serve(listener, build_webui_service_router(config)).await
}

/// Builds the browser-facing WebUI service router.
/// 构建浏览器侧 WebUI 服务路由。
///
/// # Parameters
/// 参数
/// `config` contains discovery data, static root, event bus, token, and RPC dispatcher.
/// `config` 包含发现数据、静态根目录、事件总线、令牌与 RPC 分发器。
///
/// # Returns
/// 返回值
/// An Axum router ready to be served on a loopback listener.
/// 返回可在回环监听器上提供服务的 Axum 路由。
pub fn build_webui_service_router(config: WebuiServiceHttpConfig) -> Router {
    let service_state = WebuiServiceHttpState {
        discovery: config.discovery,
        service_token: config.service_token,
        event_bus: config.event_bus,
        static_root: config.static_root,
        rpc_dispatcher: config.rpc_dispatcher,
    };
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            origin
                .to_str()
                .ok()
                .is_some_and(webui_service_origin_text_is_allowed)
        }))
        .allow_methods([
            AxumMethod::GET,
            AxumMethod::HEAD,
            AxumMethod::POST,
            AxumMethod::OPTIONS,
        ])
        .allow_headers(Any);
    Router::new()
        .route("/health", get(webui_service_health_http_handler))
        .route("/rpc", post(webui_service_rpc_http_handler))
        .route("/events", get(webui_service_events_ws_handler))
        .route(
            "/asset/{encoded_path}",
            get(webui_service_asset_http_handler).head(webui_service_asset_head_http_handler),
        )
        .route("/", get(webui_service_index_http_handler))
        .route("/index.html", get(webui_service_index_http_handler))
        .fallback(get(webui_service_static_http_handler))
        .layer(cors)
        .with_state(service_state)
}

/// Returns a browser-facing health response.
/// 返回浏览器侧健康检查响应。
async fn webui_service_health_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
) -> Json<WebuiServiceDiscoverySnapshot> {
    Json(
        server_state
            .discovery
            .lock()
            .map(|snapshot| snapshot.clone())
            .unwrap_or_else(|error| error.into_inner().clone()),
    )
}

/// Handles one browser-facing JSON RPC request.
/// 处理单个浏览器侧 JSON RPC 请求。
async fn webui_service_rpc_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
    Json(request): Json<WebuiServiceRpcHttpRequest>,
) -> AxumResponse {
    if !webui_service_request_token_is_valid(&headers, uri.0.query(), &server_state.service_token) {
        return webui_service_rpc_error_response(
            AxumStatusCode::UNAUTHORIZED,
            "unauthorized",
            "Browser service token is missing or invalid.",
        );
    }
    let result = (server_state.rpc_dispatcher)(request.method, request.params).await;
    Json(match result {
        Ok(result) => webui_service_rpc_success_envelope(result),
        Err(message) => webui_service_rpc_error_envelope("service_rpc_error", message),
    })
    .into_response()
}

/// Builds one browser-facing JSON RPC error response with a concrete HTTP status.
/// 使用明确 HTTP 状态构建单个浏览器侧 JSON RPC 错误响应。
///
/// # Parameters
/// 参数
/// `status` is the HTTP status code, `code` is the stable error code, and `message` is the human-readable error.
/// `status` 是 HTTP 状态码，`code` 是稳定错误码，`message` 是人类可读错误。
///
/// # Returns
/// 返回值
/// An Axum response containing the normalized RPC envelope.
/// 返回包含标准化 RPC 信封的 Axum 响应。
fn webui_service_rpc_error_response(
    status: AxumStatusCode,
    code: &str,
    message: &str,
) -> AxumResponse {
    (
        status,
        Json(webui_service_rpc_error_envelope(code, message)),
    )
        .into_response()
}

/// Upgrades one browser event subscription to a WebSocket stream.
/// 将单个浏览器事件订阅升级为 WebSocket 流。
async fn webui_service_events_ws_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
    websocket: WebSocketUpgrade,
) -> impl IntoResponse {
    if !webui_service_headers_origin_is_allowed(&headers) {
        return AxumStatusCode::FORBIDDEN.into_response();
    }
    if !webui_service_request_token_is_valid(&headers, uri.0.query(), &server_state.service_token) {
        return AxumStatusCode::UNAUTHORIZED.into_response();
    }
    websocket.on_upgrade(move |socket| {
        webui_service_events_socket(socket, server_state.event_bus.subscribe())
    })
}

/// Returns whether one browser WebSocket handshake carries an allowed or host-omitted Origin header.
/// 返回单个浏览器 WebSocket 握手是否携带允许来源，或由宿主省略 Origin 请求头。
///
/// # Parameters
/// 参数
/// `headers` contains the incoming WebSocket upgrade request headers.
/// `headers` 包含传入 WebSocket 升级请求头。
///
/// # Returns
/// 返回值
/// `true` when Origin is absent for a token-protected WebView handshake or points to an allowed loopback origin.
/// 当受令牌保护的 WebView 握手省略 Origin，或 Origin 指向允许的回环来源时返回 `true`。
fn webui_service_headers_origin_is_allowed(headers: &AxumHeaderMap) -> bool {
    let origin = headers.get("origin").and_then(|value| value.to_str().ok());
    origin.is_none_or(|value| webui_service_required_origin_text_is_allowed(Some(value)))
}

/// Returns whether one browser service request carries the current instance token.
/// 返回单个浏览器服务请求是否携带当前实例令牌。
///
/// # Parameters
/// 参数
/// `headers` and `query` contain the incoming request token locations, and `expected_token` is the generated instance token.
/// `headers` 和 `query` 包含传入请求的令牌位置，`expected_token` 是生成的实例令牌。
///
/// # Returns
/// 返回值
/// `true` when either header or query token matches the expected token.
/// 当 header 或 query 中的令牌与期望令牌匹配时返回 `true`。
fn webui_service_request_token_is_valid(
    headers: &AxumHeaderMap,
    query: Option<&str>,
    expected_token: &str,
) -> bool {
    let header_token = headers
        .get("x-vulcan-service-token")
        .and_then(|value| value.to_str().ok());
    webui_service_request_token_values_are_valid(header_token, query, expected_token)
}

/// Streams serialized service events to one browser WebSocket.
/// 向单个浏览器 WebSocket 推送序列化后的服务事件。
async fn webui_service_events_socket(
    mut socket: WebSocket,
    mut receiver: broadcast::Receiver<ServiceEvent>,
) {
    loop {
        let event = match receiver.recv().await {
            Ok(event) => event,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        };
        let text = match serde_json::to_string(&event) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("[service-host] failed to serialize browser service event: {error}");
                continue;
            }
        };
        if socket.send(AxumWsMessage::Text(text.into())).await.is_err() {
            break;
        }
    }
}

/// Serves the WebUI index document from the browser-facing service origin.
/// 从面向浏览器的服务同源地址提供 WebUI 入口文档。
async fn webui_service_index_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
) -> AxumResponse {
    axum_response_from_service_static_response(build_webui_static_response(
        server_state.static_root.as_deref(),
        "/",
        "no-cache",
        "Vulcan WebUI static bundle is not available.",
    ))
}

/// Serves WebUI static files and falls back to the SPA index for browser routes.
/// 提供 WebUI 静态文件，并对浏览器路由回退到 SPA 入口。
async fn webui_service_static_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumOriginalUri(uri): AxumOriginalUri,
) -> AxumResponse {
    axum_response_from_service_static_response(build_webui_static_response(
        server_state.static_root.as_deref(),
        uri.path(),
        "public, max-age=31536000, immutable",
        "Not found.",
    ))
}

/// Converts the shared static response into an Axum response body.
/// 将共享静态响应转换为 Axum 响应体。
fn axum_response_from_service_static_response(response: ServiceStaticResponse) -> AxumResponse {
    let mut builder = AxumResponse::builder().status(response.status);
    for (key, value) in response.headers {
        builder = builder.header(key, value);
    }
    builder
        .body(AxumBody::from(response.body))
        .unwrap_or_else(|_| AxumStatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Serves one browser-facing media asset by encoded local path.
/// 按编码后的本地路径提供单个浏览器侧媒体资源。
async fn webui_service_asset_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumPath(encoded_path): AxumPath<String>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
) -> AxumResponse {
    webui_service_asset_http_response(server_state, encoded_path, headers, uri, false)
}

/// Serves one browser-facing media asset metadata response by encoded local path.
/// 按编码后的本地路径提供单个浏览器侧媒体资源元数据响应。
async fn webui_service_asset_head_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumPath(encoded_path): AxumPath<String>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
) -> AxumResponse {
    webui_service_asset_http_response(server_state, encoded_path, headers, uri, true)
}

/// Builds one browser-facing media asset response shared by GET and HEAD.
/// 构建由 GET 与 HEAD 共用的单个浏览器侧媒体资源响应。
///
/// # Parameters
/// 参数
/// `server_state` carries the service token, `encoded_path` is the URL-safe media path, `headers` and `uri` contain request metadata, and `is_head` controls whether the body is omitted.
/// `server_state` 携带服务令牌，`encoded_path` 是 URL 安全媒体路径，`headers` 与 `uri` 包含请求元数据，`is_head` 控制是否省略响应体。
///
/// # Returns
/// 返回值
/// An HTTP response containing media bytes, media metadata, or a normalized media error.
/// 返回包含媒体字节、媒体元数据或标准化媒体错误的 HTTP 响应。
fn webui_service_asset_http_response(
    server_state: WebuiServiceHttpState,
    encoded_path: String,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
    is_head: bool,
) -> AxumResponse {
    if !webui_service_request_token_is_valid(&headers, uri.0.query(), &server_state.service_token) {
        return axum_response_from_service_media_response(service_media_error_response(
            AxumStatusCode::UNAUTHORIZED.as_u16(),
            "Browser service token is missing or invalid.",
        ));
    }
    let media_path = match service_media_path_from_encoded_route_segment(&encoded_path) {
        Ok(path) => path,
        Err(error) => {
            return axum_response_from_service_media_response(service_media_error_response(
                AxumStatusCode::BAD_REQUEST.as_u16(),
                &error,
            ));
        }
    };
    let range_header = headers.get("range").and_then(|value| value.to_str().ok());
    let response = match build_service_media_file_response(media_path, is_head, range_header) {
        Ok(response) => response,
        Err(error) => {
            service_media_error_response(AxumStatusCode::INTERNAL_SERVER_ERROR.as_u16(), &error)
        }
    };
    axum_response_from_service_media_response(response)
}

/// Converts the shared media response into an Axum response body.
/// 将共享媒体响应转换为 Axum 响应体。
fn axum_response_from_service_media_response(response: ServiceMediaResponse) -> AxumResponse {
    let mut builder = AxumResponse::builder().status(response.status);
    for (key, value) in response.headers {
        builder = builder.header(key, value);
    }
    builder
        .body(AxumBody::from(response.body))
        .unwrap_or_else(|_| AxumStatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use serde_json::json;
    use tower::ServiceExt;
    use vulcan_service_api::webui_service_ready_discovery_snapshot;

    /// Builds a minimal test WebUI service router.
    /// 构建最小化测试 WebUI 服务路由。
    fn test_router() -> Router {
        let (event_bus, _) = broadcast::channel(8);
        let dispatcher: WebuiServiceRpcDispatcher = Arc::new(|method, params| {
            Box::pin(async move {
                Ok(json!({
                    "method": method,
                    "params": params,
                }))
            })
        });
        build_webui_service_router(WebuiServiceHttpConfig {
            discovery: Arc::new(Mutex::new(webui_service_ready_discovery_snapshot(
                17891,
                42,
                "123",
                "test-token",
            ))),
            service_token: String::from("test-token"),
            event_bus,
            static_root: None,
            rpc_dispatcher: dispatcher,
        })
    }

    /// Verifies browser RPC rejects requests without the service token.
    /// 验证浏览器 RPC 会拒绝缺少服务令牌的请求。
    #[tokio::test]
    async fn browser_rpc_rejects_missing_token() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"method":"startup.snapshot","params":null}"#))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), AxumStatusCode::UNAUTHORIZED);
    }

    /// Verifies browser RPC dispatches authorized requests through the configured handler.
    /// 验证浏览器 RPC 会通过配置的处理器分发已授权请求。
    #[tokio::test]
    async fn browser_rpc_dispatches_authorized_request() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::from(
                        r#"{"method":"workbench.snapshot","params":{"projectId":"p1"}}"#,
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), AxumStatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response should be JSON");
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["result"]["method"], "workbench.snapshot");
        assert_eq!(payload["result"]["params"]["projectId"], "p1");
    }

    /// Verifies token-protected WebSocket handshakes tolerate host WebViews that omit Origin.
    /// 验证受令牌保护的 WebSocket 握手兼容省略 Origin 的宿主 WebView。
    #[test]
    fn browser_events_allows_missing_origin() {
        let headers = AxumHeaderMap::new();

        assert!(webui_service_headers_origin_is_allowed(&headers));
    }

    /// Verifies hostile WebSocket origins are still rejected.
    /// 验证恶意 WebSocket 来源仍会被拒绝。
    #[test]
    fn browser_events_rejects_disallowed_origin() {
        let mut headers = AxumHeaderMap::new();
        headers.insert("origin", "https://example.com".parse().unwrap());

        assert!(!webui_service_headers_origin_is_allowed(&headers));
    }

    /// Verifies missing Origin does not bypass the WebSocket service token check.
    /// 验证缺少 Origin 不会绕过 WebSocket 服务令牌检查。
    #[test]
    fn browser_events_still_requires_service_token_without_origin() {
        let headers = AxumHeaderMap::new();

        assert!(!webui_service_request_token_is_valid(
            &headers,
            None,
            "test-token",
        ));
        assert!(webui_service_request_token_is_valid(
            &headers,
            Some("token=test-token"),
            "test-token",
        ));
    }
}
