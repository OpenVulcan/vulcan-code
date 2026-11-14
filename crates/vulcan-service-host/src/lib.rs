//! HTTP and WebSocket host for the Vulcan Core Service.
//! Vulcan Core Service 的 HTTP 与 WebSocket 宿主层。

pub mod instance;
mod rpc_admission;

/// Exact developer workbench origin shared by health admission and nested HTML viewer policies.
/// 健康准入与嵌套 HTML 查看器策略共用的精确开发工作台来源。
const DEV_WEBUI_ORIGIN: &str = "http://127.0.0.1:17601";

use std::{
    collections::{HashMap, VecDeque},
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
        DefaultBodyLimit, OriginalUri as AxumOriginalUri, Path as AxumPath, State as AxumState,
        ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap as AxumHeaderMap, Method as AxumMethod, StatusCode as AxumStatusCode},
    response::{IntoResponse, Response as AxumResponse},
    routing::{get, post},
};
use futures_util::{SinkExt, StreamExt};
use tokio::{net::TcpListener, sync::broadcast};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use uuid::Uuid;
use vulcan_service_api::{
    ConversationEventLiveDeltaAccumulatorV1, ServiceEvent, ServiceEventSink, ServiceMediaResponse,
    ServiceStaticResponse, ServiceStatusSnapshot, SessionResourceContentV1Request,
    WEBUI_SERVICE_RPC_BODY_LIMIT_BYTES, WebuiServiceDiscoverySnapshot, WebuiServiceRpcHttpRequest,
    WebuiServiceSettings, build_service_media_file_response, build_webui_static_response,
    service_media_error_response, service_media_path_from_encoded_route_segment,
    webui_service_discovery_snapshot_path, webui_service_http_bind_ports,
    webui_service_origin_text_is_allowed, webui_service_request_token_values_are_valid,
    webui_service_required_origin_text_is_allowed, webui_service_rpc_error_envelope,
    webui_service_rpc_failure_envelope, webui_service_rpc_success_envelope,
};

/// Boxed asynchronous result returned by a browser-facing RPC dispatcher.
/// 浏览器侧 RPC 分发器返回的装箱异步结果。
pub type WebuiServiceRpcFuture = Pin<
    Box<
        dyn Future<Output = Result<serde_json::Value, vulcan_service_api::WebuiServiceRpcError>>
            + Send
            + 'static,
    >,
>;

/// Transport-independent browser-facing RPC dispatcher.
/// 传输无关的浏览器侧 RPC 分发器。
pub type WebuiServiceRpcDispatcher =
    Arc<dyn Fn(String, Option<serde_json::Value>) -> WebuiServiceRpcFuture + Send + Sync + 'static>;

/// Boxed asynchronous result returned by the controlled session-resource resolver.
/// 受控会话资源解析器返回的装箱异步结果。
pub type WebuiSessionResourceFuture =
    Pin<Box<dyn Future<Output = Result<ServiceMediaResponse, String>> + Send + 'static>>;

/// Resolves one opaque session/resource identity into browser-safe content bytes.
/// 将一个不透明会话/资源身份解析为浏览器安全内容字节。
pub type WebuiSessionResourceResolver = Arc<
    dyn Fn(SessionResourceContentV1Request) -> WebuiSessionResourceFuture + Send + Sync + 'static,
>;

/// Resolves one opaque browser download into an attachment response.
/// 将一项不透明浏览器下载解析为附件响应。
pub type WebuiBrowserDownloadResolver =
    Arc<dyn Fn(String, String, bool) -> WebuiSessionResourceFuture + Send + Sync + 'static>;

/// Answers one MCP POST from its `Authorization` header value and raw JSON body.
/// 根据 `Authorization` 标头值与原始 JSON 请求体应答一次 MCP POST。
pub type WebuiMcpEndpoint =
    Arc<dyn Fn(Option<String>, Vec<u8>) -> WebuiSessionResourceFuture + Send + Sync + 'static>;

/// Cleans client-owned display subscriptions when one browser event socket disconnects.
/// 在浏览器事件套接字断开时清理客户端持有的显示订阅。
pub type WebuiServiceClientDisconnected = Arc<dyn Fn(Uuid) + Send + Sync + 'static>;

/// Detailed service status snapshot supplied by the service owner.
/// 服务所有者提供的详细服务状态快照。
pub type WebuiServiceStatusProvider =
    Arc<dyn Fn() -> ServiceStatusSnapshot + Send + Sync + 'static>;

/// Boxed asynchronous result returned by an injected browser stream operation.
/// 注入的浏览器流操作返回的装箱异步结果。
pub type WebuiServiceBrowserFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value, String>> + Send>>;

/// Kernel-supplied browser stream operations exposed on the typed browser routes.
/// 内核提供的浏览器流操作，经类型化浏览器路由暴露。
///
/// The host deliberately receives plain JSON rather than kernel types, so the transport layer keeps
/// no reverse dependency on the service kernel.
/// 宿主有意接收普通 JSON 而不是内核类型，使传输层不反向依赖服务内核。
#[derive(Clone)]
pub struct WebuiServiceBrowserStream {
    /// Issues one single-use observation stream ticket for a session owned by the stated conversation.
    /// 为所声明会话拥有的浏览器签发一次性观察流凭据。
    pub issue_ticket:
        Arc<dyn Fn(String, String) -> WebuiServiceBrowserFuture + Send + Sync + 'static>,
    /// Resumes one page-specific observation stream from a ticket and optional cursor.
    /// 依据凭据与可选游标恢复一条指定页面的观察流。
    pub resume: Arc<
        dyn Fn(String, String, String, Option<serde_json::Value>) -> WebuiServiceBrowserFuture
            + Send
            + Sync
            + 'static,
    >,
    /// Reads the frames a session has produced since one cursor, without consuming a ticket.
    /// 读取某个会话自某游标以来产生的帧，且不消费凭据。
    ///
    /// This backs the continuing stream after the one-time ticket already authorised the session.
    /// 一次性凭据已授权该会话后，由此支撑持续流。
    pub poll: Arc<
        dyn Fn(String, String, serde_json::Value) -> WebuiServiceBrowserFuture
            + Send
            + Sync
            + 'static,
    >,
    /// Releases the observation subscription that a finished stream owned.
    /// 释放已结束的流所拥有的观察订阅。
    ///
    /// A disconnecting observer releases only its own subscription, so closing one observer leaves
    /// the session and every other observer untouched.
    /// 断开的观察端只释放它自己的订阅，因此关闭一个观察端不会影响会话与其他任何观察端。
    pub release: Arc<dyn Fn(String, String) -> WebuiServiceBrowserFuture + Send + Sync + 'static>,
}

/// Shared service discovery snapshot used by health checks while readiness changes.
/// 健康检查在就绪状态变化时使用的共享服务发现快照。
pub type SharedWebuiServiceDiscoverySnapshot = Arc<Mutex<WebuiServiceDiscoverySnapshot>>;

/// Maximum JSON-RPC body accepted by the MCP endpoint; tool arguments are small JSON documents.
/// MCP 端点允许接收的最大 JSON-RPC 请求体；工具参数都是小型 JSON 文档。
const WEBUI_SERVICE_MCP_BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;

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

/// Capacity of the browser service event bus, and of each event socket's pending outbox.
/// 浏览器服务事件总线的容量，同时也是每个事件套接字待发送队列的容量。
///
/// A socket that falls this far behind can no longer be caught up event by event and is closed
/// so the browser reconnects and requests authoritative baselines.
/// 落后达到该数量的套接字已无法逐条追平，会被关闭，使浏览器重连并请求权威基线。
pub const WEBUI_SERVICE_EVENT_BUS_CAPACITY: usize = 256;

/// Serialized bytes one event socket may hold pending send before it is closed for recovery.
/// 单个事件套接字在被关闭以恢复之前可积压的待发送序列化字节数。
///
/// An empty outbox always admits one event, so a single large event cannot cause a reconnect loop.
/// 空的待发送队列总会接纳一个事件，因此单个大事件不会造成重连循环。
const WEBUI_SERVICE_EVENT_OUTBOX_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Serialized bytes a merged conversation delta may grow to; a delta that would push it past this
/// size queues as the next pending delta instead, so no single message grows without bound.
/// 合并后的会话增量可增长到的序列化字节数；会使其超出该大小的增量改为作为下一条待发送增量
/// 排队，因此单条消息不会无界增长。
const WEBUI_SERVICE_EVENT_MERGED_DELTA_MAX_BYTES: usize = 256 * 1024;

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
/// `port_override` is an optional environment override such as `VULCAN_WEBUI_SERVICE_PORT`, and
/// `settings` holds the configured port range tried otherwise.
/// `port_override` 是可选环境覆盖值，例如 `VULCAN_WEBUI_SERVICE_PORT`；`settings` 保存未覆盖时尝试的
/// 已配置端口范围。
///
/// # Returns
/// 返回值
/// The bound listener and its resolved socket address, or a combined bind diagnostic.
/// 返回已绑定监听器及其解析后的套接字地址，或组合后的绑定诊断信息。
pub async fn bind_webui_service_http_listener(
    port_override: Option<&str>,
    settings: &WebuiServiceSettings,
) -> Result<(TcpListener, SocketAddr), String> {
    let mut errors = Vec::new();
    for port in webui_service_http_bind_ports(port_override, settings) {
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
    use std::io::Write;
    use std::time::{Duration, Instant};
    // A same-directory temporary file ensures readers observe complete JSON generations.
    // 同目录临时文件保证读取方只能观察到完整的 JSON 代际。
    let path = webui_service_discovery_snapshot_path(home_dir, temp_dir);
    let parent = path
        .parent()
        .ok_or("Service discovery path has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut pending = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(&mut pending, snapshot).map_err(|error| error.to_string())?;
    pending.flush().map_err(|error| error.to_string())?;
    pending
        .as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    // Windows readers can briefly deny replacement of the old generation. PersistError
    // returns the same complete temporary file, so retry only those transient OS errors.
    // Windows 读者可能短暂阻止替换旧代际；PersistError 会归还同一完整临时文件，
    // 因此只对这两种瞬时系统错误重试，绝不重新生成半成品。
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match pending.persist(&path) {
            Ok(_) => return Ok(()),
            Err(error) => {
                let transient_windows_reader =
                    cfg!(windows) && matches!(error.error.raw_os_error(), Some(5 | 32));
                if !transient_windows_reader || Instant::now() >= deadline {
                    return Err(format!("Failed to publish {}: {error}", path.display()));
                }
                pending = error.file;
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
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
    /// Kernel-owned resolver for V1 project resource content.
    /// 内核拥有的 V1 项目资源内容解析器。
    pub session_resource_resolver: WebuiSessionResourceResolver,
    /// Kernel callback that releases display subscriptions owned by a disconnected browser.
    /// 内核回调，用于释放由已断开浏览器持有的显示订阅。
    pub client_disconnected: WebuiServiceClientDisconnected,
    /// Owner-supplied detailed lifecycle snapshot served by the protected status route.
    /// 所有者提供的详细生命周期快照，由受保护状态路由提供。
    pub status_provider: Option<WebuiServiceStatusProvider>,
    /// Optional kernel-supplied browser stream operations served by the typed browser routes.
    /// 可选的内核提供浏览器流操作，由类型化浏览器路由提供。
    pub browser_stream: Option<WebuiServiceBrowserStream>,
    /// Kernel-owned resolver for authenticated Core browser downloads.
    /// 用于经过鉴权的 Core 浏览器下载的内核自有解析器。
    pub browser_download_resolver: Option<WebuiBrowserDownloadResolver>,
    /// Kernel-owned MCP endpoint; `None` answers `/mcp` with `404`.
    /// 内核拥有的 MCP 端点；为 `None` 时 `/mcp` 返回 `404`。
    pub mcp_endpoint: Option<WebuiMcpEndpoint>,
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
    /// Kernel-owned resolver for V1 project resource content.
    /// 内核拥有的 V1 项目资源内容解析器。
    session_resource_resolver: WebuiSessionResourceResolver,
    /// Kernel callback that releases display subscriptions owned by a disconnected browser.
    /// 内核回调，用于释放由已断开浏览器持有的显示订阅。
    client_disconnected: WebuiServiceClientDisconnected,
    /// Owner-supplied detailed lifecycle snapshot served by the protected status route.
    /// 所有者提供的详细生命周期快照，由受保护状态路由提供。
    status_provider: Option<WebuiServiceStatusProvider>,
    /// Optional kernel-supplied browser stream operations served by the typed browser routes.
    /// 可选的内核提供浏览器流操作，由类型化浏览器路由提供。
    browser_stream: Option<WebuiServiceBrowserStream>,
    /// Kernel-owned resolver for authenticated Core browser downloads.
    /// 用于经过鉴权的 Core 浏览器下载的内核自有解析器。
    browser_download_resolver: Option<WebuiBrowserDownloadResolver>,
    /// Kernel-owned MCP endpoint.
    /// 内核拥有的 MCP 端点。
    mcp_endpoint: Option<WebuiMcpEndpoint>,
    /// Latest event-socket generation for each browser lifetime.
    /// 每个浏览器生命周期的最新事件套接字代次。
    client_connections: Arc<Mutex<HashMap<Uuid, Uuid>>>,
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
        session_resource_resolver: config.session_resource_resolver,
        client_disconnected: config.client_disconnected,
        status_provider: config.status_provider,
        browser_stream: config.browser_stream,
        browser_download_resolver: config.browser_download_resolver,
        mcp_endpoint: config.mcp_endpoint,
        client_connections: Arc::new(Mutex::new(HashMap::new())),
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
        .route("/v1/service/status", get(webui_service_status_http_handler))
        .route(
            "/v1/browser/downloads/{download_id}/content",
            get(webui_service_browser_download_http_handler)
                .head(webui_service_browser_download_head_http_handler),
        )
        .route(
            "/rpc",
            post(webui_service_rpc_http_handler)
                .layer(DefaultBodyLimit::max(WEBUI_SERVICE_RPC_BODY_LIMIT_BYTES)),
        )
        .route(
            "/mcp",
            post(webui_service_mcp_http_handler)
                .get(webui_service_mcp_method_not_allowed)
                .delete(webui_service_mcp_method_not_allowed)
                .layer(DefaultBodyLimit::max(WEBUI_SERVICE_MCP_BODY_LIMIT_BYTES)),
        )
        .route("/events", get(webui_service_events_ws_handler))
        .route(
            "/v1/browser/stream-ticket",
            post(webui_service_browser_stream_ticket_http_handler),
        )
        .route(
            "/v1/browser/stream",
            get(webui_service_browser_stream_ws_handler),
        )
        .route(
            "/v1/browser/stream/snapshot",
            get(webui_service_browser_stream_http_handler),
        )
        .route(
            "/asset/{encoded_path}",
            get(webui_service_asset_http_handler).head(webui_service_asset_head_http_handler),
        )
        .route(
            "/v1/session-resources/{session_id}/{resource_id}",
            get(webui_service_session_resource_http_handler)
                .head(webui_service_session_resource_head_http_handler),
        )
        .route("/", get(webui_service_index_http_handler))
        .route("/index.html", get(webui_service_index_http_handler))
        .fallback(get(webui_service_static_http_handler))
        .layer(cors)
        .layer(axum::middleware::from_fn(
            webui_service_loopback_host_middleware,
        ))
        .with_state(service_state)
}

/// Refuses any request whose `Host` (or absolute-form authority) does not name the loopback
/// interface, so a page that rebinds its own DNS name to 127.0.0.1 cannot reach any route.
/// 拒绝 `Host`（或绝对形式的 authority）不指向回环接口的任何请求，使把自身 DNS 名重新绑定到
/// 127.0.0.1 的页面无法访问任何路由。
async fn webui_service_loopback_host_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> AxumResponse {
    let mut hosts = request.headers().get_all(axum::http::header::HOST).iter();
    let host_allowed = match (hosts.next(), hosts.next()) {
        (None, _) => true,
        (Some(host), None) => host
            .to_str()
            .is_ok_and(vulcan_service_api::webui_service_host_header_is_allowed),
        (Some(_), Some(_)) => false,
    };
    let authority_allowed = request.uri().authority().is_none_or(|authority| {
        vulcan_service_api::webui_service_host_header_is_allowed(authority.as_str())
    });
    if !host_allowed || !authority_allowed {
        return AxumStatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

/// Returns discovery only to the service WebUI, the fixed debug Vite WebUI, or a native client.
/// 仅向服务 WebUI、固定的调试 Vite WebUI 或原生客户端返回发现信息。
///
/// # Parameters / 参数
/// `server_state` owns the current discovery snapshot; `headers` identify a browser caller's Origin.
/// `server_state` 持有当前发现快照；`headers` 标识浏览器调用方的 Origin。
///
/// # Returns / 返回值
/// A non-cacheable snapshot for an allowed caller, or `403` without the instance token.
/// 允许的调用方获得不可缓存快照，否则返回不包含实例令牌的 `403`。
async fn webui_service_health_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
) -> AxumResponse {
    let snapshot = server_state
        .discovery
        .lock()
        .map(|snapshot| snapshot.clone())
        .unwrap_or_else(|error| error.into_inner().clone());
    if !webui_service_health_origin_is_allowed(&headers, &snapshot.base_url) {
        return AxumStatusCode::FORBIDDEN.into_response();
    }
    ([("cache-control", "no-store")], Json(snapshot)).into_response()
}

/// Allows token-bearing discovery only from the current WebUI origin or the fixed debug Vite origin.
/// 仅允许当前 WebUI 来源或固定调试 Vite 来源获取含令牌的发现信息。
///
/// # Parameters / 参数
/// `headers` contains at most one Origin; `service_origin` is the bound listener's exact base URL.
/// `headers` 最多包含一个 Origin；`service_origin` 是已绑定监听器的精确基础地址。
///
/// # Returns / 返回值
/// `true` for native requests without Origin and exact trusted browser origins.
/// 对不带 Origin 的原生请求和精确受信任的浏览器来源返回 `true`。
fn webui_service_health_origin_is_allowed(headers: &AxumHeaderMap, service_origin: &str) -> bool {
    let mut origins = headers.get_all("origin").iter();
    let Some(origin) = origins.next() else {
        return true;
    };
    if origins.next().is_some() {
        return false;
    }
    origin.to_str().is_ok_and(|origin| {
        origin == service_origin || (cfg!(debug_assertions) && origin == DEV_WEBUI_ORIGIN)
    })
}

/// Returns the detailed lifecycle snapshot owned by the service process.
/// 返回由服务进程持有的详细生命周期快照。
async fn webui_service_status_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
) -> AxumResponse {
    // A present-but-untrusted Origin is refused exactly like on the event WebSocket.
    // 存在但不受信任的 Origin 与事件 WebSocket 一样被拒绝。
    if !webui_service_headers_origin_is_allowed(&headers) {
        return AxumStatusCode::FORBIDDEN.into_response();
    }
    if !webui_service_request_token_is_valid(&headers, uri.0.query(), &server_state.service_token) {
        return AxumStatusCode::UNAUTHORIZED.into_response();
    }
    let Some(status_provider) = server_state.status_provider else {
        return AxumStatusCode::NOT_FOUND.into_response();
    };
    Json(status_provider()).into_response()
}

/// Delivery interval of one browser observation stream.
/// 一个浏览器观察流的交付间隔。
const WEBUI_SERVICE_BROWSER_STREAM_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(40);

/// Handles one typed browser stream-ticket request.
/// 处理一个类型化浏览器流凭据请求。
///
/// # Returns
/// 返回值
/// `403` for an untrusted Origin, `401` without a valid token, `404` when no browser stream is
/// configured, `400` for a malformed body, `409` for a kernel refusal, or `200` with the ticket.
/// 不受信任 Origin 返回 `403`；令牌无效返回 `401`；未配置浏览器流返回 `404`；请求体格式错误返回
/// `400`；内核拒绝返回 `409`；成功返回 `200` 与凭据。
async fn webui_service_browser_stream_ticket_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
    Json(request): Json<serde_json::Value>,
) -> AxumResponse {
    if !webui_service_headers_origin_is_allowed(&headers) {
        return AxumStatusCode::FORBIDDEN.into_response();
    }
    if !webui_service_request_token_is_valid(&headers, uri.0.query(), &server_state.service_token) {
        return AxumStatusCode::UNAUTHORIZED.into_response();
    }
    let Some(browser_stream) = server_state.browser_stream else {
        return AxumStatusCode::NOT_FOUND.into_response();
    };
    let Some(session_id) = request.get("sessionId").and_then(|value| value.as_str()) else {
        return webui_service_browser_error_response(
            AxumStatusCode::BAD_REQUEST,
            "invalid_request",
            "A browser stream ticket requires a sessionId.",
        );
    };
    // owner_session_id must cross the host/kernel boundary so Core can authorize the observed browser.
    // owner_session_id 必须穿过宿主与内核边界，以便 Core 校验被观察浏览器的归属。
    let Some(owner_session_id) = request
        .get("ownerSessionId")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty() && *value == value.trim())
    else {
        return webui_service_browser_error_response(
            AxumStatusCode::BAD_REQUEST,
            "invalid_request",
            "A browser stream ticket requires an ownerSessionId.",
        );
    };
    match (browser_stream.issue_ticket)(String::from(session_id), String::from(owner_session_id))
        .await
    {
        Ok(value) => Json(value).into_response(),
        Err(message) => webui_service_browser_error_response(
            AxumStatusCode::CONFLICT,
            "browser_unavailable",
            &message,
        ),
    }
}

/// Upgrades one browser observation stream to a WebSocket and keeps delivering frames.
/// 将单个浏览器观察流升级为 WebSocket 并持续交付帧。
///
/// # Returns
/// 返回值
/// `403` for an untrusted Origin, `401` without a valid token, `404` when no browser stream is
/// configured, `400` when the ticket or subscription parameter is missing, or the upgrading socket.
/// 不受信任 Origin 返回 `403`；令牌无效返回 `401`；未配置浏览器流返回 `404`；缺少凭据或订阅参数
/// 返回 `400`；否则返回正在升级的套接字。
///
/// # Design note / 设计说明
/// The first message is always the authoritative resume answer, so a reconnecting client adopts a
/// snapshot and a complete frame before any incremental frame arrives. Later frames are read on a
/// bounded interval; the interval is the current delivery mechanism and per-subscriber push with
/// measured backpressure replaces it in the projection step.
/// 首条消息始终是权威恢复答复，因此重连客户端在任何增量帧到达之前先采用快照与完整帧。后续帧按有界
/// 间隔读取；该间隔是当前交付机制，按订阅推送与可测量的背压将在投影步骤中取代它。
async fn webui_service_browser_stream_ws_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
    websocket: WebSocketUpgrade,
) -> AxumResponse {
    if !webui_service_headers_origin_is_allowed(&headers) {
        return AxumStatusCode::FORBIDDEN.into_response();
    }
    if !webui_service_request_token_is_valid(&headers, uri.0.query(), &server_state.service_token) {
        return AxumStatusCode::UNAUTHORIZED.into_response();
    }
    let Some(browser_stream) = server_state.browser_stream else {
        return AxumStatusCode::NOT_FOUND.into_response();
    };
    let query = webui_service_query_parameters(uri.0.query());
    let Some(ticket) = query.get("ticket").cloned() else {
        return webui_service_browser_error_response(
            AxumStatusCode::BAD_REQUEST,
            "invalid_request",
            "A browser stream requires a ticket.",
        );
    };
    let Some(subscription_id) = query.get("subscriptionId").cloned() else {
        return webui_service_browser_error_response(
            AxumStatusCode::BAD_REQUEST,
            "invalid_request",
            "A browser stream requires a subscriptionId.",
        );
    };
    let Some(page_id) = query.get("pageId").cloned() else {
        return webui_service_browser_error_response(
            AxumStatusCode::BAD_REQUEST,
            "invalid_request",
            "A browser stream requires a pageId.",
        );
    };
    let cursor = query.get("pageGeneration").map(|page_generation| {
        serde_json::json!({
            "pageGeneration": page_generation,
            "frameSequence": query.get("frameSequence"),
        })
    });
    websocket.on_upgrade(move |socket| {
        webui_service_browser_stream_socket(
            socket,
            browser_stream,
            ticket,
            page_id,
            subscription_id,
            cursor,
        )
    })
}

/// Delivers one browser observation stream until the client goes away.
/// 向一个浏览器观察流持续交付，直至客户端断开。
///
/// # Parameters
/// 参数
/// `socket` is the upgraded connection, `browser_stream` is the injected Core facade, `ticket` is
/// the single-use authorisation, and `cursor` is what the client last observed.
/// `socket` 是已升级连接，`browser_stream` 是注入的 Core 门面，`ticket` 是一次性授权，`cursor` 是
/// 客户端最后观察到的位置。
async fn webui_service_browser_stream_socket(
    mut socket: WebSocket,
    browser_stream: WebuiServiceBrowserStream,
    ticket: String,
    page_id: String,
    subscription_id: String,
    cursor: Option<serde_json::Value>,
) {
    // The one-time ticket decides which session may be observed; the client cannot name one.
    // 一次性凭据决定可观察哪个会话；客户端无法指定。
    let initial =
        match (browser_stream.resume)(ticket, page_id, subscription_id.clone(), cursor).await {
            Ok(value) => value,
            Err(message) => {
                let _ = socket
                    .send(AxumWsMessage::Text(
                        serde_json::json!({
                            "kind": "error",
                            "code": "browser_stream_refused",
                            "message": message,
                        })
                        .to_string()
                        .into(),
                    ))
                    .await;
                return;
            }
        };
    let Some(session_id) = initial
        .get("sessionId")
        .and_then(|value| value.as_str())
        .map(String::from)
    else {
        let _ = socket
            .send(AxumWsMessage::Text(
                serde_json::json!({
                    "kind": "error",
                    "code": "browser_stream_unbound",
                    "message": "The stream answer did not name its session.",
                })
                .to_string()
                .into(),
            ))
            .await;
        return;
    };
    let mut page_generation = initial
        .get("pageGeneration")
        .and_then(|value| value.as_str())
        .unwrap_or("1")
        .to_string();
    let mut last_sequence = webui_service_browser_last_frame_sequence(&initial);
    if webui_service_browser_stream_send(&mut socket, &initial)
        .await
        .is_err()
    {
        webui_service_browser_stream_release(&browser_stream, &session_id, &subscription_id).await;
        return;
    }
    webui_service_browser_stream_deliver(
        &mut socket,
        &browser_stream,
        &session_id,
        &subscription_id,
        &mut page_generation,
        &mut last_sequence,
    )
    .await;
    // Every exit path above lands here, so a finished stream always releases exactly its own
    // subscription while the session and other observers keep running.
    // 以上每条退出路径都会到达此处，因此结束的流总是恰好释放它自己的订阅，而会话与其他观察端继续运行。
    webui_service_browser_stream_release(&browser_stream, &session_id, &subscription_id).await;
}

/// Releases one finished stream's observation subscription, ignoring an already-dropped session.
/// 释放已结束流的观察订阅；会话已被丢弃时不做处理。
async fn webui_service_browser_stream_release(
    browser_stream: &WebuiServiceBrowserStream,
    session_id: &str,
    subscription_id: &str,
) {
    let _ = (browser_stream.release)(String::from(session_id), String::from(subscription_id)).await;
}

/// Delivers frames on one browser stream until the client or the service goes away.
/// 在一个浏览器流上持续交付帧，直至客户端或服务消失。
///
/// # Parameters
/// 参数
/// `page_generation` and `last_sequence` carry the delivery position and are advanced in place.
/// `page_generation` 与 `last_sequence` 承载交付位置并被就地推进。
async fn webui_service_browser_stream_deliver(
    socket: &mut WebSocket,
    browser_stream: &WebuiServiceBrowserStream,
    session_id: &str,
    subscription_id: &str,
    page_generation: &mut String,
    last_sequence: &mut Option<String>,
) {
    loop {
        // A bounded interval keeps a slow host from spinning and makes the delivery cadence explicit.
        // 有界间隔避免缓慢宿主造成空转，并使交付节奏显式可控。
        tokio::time::sleep(WEBUI_SERVICE_BROWSER_STREAM_INTERVAL).await;
        let poll = (browser_stream.poll)(
            String::from(session_id),
            String::from(subscription_id),
            serde_json::json!({
                "pageGeneration": page_generation.clone(),
                "frameSequence": last_sequence.clone(),
            }),
        )
        .await;
        let answer = match poll {
            Ok(answer) => answer,
            Err(message) => {
                let _ = webui_service_browser_stream_send(
                    socket,
                    &serde_json::json!({
                        "kind": "error",
                        "code": "browser_stream_interrupted",
                        "message": message,
                    }),
                )
                .await;
                return;
            }
        };
        // A generation change or an evicted window makes the client resynchronise rather than
        // silently continue from a position that no longer exists.
        // 代次变化或窗口淘汰使客户端重新同步，而不是从已不存在的位置静默继续。
        let answer_generation = answer
            .get("pageGeneration")
            .and_then(|value| value.as_str())
            .unwrap_or("1");
        if answer_generation != page_generation.as_str() {
            *page_generation = String::from(answer_generation);
            *last_sequence = None;
        }
        if webui_service_browser_stream_send(socket, &answer)
            .await
            .is_err()
        {
            return;
        }
        if let Some(sequence) = webui_service_browser_last_frame_sequence(&answer) {
            *last_sequence = Some(sequence);
        }
    }
}

/// Sends stream metadata as text and every image as a separate binary WebSocket message.
/// 以文本发送流元数据，并将每张图像作为独立的二进制 WebSocket 消息发送。
///
/// # Returns
/// 返回值
/// `Ok(())` when the payload was handed to the socket, or the transport error that ended the stream.
/// 载荷已交给套接字时返回 `Ok(())`；结束该流的传输错误则返回 `Err`。
async fn webui_service_browser_stream_send(
    socket: &mut WebSocket,
    payload: &serde_json::Value,
) -> Result<(), ()> {
    use base64::Engine as _;

    let mut metadata = payload.clone();
    let mut images = Vec::new();
    if let Some(frames) = metadata
        .get_mut("frames")
        .and_then(|value| value.as_array_mut())
    {
        for frame in frames {
            let Some(object) = frame.as_object_mut() else {
                return Err(());
            };
            let encoded = object
                .remove("data")
                .and_then(|value| value.as_str().map(str::to_owned))
                .ok_or(())?;
            let image = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| ())?;
            let mut header = object.clone();
            header.insert(String::from("kind"), serde_json::json!("frame"));
            let header = serde_json::to_vec(&header).map_err(|_| ())?;
            let header_length = u32::try_from(header.len()).map_err(|_| ())?;
            let mut packet = Vec::with_capacity(4 + header.len() + image.len());
            packet.extend_from_slice(&header_length.to_be_bytes());
            packet.extend_from_slice(&header);
            packet.extend_from_slice(&image);
            images.push(packet);
        }
    }
    socket
        .send(AxumWsMessage::Text(metadata.to_string().into()))
        .await
        .map_err(|_| ())?;
    for image in images {
        socket
            .send(AxumWsMessage::Binary(image.into()))
            .await
            .map_err(|_| ())?;
    }
    Ok(())
}

/// Returns the highest frame sequence carried by one stream answer.
/// 返回一个流答复所携带的最高帧序号。
fn webui_service_browser_last_frame_sequence(answer: &serde_json::Value) -> Option<String> {
    answer
        .get("frames")?
        .as_array()?
        .iter()
        .filter_map(|frame| frame.get("sequence").and_then(|value| value.as_str()))
        .next_back()
        .map(String::from)
}

/// Handles one typed browser stream resume request.
/// 处理一个类型化浏览器流恢复请求。
///
/// # Returns
/// 返回值
/// `403` for an untrusted Origin, `401` without a valid token, `404` when no browser stream is
/// configured, `400` for missing query parameters, `409` for a refused ticket, or `200` with the
/// authoritative resume answer.
/// 不受信任 Origin 返回 `403`；令牌无效返回 `401`；未配置浏览器流返回 `404`；缺少查询参数返回
/// `400`；凭据被拒返回 `409`；成功返回 `200` 与权威恢复答复。
async fn webui_service_browser_stream_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
) -> AxumResponse {
    if !webui_service_headers_origin_is_allowed(&headers) {
        return AxumStatusCode::FORBIDDEN.into_response();
    }
    if !webui_service_request_token_is_valid(&headers, uri.0.query(), &server_state.service_token) {
        return AxumStatusCode::UNAUTHORIZED.into_response();
    }
    let Some(browser_stream) = server_state.browser_stream else {
        return AxumStatusCode::NOT_FOUND.into_response();
    };
    let query = webui_service_query_parameters(uri.0.query());
    let Some(ticket) = query.get("ticket") else {
        return webui_service_browser_error_response(
            AxumStatusCode::BAD_REQUEST,
            "invalid_request",
            "A browser stream resume requires a ticket.",
        );
    };
    let Some(subscription_id) = query.get("subscriptionId") else {
        return webui_service_browser_error_response(
            AxumStatusCode::BAD_REQUEST,
            "invalid_request",
            "A browser stream resume requires a subscriptionId.",
        );
    };
    let Some(page_id) = query.get("pageId") else {
        return webui_service_browser_error_response(
            AxumStatusCode::BAD_REQUEST,
            "invalid_request",
            "A browser stream resume requires a pageId.",
        );
    };
    // The cursor is rebuilt from query text so a client that has no position simply omits it.
    // 游标由查询文本重建，因此尚无位置的客户端只需省略它。
    let cursor = match (query.get("pageGeneration"), query.get("frameSequence")) {
        (Some(page_generation), frame_sequence) => Some(serde_json::json!({
            "pageGeneration": page_generation,
            "frameSequence": frame_sequence,
        })),
        (None, _) => None,
    };
    match (browser_stream.resume)(
        ticket.clone(),
        page_id.clone(),
        subscription_id.clone(),
        cursor,
    )
    .await
    {
        Ok(value) => {
            let Some(session_id) = value.get("sessionId").and_then(|value| value.as_str()) else {
                return webui_service_browser_error_response(
                    AxumStatusCode::CONFLICT,
                    "browser_stream_unbound",
                    "The browser stream answer did not name its session.",
                );
            };
            // An HTTP snapshot has no continuing socket to own this subscription. Release it
            // after materialising the answer so subsequent frame capture serves only live viewers.
            // HTTP 快照没有持续套接字持有该订阅；生成答复后立即释放，使后续画面采集只服务活动观察者。
            webui_service_browser_stream_release(&browser_stream, session_id, subscription_id)
                .await;
            Json(value).into_response()
        }
        Err(message) => webui_service_browser_error_response(
            AxumStatusCode::CONFLICT,
            "browser_stream_refused",
            &message,
        ),
    }
}

/// Parses one percent-decoded query string into a parameter map.
/// 将一个百分号解码后的查询字符串解析为参数映射。
///
/// # Returns
/// 返回值
/// Each present key mapped to its decoded value; a repeated key keeps its first occurrence.
/// 返回每个存在键到其解码值的映射；重复键保留首次出现。
fn webui_service_query_parameters(query: Option<&str>) -> HashMap<String, String> {
    let mut parameters = HashMap::new();
    let Some(query) = query else {
        return parameters;
    };
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (key, value),
            None => (pair, ""),
        };
        let key = webui_service_percent_decode(key);
        if key.is_empty() || parameters.contains_key(&key) {
            continue;
        }
        parameters.insert(key, webui_service_percent_decode(value));
    }
    parameters
}

/// Decodes one percent-encoded query component, treating `+` as a space.
/// 解码一个百分号编码的查询分量，并将 `+` 视为空格。
fn webui_service_percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok());
                match hex {
                    Some(byte) => {
                        decoded.push(byte);
                        index += 3;
                    }
                    None => {
                        decoded.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Builds one typed browser-route error response.
/// 构建一个类型化浏览器路由错误响应。
fn webui_service_browser_error_response(
    status: AxumStatusCode,
    code: &str,
    message: &str,
) -> AxumResponse {
    (
        status,
        Json(serde_json::json!({
            "ok": false,
            "error": { "code": code, "message": message },
        })),
    )
        .into_response()
}

/// Handles one already-admitted browser-facing JSON RPC request.
/// 处理一项已经通过准入的浏览器 JSON RPC 请求。
async fn webui_service_rpc_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    _admission: rpc_admission::RpcAdmission,
    Json(request): Json<WebuiServiceRpcHttpRequest>,
) -> AxumResponse {
    let result = (server_state.rpc_dispatcher)(request.method, request.params).await;
    Json(match result {
        Ok(result) => webui_service_rpc_success_envelope(result),
        Err(error) => webui_service_rpc_failure_envelope(&error),
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
    let Some(client_id) = webui_service_query_uuid(uri.0.query(), "clientId") else {
        return AxumStatusCode::BAD_REQUEST.into_response();
    };
    let connection_id = Uuid::new_v4();
    websocket.on_upgrade(move |socket| {
        let replaced = register_webui_service_client_connection(
            &server_state.client_connections,
            client_id,
            connection_id,
        );
        if replaced {
            (server_state.client_disconnected)(client_id);
        }
        webui_service_events_socket(
            socket,
            server_state.event_bus.subscribe(),
            client_id,
            connection_id,
            server_state.client_connections,
            server_state.client_disconnected,
        )
    })
}

/// Registers the newest event-socket generation for one browser lifetime.
/// 为一个浏览器生命周期注册最新事件套接字代次。
///
/// # Parameters / 参数
/// - `connections`: Shared latest-generation registry.
/// - `client_id`: Stable browser lifetime identity.
/// - `connection_id`: Unique identity of the newly upgraded socket.
/// - `connections`：共享的最新代次注册表。
/// - `client_id`：稳定浏览器生命周期标识。
/// - `connection_id`：新升级套接字的唯一标识。
///
/// # Returns / 返回值
/// Returns true when an older socket generation was replaced and its subscriptions must be cleared.
/// 当旧套接字代次被替换且其订阅必须清理时返回 true。
fn register_webui_service_client_connection(
    connections: &Arc<Mutex<HashMap<Uuid, Uuid>>>,
    client_id: Uuid,
    connection_id: Uuid,
) -> bool {
    connections
        .lock()
        .map(|mut connections| connections.insert(client_id, connection_id).is_some())
        .unwrap_or(true)
}

/// Releases a browser only when the closing socket is still its newest generation.
/// 仅当正在关闭的套接字仍是浏览器最新代次时释放该浏览器。
///
/// # Parameters / 参数
/// - `connections`: Shared latest-generation registry.
/// - `client_id`: Stable browser lifetime identity.
/// - `connection_id`: Unique identity of the closing socket.
/// - `connections`：共享的最新代次注册表。
/// - `client_id`：稳定浏览器生命周期标识。
/// - `connection_id`：正在关闭套接字的唯一标识。
///
/// # Returns / 返回值
/// Returns true only when the closing socket owned the newest generation and removed it.
/// 仅当关闭套接字持有最新代次并将其移除时返回 true。
fn release_webui_service_client_connection(
    connections: &Arc<Mutex<HashMap<Uuid, Uuid>>>,
    client_id: Uuid,
    connection_id: Uuid,
) -> bool {
    let Ok(mut connections) = connections.lock() else {
        return true;
    };
    if connections.get(&client_id) != Some(&connection_id) {
        return false;
    }
    connections.remove(&client_id);
    true
}

/// Parses one exact UUID query parameter without accepting aliases or malformed identities.
/// 解析一个精确 UUID 查询参数，不接受别名或格式错误的标识。
///
/// # Parameters / 参数
/// - `query`: Optional raw request query string.
/// - `key`: Exact ASCII query key.
/// - `query`：可选原始请求查询字符串。
/// - `key`：精确 ASCII 查询键。
///
/// # Returns / 返回值
/// Returns the parsed UUID only when one exact parameter is valid.
/// 仅在一个精确参数有效时返回解析后的 UUID。
fn webui_service_query_uuid(query: Option<&str>, key: &str) -> Option<Uuid> {
    let mut matches = query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter_map(|(name, value)| (name == key).then_some(value));
    let value = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Uuid::parse_str(value).ok()
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

/// Browser-bound events waiting for one event socket, with streaming deltas coalesced.
/// 等待单个事件套接字发送的浏览器事件，其中流式增量已合并。
#[derive(Default)]
struct WebuiServiceEventOutbox {
    /// Pending events in delivery order.
    /// 按交付顺序排列的待发送事件。
    entries: VecDeque<WebuiServiceEventOutboxEntry>,
    /// Serialized bytes of `entries`, measured by payload for conversation deltas.
    /// `entries` 的序列化字节数，会话增量按其载荷计量。
    bytes: usize,
}

/// One pending event of an event socket.
/// 事件套接字的一条待发送事件。
enum WebuiServiceEventOutboxEntry {
    /// A volatile conversation delta that later deltas of its session extend in place.
    /// 一条 volatile 会话增量，其会话的后续增量在原地延长它。
    ConversationDelta(ConversationEventLiveDeltaAccumulatorV1),
    /// Any other event, serialized on arrival. `conversation_session` names the session of a
    /// conversation delta that cannot be extended, which deltas of other sessions may pass over.
    /// 其他任意事件，到达时即序列化。`conversation_session` 指明不可延长的会话增量所属会话，
    /// 其他会话的增量可以越过它。
    Serialized {
        json: String,
        conversation_session: Option<String>,
    },
}

impl WebuiServiceEventOutboxEntry {
    /// Bytes this entry counts toward the outbox bound.
    /// 该条目计入待发送队列上限的字节数。
    fn bytes(&self) -> usize {
        match self {
            Self::ConversationDelta(delta) => delta.serialized_bytes(),
            Self::Serialized { json, .. } => json.len(),
        }
    }
}

/// An outbox that cannot take another event; the socket must close so the browser recovers.
/// 无法再接纳事件的待发送队列；套接字必须关闭，使浏览器恢复。
#[derive(Debug, PartialEq, Eq)]
struct WebuiServiceEventOutboxFull {
    /// Pending events when the bound was reached.
    /// 达到上限时的待发送事件数。
    events: usize,
    /// Pending bytes when the bound was reached.
    /// 达到上限时的待发送字节数。
    bytes: usize,
}

impl WebuiServiceEventOutbox {
    /// Queues one event, extending the pending delta it continues when it is a volatile delta.
    /// 入队一个事件；若为 volatile 增量，则延长其所延续的待发送增量。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the next bus event visible to this socket.
    /// `event` 是本套接字可见的下一个总线事件。
    ///
    /// # Returns
    /// 返回值
    /// Returns `Err` when the event count or byte bound is reached and the socket must close.
    /// Events that fail to serialize are logged and skipped.
    /// 达到事件数或字节数上限、套接字必须关闭时返回 `Err`。序列化失败的事件记录日志后跳过。
    fn push(&mut self, event: ServiceEvent) -> Result<(), WebuiServiceEventOutboxFull> {
        let event = match event {
            ServiceEvent::ConversationEventDeltaV1(delta) => {
                match ConversationEventLiveDeltaAccumulatorV1::start(*delta) {
                    Ok(delta) => return self.push_conversation_delta(delta),
                    Err(delta) => ServiceEvent::ConversationEventDeltaV1(delta),
                }
            }
            event => event,
        };
        let conversation_session = match &event {
            ServiceEvent::ConversationEventDeltaV1(delta) => Some(delta.batch.session_id.clone()),
            _ => None,
        };
        match serde_json::to_string(&event) {
            Ok(json) => self.push_entry(WebuiServiceEventOutboxEntry::Serialized {
                json,
                conversation_session,
            }),
            Err(error) => {
                eprintln!("[service-host] failed to serialize browser service event: {error}");
                Ok(())
            }
        }
    }

    /// Extends the newest pending delta of the session with `next`, or queues `next` after it.
    /// 用 `next` 延长该会话最新的待发送增量，或把 `next` 排在其后。
    ///
    /// Deltas of other sessions are independent and may be passed over; any other event keeps
    /// its order relative to `next`, so the search stops there. A merge grows the pending delta
    /// by at most the size of `next`, so the byte bound is checked against that before merging.
    /// 其他会话的增量彼此独立，可以越过；其他任何事件都须保持与 `next` 的相对顺序，搜索在此停止。
    /// 合并使待发送增量最多增长 `next` 的大小，因此合并前按该大小检查字节上限。
    fn push_conversation_delta(
        &mut self,
        next: ConversationEventLiveDeltaAccumulatorV1,
    ) -> Result<(), WebuiServiceEventOutboxFull> {
        let incoming = next.serialized_bytes();
        if !self.admits(incoming) {
            return Err(self.full());
        }
        let mut target = None;
        for entry in self.entries.iter_mut().rev() {
            match entry {
                WebuiServiceEventOutboxEntry::ConversationDelta(pending)
                    if pending.session_id() == next.session_id() =>
                {
                    target = Some(pending);
                    break;
                }
                WebuiServiceEventOutboxEntry::ConversationDelta(_) => {}
                WebuiServiceEventOutboxEntry::Serialized {
                    conversation_session: Some(session),
                    ..
                } if session != next.session_id() => {}
                WebuiServiceEventOutboxEntry::Serialized { .. } => break,
            }
        }
        let next = match target {
            Some(pending)
                if pending.serialized_bytes() + incoming
                    <= WEBUI_SERVICE_EVENT_MERGED_DELTA_MAX_BYTES =>
            {
                let before = pending.serialized_bytes();
                match pending.merge(next) {
                    Ok(()) => {
                        self.bytes = self.bytes + pending.serialized_bytes() - before;
                        return Ok(());
                    }
                    Err(next) => *next,
                }
            }
            _ => next,
        };
        self.push_entry(WebuiServiceEventOutboxEntry::ConversationDelta(next))
    }

    /// Appends one entry when both the event count and byte bounds allow it.
    /// 在事件数与字节数上限都允许时追加一个条目。
    fn push_entry(
        &mut self,
        entry: WebuiServiceEventOutboxEntry,
    ) -> Result<(), WebuiServiceEventOutboxFull> {
        let bytes = entry.bytes();
        if self.entries.len() >= WEBUI_SERVICE_EVENT_BUS_CAPACITY || !self.admits(bytes) {
            return Err(self.full());
        }
        self.entries.push_back(entry);
        self.bytes += bytes;
        Ok(())
    }

    /// Reports whether `bytes` more fit, always admitting one event into an empty outbox.
    /// 判断能否再容纳 `bytes` 字节；空的待发送队列总会接纳一个事件。
    fn admits(&self, bytes: usize) -> bool {
        self.entries.is_empty() || self.bytes + bytes <= WEBUI_SERVICE_EVENT_OUTBOX_MAX_BYTES
    }

    /// Describes the outbox at the moment it cannot take an event.
    /// 描述无法接纳事件时的待发送队列。
    fn full(&self) -> WebuiServiceEventOutboxFull {
        WebuiServiceEventOutboxFull {
            events: self.entries.len(),
            bytes: self.bytes,
        }
    }

    /// Removes and serializes the next deliverable event.
    /// 移除并序列化下一个可交付事件。
    ///
    /// # Returns
    /// 返回值
    /// Returns `None` when nothing is pending; events that fail to serialize are logged and skipped.
    /// 无待发送事件时返回 `None`；序列化失败的事件记录日志后跳过。
    fn pop_serialized(&mut self) -> Option<String> {
        while let Some(entry) = self.entries.pop_front() {
            self.bytes -= entry.bytes();
            let delta = match entry {
                WebuiServiceEventOutboxEntry::Serialized { json, .. } => return Some(json),
                WebuiServiceEventOutboxEntry::ConversationDelta(delta) => delta.into_delta(),
            };
            match serde_json::to_string(&ServiceEvent::ConversationEventDeltaV1(Box::new(delta))) {
                Ok(text) => return Some(text),
                Err(error) => {
                    eprintln!("[service-host] failed to serialize browser service event: {error}");
                }
            }
        }
        None
    }
}

/// Streams serialized service events to one browser WebSocket.
/// 向单个浏览器 WebSocket 推送序列化后的服务事件。
///
/// The bus is drained while a send is in flight, so a slow browser receives merged streaming
/// deltas instead of lagging the bus; only an outbox that reaches its event or byte bound closes
/// the socket.
/// 发送进行期间持续消费总线，因此慢速浏览器收到合并后的流式增量而不会使总线落后；只有达到事件
/// 数或字节数上限的待发送队列才会关闭套接字。
async fn webui_service_events_socket(
    socket: WebSocket,
    receiver: broadcast::Receiver<ServiceEvent>,
    client_id: Uuid,
    connection_id: Uuid,
    client_connections: Arc<Mutex<HashMap<Uuid, Uuid>>>,
    client_disconnected: WebuiServiceClientDisconnected,
) {
    let (sink, inbound) = socket.split();
    pump_webui_service_events(sink, inbound, receiver, client_id).await;
    if release_webui_service_client_connection(&client_connections, client_id, connection_id) {
        client_disconnected(client_id);
    }
}

/// Forwards bus events visible to one browser until either side closes.
/// 向单个浏览器转发其可见的总线事件，直到任一端关闭。
///
/// # Parameters
/// 参数
/// `sink` and `inbound` are the two halves of the browser socket, `receiver` is this socket's bus
/// subscription, and `client_id` is the browser lifetime that filters client-owned events.
/// `sink` 与 `inbound` 是浏览器套接字的两半，`receiver` 是本套接字的总线订阅，`client_id` 是用于
/// 过滤客户端所属事件的浏览器生命周期。
///
/// # Returns
/// 返回值
/// Returns when the browser closes, a send fails, or the bus closes or overflows this socket.
/// 浏览器关闭、发送失败，或总线关闭或使本套接字溢出时返回。
async fn pump_webui_service_events<Sink, Inbound>(
    sink: Sink,
    mut inbound: Inbound,
    mut receiver: broadcast::Receiver<ServiceEvent>,
    client_id: Uuid,
) where
    Sink: futures_util::Sink<AxumWsMessage> + Unpin + Send + 'static,
    Sink::Error: Send,
    Inbound: futures_util::Stream<Item = Result<AxumWsMessage, axum::Error>> + Unpin,
{
    // The in-flight send owns the sink and hands it back when it completes.
    // 进行中的发送持有发送端，并在完成时交还。
    type InFlightSend<Sink, Error> =
        Pin<Box<dyn Future<Output = (Sink, Result<(), Error>)> + Send>>;
    let mut idle_sink = Some(sink);
    let mut sending: Option<InFlightSend<Sink, Sink::Error>> = None;
    let mut outbox = WebuiServiceEventOutbox::default();
    loop {
        if let Some(mut sink) = idle_sink.take() {
            match outbox.pop_serialized() {
                Some(text) => {
                    sending = Some(Box::pin(async move {
                        let result = sink.send(AxumWsMessage::Text(text.into())).await;
                        (sink, result)
                    }));
                }
                None => idle_sink = Some(sink),
            }
        }
        tokio::select! {
            inbound = inbound.next() => match inbound {
                Some(Ok(AxumWsMessage::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            sent = async { sending.as_mut().expect("guarded in-flight send").await }, if sending.is_some() => {
                sending = None;
                let (sink, result) = sent;
                if result.is_err() {
                    break;
                }
                idle_sink = Some(sink);
            }
            event = receiver.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        // A lossy event socket cannot preserve ordered terminal or workbench state;
                        // closing it forces the browser to reconnect and request authoritative baselines.
                        // 丢失事件的套接字无法保持终端或工作台有序状态；关闭它会强制浏览器重连并请求权威基线。
                        eprintln!(
                            "[service-host] browser event socket client={client_id} lagged by {skipped} events; reconnecting"
                        );
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if !webui_service_event_is_visible_to_client(&event, client_id) {
                    continue;
                }
                if let Err(full) = outbox.push(event) {
                    // Same recovery as a lagged bus: the browser rebuilds from authoritative baselines.
                    // 与总线落后相同的恢复方式：浏览器从权威基线重建。
                    eprintln!(
                        "[service-host] browser event socket client={client_id} outbox full with {} events and {} bytes; reconnecting",
                        full.events, full.bytes
                    );
                    break;
                }
            }
        }
    }
}

/// Filters client-owned events to their exact browser while leaving shared events broadcast.
/// 将客户端所属事件限制到其精确浏览器，同时保持共享事件广播。
///
/// # Parameters / 参数
/// - `event`: Service event awaiting WebSocket serialization.
/// - `client_id`: Exact browser/WebView lifetime owning this event socket.
/// - `event`：等待 WebSocket 序列化的服务事件。
/// - `client_id`：持有当前事件套接字的精确浏览器或 WebView 生命周期。
///
/// # Returns / 返回值
/// Returns true for shared events or client-owned events explicitly assigned to this client.
/// 对共享事件或明确分配给当前客户端的所属事件返回 true。
fn webui_service_event_is_visible_to_client(event: &ServiceEvent, client_id: Uuid) -> bool {
    let expected_client_id = client_id.as_hyphenated().to_string();
    match event {
        ServiceEvent::ConversationRunSettled(event) => {
            event.notification_client_id.as_deref() == Some(expected_client_id.as_str())
        }
        ServiceEvent::CommandSessionDisplayChanged(event) => {
            event.display_client_id == expected_client_id
        }
        ServiceEvent::ShellCommandOutputChanged(event) => {
            event.display_client_id == expected_client_id
        }
        ServiceEvent::ProjectTerminalDisplayChanged(event) => {
            event.display_client_id == expected_client_id
        }
        _ => true,
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
        "Vulcan WebUI static bundle is not available.",
    ))
}

/// Returns whether one browser request path belongs to the service API surface.
/// 返回单个浏览器请求路径是否属于服务 API 表面。
///
/// # Parameters
/// 参数
/// `path` is the raw request path without any query string.
/// `path` 是不含查询串的原始请求路径。
///
/// # Returns
/// 返回值
/// `true` for the reserved service prefixes, which must never fall back to the SPA index.
/// 对保留的服务前缀返回 `true`；这些路径绝不能回退到 SPA 入口。
fn webui_service_path_is_reserved_api(path: &str) -> bool {
    // A missing API route must surface as an API error, not as a 200 HTML application shell.
    // 缺失的 API 路由必须表现为 API 错误，而不是 200 HTML 应用外壳。
    const RESERVED_PREFIXES: [&str; 5] = ["/v1/", "/rpc", "/events", "/asset/", "/health"];
    RESERVED_PREFIXES
        .iter()
        .any(|prefix| path == prefix.trim_end_matches('/') || path.starts_with(prefix))
}

/// Serves WebUI static files and falls back to the SPA index for browser routes.
/// 提供 WebUI 静态文件，并对浏览器路由回退到 SPA 入口。
async fn webui_service_static_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumOriginalUri(uri): AxumOriginalUri,
) -> AxumResponse {
    if webui_service_path_is_reserved_api(uri.path()) {
        return axum_response_from_service_static_response(ServiceStaticResponse {
            status: 404,
            headers: vec![(
                String::from("content-type"),
                String::from("application/json; charset=utf-8"),
            )],
            body: String::from("{\"error\":\"unknown_service_route\"}").into_bytes(),
        });
    }
    axum_response_from_service_static_response(build_webui_static_response(
        server_state.static_root.as_deref(),
        uri.path(),
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

/// Serves one MCP Streamable HTTP POST; the kernel endpoint authenticates the bearer token.
/// 提供一次 MCP Streamable HTTP POST；内核端点负责校验 Bearer 令牌。
///
/// # Returns
/// 返回值
/// `403` for any browser Origin (DNS-rebinding defence), `404` without an endpoint, otherwise the
/// endpoint's answer.
/// 带任何浏览器 Origin 时返回 `403`（防御 DNS 重绑定）；没有端点时返回 `404`；否则返回端点应答。
async fn webui_service_mcp_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    body: axum::body::Bytes,
) -> AxumResponse {
    if headers.contains_key(axum::http::header::ORIGIN) {
        return mcp_plain_text_response(
            AxumStatusCode::FORBIDDEN,
            "Browser origins cannot call the MCP endpoint.",
        );
    }
    let Some(endpoint) = server_state.mcp_endpoint else {
        return mcp_plain_text_response(
            AxumStatusCode::NOT_FOUND,
            "The MCP endpoint is not available.",
        );
    };
    let authorization = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    match endpoint(authorization, body.to_vec()).await {
        Ok(response) => axum_response_from_service_media_response(response),
        Err(error) => mcp_plain_text_response(AxumStatusCode::INTERNAL_SERVER_ERROR, &error),
    }
}

/// Rejects the optional SSE stream and session deletion; this server is stateless.
/// 拒绝可选的 SSE 流与会话删除；本服务器无状态。
async fn webui_service_mcp_method_not_allowed() -> AxumResponse {
    let mut response = mcp_plain_text_response(
        AxumStatusCode::METHOD_NOT_ALLOWED,
        "The MCP endpoint accepts POST only.",
    );
    response.headers_mut().insert(
        axum::http::header::ALLOW,
        axum::http::HeaderValue::from_static("POST"),
    );
    response
}

/// Builds one uncached plain-text MCP transport response.
/// 构建一个不缓存的纯文本 MCP 传输响应。
fn mcp_plain_text_response(status: AxumStatusCode, message: &str) -> AxumResponse {
    (
        status,
        [
            (
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            ),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        message.to_string(),
    )
        .into_response()
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

/// Serves one V1 project resource through its opaque session/resource identity.
/// 通过不透明的会话/资源身份提供一个 V1 项目资源。
async fn webui_service_session_resource_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumPath((session_id, resource_id)): AxumPath<(String, String)>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
) -> AxumResponse {
    webui_service_session_resource_http_response(
        server_state,
        session_id,
        resource_id,
        headers,
        uri,
        false,
    )
    .await
}

/// Serves only V1 project-resource response headers.
/// 仅提供 V1 项目资源响应头。
async fn webui_service_session_resource_head_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumPath((session_id, resource_id)): AxumPath<(String, String)>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
) -> AxumResponse {
    webui_service_session_resource_http_response(
        server_state,
        session_id,
        resource_id,
        headers,
        uri,
        true,
    )
    .await
}

/// Serves one completed browser download through an authenticated opaque identity.
/// 通过经过鉴权的不透明身份提供一项已完成浏览器下载。
async fn webui_service_browser_download_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumPath(download_id): AxumPath<String>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
) -> AxumResponse {
    webui_service_browser_download_response(server_state, download_id, headers, uri, false).await
}

/// Serves download headers without its file body.
/// 仅提供下载响应头而不返回文件正文。
async fn webui_service_browser_download_head_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumPath(download_id): AxumPath<String>,
    headers: AxumHeaderMap,
    uri: AxumOriginalUri,
) -> AxumResponse {
    webui_service_browser_download_response(server_state, download_id, headers, uri, true).await
}

/// Applies service authentication before asking the Core resolver for attachment bytes.
/// 在向 Core 解析器请求附件字节之前执行服务鉴权。
async fn webui_service_browser_download_response(
    server_state: WebuiServiceHttpState,
    download_id: String,
    headers: AxumHeaderMap,
    _uri: AxumOriginalUri,
    is_head: bool,
) -> AxumResponse {
    if !webui_service_headers_origin_is_allowed(&headers) {
        return AxumStatusCode::FORBIDDEN.into_response();
    }
    // Attachment bytes require a header credential; URL query tokens leak into browser history.
    // 附件字节要求标头凭据；URL 查询令牌会泄漏到浏览器历史中。
    if !webui_service_request_token_is_valid(&headers, None, &server_state.service_token) {
        return axum_response_from_service_media_response(service_media_error_response(
            AxumStatusCode::UNAUTHORIZED.as_u16(),
            "Browser service token is missing or invalid.",
        ));
    }
    let Some(resolve) = server_state.browser_download_resolver else {
        return AxumStatusCode::NOT_FOUND.into_response();
    };
    // owner_session_id is required for the Core browser's immutable conversation boundary.
    // owner_session_id 是 Core 浏览器不可变会话边界所必需的身份。
    let Some(owner_session_id) = headers
        .get("x-vulcan-browser-owner-session-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && *value == value.trim())
    else {
        return AxumStatusCode::NOT_FOUND.into_response();
    };
    match resolve(download_id, owner_session_id.to_owned(), is_head).await {
        Ok(response) => axum_response_from_service_media_response(response),
        Err(error) => axum_response_from_service_media_response(service_media_error_response(
            AxumStatusCode::NOT_FOUND.as_u16(),
            &error,
        )),
    }
}

/// Builds one authenticated V1 project-resource response without decoding a host path.
/// 构建一个经过鉴权且不解码宿主路径的 V1 项目资源响应。
///
/// # Parameters
/// 参数
/// `server_state` owns authentication and the kernel resolver, the two route segments identify
/// the logical resource, `headers` and `uri` carry authentication/range metadata, and `is_head`
/// selects metadata-only delivery.
/// `server_state` 拥有鉴权与内核解析器，两个路由片段标识逻辑资源，`headers` 与 `uri` 携带鉴权/范围元数据，
/// `is_head` 选择仅元数据传输。
///
/// # Returns
/// 返回值
/// Returns controlled bytes, a normalized client error, or an explicit resolver failure.
/// 返回受控字节、规范化客户端错误或显式解析失败。
async fn webui_service_session_resource_http_response(
    server_state: WebuiServiceHttpState,
    session_id: String,
    resource_id: String,
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
    let resource_id = match resource_id.parse::<u64>() {
        Ok(resource_id) => resource_id,
        Err(error) => {
            return axum_response_from_service_media_response(service_media_error_response(
                AxumStatusCode::BAD_REQUEST.as_u16(),
                &format!("Project resource id is invalid: {error}"),
            ));
        }
    };
    let request = SessionResourceContentV1Request {
        session_id,
        resource_id,
        is_head,
        range_header: headers
            .get("range")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string),
    };
    let response = match (server_state.session_resource_resolver)(request).await {
        Ok(response) => response,
        Err(error) => {
            service_media_error_response(AxumStatusCode::INTERNAL_SERVER_ERROR.as_u16(), &error)
        }
    };
    axum_response_from_service_media_response(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use tower::ServiceExt;
    use vulcan_service_api::build_service_project_resource_response;
    use vulcan_service_api::{WEBUI_SERVICE_HTTP_PORT, webui_service_ready_discovery_snapshot};

    /// Verifies an exclusive Windows reader cannot strand a ready discovery generation.
    /// 验证 Windows 独占读者不会让已就绪发现代际永久停在旧文件。
    #[cfg(windows)]
    #[test]
    fn discovery_publication_retries_a_transient_windows_reader_lock() {
        use std::os::windows::fs::OpenOptionsExt;
        use std::sync::mpsc;
        use std::time::Duration;

        // home and fallback belong only to this disposable test directory.
        // home 与 fallback 仅属于本次可丢弃的测试目录。
        let directory = tempfile::tempdir().expect("create isolated discovery root");
        let home = directory.path().join("home");
        let fallback = directory.path().join("fallback");
        let starting = webui_service_ready_discovery_snapshot(17891, 41, "100", "old-token");
        write_webui_service_discovery_snapshot(&starting, Some(home.clone()), fallback.clone())
            .expect("publish first generation");
        let path = webui_service_discovery_snapshot_path(Some(home.clone()), fallback.clone());
        // A zero-share handle reproduces the Windows replacement denial from a reader.
        // 零共享句柄复现 Windows 读者阻止替换目标文件的实际错误。
        let reader = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .expect("hold discovery generation open");
        let (started_tx, started_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let writer = std::thread::spawn(move || {
            let ready = webui_service_ready_discovery_snapshot(17891, 41, "100", "new-token");
            started_tx.send(()).expect("announce attempted publication");
            result_tx
                .send(write_webui_service_discovery_snapshot(
                    &ready,
                    Some(home),
                    fallback,
                ))
                .expect("report publication result");
        });
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("writer started");
        assert!(matches!(
            result_rx.recv_timeout(Duration::from_millis(120)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        drop(reader);
        result_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("writer finished after reader release")
            .expect("publish ready generation");
        writer.join().expect("writer thread joined");
        let published: WebuiServiceDiscoverySnapshot =
            serde_json::from_slice(&fs::read(path).expect("read final generation"))
                .expect("parse final generation");
        assert_eq!(published.service_token, "new-token");
    }

    /// Verifies the event socket accepts only one exact well-formed client UUID query value.
    /// 验证事件套接字只接受一个精确且格式正确的客户端 UUID 查询值。
    #[test]
    fn event_socket_client_id_query_is_strict() {
        let client_id = Uuid::new_v4();
        let query = format!("token=test-token&clientId={client_id}");
        assert_eq!(
            webui_service_query_uuid(Some(&query), "clientId"),
            Some(client_id)
        );
        assert_eq!(
            webui_service_query_uuid(Some("clientId=invalid"), "clientId"),
            None
        );
        assert_eq!(
            webui_service_query_uuid(Some(&format!("client_id={client_id}")), "clientId"),
            None
        );
        assert_eq!(
            webui_service_query_uuid(
                Some(&format!("clientId={client_id}&clientId={client_id}")),
                "clientId"
            ),
            None
        );
    }

    /// Verifies a late close from an older socket cannot release a newer replacement generation.
    /// 验证旧套接字的延迟关闭不能释放较新的替代代次。
    #[test]
    fn event_socket_replacement_ignores_late_old_disconnect() {
        let connections = Arc::new(Mutex::new(HashMap::new()));
        let client_id = Uuid::new_v4();
        let old_connection_id = Uuid::new_v4();
        let new_connection_id = Uuid::new_v4();

        assert!(!register_webui_service_client_connection(
            &connections,
            client_id,
            old_connection_id,
        ));
        assert!(register_webui_service_client_connection(
            &connections,
            client_id,
            new_connection_id,
        ));
        assert!(!release_webui_service_client_connection(
            &connections,
            client_id,
            old_connection_id,
        ));
        assert!(release_webui_service_client_connection(
            &connections,
            client_id,
            new_connection_id,
        ));
    }

    /// Verifies terminal content is serialized only onto its exact owning browser socket.
    /// 验证终端内容只会序列化到其精确归属浏览器套接字。
    #[test]
    fn terminal_display_event_is_filtered_by_client_id() {
        let owner = Uuid::new_v4();
        let other = Uuid::new_v4();
        let event = ServiceEvent::ProjectTerminalDisplayChanged(Box::new(
            vulcan_service_api::ProjectTerminalDisplayChangedEvent {
                display_client_id: owner.to_string(),
                subscription_id: String::from("subscription-a"),
                terminal: vulcan_service_api::ProjectTerminalSummary {
                    id: Uuid::new_v4().to_string(),
                    project_id: String::from("project-a"),
                    name: String::from("Terminal 1"),
                    shell_id: String::from("pwsh"),
                    cwd: String::from("."),
                    status: String::from("running"),
                    exit_code: None,
                    failure_reason: None,
                    last_seq: 1,
                    dropped_before_seq: 0,
                    state_revision: 0,
                    last_accepted_input_seq: 0,
                    rows: 24,
                    cols: 80,
                },
                chunks: Vec::new(),
            },
        ));
        assert!(webui_service_event_is_visible_to_client(&event, owner));
        assert!(!webui_service_event_is_visible_to_client(&event, other));
    }

    /// Verifies a conversation settlement reaches only its exact notification owner.
    /// 验证会话终态事件只会到达其精确通知所有者。
    #[test]
    fn conversation_settlement_event_is_filtered_by_notification_client_id() {
        let owner = Uuid::new_v4();
        let other = Uuid::new_v4();
        // ownedEvent carries the exact WebUI connection identity selected by the initiating action.
        // ownedEvent 携带由发起动作选定的精确 WebUI 连接身份。
        let owned_event = ServiceEvent::ConversationRunSettled(Box::new(
            vulcan_service_api::ConversationRunSettledEvent {
                event_id: String::from("conversation-settled:run-a"),
                conversation_id: Uuid::new_v4().to_string(),
                project_id: None,
                run_id: Uuid::new_v4().to_string(),
                outcome: vulcan_service_api::ConversationRunOutcome::Completed,
                occurred_at: String::from("1000"),
                notification_client_id: Some(owner.to_string()),
            },
        ));
        assert!(webui_service_event_is_visible_to_client(
            &owned_event,
            owner
        ));
        assert!(!webui_service_event_is_visible_to_client(
            &owned_event,
            other
        ));

        // unownedEvent represents legacy or non-client work and must not select an arbitrary UI.
        // unownedEvent 表示旧式或非客户端工作，不得任选一个界面投递。
        let unowned_event = ServiceEvent::ConversationRunSettled(Box::new(
            vulcan_service_api::ConversationRunSettledEvent {
                event_id: String::from("conversation-settled:run-b"),
                conversation_id: Uuid::new_v4().to_string(),
                project_id: None,
                run_id: Uuid::new_v4().to_string(),
                outcome: vulcan_service_api::ConversationRunOutcome::Failed,
                occurred_at: String::from("1001"),
                notification_client_id: None,
            },
        ));
        assert!(!webui_service_event_is_visible_to_client(
            &unowned_event,
            owner
        ));
        assert!(!webui_service_event_is_visible_to_client(
            &unowned_event,
            other
        ));
    }

    /// Builds one volatile conversation delta for `session_id` covering `base..revision`.
    /// 为 `session_id` 构建一条覆盖 `base..revision` 的 volatile 会话增量。
    fn volatile_delta(session_id: &str, base: u64, revision: u64) -> ServiceEvent {
        conversation_delta(json!({
            "session_id": session_id,
            "window_generation": 1,
            "events": [],
            "service_epoch": "epoch-outbox",
            "revision": revision,
            "base_memory_revision": base,
            "durability": "volatile",
        }))
    }

    /// Builds one volatile delta for session `a` whose seq 1 text grows from `text_len` by `text`.
    /// 为会话 `a` 构建一条 volatile 增量，其 seq 1 正文从 `text_len` 处追加 `text`。
    fn text_append_delta(base: u64, text_len: u64, text: &str) -> ServiceEvent {
        conversation_delta(json!({
            "session_id": "a",
            "window_generation": 1,
            "events": [],
            "service_epoch": "epoch-outbox",
            "revision": base + 1,
            "base_memory_revision": base,
            "durability": "volatile",
            "text_appends": [{
                "seq": 1,
                "event_revision": base + 1,
                "updated_at_ms": 1_000 + base,
                "base_text_utf16_len": text_len,
                "appended_text": text,
            }],
        }))
    }

    /// Decodes one conversation delta service event from its wire shape.
    /// 从线上形状解码一个会话增量服务事件。
    fn conversation_delta(value: serde_json::Value) -> ServiceEvent {
        let delta: vulcan_service_api::ConversationEventLiveDeltaV1 =
            serde_json::from_value(value).expect("conversation delta");
        ServiceEvent::ConversationEventDeltaV1(Box::new(delta))
    }

    /// Builds one event that is not a conversation delta.
    /// 构建一个非会话增量事件。
    fn unmergeable_event(revision: u64) -> ServiceEvent {
        ServiceEvent::PluginUiInvalidated(Box::new(vulcan_service_api::PluginUiInvalidatedEvent {
            plugin_id: String::from("plugin-outbox"),
            topic: String::from("topic"),
            revision,
        }))
    }

    /// Describes pending events as (session or kind, base, revision) for order assertions.
    /// 将待发送事件描述为（会话或种类、基线、修订），用于顺序断言。
    fn outbox_shape(outbox: &WebuiServiceEventOutbox) -> Vec<(String, Option<u64>, u64)> {
        outbox
            .entries
            .iter()
            .map(|entry| match entry {
                WebuiServiceEventOutboxEntry::ConversationDelta(delta) => {
                    let delta = delta.clone().into_delta();
                    (
                        delta.batch.session_id,
                        delta.base_memory_revision,
                        delta.revision,
                    )
                }
                WebuiServiceEventOutboxEntry::Serialized { json, .. } => {
                    let event: serde_json::Value = serde_json::from_str(json).expect("event json");
                    assert_eq!(event["kind"], "pluginUiInvalidated", "unexpected {json}");
                    (
                        String::from("plugin"),
                        None,
                        event["payload"]["revision"].as_u64().expect("revision"),
                    )
                }
            })
            .collect()
    }

    /// Contiguous deltas queued behind a blocked send collapse into one revision range.
    /// 在阻塞发送之后排队的相邻增量折叠为一个修订区间。
    #[test]
    fn outbox_merges_contiguous_deltas_across_other_sessions_only() {
        let mut outbox = WebuiServiceEventOutbox::default();
        for event in [
            volatile_delta("a", 1, 2),
            volatile_delta("b", 7, 8),
            volatile_delta("a", 2, 3),
            volatile_delta("b", 8, 9),
            volatile_delta("a", 3, 4),
        ] {
            outbox.push(event).expect("queue delta");
        }
        assert_eq!(
            outbox_shape(&outbox),
            vec![
                (String::from("a"), Some(1), 4),
                (String::from("b"), Some(7), 9),
            ]
        );

        // Any other event is an ordering barrier; a later delta queues after it.
        // 其他任何事件都是顺序屏障；之后的增量排在它后面。
        outbox.push(unmergeable_event(1)).expect("queue barrier");
        outbox.push(volatile_delta("a", 4, 5)).expect("queue delta");
        // A gap is never bridged by merging.
        // 合并绝不跨越缺口。
        outbox
            .push(volatile_delta("a", 6, 7))
            .expect("queue gap delta");
        assert_eq!(
            outbox_shape(&outbox),
            vec![
                (String::from("a"), Some(1), 4),
                (String::from("b"), Some(7), 9),
                (String::from("plugin"), None, 1),
                (String::from("a"), Some(4), 5),
                (String::from("a"), Some(6), 7),
            ]
        );
        let first: serde_json::Value =
            serde_json::from_str(&outbox.pop_serialized().expect("first event"))
                .expect("event json");
        assert_eq!(first["kind"], "conversationEventDeltaV1");
        assert_eq!(first["payload"]["base_memory_revision"], 1);
        assert_eq!(first["payload"]["revision"], 4);
    }

    /// Deltas published while a send is blocked reach the browser as one merged delta, in order.
    /// 发送阻塞期间发布的增量以一条合并增量按序到达浏览器。
    #[tokio::test]
    async fn event_pump_merges_deltas_while_a_send_is_blocked() {
        let (bus, receiver) = broadcast::channel(WEBUI_SERVICE_EVENT_BUS_CAPACITY);
        // Each send waits for one permit, so the test decides when the browser drains.
        // 每次发送等待一个许可，由测试决定浏览器何时消费。
        let permits = Arc::new(tokio::sync::Semaphore::new(0));
        let delivered = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let sink = futures_util::sink::unfold(
            (permits.clone(), delivered.clone()),
            |(permits, delivered), message: AxumWsMessage| async move {
                permits.acquire().await.expect("gate open").forget();
                let AxumWsMessage::Text(text) = message else {
                    panic!("expected text frame");
                };
                delivered
                    .lock()
                    .expect("delivered log")
                    .push(serde_json::from_str(text.as_str()).expect("event json"));
                Ok::<_, axum::Error>((permits, delivered))
            },
        );
        let pump = tokio::spawn(pump_webui_service_events(
            Box::pin(sink),
            futures_util::stream::pending(),
            receiver,
            Uuid::new_v4(),
        ));

        bus.send(volatile_delta("a", 0, 1))
            .expect("bus has the pump subscriber");
        wait_until_bus_drained(&bus).await;
        for event in [
            volatile_delta("a", 1, 2),
            volatile_delta("a", 2, 3),
            volatile_delta("a", 3, 4),
            unmergeable_event(9),
            volatile_delta("a", 4, 5),
        ] {
            bus.send(event).expect("bus has the pump subscriber");
        }
        wait_until_bus_drained(&bus).await;
        permits.add_permits(usize::MAX >> 4);
        while delivered.lock().expect("delivered log").len() < 4 {
            tokio::task::yield_now().await;
        }
        drop(bus);
        pump.await.expect("pump exits when the bus closes");

        let delivered = delivered.lock().expect("delivered log");
        let shape = delivered
            .iter()
            .map(|event| {
                (
                    event["kind"].as_str().expect("kind").to_string(),
                    event["payload"]["base_memory_revision"].as_u64(),
                    event["payload"]["revision"].as_u64().expect("revision"),
                )
            })
            .collect::<Vec<_>>();
        let delta = String::from("conversationEventDeltaV1");
        assert_eq!(
            shape,
            vec![
                (delta.clone(), Some(0), 1),
                (delta.clone(), Some(1), 4),
                (String::from("pluginUiInvalidated"), None, 9),
                (delta, Some(4), 5),
            ]
        );
    }

    /// A blocked browser overflowing its private outbox cannot terminate another browser's stream.
    /// 阻塞浏览器的私有队列溢出不能终止另一个浏览器的推送。
    #[tokio::test]
    async fn overflowing_browser_does_not_interrupt_another_browser() {
        // Both sockets observe the same bus, as desktop and web do even on different sessions.
        // 两条连接观察同一总线，对应桌面端与网页端即使选中不同会话的行为。
        let (bus, slow_receiver) = broadcast::channel(WEBUI_SERVICE_EVENT_BUS_CAPACITY);
        let fast_receiver = bus.subscribe();
        // The slow socket never completes its first send; dropping its pump must release its receiver.
        // 慢连接永远无法完成首次发送；丢弃发送泵必须释放其订阅。
        let slow_sink = futures_util::sink::unfold((), |(), _: AxumWsMessage| {
            futures_util::future::pending::<Result<(), axum::Error>>()
        });
        let slow = tokio::spawn(pump_webui_service_events(
            Box::pin(slow_sink),
            futures_util::stream::pending(),
            slow_receiver,
            Uuid::new_v4(),
        ));
        // The healthy socket records every revision without artificial backpressure.
        // 健康连接记录每个修订，不施加人为背压。
        let delivered = Arc::new(Mutex::new(Vec::<u64>::new()));
        let fast_sink =
            futures_util::sink::unfold(delivered.clone(), |delivered, message| async move {
                let AxumWsMessage::Text(text) = message else {
                    panic!("expected event text");
                };
                let event: serde_json::Value =
                    serde_json::from_str(text.as_str()).expect("event json");
                delivered
                    .lock()
                    .expect("delivery log")
                    .push(event["payload"]["revision"].as_u64().expect("revision"));
                Ok::<_, axum::Error>(delivered)
            });
        let fast = tokio::spawn(pump_webui_service_events(
            Box::pin(fast_sink),
            futures_util::stream::pending(),
            fast_receiver,
            Uuid::new_v4(),
        ));
        // One in-flight send plus a full queue plus one overflow deterministically closes only the slow pump.
        // 一条在途发送、一个满队列及一条溢出事件确定性地仅关闭慢发送泵。
        let event_count = WEBUI_SERVICE_EVENT_BUS_CAPACITY as u64 + 2;
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            for revision in 1..=event_count {
                bus.send(unmergeable_event(revision))
                    .expect("active browsers");
                wait_until_bus_drained(&bus).await;
            }
            slow.await.expect("overflow releases slow browser");
            assert_eq!(bus.receiver_count(), 1);
            bus.send(unmergeable_event(event_count + 1))
                .expect("healthy browser survives");
            while delivered.lock().expect("delivery log").len() < (event_count + 1) as usize {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("overflow and independent delivery complete within deadline");
        drop(bus);
        fast.await.expect("healthy pump exits on bus close");
        assert_eq!(
            *delivered.lock().expect("delivery log"),
            (1..=event_count + 1).collect::<Vec<_>>()
        );
    }

    /// Yields until the pump has taken every event off the bus.
    /// 让出执行权，直到泵已从总线取走全部事件。
    async fn wait_until_bus_drained(bus: &broadcast::Sender<ServiceEvent>) {
        while !bus.is_empty() {
            tokio::task::yield_now().await;
        }
    }

    /// Unmergeable events are bounded by the bus capacity; deltas that merge never overflow it.
    /// 不可合并事件以总线容量为上限；可合并的增量不会使其溢出。
    #[test]
    fn outbox_bounds_unmergeable_events_by_bus_capacity() {
        let mut outbox = WebuiServiceEventOutbox::default();
        outbox.push(volatile_delta("a", 0, 1)).expect("queue delta");
        for revision in 1..=(WEBUI_SERVICE_EVENT_BUS_CAPACITY as u64) * 2 {
            outbox
                .push(volatile_delta("a", revision, revision + 1))
                .expect("merged delta never overflows");
        }
        assert_eq!(outbox.entries.len(), 1);
        for revision in 1..WEBUI_SERVICE_EVENT_BUS_CAPACITY as u64 {
            outbox
                .push(unmergeable_event(revision))
                .expect("within capacity");
        }
        assert_eq!(outbox.entries.len(), WEBUI_SERVICE_EVENT_BUS_CAPACITY);
        let full = outbox
            .push(unmergeable_event(0))
            .expect_err("outbox is full");
        assert_eq!(full.events, WEBUI_SERVICE_EVENT_BUS_CAPACITY);
        assert_eq!(full.bytes, outbox.bytes);
    }

    /// Merged deltas are split at the single-message bound and the whole backlog at the byte
    /// bound, after which the socket must close; the queued ranges stay contiguous.
    /// 合并增量在单条消息上限处拆分，整个积压在字节上限处截止，此后套接字必须关闭；已排队的
    /// 区间保持相邻。
    #[test]
    fn outbox_bounds_merged_delta_and_pending_bytes() {
        const CHUNK: usize = 16 * 1024;
        let text = "x".repeat(CHUNK);
        let mut outbox = WebuiServiceEventOutbox::default();
        let mut queued = 0_u64;
        let full = loop {
            match outbox.push(text_append_delta(queued, queued * CHUNK as u64, &text)) {
                Ok(()) => queued += 1,
                Err(full) => break full,
            }
            assert!(
                queued as usize * CHUNK <= WEBUI_SERVICE_EVENT_OUTBOX_MAX_BYTES * 2,
                "the byte bound never stopped the backlog"
            );
        };

        // The rejected delta, smaller than two chunks, would have crossed the byte bound.
        // 被拒绝的增量小于两个分块，会越过字节上限；待发送队列仍在上限之内。
        assert_eq!(full.bytes, outbox.bytes);
        assert!(outbox.bytes <= WEBUI_SERVICE_EVENT_OUTBOX_MAX_BYTES);
        assert!(outbox.bytes + 2 * CHUNK > WEBUI_SERVICE_EVENT_OUTBOX_MAX_BYTES);
        assert!(outbox.entries.len() > 1, "merged deltas were never split");
        let mut tracked = 0;
        for entry in &outbox.entries {
            let WebuiServiceEventOutboxEntry::ConversationDelta(delta) = entry else {
                panic!("expected only conversation deltas");
            };
            assert!(delta.serialized_bytes() <= WEBUI_SERVICE_EVENT_MERGED_DELTA_MAX_BYTES);
            tracked += delta.serialized_bytes();
        }
        assert_eq!(tracked, outbox.bytes);

        // Delivered payloads rebuild the whole text in order, one contiguous range after another.
        // 交付的载荷按序、逐个相邻区间重建完整正文。
        let mut revision = 0;
        let mut text_len = 0;
        while let Some(json) = outbox.pop_serialized() {
            let event: serde_json::Value = serde_json::from_str(&json).expect("event json");
            let payload = &event["payload"];
            assert_eq!(payload["base_memory_revision"], revision);
            assert_eq!(payload["text_appends"][0]["base_text_utf16_len"], text_len);
            revision = payload["revision"].as_u64().expect("revision");
            text_len += payload["text_appends"][0]["appended_text"]
                .as_str()
                .expect("appended text")
                .len() as u64;
        }
        assert_eq!(revision, queued);
        assert_eq!(text_len, queued * CHUNK as u64);
        assert_eq!(outbox.bytes, 0);
    }

    /// An empty outbox admits one event beyond the byte bound, so it cannot loop reconnecting.
    /// 空的待发送队列会接纳一个超出字节上限的事件，因此不会陷入重连循环。
    #[test]
    fn outbox_admits_one_oversized_event_when_empty() {
        let mut outbox = WebuiServiceEventOutbox::default();
        let oversized = "x".repeat(WEBUI_SERVICE_EVENT_OUTBOX_MAX_BYTES);
        outbox
            .push(text_append_delta(0, 0, &oversized))
            .expect("an empty outbox admits one event");
        let full = outbox
            .push(unmergeable_event(1))
            .expect_err("nothing fits behind an oversized event");
        assert_eq!(full.events, 1);
        assert!(outbox.pop_serialized().is_some());
        outbox
            .push(unmergeable_event(1))
            .expect("the drained outbox admits events again");
    }

    /// Builds a minimal test WebUI service router.
    /// 构建最小化测试 WebUI 服务路由。
    fn test_router() -> Router {
        let dispatcher: WebuiServiceRpcDispatcher = Arc::new(|method, params| {
            Box::pin(async move {
                Ok(json!({
                    "method": method,
                    "params": params,
                }))
            })
        });
        test_router_with_dispatcher(dispatcher)
    }

    /// Builds an isolated router with the supplied dispatcher; returns the real HTTP router.
    /// 使用提供的分发器构造隔离路由；返回真实 HTTP 路由。
    fn test_router_with_dispatcher(dispatcher: WebuiServiceRpcDispatcher) -> Router {
        let (event_bus, _) = broadcast::channel(8);
        build_webui_service_router(WebuiServiceHttpConfig {
            discovery: Arc::new(Mutex::new(webui_service_ready_discovery_snapshot(
                WEBUI_SERVICE_HTTP_PORT,
                42,
                "123",
                "test-token",
            ))),
            service_token: String::from("test-token"),
            event_bus,
            static_root: None,
            rpc_dispatcher: dispatcher,
            session_resource_resolver: Arc::new(|request| {
                Box::pin(async move {
                    build_service_project_resource_response(
                        format!("{}:{}", request.session_id, request.resource_id).as_bytes(),
                        Some("text/plain"),
                        request.is_head,
                        request.range_header.as_deref(),
                    )
                })
            }),
            client_disconnected: Arc::new(|_client_id| {}),
            status_provider: None,
            browser_stream: None,
            browser_download_resolver: None,
            mcp_endpoint: None,
        })
    }

    /// Builds a minimal status provider used by protected status route tests.
    /// 构建受保护状态路由测试使用的最小状态提供器。
    fn test_status_provider() -> WebuiServiceStatusProvider {
        use vulcan_service_api::ServiceActiveWorkSummary;
        use vulcan_service_api::ServiceLifecycle;
        Arc::new(|| ServiceStatusSnapshot {
            lifecycle: ServiceLifecycle::Ready,
            pid: 42,
            started_at: String::from("123"),
            revision: 7,
            last_failure: None,
            active_work: ServiceActiveWorkSummary::default(),
        })
    }

    /// Verifies a different loopback website cannot read the discovery token.
    /// 验证其他回环网站无法读取发现令牌。
    #[tokio::test]
    async fn health_rejects_unrelated_loopback_origin() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header("origin", "http://127.0.0.1:5173")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(response.into_body(), 1024)
            .await
            .expect("body should read");
        assert!(!String::from_utf8_lossy(&body).contains("test-token"));
    }

    /// Verifies the service WebUI and native probes retain non-cacheable discovery.
    /// 验证服务 WebUI 和原生探测仍可获取不可缓存的发现信息。
    #[tokio::test]
    async fn health_accepts_service_origin_and_native_probe() {
        for origin in [
            Some(format!("http://127.0.0.1:{WEBUI_SERVICE_HTTP_PORT}")),
            None,
        ] {
            let mut request = Request::builder().uri("/health");
            if let Some(origin) = &origin {
                request = request.header("origin", origin);
            }
            let response = test_router()
                .oneshot(request.body(Body::empty()).expect("request should build"))
                .await
                .expect("router should respond");
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()["cache-control"], "no-store");
            let body = to_bytes(response.into_body(), 1024)
                .await
                .expect("body should read");
            let snapshot: WebuiServiceDiscoverySnapshot =
                serde_json::from_slice(&body).expect("discovery should parse");
            assert_eq!(snapshot.service_token, "test-token");
        }
    }

    /// Verifies a rebound DNS name cannot read discovery while loopback names still can.
    /// 验证重新绑定的 DNS 名无法读取发现信息，而回环名称仍然可以。
    #[tokio::test]
    async fn health_requires_loopback_host() {
        for (host, expected) in [
            ("attacker.example:43110", StatusCode::FORBIDDEN),
            ("127.0.0.1:43110", StatusCode::OK),
            ("localhost:43110", StatusCode::OK),
        ] {
            let response = test_router()
                .oneshot(
                    Request::builder()
                        .uri("/health")
                        .header("host", host)
                        .body(Body::empty())
                        .expect("request should build"),
                )
                .await
                .expect("router should respond");
            assert_eq!(response.status(), expected, "{host}");
            let body = to_bytes(response.into_body(), 4096)
                .await
                .expect("body should read");
            assert_eq!(
                String::from_utf8_lossy(&body).contains("test-token"),
                expected == StatusCode::OK,
                "{host}"
            );
        }
    }

    /// Verifies only a debug build admits the fixed Vite workbench origin.
    /// 验证仅调试构建允许固定的 Vite 工作台来源。
    #[tokio::test]
    async fn health_debug_vite_origin_matches_build_profile() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .header("origin", "http://127.0.0.1:17601")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        let expected = if cfg!(debug_assertions) {
            StatusCode::OK
        } else {
            StatusCode::FORBIDDEN
        };
        assert_eq!(response.status(), expected);
    }

    /// Unauthorized bodies must never be polled, even when they advertise a large or invalid carrier.
    /// 未授权请求体绝不能被轮询，即使请求声明了超大或非法载体。
    #[tokio::test]
    async fn browser_rpc_admission_precedes_every_body_read() {
        for (token, origin, expected) in [
            (None, None, StatusCode::UNAUTHORIZED),
            (Some("wrong-token"), None, StatusCode::UNAUTHORIZED),
            (
                Some("test-token"),
                Some("https://untrusted.invalid"),
                StatusCode::FORBIDDEN,
            ),
        ] {
            let unread = futures_util::stream::poll_fn(
                |_| -> std::task::Poll<Option<Result<axum::body::Bytes, std::io::Error>>> {
                    panic!("authentication must reject before polling any request-body bytes")
                },
            );
            let mut request = Request::builder()
                .method("POST")
                .uri("/rpc")
                .header("content-type", "application/json")
                .header(
                    "content-length",
                    super::WEBUI_SERVICE_RPC_BODY_LIMIT_BYTES.to_string(),
                );
            if let Some(token) = token {
                request = request.header("x-vulcan-service-token", token);
            }
            if let Some(origin) = origin {
                request = request.header("origin", origin);
            }
            let response = test_router()
                .oneshot(request.body(Body::from_stream(unread)).expect("request"))
                .await
                .expect("admission response");
            assert_eq!(response.status(), expected);
            let bytes = to_bytes(response.into_body(), 4096)
                .await
                .expect("bounded rejection");
            let envelope: serde_json::Value =
                serde_json::from_slice(&bytes).expect("existing error envelope");
            assert_eq!(envelope["ok"], false);
        }
    }

    /// Authorized requests still pass through JSON syntax and size validation after admission.
    /// 授权请求仍在准入后经过 JSON 语法与大小校验。
    #[tokio::test]
    async fn browser_rpc_admission_does_not_bypass_body_validation() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::from("{invalid"))
                    .expect("request"),
            )
            .await
            .expect("parser response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// Verifies HTTP preserves every dispatch category independently of diagnostic wording.
    /// 验证 HTTP 保留每个分发类别，不依赖诊断文案。
    #[tokio::test]
    async fn browser_rpc_preserves_typed_failure_codes() {
        use vulcan_service_api::WebuiServiceRpcError;
        for (error, code) in [
            (
                WebuiServiceRpcError::InvalidParams("unknown method".into()),
                "invalid_params",
            ),
            (
                WebuiServiceRpcError::UnknownMethod("any".into()),
                "method_not_found",
            ),
            (
                WebuiServiceRpcError::Service("invalid params".into()),
                "service_rpc_error",
            ),
            (
                WebuiServiceRpcError::Serialization("任意文案".into()),
                "response_serialization_failed",
            ),
        ] {
            let expected_message = error.to_string();
            let dispatcher: WebuiServiceRpcDispatcher = Arc::new(move |_, _| {
                let error = error.clone();
                Box::pin(async move { Err(error) })
            });
            let response = test_router_with_dispatcher(dispatcher)
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/rpc")
                        .header("content-type", "application/json")
                        .header("x-vulcan-service-token", "test-token")
                        .body(Body::from(r#"{"method":"fixture","params":{}}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), 4096).await.unwrap();
            let envelope: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(envelope["error"]["code"], code);
            assert_eq!(envelope["error"]["message"], expected_message);
        }
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

    /// Verifies the session-resource route authenticates and forwards only logical identities.
    /// 验证会话资源路由会进行鉴权且仅转发逻辑身份。
    #[tokio::test]
    async fn session_resource_route_forwards_opaque_identity() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/v1/session-resources/session-a/17")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), AxumStatusCode::OK);
        let body = to_bytes(response.into_body(), 1024)
            .await
            .expect("resource body should read");
        assert_eq!(body.as_ref(), b"session-a:17");
    }

    /// Verifies the session-resource route rejects requests without the service token.
    /// 验证会话资源路由会拒绝缺少服务令牌的请求。
    #[tokio::test]
    async fn session_resource_route_rejects_missing_token() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/v1/session-resources/session-a/17")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), AxumStatusCode::UNAUTHORIZED);
    }

    /// Verifies browser RPC accepts payloads above Axum's default two megabyte JSON limit.
    /// 验证浏览器 RPC 可以接收超过 Axum 默认两兆 JSON 限制的载荷。
    #[tokio::test]
    async fn browser_rpc_accepts_large_authorized_request_body() {
        // large_clipboard_payload simulates base64-rich clipboard data pasted from a browser.
        // large_clipboard_payload 模拟从浏览器粘贴时包含大量 base64 数据的剪贴板载荷。
        let large_clipboard_payload = "x".repeat(2_097_152 + 1024);

        // request_body stores one JSON RPC body that would exceed Axum's default body limit.
        // request_body 保存一个会超过 Axum 默认请求体限制的 JSON RPC 请求体。
        let request_body = json!({
            "method": "workbench.snapshot",
            "params": {
                "clipboardData": large_clipboard_payload,
            },
        })
        .to_string();

        // response stores the HTTP response produced by the browser service router.
        // response 保存浏览器服务路由生成的 HTTP 响应。
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/rpc")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::from(request_body))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), AxumStatusCode::OK);
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
        headers.insert(
            "origin",
            "https://example.com"
                .parse()
                .expect("disallowed test origin should be a valid header value"),
        );

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

    /// Verifies the SPA fallback never masks a missing service API route.
    /// 验证 SPA 回退绝不掩盖缺失的服务 API 路由。
    #[test]
    fn reserved_service_paths_are_not_spa_fallback_candidates() {
        // Reserved API surfaces must stay API errors.
        // 保留的 API 表面必须保持为 API 错误。
        for path in [
            "/v1/unknown",
            "/v1/",
            "/rpc/extra",
            "/events/extra",
            "/asset/missing",
            "/health/detail",
        ] {
            assert!(
                webui_service_path_is_reserved_api(path),
                "{path} must be treated as a service API path"
            );
        }
        // Real browser routes and static files still reach the SPA fallback.
        // 真实浏览器路由与静态文件仍会走 SPA 回退。
        for path in [
            "/",
            "/index.html",
            "/workbench",
            "/settings/profile",
            "/assets/app.js",
        ] {
            assert!(
                !webui_service_path_is_reserved_api(path),
                "{path} must remain a static or SPA path"
            );
        }
    }

    /// Builds a status-enabled test router sharing the minimal test RPC surface.
    /// 构建共享最小测试 RPC 的带状态测试路由。
    fn status_test_router() -> Router {
        build_webui_service_router(status_test_router_config())
    }

    /// Builds the configuration behind `status_test_router`.
    /// 构建 `status_test_router` 背后的配置。
    fn status_test_router_config() -> WebuiServiceHttpConfig {
        let (event_bus, _) = broadcast::channel(8);
        let dispatcher: WebuiServiceRpcDispatcher = Arc::new(|method, params| {
            Box::pin(async move {
                Ok(json!({
                    "method": method,
                    "params": params,
                }))
            })
        });
        WebuiServiceHttpConfig {
            discovery: Arc::new(Mutex::new(webui_service_ready_discovery_snapshot(
                WEBUI_SERVICE_HTTP_PORT,
                42,
                "123",
                "test-token",
            ))),
            service_token: String::from("test-token"),
            event_bus,
            static_root: None,
            rpc_dispatcher: dispatcher,
            session_resource_resolver: Arc::new(|request| {
                Box::pin(async move {
                    build_service_project_resource_response(
                        format!("{}:{}", request.session_id, request.resource_id).as_bytes(),
                        Some("text/plain"),
                        request.is_head,
                        request.range_header.as_deref(),
                    )
                })
            }),
            client_disconnected: Arc::new(|_client_id| {}),
            status_provider: Some(test_status_provider()),
            browser_stream: None,
            browser_download_resolver: None,
            mcp_endpoint: None,
        }
    }

    /// Browser stream releases recorded by the test facade as `(session, subscription)` pairs.
    /// 测试门面记录的浏览器流释放，形如 `(session, subscription)` 对。
    type RecordedStreamReleases = Arc<Mutex<Vec<(String, String)>>>;

    /// Builds a test router with an injected browser stream facade.
    /// 构建一个注入了浏览器流门面的测试路由。
    fn test_router_with_browser_stream() -> (Router, RecordedStreamReleases) {
        let releases = Arc::new(Mutex::new(Vec::new()));
        let mut config = WebuiServiceHttpConfig {
            discovery: Arc::new(Mutex::new(webui_service_ready_discovery_snapshot(
                WEBUI_SERVICE_HTTP_PORT,
                42,
                "123",
                "test-token",
            ))),
            service_token: String::from("test-token"),
            event_bus: broadcast::channel(8).0,
            static_root: None,
            rpc_dispatcher: Arc::new(|_method, _params| Box::pin(async move { Ok(json!(null)) })),
            session_resource_resolver: Arc::new(|_request| {
                Box::pin(async move { Err(String::from("unused")) })
            }),
            client_disconnected: Arc::new(|_client_id| {}),
            status_provider: None,
            browser_stream: None,
            browser_download_resolver: None,
            mcp_endpoint: None,
        };
        config.browser_stream = Some(WebuiServiceBrowserStream {
            issue_ticket: Arc::new(|session_id, owner_session_id| {
                Box::pin(async move {
                    // The fixture keeps two browser owners distinct, matching the Core callback contract.
                    // 夹具区分两个浏览器所有者，与 Core 回调契约一致。
                    let correct_owner = match session_id.as_str() {
                        "session-a" => "conversation-a",
                        "session-b" => "conversation-b",
                        _ => return Err(String::from("unknown_session")),
                    };
                    if owner_session_id != correct_owner {
                        return Err(String::from("unknown_session"));
                    }
                    Ok(json!({ "ticket": "ticket-1", "sessionId": session_id }))
                })
            }),
            resume: Arc::new(|ticket, page_id, subscription_id, cursor| {
                Box::pin(async move {
                    if ticket != "ticket-1" {
                        return Err(String::from("invalid_request"));
                    }
                    Ok(json!({
                        "mode": "snapshot",
                        "pageGeneration": "1",
                        "frames": [],
                        "droppedFrames": 0,
                        "discardedInputs": 0,
                        "sessionId": "session-a",
                        "subscriptionId": subscription_id,
                        "pageId": page_id,
                        "cursor": cursor,
                    }))
                })
            }),
            poll: Arc::new(|_session_id, _subscription_id, cursor| {
                Box::pin(async move {
                    // A continuing stream answers with the generation the client is already on, so
                    // the socket keeps delivering instead of resynchronising.
                    // 持续流以客户端已处于的代次应答，因此套接字继续交付而不是重新同步。
                    Ok(json!({
                        "mode": "incremental",
                        "pageGeneration": cursor
                            .get("pageGeneration")
                            .and_then(|value| value.as_str())
                            .unwrap_or("1"),
                        "frames": [],
                        "droppedFrames": 0,
                        "discardedInputs": 0,
                    }))
                })
            }),
            release: Arc::new({
                let releases = Arc::clone(&releases);
                move |session_id, subscription_id| {
                    let releases = Arc::clone(&releases);
                    Box::pin(async move {
                        releases
                            .lock()
                            .expect("release log")
                            .push((session_id, subscription_id));
                        Ok(json!({ "released": true }))
                    })
                }
            }),
        });
        (build_webui_service_router(config), releases)
    }

    /// Verifies the typed browser stream routes enforce token and Origin boundaries.
    /// 验证类型化浏览器流路由强制令牌与 Origin 边界。
    #[tokio::test]
    async fn typed_browser_stream_routes_enforce_authentication() {
        let (router, releases) = test_router_with_browser_stream();
        let body =
            json!({ "sessionId": "session-a", "ownerSessionId": "conversation-a" }).to_string();

        let unauthenticated = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/browser/stream-ticket")
                    .header("content-type", "application/json")
                    .body(Body::from(body.clone()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        let untrusted_origin = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/browser/stream-ticket")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .header("origin", "https://example.test")
                    .body(Body::from(body.clone()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(untrusted_origin.status(), StatusCode::FORBIDDEN);

        // A missing sessionId is a client error, not a silent success.
        // 缺少 sessionId 是客户端错误，而不是静默成功。
        let malformed = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/browser/stream-ticket")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::from(json!({}).to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);

        // The owner is mandatory even when a valid browser ID and service token are supplied.
        // 即使提供有效浏览器 ID 和服务令牌，也必须提供所有者。
        let no_owner = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/browser/stream-ticket")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::from(json!({ "sessionId": "session-a" }).to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(no_owner.status(), StatusCode::BAD_REQUEST);

        // A kernel refusal is reported as a refusal.
        // 内核拒绝被报告为拒绝。
        let refused = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/browser/stream-ticket")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::from(
                        json!({ "sessionId": "missing", "ownerSessionId": "conversation-a" })
                            .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(refused.status(), StatusCode::CONFLICT);

        // A valid service token and another known browser ID do not grant its owner's stream ticket.
        // 有效服务令牌加上另一个已知浏览器 ID 也不能取得其所有者的流凭据。
        let cross_owner = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/browser/stream-ticket")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::from(
                        json!({ "sessionId": "session-b", "ownerSessionId": "conversation-a" })
                            .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(cross_owner.status(), StatusCode::CONFLICT);

        // The same browser remains observable through its correct conversation route.
        // 同一浏览器仍可通过其正确的会话路由观察。
        let own_browser = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/browser/stream-ticket")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::from(
                        json!({ "sessionId": "session-b", "ownerSessionId": "conversation-b" })
                            .to_string(),
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(own_browser.status(), StatusCode::OK);

        let issued = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/browser/stream-ticket")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::from(body))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(issued.status(), StatusCode::OK);
        let issued_body = axum::body::to_bytes(issued.into_body(), usize::MAX)
            .await
            .expect("body");
        assert!(String::from_utf8_lossy(&issued_body).contains("ticket-1"));

        // A resume without a ticket is refused before any stream is opened.
        // 缺少凭据的恢复在任何流开启之前即被拒绝。
        let no_ticket = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/browser/stream/snapshot?subscriptionId=observer-a")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(no_ticket.status(), StatusCode::BAD_REQUEST);

        // A cursor supplied in the query reaches the resolver as a structured cursor.
        // 查询中提供的游标以结构化游标形式到达解析器。
        let resumed = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/browser/stream/snapshot?ticket=ticket-1&pageId=page-1&subscriptionId=observer-a&pageGeneration=1&frameSequence=4")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(resumed.status(), StatusCode::OK);
        let resumed_body = axum::body::to_bytes(resumed.into_body(), usize::MAX)
            .await
            .expect("body");
        let resumed_text = String::from_utf8_lossy(&resumed_body);
        assert!(resumed_text.contains("\"mode\":\"snapshot\""));
        assert!(resumed_text.contains("frameSequence"));
        assert_eq!(
            *releases.lock().expect("release log"),
            vec![(String::from("session-a"), String::from("observer-a"))]
        );
    }

    /// Verifies the typed browser routes are absent when no stream is configured.
    /// 验证未配置流时类型化浏览器路由不存在。
    #[tokio::test]
    async fn typed_browser_stream_routes_are_absent_without_configuration() {
        let router = test_router();
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/browser/stream-ticket")
                    .header("content-type", "application/json")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::from(json!({ "sessionId": "session-a" }).to_string()))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    /// Verifies the protected status route rejects requests without the service token.
    /// 验证受保护状态路由会拒绝缺少服务令牌的请求。
    #[tokio::test]
    async fn service_status_route_rejects_missing_token() {
        let response = status_test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/v1/service/status")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), AxumStatusCode::UNAUTHORIZED);
    }

    /// Verifies the protected status route serves the owner-supplied lifecycle snapshot.
    /// 验证受保护状态路由提供所有者给出的生命周期快照。
    #[tokio::test]
    async fn service_status_route_serves_owner_snapshot() {
        let response = status_test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/v1/service/status")
                    .header("x-vulcan-service-token", "test-token")
                    .body(Body::empty())
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
        assert_eq!(payload["lifecycle"], "ready");
        assert_eq!(payload["revision"], 7);
        assert_eq!(payload["activeWork"]["activeStreamTasks"], 0);
    }

    /// Builds a router whose MCP endpoint echoes the authorization header and body length.
    /// 构建一个 MCP 端点回显授权标头与请求体长度的路由。
    fn mcp_test_router() -> Router {
        let mut router_config = status_test_router_config();
        router_config.mcp_endpoint = Some(Arc::new(|authorization, body| {
            Box::pin(async move {
                Ok(ServiceMediaResponse {
                    status: 200,
                    headers: vec![(
                        String::from("content-type"),
                        String::from("application/json"),
                    )],
                    body: serde_json::to_vec(&json!({
                        "authorization": authorization,
                        "length": body.len(),
                    }))
                    .expect("echo body"),
                })
            })
        }));
        build_webui_service_router(router_config)
    }

    /// Sends one request to the MCP route.
    /// 向 MCP 路由发送一个请求。
    async fn mcp_request(
        router: Router,
        method: &str,
        origin: Option<&str>,
    ) -> axum::http::Response<Body> {
        let mut request = axum::http::Request::builder()
            .method(method)
            .uri("/mcp")
            .header("authorization", "Bearer vmcp_test");
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        router
            .oneshot(
                request
                    .body(Body::from("{}"))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond")
    }

    /// Verifies the MCP route forwards POST bodies and the bearer header to the kernel endpoint.
    /// 验证 MCP 路由把 POST 请求体与 Bearer 标头转交给内核端点。
    #[tokio::test]
    async fn mcp_route_forwards_posts_to_the_endpoint() {
        let response = mcp_request(mcp_test_router(), "POST", None).await;
        assert_eq!(response.status(), AxumStatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should read");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("json body");
        assert_eq!(payload["authorization"], "Bearer vmcp_test");
        assert_eq!(payload["length"], 2);
    }

    /// Verifies browser origins, other methods, and a missing endpoint are refused.
    /// 验证浏览器来源、其他方法与缺失端点都会被拒绝。
    #[tokio::test]
    async fn mcp_route_refuses_browsers_other_methods_and_absent_endpoints() {
        let response = mcp_request(mcp_test_router(), "POST", Some("http://127.0.0.1:17601")).await;
        assert_eq!(response.status(), AxumStatusCode::FORBIDDEN);
        let response = mcp_request(mcp_test_router(), "GET", None).await;
        assert_eq!(response.status(), AxumStatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::ALLOW)
                .and_then(|value| value.to_str().ok()),
            Some("POST")
        );
        let response = mcp_request(status_test_router(), "POST", None).await;
        assert_eq!(response.status(), AxumStatusCode::NOT_FOUND);
    }
}
