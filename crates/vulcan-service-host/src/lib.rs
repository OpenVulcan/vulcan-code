//! HTTP and WebSocket host for the Vulcan Core Service.
//! Vulcan Core Service 的 HTTP 与 WebSocket 宿主层。

use std::{
    collections::{BTreeMap, HashMap},
    convert::Infallible,
    fs::{self, OpenOptions},
    future::Future,
    io::Write,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    body::Body as AxumBody,
    extract::{
        DefaultBodyLimit, OriginalUri as AxumOriginalUri, Path as AxumPath, State as AxumState,
        rejection::JsonRejection,
        ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade},
    },
    http::{
        HeaderMap as AxumHeaderMap, HeaderValue as AxumHeaderValue, Method as AxumMethod,
        StatusCode as AxumStatusCode,
    },
    response::{
        IntoResponse, Response as AxumResponse,
        sse::{Event as AxumSseEvent, KeepAlive, Sse},
    },
    routing::{get, post},
};
use tokio::{net::TcpListener, sync::broadcast};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use uuid::Uuid;
use vulcan_ai_contract::{
    AiStreamEvent, ContentItem, EmbeddingRequest, RequestUsage, RerankRequest, UsageValue,
    derive_usage_sum,
};
use vulcan_service_api::{
    INFERENCE_SERVICE_API_VERSION, InferenceServiceDiscoverySnapshot,
    InferenceServiceErrorEnvelope, InferenceServiceHealthSnapshot, InferenceServiceHttpError,
    InferenceServiceRequest, InferenceServiceSuccessEnvelope, OpenAiErrorEnvelope,
    OpenAiErrorObject, ResponsesCreateRequest, ServiceEvent, ServiceEventSink,
    ServiceMediaResponse, ServiceStaticResponse, WebuiServiceDiscoverySnapshot,
    WebuiServiceRpcHttpRequest, build_service_media_file_response, build_webui_static_response,
    inference_service_discovery_snapshot_path, service_media_error_response,
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

/// Cleans client-owned display subscriptions when one browser event socket disconnects.
/// 在浏览器事件套接字断开时清理客户端持有的显示订阅。
pub type WebuiServiceClientDisconnected = Arc<dyn Fn(Uuid) + Send + Sync + 'static>;

/// Boxed asynchronous result returned by the controlled inference dispatcher.
/// 受控推理分发器返回的装箱异步结果。
pub type InferenceServiceRpcFuture = Pin<
    Box<dyn Future<Output = Result<serde_json::Value, InferenceServiceHttpError>> + Send + 'static>,
>;

/// Dispatches one versioned inference request after kernel-owned token authorization.
/// 在内核拥有的令牌授权后分派一项版本化推理请求。
pub type InferenceServiceRpcDispatcher = Arc<
    dyn Fn(String, InferenceServiceRequest) -> InferenceServiceRpcFuture + Send + Sync + 'static,
>;

/// Cancels one active Responses execution when its streaming consumer disconnects.
/// 当流式消费者断开时取消一项活动 Responses 执行。
pub struct InferenceServiceCancellationGuard {
    /// One-shot synchronous cancellation callback owned by the kernel.
    /// 由内核持有的一次性同步取消回调。
    cancel: Option<Box<dyn FnOnce() + Send + 'static>>,
}

impl InferenceServiceCancellationGuard {
    /// Creates one armed cancellation guard.
    /// 创建一个已启用的取消守卫。
    pub fn new(cancel: impl FnOnce() + Send + 'static) -> Self {
        Self {
            cancel: Some(Box::new(cancel)),
        }
    }

    /// Disarms cancellation after the execution emits a terminal event.
    /// 在执行发出终态事件后解除取消。
    pub fn disarm(&mut self) {
        self.cancel = None;
    }
}

impl Drop for InferenceServiceCancellationGuard {
    /// Signals cooperative cancellation only while the guard remains armed.
    /// 仅在守卫仍启用时发出协作式取消信号。
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel();
        }
    }
}

/// Carries ordered replay plus live delivery for one Responses stream.
/// 承载一项 Responses 流的有序重放与实时投递。
pub struct InferenceServiceEventSubscription {
    /// Events emitted before the subscriber attached.
    /// 订阅者接入前已经发出的事件。
    pub replay: Vec<AiStreamEvent>,
    /// Live bounded receiver created atomically with the replay snapshot.
    /// 与重放快照原子创建的实时有界接收器。
    pub receiver: broadcast::Receiver<AiStreamEvent>,
    /// Cancellation guard armed until the stream observes a terminal event.
    /// 在流观察到终态事件前保持启用的取消守卫。
    pub cancellation_guard: InferenceServiceCancellationGuard,
}

/// Boxed event-subscription result for one caller-owned LLM execution.
/// 一项调用方所有 LLM 执行的装箱事件订阅结果。
pub type InferenceServiceEventFuture = Pin<
    Box<
        dyn Future<Output = Result<InferenceServiceEventSubscription, InferenceServiceHttpError>>
            + Send
            + 'static,
    >,
>;

/// Authorizes and opens one restricted LLM event subscription.
/// 授权并打开一项受限 LLM 事件订阅。
pub type InferenceServiceEventSubscriber =
    Arc<dyn Fn(String, String) -> InferenceServiceEventFuture + Send + Sync + 'static>;

/// Shared service discovery snapshot used by health checks while readiness changes.
/// 健康检查在就绪状态变化时使用的共享服务发现快照。
pub type SharedWebuiServiceDiscoverySnapshot = Arc<Mutex<WebuiServiceDiscoverySnapshot>>;

/// Shared protected inference discovery snapshot used by health checks and rotation.
/// 健康检查与轮换使用的共享受保护推理发现快照。
pub type SharedInferenceServiceDiscoverySnapshot = Arc<Mutex<InferenceServiceDiscoverySnapshot>>;

/// Maximum JSON body size accepted by the browser-facing RPC endpoint.
/// 浏览器侧 RPC 端点允许接收的最大 JSON 请求体大小。
const WEBUI_SERVICE_RPC_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// Maximum JSON body size accepted by one inference operation.
/// 单项推理操作允许接收的最大 JSON 请求体大小。
const INFERENCE_SERVICE_BODY_LIMIT_BYTES: usize = 16 * 1024 * 1024;

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

/// Persists the protected inference endpoint and its ephemeral access token.
/// 持久化受保护推理端点及其临时访问令牌。
///
/// # Parameters
/// 参数
/// `snapshot` is the exact protected discovery document.
/// `snapshot` 是精确的受保护发现文档。
/// `home_dir` and `temp_dir` resolve its user-scoped path.
/// `home_dir` 与 `temp_dir` 用于解析其用户作用域路径。
///
/// # Returns
/// 返回值
/// Success after the complete document is written, otherwise an actionable diagnostic.
/// 完整文档写入后返回成功，否则返回可操作诊断。
pub fn write_inference_service_discovery_snapshot(
    snapshot: &InferenceServiceDiscoverySnapshot,
    home_dir: Option<PathBuf>,
    temp_dir: PathBuf,
) -> Result<(), String> {
    let path = inference_service_discovery_snapshot_path(home_dir, temp_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create inference discovery directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let body = serde_json::to_string_pretty(snapshot)
        .map_err(|error| format!("failed to encode inference discovery snapshot: {error}"))?;
    write_protected_file_atomically(path.as_path(), body.as_bytes())
}

/// Publishes a secret-bearing discovery document through a same-directory atomic replacement.
/// 通过同目录原子替换发布一份含秘密信息的发现文档。
///
/// # Parameters
/// 参数
/// `path` is the stable discovery location and `contents` is the complete encoded document.
/// `path` 是稳定发现位置，`contents` 是完整编码文档。
///
/// # Returns
/// 返回值
/// Success only after a private regular staging file has replaced the destination.
/// 仅在私有普通暂存文件替换目标后返回成功。
fn write_protected_file_atomically(path: &Path, contents: &[u8]) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (!metadata.file_type().is_file() || metadata.file_type().is_symlink())
    {
        return Err(format!(
            "refusing to replace non-regular inference discovery path {}",
            path.display()
        ));
    }
    let parent = path.parent().ok_or_else(|| {
        format!(
            "inference discovery path {} has no parent directory",
            path.display()
        )
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| String::from("system clock is earlier than the Unix epoch"))?
        .as_nanos();
    let staged_path = parent.join(format!(
        ".inference-service.{}.{}.tmp",
        std::process::id(),
        nonce
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut staged = options.open(staged_path.as_path()).map_err(|error| {
        format!(
            "failed to create protected inference discovery staging file {}: {error}",
            staged_path.display()
        )
    })?;
    let publication = (|| {
        harden_protected_file_permissions(staged_path.as_path())?;
        staged.write_all(contents).map_err(|error| {
            format!(
                "failed to write protected inference discovery staging file {}: {error}",
                staged_path.display()
            )
        })?;
        staged.sync_all().map_err(|error| {
            format!(
                "failed to flush protected inference discovery staging file {}: {error}",
                staged_path.display()
            )
        })?;
        drop(staged);
        replace_staged_file(staged_path.as_path(), path)
    })();
    if publication.is_err() {
        let _ = fs::remove_file(staged_path.as_path());
    }
    publication
}

/// Restricts one secret-bearing staging file to its owner and Windows system administrators.
/// 将一份含秘密信息的暂存文件限制为仅文件所有者与 Windows 系统管理员可访问。
///
/// # Parameters
/// 参数
/// `path` is the newly created regular staging file that does not yet contain secret bytes.
/// `path` 是尚未写入秘密字节的新建普通暂存文件。
///
/// # Returns
/// 返回值
/// Success after inherited Windows access entries are removed and the protected allow-list is applied.
/// 删除继承的 Windows 访问项并应用受保护允许列表后返回成功。
/// Windows creation flag that prevents a console window for a child process.
/// 防止子进程创建控制台窗口的 Windows 创建标志。
#[cfg(all(windows, not(debug_assertions)))]
const WINDOWS_CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply the Release-only no-console flag to a Windows service-host child process.
/// 为 Windows 服务宿主子进程应用仅 Release 生效的无控制台标志。
#[cfg(windows)]
fn apply_release_no_window_to_command(command: &mut std::process::Command) {
    #[cfg(not(debug_assertions))]
    {
        use std::os::windows::process::CommandExt;

        command.creation_flags(WINDOWS_CREATE_NO_WINDOW);
    }
    #[cfg(debug_assertions)]
    {
        let _ = command;
    }
}

/// Harden one Windows discovery staging file before publishing it.
/// 在发布前保护一个 Windows 发现文件暂存副本。
#[cfg(windows)]
fn harden_protected_file_permissions(path: &Path) -> Result<(), String> {
    let system_root = std::env::var_os("SystemRoot")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            String::from("Windows SystemRoot is unavailable for protected file ACLs.")
        })?;
    let icacls = PathBuf::from(system_root)
        .join("System32")
        .join("icacls.exe");
    // command owns the ACL subprocess whose console must stay hidden in Release.
    // command 持有 Release 下必须保持隐藏控制台的 ACL 子进程。
    let mut command = std::process::Command::new(icacls.as_path());
    command.arg(path).args([
        "/inheritance:r",
        "/grant:r",
        "*S-1-3-4:(F)",
        "*S-1-5-18:(F)",
        "*S-1-5-32-544:(F)",
    ]);
    apply_release_no_window_to_command(&mut command);
    let output = command.output().map_err(|error| {
        format!(
            "failed to launch Windows ACL protection for {}: {error}",
            path.display()
        )
    })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(output.stderr.as_slice())
        .trim()
        .to_string();
    let diagnostic = if stderr.is_empty() {
        format!("exit status {}", output.status)
    } else {
        stderr
    };
    Err(format!(
        "failed to protect inference discovery staging file {}: {diagnostic}",
        path.display()
    ))
}

/// Preserves the creation-time `0600` mode already applied on non-Windows hosts.
/// 在非 Windows 宿主上保留创建时已经应用的 `0600` 权限。
///
/// # Parameters
/// 参数
/// `path` is already protected by the platform-specific creation mode.
/// `path` 已由平台特定创建模式保护。
///
/// # Returns
/// 返回值
/// Immediate success because no post-creation permission change is required.
/// 因无需创建后权限变更而立即返回成功。
#[cfg(not(windows))]
fn harden_protected_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

/// Replaces one discovery destination with a complete same-directory staging file.
/// 使用完整的同目录暂存文件替换发现目标。
///
/// # Parameters
/// 参数
/// `staged_path` and `target_path` are regular files on the same filesystem.
/// `staged_path` 与 `target_path` 是同一文件系统上的普通文件。
///
/// # Returns
/// 返回值
/// Success after the standard-library same-filesystem replacement publishes the complete document.
/// 标准库同文件系统替换发布完整文档后返回成功。
fn replace_staged_file(staged_path: &Path, target_path: &Path) -> Result<(), String> {
    fs::rename(staged_path, target_path).map_err(|error| {
        format!(
            "failed to publish protected inference discovery {}: {error}",
            target_path.display()
        )
    })
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
    /// Kernel callback that releases display subscriptions owned by a disconnected browser.
    /// 内核回调，用于释放由已断开浏览器持有的显示订阅。
    pub client_disconnected: WebuiServiceClientDisconnected,
    /// Protected inference discovery state without browser-token reuse.
    /// 不复用浏览器令牌的受保护推理发现状态。
    pub inference_discovery: SharedInferenceServiceDiscoverySnapshot,
    /// Kernel-owned controlled inference dispatcher.
    /// 内核拥有的受控推理分发器。
    pub inference_dispatcher: InferenceServiceRpcDispatcher,
    /// Kernel-owned restricted LLM event subscriber.
    /// 内核拥有的受限 LLM 事件订阅器。
    pub inference_event_subscriber: InferenceServiceEventSubscriber,
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
    /// Kernel callback that releases display subscriptions owned by a disconnected browser.
    /// 内核回调，用于释放由已断开浏览器持有的显示订阅。
    client_disconnected: WebuiServiceClientDisconnected,
    /// Latest event-socket generation for each browser lifetime.
    /// 每个浏览器生命周期的最新事件套接字代次。
    client_connections: Arc<Mutex<HashMap<Uuid, Uuid>>>,
    /// Protected inference discovery state.
    /// 受保护推理发现状态。
    inference_discovery: SharedInferenceServiceDiscoverySnapshot,
    /// Controlled inference request dispatcher.
    /// 受控推理请求分发器。
    inference_dispatcher: InferenceServiceRpcDispatcher,
    /// Restricted LLM event subscriber.
    /// 受限 LLM 事件订阅器。
    inference_event_subscriber: InferenceServiceEventSubscriber,
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
        client_disconnected: config.client_disconnected,
        client_connections: Arc::new(Mutex::new(HashMap::new())),
        inference_discovery: config.inference_discovery,
        inference_dispatcher: config.inference_dispatcher,
        inference_event_subscriber: config.inference_event_subscriber,
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
        .route(
            "/rpc",
            post(webui_service_rpc_http_handler)
                .layer(DefaultBodyLimit::max(WEBUI_SERVICE_RPC_BODY_LIMIT_BYTES)),
        )
        .route("/events", get(webui_service_events_ws_handler))
        .route("/v1/models", get(responses_models_http_handler))
        .route(
            "/v1/responses",
            post(responses_create_http_handler)
                .layer(DefaultBodyLimit::max(INFERENCE_SERVICE_BODY_LIMIT_BYTES)),
        )
        .route(
            "/inference/v1/health",
            get(inference_service_health_http_handler),
        )
        .route(
            "/inference/v1/capabilities/{consumer_profile_id}",
            get(inference_service_capabilities_http_handler),
        )
        .route(
            "/inference/v1/embeddings",
            post(inference_service_embeddings_http_handler)
                .layer(DefaultBodyLimit::max(INFERENCE_SERVICE_BODY_LIMIT_BYTES)),
        )
        .route(
            "/inference/v1/rerank",
            post(inference_service_rerank_http_handler)
                .layer(DefaultBodyLimit::max(INFERENCE_SERVICE_BODY_LIMIT_BYTES)),
        )
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

/// Returns only consumer-authorized and currently routable models using the standard list shape.
/// 使用标准列表形态仅返回消费者已授权且当前可路由的模型。
async fn responses_models_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
) -> AxumResponse {
    responses_dispatch_http(&server_state, &headers, InferenceServiceRequest::ListModels).await
}

/// Executes one standard Responses request as JSON or SSE according to the request body.
/// 根据请求体以 JSON 或 SSE 执行一项标准 Responses 请求。
async fn responses_create_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    request: Result<Json<ResponsesCreateRequest>, JsonRejection>,
) -> AxumResponse {
    let Json(request) = match request {
        Ok(request) => request,
        Err(error) => {
            return openai_error_response(
                AxumStatusCode::BAD_REQUEST,
                "invalid_request_error",
                "invalid_json",
                None,
                format!("Responses request JSON is invalid: {error}"),
            );
        }
    };
    if let Err(error) = request.validate_compatibility_level() {
        return openai_error_response(
            AxumStatusCode::BAD_REQUEST,
            error.error_type.as_str(),
            error.code.as_str(),
            error.param,
            error.message,
        );
    }
    if request.stream {
        responses_create_sse(server_state, headers, request).await
    } else {
        responses_dispatch_http(
            &server_state,
            &headers,
            InferenceServiceRequest::CreateResponse { request },
        )
        .await
    }
}

/// Starts one authorized execution and exposes only standard Responses SSE events.
/// 启动一项已授权执行并且只暴露标准 Responses SSE 事件。
async fn responses_create_sse(
    server_state: WebuiServiceHttpState,
    headers: AxumHeaderMap,
    request: ResponsesCreateRequest,
) -> AxumResponse {
    let Some(token) = inference_service_token_from_headers(&headers) else {
        return openai_error_response(
            AxumStatusCode::UNAUTHORIZED,
            "authentication_error",
            "invalid_api_key",
            None,
            "Responses API bearer token is missing.",
        );
    };
    let model = request.model.clone();
    let include_reasoning_encrypted_content = request
        .include
        .iter()
        .any(|field| field == "reasoning.encrypted_content");
    let start = match (server_state.inference_dispatcher)(
        token.clone(),
        InferenceServiceRequest::StartResponse { request },
    )
    .await
    {
        Ok(start) => start,
        Err(error) => return standard_inference_error_response(error),
    };
    let Some(execution_id) = start
        .get("execution_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
    else {
        return openai_error_response(
            AxumStatusCode::INTERNAL_SERVER_ERROR,
            "api_error",
            "invalid_execution_start",
            None,
            "Responses execution did not return an execution identifier.",
        );
    };
    let subscription =
        match (server_state.inference_event_subscriber)(token, execution_id.clone()).await {
            Ok(subscription) => subscription,
            Err(error) => return standard_inference_error_response(error),
        };
    let request_id = format!("req_{}", execution_id.replace('-', ""));
    let event_stream = responses_sse_event_stream(
        execution_id,
        model,
        include_reasoning_encrypted_content,
        subscription,
    );
    let response = Sse::new(event_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response();
    with_openai_request_id(response, request_id)
}

/// Converts normalized execution events into one ordered standard Responses SSE stream.
/// 将归一化执行事件转换为一条有序的标准 Responses SSE 流。
fn responses_sse_event_stream(
    execution_id: String,
    model: String,
    include_reasoning_encrypted_content: bool,
    subscription: InferenceServiceEventSubscription,
) -> impl futures_core::Stream<Item = Result<AxumSseEvent, Infallible>> {
    async_stream::stream! {
        let InferenceServiceEventSubscription {
            replay,
            mut receiver,
            mut cancellation_guard,
        } = subscription;
        let mut state = ResponsesSseState::new(
            execution_id,
            model,
            include_reasoning_encrypted_content,
        );
        for event in replay {
            let terminal = matches!(
                event,
                AiStreamEvent::ResponseCompleted
                    | AiStreamEvent::ResponseFailed { .. }
                    | AiStreamEvent::ResponseCancelled
            );
            for projected in state.project(event) {
                yield Ok(projected);
            }
            if terminal {
                cancellation_guard.disarm();
                return;
            }
        }
        loop {
            let event = match receiver.recv().await {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    yield Ok(state.standard_event(
                        "error",
                        serde_json::json!({
                            "type": "error",
                            "code": "stream_backpressure",
                            "message": format!("Responses SSE consumer lagged behind by {skipped} buffered events."),
                            "param": null
                        }),
                    ));
                    break;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };
            let terminal = matches!(
                event,
                AiStreamEvent::ResponseCompleted
                    | AiStreamEvent::ResponseFailed { .. }
                    | AiStreamEvent::ResponseCancelled
            );
            for projected in state.project(event) {
                yield Ok(projected);
            }
            if terminal {
                cancellation_guard.disarm();
                break;
            }
        }
    }
}

/// Retains the minimum state needed to emit self-consistent standard stream items.
/// 保留生成自洽标准流项目所需的最小状态。
struct ResponsesSseState {
    /// Public response identifier derived from the host execution identity.
    /// 由宿主执行身份派生的公共响应标识。
    response_id: String,
    /// Public message item identifier.
    /// 公共消息项目标识。
    message_id: String,
    /// Requested public model identity.
    /// 请求的公共模型身份。
    model: String,
    /// Stable creation timestamp shared by every lifecycle snapshot.
    /// 所有生命周期快照共享的稳定创建时间戳。
    created_at: u64,
    /// Accumulated visible output text.
    /// 累积的可见输出文本。
    output_text: String,
    /// Accumulated assistant refusal text.
    /// 累积的助手拒绝文本。
    refusal_text: String,
    /// Whether the caller explicitly selected encrypted reasoning output.
    /// 调用方是否显式选择了加密推理输出。
    include_reasoning_encrypted_content: bool,
    /// Latest normalized usage snapshot.
    /// 最新归一化用量快照。
    usage: RequestUsage,
    /// Ordered standard output items accumulated for the terminal response object.
    /// 为终态响应对象累积的有序标准输出项目。
    output_items: Vec<serde_json::Value>,
    /// Output index assigned when the assistant message first appears.
    /// 助手消息首次出现时分配的输出索引。
    message_index: Option<usize>,
    /// Streamed reasoning items keyed by their exact standard item identity.
    /// 按精确标准项目身份索引的流式推理项目。
    reasoning_items: BTreeMap<String, ResponsesSseReasoningItem>,
    /// Output indices for streamed function calls keyed by stream item identifier.
    /// 按流项目标识索引的流式函数调用输出索引。
    tool_indices: BTreeMap<String, usize>,
    /// Output indices for image-generation items keyed by standard item identifier.
    /// 按标准项目标识索引的图片生成输出索引。
    image_indices: BTreeMap<String, usize>,
    /// Monotonic sequence number assigned to every emitted standard stream event.
    /// 分配给每项标准流事件的单调序列号。
    sequence_number: u64,
    /// Whether the response lifecycle header has been emitted.
    /// 是否已经发出响应生命周期头部。
    started: bool,
    /// Whether the message item has been opened.
    /// 是否已经打开消息项目。
    message_open: bool,
    /// Whether an output-text content part has been opened.
    /// 是否已经打开输出文本内容部分。
    text_open: bool,
    /// Whether a refusal content part has been opened.
    /// 是否已经打开拒绝内容部分。
    refusal_open: bool,
}

/// Retains one independently streamed standard reasoning item.
/// 保留一项独立流式标准推理项目。
struct ResponsesSseReasoningItem {
    /// Stable output index assigned when the item opens.
    /// 项目打开时分配的稳定输出索引。
    output_index: usize,
    /// Accumulated public summary text.
    /// 累积公共摘要文本。
    text: String,
    /// Optional provider continuation signature.
    /// 可选供应商延续签名。
    signature: Option<String>,
}

impl ResponsesSseState {
    /// Creates an empty stream projection state for one execution.
    /// 为一项执行创建空的流投影状态。
    fn new(execution_id: String, model: String, include_reasoning_encrypted_content: bool) -> Self {
        let compact_id = execution_id.replace('-', "");
        Self {
            response_id: format!("resp_{compact_id}"),
            message_id: format!("msg_{compact_id}"),
            model,
            created_at: current_unix_seconds(),
            output_text: String::new(),
            refusal_text: String::new(),
            include_reasoning_encrypted_content,
            usage: RequestUsage::default(),
            output_items: Vec::new(),
            message_index: None,
            reasoning_items: BTreeMap::new(),
            tool_indices: BTreeMap::new(),
            image_indices: BTreeMap::new(),
            sequence_number: 0,
            started: false,
            message_open: false,
            text_open: false,
            refusal_open: false,
        }
    }

    /// Projects one internal normalized event without exposing private event names.
    /// 投影一项内部归一化事件且不暴露私有事件名称。
    fn project(&mut self, event: AiStreamEvent) -> Vec<AxumSseEvent> {
        let mut projected = Vec::new();
        if !self.started {
            self.started = true;
            let response = self.response_json("in_progress");
            projected.push(self.standard_event(
                "response.created",
                serde_json::json!({"type": "response.created", "sequence_number": 0, "response": response}),
            ));
            projected.push(self.standard_event(
                "response.in_progress",
                serde_json::json!({"type": "response.in_progress", "sequence_number": 1, "response": self.response_json("in_progress")}),
            ));
        }
        match event {
            AiStreamEvent::TransportObserved { .. }
            | AiStreamEvent::ResponseStarted { .. }
            | AiStreamEvent::ResponseMetadataUpdated { .. }
            | AiStreamEvent::ResponseItemCompleted { .. }
            | AiStreamEvent::ProviderContextItemCaptured { .. }
            | AiStreamEvent::ResourceProgress { .. } => {}
            AiStreamEvent::ReasoningCompleted { item_id, signature } => {
                let output_index = self.ensure_reasoning_item(item_id.as_str(), &mut projected);
                self.reasoning_items
                    .get_mut(item_id.as_str())
                    .expect("reasoning item exists after opening")
                    .signature = signature;
                self.output_items[output_index] =
                    self.reasoning_item(item_id.as_str(), "in_progress");
            }
            AiStreamEvent::ReasoningStarted { item_id } => {
                self.ensure_reasoning_item(item_id.as_str(), &mut projected);
            }
            AiStreamEvent::ImageGenerationPartial {
                item_id,
                partial_image_index,
                partial_image_b64,
            } => {
                let output_index =
                    if let Some(output_index) = self.image_indices.get(item_id.as_str()).copied() {
                        output_index
                    } else {
                        let output_index = self.output_items.len();
                        let item = standard_image_generation_placeholder(item_id.as_str());
                        self.output_items.push(item.clone());
                        self.image_indices.insert(item_id.clone(), output_index);
                        projected.push(self.standard_event(
                            "response.output_item.added",
                            serde_json::json!({
                                "type": "response.output_item.added",
                                "output_index": output_index,
                                "item": item
                            }),
                        ));
                        output_index
                    };
                projected.push(self.standard_event(
                    "response.image_generation_call.partial_image",
                    serde_json::json!({
                        "type": "response.image_generation_call.partial_image",
                        "item_id": item_id,
                        "output_index": output_index,
                        "partial_image_index": partial_image_index,
                        "partial_image_b64": partial_image_b64
                    }),
                ));
            }
            AiStreamEvent::ResponseItemStarted { item_id, item } => match item {
                ContentItem::Text { text } => {
                    if !self.message_open {
                        self.message_id = item_id;
                    }
                    projected.extend(self.project(AiStreamEvent::ResponseTextDelta {
                        item_id: self.message_id.clone(),
                        delta: text,
                    }));
                }
                ContentItem::Refusal { text } => {
                    projected.extend(self.project_refusal(item_id, text));
                }
                ContentItem::StructuredJson { value } => {
                    if !self.message_open {
                        self.message_id = item_id;
                    }
                    projected.extend(self.project(AiStreamEvent::ResponseTextDelta {
                        item_id: self.message_id.clone(),
                        delta: value.to_string(),
                    }));
                }
                ContentItem::Reasoning { text, signature } => {
                    projected.extend(self.project(AiStreamEvent::ReasoningStarted {
                        item_id: item_id.clone(),
                    }));
                    projected.extend(self.project(AiStreamEvent::ReasoningDelta {
                        item_id: item_id.clone(),
                        delta: text,
                    }));
                    projected.extend(
                        self.project(AiStreamEvent::ReasoningCompleted { item_id, signature }),
                    );
                }
                ContentItem::ToolCall { call } => {
                    let output_index = self.output_items.len();
                    let item = standard_function_call_item(item_id.as_str(), &call, "completed");
                    self.output_items.push(item.clone());
                    self.tool_indices.insert(call.id.clone(), output_index);
                    projected.push(self.standard_event(
                        "response.output_item.added",
                        serde_json::json!({
                            "type": "response.output_item.added",
                            "output_index": output_index,
                            "item": item
                        }),
                    ));
                    projected.push(self.standard_event(
                        "response.output_item.done",
                        serde_json::json!({
                            "type": "response.output_item.done",
                            "output_index": output_index,
                            "item": standard_function_call_item(item_id.as_str(), &call, "completed")
                        }),
                    ));
                }
                ContentItem::Resource { resource } => {
                    let output_index = if let Some(output_index) =
                        self.image_indices.get(item_id.as_str()).copied()
                    {
                        output_index
                    } else {
                        let output_index = self.output_items.len();
                        self.image_indices.insert(item_id.clone(), output_index);
                        output_index
                    };
                    let item =
                        standard_image_generation_item(item_id.as_str(), &resource, "completed");
                    if output_index == self.output_items.len() {
                        self.output_items.push(item.clone());
                        projected.push(self.standard_event(
                            "response.output_item.added",
                            serde_json::json!({
                                "type": "response.output_item.added",
                                "output_index": output_index,
                                "item": item
                            }),
                        ));
                    } else {
                        self.output_items[output_index] = item.clone();
                    }
                    projected.push(self.standard_event(
                        "response.output_item.done",
                        serde_json::json!({
                            "type": "response.output_item.done",
                            "output_index": output_index,
                            "item": standard_image_generation_item(item_id.as_str(), &resource, "completed")
                        }),
                    ));
                }
                ContentItem::ToolResult { .. } | ContentItem::Citation { .. } => {}
            },
            AiStreamEvent::ResponseTextDelta { delta: text, .. } => {
                if self.refusal_open {
                    projected.push(self.standard_event(
                        "error",
                        serde_json::json!({
                            "type": "error",
                            "code": "invalid_provider_output",
                            "message": "A response cannot mix output text and refusal content in one message item.",
                            "param": null
                        }),
                    ));
                    return projected;
                }
                let opening_message = !self.message_open;
                if opening_message {
                    self.message_open = true;
                    let output_index = self.output_items.len();
                    self.message_index = Some(output_index);
                }
                let opening_text = !self.text_open;
                if opening_text {
                    self.text_open = true;
                }
                let output_index = self
                    .message_index
                    .expect("message index exists after opening the message item");
                if opening_message {
                    let item = self.message_item("in_progress", "");
                    self.output_items.push(item.clone());
                    projected.push(self.standard_event(
                        "response.output_item.added",
                        serde_json::json!({
                            "type": "response.output_item.added",
                            "output_index": output_index,
                            "item": item
                        }),
                    ));
                }
                if opening_text {
                    self.output_items[output_index] =
                        self.message_item("in_progress", self.output_text.as_str());
                    projected.push(self.standard_event(
                        "response.content_part.added",
                        serde_json::json!({
                            "type": "response.content_part.added",
                            "item_id": self.message_id,
                            "output_index": output_index,
                            "content_index": 0,
                            "part": {"type": "output_text", "text": "", "annotations": []}
                        }),
                    ));
                }
                self.output_text.push_str(text.as_str());
                self.output_items[output_index] =
                    self.message_item("in_progress", self.output_text.as_str());
                projected.push(self.standard_event(
                    "response.output_text.delta",
                    serde_json::json!({
                        "type": "response.output_text.delta",
                        "item_id": self.message_id,
                        "output_index": output_index,
                        "content_index": 0,
                        "delta": text
                    }),
                ));
            }
            AiStreamEvent::ReasoningDelta {
                item_id,
                delta: text,
            } => {
                let output_index = self.ensure_reasoning_item(item_id.as_str(), &mut projected);
                self.reasoning_items
                    .get_mut(item_id.as_str())
                    .expect("reasoning item exists after opening")
                    .text
                    .push_str(text.as_str());
                self.output_items[output_index] =
                    self.reasoning_item(item_id.as_str(), "in_progress");
                projected.push(self.standard_event(
                    "response.reasoning_summary_text.delta",
                    serde_json::json!({
                        "type": "response.reasoning_summary_text.delta",
                        "item_id": item_id,
                        "output_index": output_index,
                        "summary_index": 0,
                        "delta": text
                    }),
                ));
            }
            AiStreamEvent::ToolCallStarted {
                item_id,
                call_id,
                tool_name,
                ..
            } => {
                let output_index = self.output_items.len();
                let item = serde_json::json!({
                    "id": item_id,
                    "type": "function_call",
                    "status": "in_progress",
                    "call_id": call_id,
                    "name": tool_name,
                    "arguments": ""
                });
                self.output_items.push(item.clone());
                self.tool_indices.insert(item_id.clone(), output_index);
                projected.push(self.standard_event(
                    "response.output_item.added",
                    serde_json::json!({
                        "type": "response.output_item.added",
                        "output_index": output_index,
                        "item": item
                    }),
                ));
            }
            AiStreamEvent::ToolCallArgumentsDelta { item_id, delta, .. } => {
                let Some(output_index) = self.tool_indices.get(item_id.as_str()).copied() else {
                    projected.push(self.standard_event(
                        "error",
                        serde_json::json!({"type": "error", "code": "invalid_tool_call_sequence", "message": "Function call arguments arrived before the function call item.", "param": item_id}),
                    ));
                    return projected;
                };
                let arguments = self.output_items[output_index]
                    .get("arguments")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.output_items[output_index]["arguments"] =
                    serde_json::Value::String(format!("{arguments}{delta}"));
                projected.push(self.standard_event(
                    "response.function_call_arguments.delta",
                    serde_json::json!({"type": "response.function_call_arguments.delta", "item_id": item_id, "output_index": output_index, "delta": delta}),
                ));
            }
            AiStreamEvent::ToolCallCompleted { item_id, call } => {
                let output_index = if let Some(output_index) =
                    self.tool_indices.get(item_id.as_str()).copied()
                {
                    output_index
                } else {
                    let output_index = self.output_items.len();
                    self.tool_indices.insert(item_id.clone(), output_index);
                    self.output_items.push(standard_function_call_item(
                        item_id.as_str(),
                        &call,
                        "in_progress",
                    ));
                    projected.push(self.standard_event(
                        "response.output_item.added",
                        serde_json::json!({
                            "type": "response.output_item.added",
                            "output_index": output_index,
                            "item": standard_function_call_item(item_id.as_str(), &call, "in_progress")
                        }),
                    ));
                    output_index
                };
                let completed_item =
                    standard_function_call_item(item_id.as_str(), &call, "completed");
                self.output_items[output_index] = completed_item.clone();
                projected.push(self.standard_event(
                    "response.function_call_arguments.done",
                    serde_json::json!({"type": "response.function_call_arguments.done", "item_id": item_id, "output_index": output_index, "arguments": call.arguments.to_string()}),
                ));
                projected.push(self.standard_event(
                    "response.output_item.done",
                    serde_json::json!({"type": "response.output_item.done", "output_index": output_index, "item": completed_item}),
                ));
            }
            AiStreamEvent::ToolCallAborted { item_id, message } => {
                projected.push(self.standard_event(
                    "error",
                    serde_json::json!({"type": "error", "code": "invalid_tool_call", "message": message, "param": item_id}),
                ));
            }
            AiStreamEvent::UsageUpdated { usage } => self.usage.merge_update(*usage),
            AiStreamEvent::ResponseCompleted => projected.extend(self.completed_events()),
            AiStreamEvent::ResponseFailed { error } => {
                let code = error
                    .upstream_code
                    .clone()
                    .unwrap_or_else(|| String::from("provider_error"));
                let error_object = serde_json::json!({
                    "code": code,
                    "message": error.message,
                    "param": error.upstream_parameter
                });
                let mut response = self.response_json("failed");
                response["error"] = error_object;
                projected.push(self.standard_event(
                    "response.failed",
                    serde_json::json!({"type": "response.failed", "response": response}),
                ));
            }
            AiStreamEvent::ResponseCancelled => {
                let error = serde_json::json!({
                    "code": "cancelled",
                    "message": "Responses execution was cancelled."
                });
                let mut response = self.response_json("failed");
                response["error"] = error;
                projected.push(self.standard_event(
                    "response.failed",
                    serde_json::json!({
                        "type": "response.failed",
                        "response": response
                    }),
                ));
            }
        }
        projected
    }

    /// Emits final item closures followed by the authoritative completed response.
    /// 发出最终项目闭合事件以及权威完成响应。
    fn completed_events(&mut self) -> Vec<AxumSseEvent> {
        let mut events = Vec::new();
        let mut reasoning_ids = self.reasoning_items.keys().cloned().collect::<Vec<_>>();
        reasoning_ids.sort_by_key(|item_id| {
            self.reasoning_items
                .get(item_id)
                .map(|item| item.output_index)
                .unwrap_or(usize::MAX)
        });
        for item_id in reasoning_ids {
            let reasoning = self
                .reasoning_items
                .get(item_id.as_str())
                .expect("ordered reasoning item remains available");
            let output_index = reasoning.output_index;
            let reasoning_text = reasoning.text.clone();
            let completed_item = self.reasoning_item(item_id.as_str(), "completed");
            self.output_items[output_index] = completed_item.clone();
            events.push(self.standard_event(
                "response.reasoning_summary_text.done",
                serde_json::json!({
                    "type": "response.reasoning_summary_text.done",
                    "item_id": item_id,
                    "output_index": output_index,
                    "summary_index": 0,
                    "text": reasoning_text
                }),
            ));
            events.push(self.standard_event(
                "response.output_item.done",
                serde_json::json!({
                    "type": "response.output_item.done",
                    "output_index": output_index,
                    "item": completed_item
                }),
            ));
        }
        if self.message_open {
            let output_index = self
                .message_index
                .expect("message index exists for an open message item");
            self.output_items[output_index] =
                self.message_item("completed", self.output_text.as_str());
            if self.text_open {
                events.push(self.standard_event(
                    "response.output_text.done",
                    serde_json::json!({
                        "type": "response.output_text.done",
                        "item_id": self.message_id,
                        "output_index": output_index,
                        "content_index": 0,
                        "text": self.output_text
                    }),
                ));
            }
            if self.refusal_open {
                events.push(self.standard_event(
                    "response.refusal.done",
                    serde_json::json!({
                        "type": "response.refusal.done",
                        "item_id": self.message_id,
                        "output_index": output_index,
                        "content_index": 0,
                        "refusal": self.refusal_text
                    }),
                ));
            }
            events.push(self.standard_event(
                "response.output_item.done",
                serde_json::json!({
                    "type": "response.output_item.done",
                    "output_index": output_index,
                    "item": self.message_item("completed", self.output_text.as_str())
                }),
            ));
        }
        events.push(self.standard_event(
            "response.completed",
            serde_json::json!({"type": "response.completed", "response": self.response_json("completed")}),
        ));
        events
    }

    /// Builds the current standard response object.
    /// 构建当前标准响应对象。
    fn response_json(&self, status: &str) -> serde_json::Value {
        let mut output = self.output_items.clone();
        if let Some(output_index) = self.message_index {
            output[output_index] = self.message_item(status, self.output_text.as_str());
        }
        for (item_id, reasoning) in &self.reasoning_items {
            output[reasoning.output_index] = self.reasoning_item(item_id.as_str(), status);
        }
        serde_json::json!({
            "id": self.response_id,
            "object": "response",
            "created_at": self.created_at,
            "status": status,
            "model": self.model,
            "output": output,
            "usage": standard_responses_usage(&self.usage),
            "error": null,
            "incomplete_details": null
        })
    }

    /// Builds one standard assistant message output item.
    /// 构建一项标准助手消息输出项目。
    fn message_item(&self, status: &str, text: &str) -> serde_json::Value {
        let mut content = Vec::new();
        if self.text_open {
            content.push(serde_json::json!({
                "type": "output_text",
                "text": text,
                "annotations": []
            }));
        }
        if self.refusal_open {
            content.push(serde_json::json!({
                "type": "refusal",
                "refusal": self.refusal_text
            }));
        }
        serde_json::json!({
            "id": self.message_id,
            "type": "message",
            "status": status,
            "role": "assistant",
            "content": content
        })
    }

    /// Projects one complete refusal item into standard message-content events.
    /// 将一项完整拒绝内容投影为标准消息内容事件。
    fn project_refusal(&mut self, item_id: String, text: String) -> Vec<AxumSseEvent> {
        let mut projected = Vec::new();
        if self.text_open {
            projected.push(self.standard_event(
                "error",
                serde_json::json!({
                    "type": "error",
                    "code": "invalid_provider_output",
                    "message": "A response cannot mix output text and refusal content in one message item.",
                    "param": null
                }),
            ));
            return projected;
        }
        let opening_message = !self.message_open;
        if opening_message {
            self.message_open = true;
            self.message_id = item_id;
            self.message_index = Some(self.output_items.len());
        }
        let opening_refusal = !self.refusal_open;
        self.refusal_open = true;
        let output_index = self
            .message_index
            .expect("message index exists after opening refusal content");
        if opening_message {
            let item = self.message_item("in_progress", self.output_text.as_str());
            self.output_items.push(item.clone());
            projected.push(self.standard_event(
                "response.output_item.added",
                serde_json::json!({
                    "type": "response.output_item.added",
                    "output_index": output_index,
                    "item": item
                }),
            ));
        }
        if opening_refusal {
            projected.push(self.standard_event(
                "response.content_part.added",
                serde_json::json!({
                    "type": "response.content_part.added",
                    "item_id": self.message_id,
                    "output_index": output_index,
                    "content_index": 0,
                    "part": {"type": "refusal", "refusal": ""}
                }),
            ));
        }
        self.refusal_text.push_str(text.as_str());
        self.output_items[output_index] =
            self.message_item("in_progress", self.output_text.as_str());
        projected.push(self.standard_event(
            "response.refusal.delta",
            serde_json::json!({
                "type": "response.refusal.delta",
                "item_id": self.message_id,
                "output_index": output_index,
                "content_index": 0,
                "delta": text
            }),
        ));
        projected
    }

    /// Ensures one reasoning item has a stable output position and emits its opening event once.
    /// 确保一项推理内容具有稳定输出位置，并且仅发出一次开启事件。
    fn ensure_reasoning_item(&mut self, item_id: &str, projected: &mut Vec<AxumSseEvent>) -> usize {
        if let Some(reasoning) = self.reasoning_items.get(item_id) {
            return reasoning.output_index;
        }
        let output_index = self.output_items.len();
        self.reasoning_items.insert(
            item_id.to_string(),
            ResponsesSseReasoningItem {
                output_index,
                text: String::new(),
                signature: None,
            },
        );
        let item = self.reasoning_item(item_id, "in_progress");
        self.output_items.push(item.clone());
        projected.push(self.standard_event(
            "response.output_item.added",
            serde_json::json!({
                "type": "response.output_item.added",
                "output_index": output_index,
                "item": item
            }),
        ));
        output_index
    }

    /// Builds one standard reasoning output item from the selected accumulated summary text.
    /// 根据选定推理项的累积摘要文本构建标准推理输出项目。
    fn reasoning_item(&self, item_id: &str, status: &str) -> serde_json::Value {
        let reasoning = self
            .reasoning_items
            .get(item_id)
            .expect("reasoning item must exist before projection");
        let mut item = serde_json::json!({
            "id": item_id,
            "type": "reasoning",
            "status": status,
            "summary": [{"type": "summary_text", "text": reasoning.text}]
        });
        if self.include_reasoning_encrypted_content
            && let Some(signature) = reasoning.signature.as_ref()
            && let Some(signature) =
                vulcan_ai_contract::provider_continuation_payload("openai", signature)
        {
            item["encrypted_content"] = serde_json::Value::String(signature.to_string());
        }
        item
    }

    /// Builds one named standard event and assigns its monotonic sequence number.
    /// 构建一项命名标准事件并分配其单调序列号。
    fn standard_event(
        &mut self,
        event_name: &'static str,
        mut payload: serde_json::Value,
    ) -> AxumSseEvent {
        payload["sequence_number"] = serde_json::json!(self.sequence_number);
        self.sequence_number = self.sequence_number.saturating_add(1);
        standard_sse_event(event_name, payload)
    }
}

/// Builds one standard function-call output item with a stable stream item identity.
/// 使用稳定流项目身份构建一项标准函数调用输出项目。
fn standard_function_call_item(
    item_id: &str,
    call: &vulcan_ai_contract::ToolCall,
    status: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": item_id,
        "type": "function_call",
        "status": status,
        "call_id": call.id,
        "name": call.name,
        "arguments": call.arguments.to_string()
    })
}

/// Builds one standard image-generation output item from a normalized image resource.
/// 根据归一化图片资源构建一项标准图片生成输出项目。
fn standard_image_generation_item(
    item_id: &str,
    resource: &vulcan_ai_contract::ResourceReference,
    status: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": item_id,
        "type": "image_generation_call",
        "status": status,
        "result": resource.url
    })
}

/// Builds one in-progress standard image-generation item before preview bytes complete.
/// 在预览字节完成前构建一项进行中的标准图片生成项目。
fn standard_image_generation_placeholder(item_id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": item_id,
        "type": "image_generation_call",
        "status": "in_progress"
    })
}

/// Builds one named SSE event whose payload carries the same standard event type.
/// 构建一项命名 SSE 事件，其载荷携带相同的标准事件类型。
fn standard_sse_event(event_name: &'static str, payload: serde_json::Value) -> AxumSseEvent {
    AxumSseEvent::default()
        .event(event_name)
        .data(payload.to_string())
}

/// Projects normalized usage into the standard Responses usage object.
/// 将归一化用量投影为标准 Responses 用量对象。
fn standard_responses_usage(usage: &RequestUsage) -> serde_json::Value {
    let total_tokens = match usage.total_tokens {
        UsageValue::Unknown => derive_usage_sum(usage.input_tokens, usage.output_tokens),
        known => known,
    };
    serde_json::json!({
        "input_tokens": known_usage_count(usage.input_tokens),
        "input_tokens_details": {"cached_tokens": known_usage_count(usage.cache_read_tokens)},
        "output_tokens": known_usage_count(usage.output_tokens),
        "output_tokens_details": {"reasoning_tokens": known_usage_count(usage.reasoning_tokens)},
        "total_tokens": known_usage_count(total_tokens)
    })
}

/// Returns one known normalized usage count or JSON null when unavailable.
/// 返回一个已知归一化用量计数；不可用时返回 JSON null。
fn known_usage_count(value: UsageValue) -> serde_json::Value {
    match value {
        UsageValue::Known { value, .. } => serde_json::json!(value),
        UsageValue::Unknown => serde_json::Value::Null,
    }
}

/// Returns the current Unix timestamp in whole seconds for standard response objects.
/// 返回标准响应对象使用的当前 Unix 整秒时间戳。
fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Maps one controlled inference failure into a standard Responses error envelope.
/// 将一项受控推理失败映射为标准 Responses 错误信封。
fn standard_inference_error_response(error: InferenceServiceHttpError) -> AxumResponse {
    let status =
        AxumStatusCode::from_u16(error.status).unwrap_or(AxumStatusCode::INTERNAL_SERVER_ERROR);
    let (code, param) = error
        .code
        .split_once(':')
        .map_or((error.code.as_str(), None), |(code, param)| {
            (code, Some(param.to_string()))
        });
    openai_error_response(
        status,
        if error.status == 401 {
            "authentication_error"
        } else {
            "api_error"
        },
        code,
        param,
        error.message,
    )
}

/// Dispatches one standard Responses-surface request without the private inference envelope.
/// 分发一个标准 Responses 界面请求，且不使用私有推理信封。
async fn responses_dispatch_http(
    server_state: &WebuiServiceHttpState,
    headers: &AxumHeaderMap,
    request: InferenceServiceRequest,
) -> AxumResponse {
    let Some(token) = inference_service_token_from_headers(headers) else {
        return openai_error_response(
            AxumStatusCode::UNAUTHORIZED,
            "authentication_error",
            "invalid_api_key",
            None,
            "Responses API bearer token is missing.",
        );
    };
    match (server_state.inference_dispatcher)(token, request).await {
        Ok(result) => {
            let request_id = result
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(new_openai_request_id);
            with_openai_request_id(Json(result).into_response(), request_id)
        }
        Err(error) => standard_inference_error_response(error),
    }
}

/// Builds one standard OpenAI-compatible error response.
/// 构建一个标准 OpenAI 兼容错误响应。
fn openai_error_response(
    status: AxumStatusCode,
    error_type: &str,
    code: &str,
    param: Option<String>,
    message: impl Into<String>,
) -> AxumResponse {
    let response = (
        status,
        Json(OpenAiErrorEnvelope {
            error: OpenAiErrorObject {
                message: message.into(),
                error_type: error_type.to_string(),
                param,
                code: code.to_string(),
            },
        }),
    )
        .into_response();
    with_openai_request_id(response, new_openai_request_id())
}

/// Adds one stable request identifier header to a standard OpenAI-compatible response.
/// 为标准 OpenAI 兼容响应增加一个稳定请求标识头。
fn with_openai_request_id(mut response: AxumResponse, request_id: String) -> AxumResponse {
    if let Ok(value) = AxumHeaderValue::from_str(request_id.as_str()) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

/// Creates one process-unique public request identifier.
/// 创建一个进程内唯一的公共请求标识。
fn new_openai_request_id() -> String {
    format!("req_{}", Uuid::new_v4().simple())
}

/// Returns non-secret health for the controlled inference surface.
/// 返回受控推理界面的非秘密健康信息。
async fn inference_service_health_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
) -> Json<InferenceServiceHealthSnapshot> {
    let discovery = server_state
        .inference_discovery
        .lock()
        .map(|snapshot| snapshot.clone())
        .unwrap_or_else(|error| error.into_inner().clone());
    Json(InferenceServiceHealthSnapshot {
        api_version: INFERENCE_SERVICE_API_VERSION,
        status: discovery.status,
        process_id: discovery.process_id,
        started_at_unix_ms: discovery.started_at_unix_ms,
    })
}

/// Returns one authorized consumer capability snapshot.
/// 返回一份已授权消费者能力快照。
async fn inference_service_capabilities_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumPath(consumer_profile_id): AxumPath<String>,
    headers: AxumHeaderMap,
) -> AxumResponse {
    inference_service_dispatch_http(
        &server_state,
        &headers,
        InferenceServiceRequest::Capabilities {
            consumer_profile_id,
        },
    )
    .await
}

/// Executes one authorized embedding batch.
/// 执行一批已授权向量。
async fn inference_service_embeddings_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    Json(request): Json<EmbeddingRequest>,
) -> AxumResponse {
    inference_service_dispatch_http(
        &server_state,
        &headers,
        InferenceServiceRequest::CreateEmbeddings { request },
    )
    .await
}

/// Executes one authorized document rerank.
/// 执行一次已授权文档重排。
async fn inference_service_rerank_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    Json(request): Json<RerankRequest>,
) -> AxumResponse {
    inference_service_dispatch_http(
        &server_state,
        &headers,
        InferenceServiceRequest::Rerank { request },
    )
    .await
}

/// Dispatches one authenticated inference request through the kernel-owned boundary.
/// 通过内核拥有的边界分派一项已鉴权推理请求。
///
/// # Parameters
/// 参数
/// `server_state` owns callbacks, `headers` carry the ephemeral token, and `request`
/// is one closed transport operation.
/// `server_state` 拥有回调，`headers` 承载临时令牌，`request` 是一项封闭传输操作。
///
/// # Returns
/// 返回值
/// A versioned success or controlled error response.
/// 返回版本化成功或受控错误响应。
async fn inference_service_dispatch_http(
    server_state: &WebuiServiceHttpState,
    headers: &AxumHeaderMap,
    request: InferenceServiceRequest,
) -> AxumResponse {
    let Some(token) = inference_service_token_from_headers(headers) else {
        return inference_service_error_response(InferenceServiceHttpError {
            status: 401,
            code: String::from("inference.unauthorized"),
            message: String::from("Inference access token is missing."),
        });
    };
    match (server_state.inference_dispatcher)(token, request).await {
        Ok(result) => Json(InferenceServiceSuccessEnvelope {
            api_version: INFERENCE_SERVICE_API_VERSION,
            result,
        })
        .into_response(),
        Err(error) => inference_service_error_response(error),
    }
}

/// Extracts one opaque inference token from the dedicated header or Bearer authorization.
/// 从专用 Header 或 Bearer 鉴权中提取一个不透明推理令牌。
///
/// # Parameters
/// 参数
/// `headers` contains the incoming loopback request headers.
/// `headers` 包含传入回环请求 Header。
///
/// # Returns
/// 返回值
/// The exact token when present and non-empty.
/// 存在且非空时返回精确令牌。
fn inference_service_token_from_headers(headers: &AxumHeaderMap) -> Option<String> {
    if let Some(token) = headers
        .get("x-vulcan-inference-token")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
    {
        return Some(token.to_string());
    }
    headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

/// Converts one controlled inference error into its exact HTTP response.
/// 将一项受控推理错误转换为精确 HTTP 响应。
///
/// # Parameters
/// 参数
/// `error` contains a concrete status, code, and non-secret diagnostic.
/// `error` 包含明确状态、代码与非秘密诊断。
///
/// # Returns
/// 返回值
/// A versioned JSON error envelope.
/// 返回版本化 JSON 错误信封。
fn inference_service_error_response(error: InferenceServiceHttpError) -> AxumResponse {
    let status =
        AxumStatusCode::from_u16(error.status).unwrap_or(AxumStatusCode::INTERNAL_SERVER_ERROR);
    (
        status,
        Json(InferenceServiceErrorEnvelope {
            api_version: INFERENCE_SERVICE_API_VERSION,
            code: error.code,
            message: error.message,
        }),
    )
        .into_response()
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

/// Streams serialized service events to one browser WebSocket.
/// 向单个浏览器 WebSocket 推送序列化后的服务事件。
async fn webui_service_events_socket(
    mut socket: WebSocket,
    mut receiver: broadcast::Receiver<ServiceEvent>,
    client_id: Uuid,
    connection_id: Uuid,
    client_connections: Arc<Mutex<HashMap<Uuid, Uuid>>>,
    client_disconnected: WebuiServiceClientDisconnected,
) {
    loop {
        tokio::select! {
            inbound = socket.recv() => match inbound {
                Some(Ok(AxumWsMessage::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            event = receiver.recv() => {
                let event = match event {
                    Ok(event) => event,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        // A lossy event socket cannot preserve ordered terminal or workbench state;
                        // closing it forces the browser to reconnect and request authoritative baselines.
                        // 丢失事件的套接字无法保持终端或工作台有序状态；关闭它会强制浏览器重连并请求权威基线。
                        eprintln!(
                            "[service-host] browser event socket lagged by {skipped} events; reconnecting"
                        );
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if !webui_service_event_is_visible_to_client(&event, client_id) {
                    continue;
                }
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
    }
    if release_webui_service_client_connection(&client_connections, client_id, connection_id) {
        client_disconnected(client_id);
    }
}

/// Filters terminal content events to their owning browser while leaving shared events broadcast.
/// 将终端内容事件限制到其归属浏览器，同时保持共享事件广播。
///
/// # Parameters / 参数
/// - `event`: Service event awaiting WebSocket serialization.
/// - `client_id`: Exact browser/WebView lifetime owning this event socket.
/// - `event`：等待 WebSocket 序列化的服务事件。
/// - `client_id`：持有当前事件套接字的精确浏览器或 WebView 生命周期。
///
/// # Returns / 返回值
/// Returns true for shared events or terminal events explicitly owned by this client.
/// 对共享事件或由当前客户端明确持有的终端事件返回 true。
fn webui_service_event_is_visible_to_client(event: &ServiceEvent, client_id: Uuid) -> bool {
    let expected_client_id = client_id.as_hyphenated().to_string();
    match event {
        ServiceEvent::CommandSessionDisplayChanged(event) => {
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
    use vulcan_service_api::{WEBUI_SERVICE_HTTP_PORT, webui_service_ready_discovery_snapshot};

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

    /// Builds one process-unique temporary path for filesystem publication tests.
    /// 为文件系统发布测试构建一个进程内唯一的临时路径。
    ///
    /// # Parameters
    /// 参数
    /// `label` identifies the test-owned path in diagnostics.
    /// `label` 用于在诊断信息中标识测试拥有的路径。
    ///
    /// # Returns
    /// 返回值
    /// A path below the operating-system temporary directory that does not need to exist yet.
    /// 返回操作系统临时目录下尚不需要存在的路径。
    fn unique_test_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should follow the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "vulcan-service-host-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    /// Verifies protected discovery publication replaces complete regular-file contents.
    /// 验证受保护的发现信息发布会完整替换普通文件内容。
    #[test]
    fn protected_discovery_publication_replaces_regular_file() {
        let directory = unique_test_path("replace");
        fs::create_dir_all(directory.as_path()).expect("test directory should be created");
        let target = directory.join("inference-service.json");

        write_protected_file_atomically(target.as_path(), b"{\"revision\":1}")
            .expect("initial discovery document should publish");
        write_protected_file_atomically(target.as_path(), b"{\"revision\":2}")
            .expect("replacement discovery document should publish");

        assert_eq!(
            fs::read(target.as_path()).expect("published discovery document should read"),
            b"{\"revision\":2}"
        );
        fs::remove_dir_all(directory.as_path()).expect("test directory should be removed");
    }

    /// Verifies protected discovery publication refuses to replace a directory.
    /// 验证受保护的发现信息发布会拒绝替换目录。
    #[test]
    fn protected_discovery_publication_rejects_non_regular_target() {
        let directory = unique_test_path("non-regular");
        let target = directory.join("inference-service.json");
        fs::create_dir_all(target.as_path()).expect("directory target should be created");

        let error = write_protected_file_atomically(target.as_path(), b"{}")
            .expect_err("directory target must be rejected");

        assert!(error.contains("non-regular inference discovery path"));
        fs::remove_dir_all(directory.as_path()).expect("test directory should be removed");
    }

    /// Verifies Unix discovery files are private to the current user.
    /// 验证 Unix 发现文件仅允许当前用户访问。
    #[cfg(unix)]
    #[test]
    fn protected_discovery_publication_uses_private_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = unique_test_path("permissions");
        fs::create_dir_all(directory.as_path()).expect("test directory should be created");
        let target = directory.join("inference-service.json");

        write_protected_file_atomically(target.as_path(), b"{}")
            .expect("discovery document should publish");

        let mode = fs::metadata(target.as_path())
            .expect("published discovery metadata should read")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        fs::remove_dir_all(directory.as_path()).expect("test directory should be removed");
    }

    /// Verifies Windows discovery files reject inherited readers and retain only the protected allow-list.
    /// 验证 Windows 发现文件拒绝继承读取者并仅保留受保护允许列表。
    #[cfg(windows)]
    #[test]
    fn protected_discovery_publication_uses_private_windows_acl() {
        let directory = unique_test_path("windows-permissions");
        fs::create_dir_all(directory.as_path()).expect("test directory should be created");
        let target = directory.join("inference-service.json");
        let saved_acl = directory.join("inference-service.acl");

        write_protected_file_atomically(target.as_path(), b"{}")
            .expect("discovery document should publish");

        let system_root = std::env::var_os("SystemRoot").expect("Windows SystemRoot should exist");
        let output = std::process::Command::new(
            PathBuf::from(system_root)
                .join("System32")
                .join("icacls.exe"),
        )
        .arg(target.as_path())
        .args(["/save"])
        .arg(saved_acl.as_path())
        .arg("/q")
        .output()
        .expect("Windows ACL export should launch");
        assert!(
            output.status.success(),
            "Windows ACL export failed: {}",
            String::from_utf8_lossy(output.stderr.as_slice())
        );
        let bytes = fs::read(saved_acl.as_path()).expect("saved Windows ACL should read");
        let code_units = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .filter(|unit| *unit != 0xfeff)
            .collect::<Vec<_>>();
        let descriptor =
            String::from_utf16(code_units.as_slice()).expect("saved Windows ACL should be UTF-16");
        assert!(descriptor.contains("D:P"));
        assert!(descriptor.contains("(A;;FA;;;BA)"));
        assert!(descriptor.contains("(A;;FA;;;SY)"));
        assert!(descriptor.contains("(A;;FA;;;OW)"));
        fs::remove_dir_all(directory.as_path()).expect("test directory should be removed");
    }

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
        let inference_dispatcher: InferenceServiceRpcDispatcher =
            Arc::new(|_access_token, request| {
                Box::pin(async move {
                    match request {
                        InferenceServiceRequest::ListModels => Ok(json!({
                            "object": "list",
                            "data": [{
                                "id": "test-model",
                                "object": "model",
                                "created": 0,
                                "owned_by": "test-provider"
                            }]
                        })),
                        InferenceServiceRequest::CreateResponse { request } => Ok(json!({
                            "id": "resp_test",
                            "object": "response",
                            "status": "completed",
                            "model": request.model,
                            "output": []
                        })),
                        InferenceServiceRequest::StartResponse { .. } => {
                            Ok(json!({"execution_id": "test-execution"}))
                        }
                        _ => Ok(serde_json::Value::Null),
                    }
                })
            });
        let inference_event_subscriber: InferenceServiceEventSubscriber =
            Arc::new(|_access_token, _execution_id| {
                Box::pin(async move {
                    let (_sender, receiver) = broadcast::channel(8);
                    Ok(InferenceServiceEventSubscription {
                        replay: vec![
                            AiStreamEvent::ResponseStarted {
                                execution_id: String::from("test-execution"),
                            },
                            AiStreamEvent::ResponseTextDelta {
                                item_id: String::from("test-message"),
                                delta: String::from("hello"),
                            },
                            AiStreamEvent::ResponseCompleted,
                        ],
                        receiver,
                        cancellation_guard: InferenceServiceCancellationGuard::new(|| {}),
                    })
                })
            });
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
            client_disconnected: Arc::new(|_client_id| {}),
            inference_discovery: Arc::new(Mutex::new(InferenceServiceDiscoverySnapshot {
                api_version: INFERENCE_SERVICE_API_VERSION,
                status: String::from("ready"),
                base_url: format!("http://127.0.0.1:{WEBUI_SERVICE_HTTP_PORT}"),
                process_id: 42,
                caller_id: String::from("test-caller"),
                consumer_profile_id: String::from("test-profile"),
                access_token: String::from("test-inference-token"),
                expires_at_unix_ms: u64::MAX,
                started_at_unix_ms: String::from("123"),
            })),
            inference_dispatcher,
            inference_event_subscriber,
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

    /// Verifies malformed standard request fields produce an OpenAI-compatible parameter error.
    /// 验证格式错误的标准请求字段会产生 OpenAI 兼容参数错误。
    #[tokio::test]
    async fn responses_create_rejects_unsupported_field_with_standard_error() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/v1/responses")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer test-inference-token")
                    .body(Body::from(
                        r#"{"model":"test-model","input":"hello","background":true}"#,
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), AxumStatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response should be JSON");
        assert_eq!(payload["error"]["type"], "invalid_request_error");
        assert_eq!(payload["error"]["param"], "background");
        assert_eq!(payload["error"]["code"], "unsupported_parameter");
    }

    /// Verifies non-streaming Responses returns the unwrapped standard response object.
    /// 验证非流式 Responses 返回不带私有信封的标准响应对象。
    #[tokio::test]
    async fn responses_create_returns_standard_json_object() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/v1/responses")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer test-inference-token")
                    .body(Body::from(r#"{"model":"test-model","input":"hello"}"#))
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
        assert_eq!(payload["object"], "response");
        assert_eq!(payload["model"], "test-model");
        assert!(payload.get("api_version").is_none());
    }

    /// Verifies streaming Responses emits named standard SSE lifecycle events through completion.
    /// 验证流式 Responses 会发出命名的标准 SSE 生命周期事件直至完成。
    #[tokio::test]
    async fn responses_create_streams_standard_sse_events() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/v1/responses")
                    .header("content-type", "application/json")
                    .header("authorization", "Bearer test-inference-token")
                    .body(Body::from(
                        r#"{"model":"test-model","input":"hello","stream":true}"#,
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), AxumStatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should read");
        let text = String::from_utf8(body.to_vec()).expect("SSE body should be UTF-8");
        assert!(text.contains("event: response.created"));
        assert!(text.contains("event: response.output_text.delta"));
        assert!(text.contains("\"delta\":\"hello\""));
        assert!(text.contains("event: response.completed"));
        assert!(!text.contains("execution_id"));
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

    /// Verifies protected inference operations reject callers without the dedicated token.
    /// 验证受保护推理操作拒绝缺少专用令牌的调用方。
    #[tokio::test]
    async fn inference_capabilities_reject_missing_token() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/inference/v1/capabilities/test-profile")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), AxumStatusCode::UNAUTHORIZED);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response should be JSON");
        assert_eq!(payload["api_version"], INFERENCE_SERVICE_API_VERSION);
        assert_eq!(payload["code"], "inference.unauthorized");
    }

    /// Verifies `/v1/models` uses the standard error envelope when bearer authentication is missing.
    /// 验证 `/v1/models` 在缺少 Bearer 鉴权时使用标准错误信封。
    #[tokio::test]
    async fn responses_models_rejects_missing_bearer_with_standard_error() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/v1/models")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), AxumStatusCode::UNAUTHORIZED);
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("body should read");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("response should be JSON");
        assert_eq!(payload["error"]["type"], "authentication_error");
        assert_eq!(payload["error"]["code"], "invalid_api_key");
        assert!(payload.get("api_version").is_none());
    }

    /// Verifies `/v1/models` returns the unwrapped standard model-list shape.
    /// 验证 `/v1/models` 返回不带私有信封的标准模型列表形态。
    #[tokio::test]
    async fn responses_models_returns_standard_list_shape() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/v1/models")
                    .header("authorization", "Bearer test-inference-token")
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
        assert_eq!(payload["object"], "list");
        assert_eq!(payload["data"][0]["id"], "test-model");
        assert!(payload.get("api_version").is_none());
    }

    /// Verifies the dedicated token authorizes the closed capability request envelope.
    /// 验证专用令牌可授权封闭能力请求信封。
    #[tokio::test]
    async fn inference_capabilities_accept_dedicated_token() {
        let response = test_router()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri("/inference/v1/capabilities/test-profile")
                    .header("x-vulcan-inference-token", "test-inference-token")
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
        assert_eq!(payload["api_version"], INFERENCE_SERVICE_API_VERSION);
        assert_eq!(payload["result"], serde_json::Value::Null);
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

    /// Verifies encrypted reasoning content is emitted only after explicit public selection.
    /// 验证加密推理内容仅在公共请求显式选择后发出。
    #[test]
    fn responses_sse_gates_encrypted_reasoning_content() {
        for (include, expected) in [(false, false), (true, true)] {
            let mut state = ResponsesSseState::new(
                String::from("execution-reasoning"),
                String::from("test-model"),
                include,
            );
            state.project(AiStreamEvent::ReasoningStarted {
                item_id: String::from("rs_1"),
            });
            state.project(AiStreamEvent::ReasoningDelta {
                item_id: String::from("rs_1"),
                delta: String::from("checked"),
            });
            state.project(AiStreamEvent::ReasoningCompleted {
                item_id: String::from("rs_1"),
                signature: Some(String::from("openai#opaque")),
            });

            let response = state.response_json("completed");
            assert_eq!(
                response["output"][0].get("encrypted_content").is_some(),
                expected
            );
            if include {
                assert_eq!(response["output"][0]["encrypted_content"], "opaque");
            }
        }
    }

    /// Verifies interleaved reasoning items retain independent identities, text, and signatures.
    /// 验证交错推理项分别保留独立身份、文本与签名。
    #[test]
    fn responses_sse_preserves_multiple_reasoning_items() {
        let mut state = ResponsesSseState::new(
            String::from("execution-multi-reasoning"),
            String::from("test-model"),
            true,
        );
        state.project(AiStreamEvent::ReasoningDelta {
            item_id: String::from("rs_1"),
            delta: String::from("first-a"),
        });
        state.project(AiStreamEvent::ReasoningDelta {
            item_id: String::from("rs_2"),
            delta: String::from("second"),
        });
        state.project(AiStreamEvent::ReasoningDelta {
            item_id: String::from("rs_1"),
            delta: String::from("-b"),
        });
        state.project(AiStreamEvent::ReasoningCompleted {
            item_id: String::from("rs_2"),
            signature: Some(String::from("openai#signature-two")),
        });
        state.project(AiStreamEvent::ReasoningCompleted {
            item_id: String::from("rs_1"),
            signature: Some(String::from("openai#signature-one")),
        });

        let response = state.response_json("completed");
        assert_eq!(response["output"].as_array().map(Vec::len), Some(2));
        assert_eq!(response["output"][0]["id"], "rs_1");
        assert_eq!(response["output"][0]["summary"][0]["text"], "first-a-b");
        assert_eq!(response["output"][0]["encrypted_content"], "signature-one");
        assert_eq!(response["output"][1]["id"], "rs_2");
        assert_eq!(response["output"][1]["summary"][0]["text"], "second");
        assert_eq!(response["output"][1]["encrypted_content"], "signature-two");
    }

    /// Verifies partial usage updates retain earlier known fields in the completed SSE response.
    /// 验证分段用量更新会在 SSE 完成响应中保留先前已知字段。
    #[test]
    fn responses_sse_merges_partial_usage_updates() {
        let reported = |value| UsageValue::Known {
            value,
            source: vulcan_ai_contract::UsageValueSource::Reported,
        };
        let mut state = ResponsesSseState::new(
            String::from("execution-usage"),
            String::from("test-model"),
            false,
        );
        state.project(AiStreamEvent::UsageUpdated {
            usage: Box::new(RequestUsage {
                input_tokens: reported(8),
                ..RequestUsage::default()
            }),
        });
        state.project(AiStreamEvent::UsageUpdated {
            usage: Box::new(RequestUsage {
                output_tokens: reported(2),
                ..RequestUsage::default()
            }),
        });

        let response = state.response_json("completed");
        assert_eq!(response["usage"]["input_tokens"], 8);
        assert_eq!(response["usage"]["output_tokens"], 2);
        assert_eq!(response["usage"]["total_tokens"], 10);
    }

    /// Verifies dropping an armed stream guard requests cooperative execution cancellation.
    /// 验证丢弃仍启用的流守卫会请求协作式取消执行。
    #[test]
    fn responses_stream_guard_cancels_when_dropped_armed() {
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = Arc::clone(&cancelled);
        let guard = InferenceServiceCancellationGuard::new(move || {
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        drop(guard);

        assert!(cancelled.load(std::sync::atomic::Ordering::SeqCst));
    }

    /// Verifies observing a terminal event disarms disconnect cancellation.
    /// 验证观察到终态事件后会解除断连取消。
    #[test]
    fn responses_stream_guard_does_not_cancel_after_disarm() {
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = Arc::clone(&cancelled);
        let mut guard = InferenceServiceCancellationGuard::new(move || {
            observed.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        guard.disarm();
        drop(guard);

        assert!(!cancelled.load(std::sync::atomic::Ordering::SeqCst));
    }
}
