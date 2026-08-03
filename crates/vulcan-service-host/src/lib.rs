//! HTTP and WebSocket host for the Vulcan Core Service.
//! Vulcan Core Service 的 HTTP 与 WebSocket 宿主层。

use std::{
    fs::{self, OpenOptions},
    future::Future,
    io::Write,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
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
use tokio::{net::TcpListener, sync::broadcast};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use vulcan_ai_contract::{
    EmbeddingRequest, LlmInferenceRequest, LlmInferenceStreamEvent, RerankRequest,
};
use vulcan_service_api::{
    INFERENCE_SERVICE_API_VERSION, InferenceServiceDiscoverySnapshot,
    InferenceServiceErrorEnvelope, InferenceServiceHealthSnapshot, InferenceServiceHttpError,
    InferenceServiceRequest, InferenceServiceSuccessEnvelope, ServiceEvent, ServiceEventSink,
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

/// Carries ordered replay plus live delivery for one restricted LLM stream.
/// 承载一项受限 LLM 流的有序重放与实时投递。
pub struct InferenceServiceEventSubscription {
    /// Events emitted before the subscriber attached.
    /// 订阅者接入前已经发出的事件。
    pub replay: Vec<LlmInferenceStreamEvent>,
    /// Live bounded receiver created atomically with the replay snapshot.
    /// 与重放快照原子创建的实时有界接收器。
    pub receiver: broadcast::Receiver<LlmInferenceStreamEvent>,
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
        .route(
            "/inference/v1/health",
            get(inference_service_health_http_handler),
        )
        .route(
            "/inference/v1/capabilities/{consumer_profile_id}",
            get(inference_service_capabilities_http_handler),
        )
        .route(
            "/inference/v1/llm/start",
            post(inference_service_llm_start_http_handler)
                .layer(DefaultBodyLimit::max(INFERENCE_SERVICE_BODY_LIMIT_BYTES)),
        )
        .route(
            "/inference/v1/llm/execute",
            post(inference_service_llm_execute_http_handler)
                .layer(DefaultBodyLimit::max(INFERENCE_SERVICE_BODY_LIMIT_BYTES)),
        )
        .route(
            "/inference/v1/llm/{execution_id}",
            get(inference_service_llm_status_http_handler),
        )
        .route(
            "/inference/v1/llm/{execution_id}/cancel",
            post(inference_service_llm_cancel_http_handler),
        )
        .route(
            "/inference/v1/llm/{execution_id}/events",
            get(inference_service_llm_events_ws_handler),
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

/// Starts one restricted LLM execution.
/// 启动一项受限 LLM 执行。
async fn inference_service_llm_start_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    Json(request): Json<LlmInferenceRequest>,
) -> AxumResponse {
    inference_service_dispatch_http(
        &server_state,
        &headers,
        InferenceServiceRequest::StartLlm { request },
    )
    .await
}

/// Executes and aggregates one restricted LLM request.
/// 执行并聚合一项受限 LLM 请求。
async fn inference_service_llm_execute_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    headers: AxumHeaderMap,
    Json(request): Json<LlmInferenceRequest>,
) -> AxumResponse {
    inference_service_dispatch_http(
        &server_state,
        &headers,
        InferenceServiceRequest::ExecuteLlm { request },
    )
    .await
}

/// Returns one caller-owned restricted LLM status.
/// 返回一项调用方所有的受限 LLM 状态。
async fn inference_service_llm_status_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumPath(execution_id): AxumPath<String>,
    headers: AxumHeaderMap,
) -> AxumResponse {
    inference_service_dispatch_http(
        &server_state,
        &headers,
        InferenceServiceRequest::LlmStatus { execution_id },
    )
    .await
}

/// Cancels one caller-owned restricted LLM execution.
/// 取消一项调用方所有的受限 LLM 执行。
async fn inference_service_llm_cancel_http_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumPath(execution_id): AxumPath<String>,
    headers: AxumHeaderMap,
) -> AxumResponse {
    inference_service_dispatch_http(
        &server_state,
        &headers,
        InferenceServiceRequest::CancelLlm { execution_id },
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

/// Opens one caller-owned restricted LLM event stream.
/// 打开一项调用方所有的受限 LLM 事件流。
async fn inference_service_llm_events_ws_handler(
    AxumState(server_state): AxumState<WebuiServiceHttpState>,
    AxumPath(execution_id): AxumPath<String>,
    headers: AxumHeaderMap,
    websocket: WebSocketUpgrade,
) -> AxumResponse {
    let Some(token) = inference_service_token_from_headers(&headers) else {
        return inference_service_error_response(InferenceServiceHttpError {
            status: 401,
            code: String::from("inference.unauthorized"),
            message: String::from("Inference access token is missing."),
        });
    };
    let subscription = match (server_state.inference_event_subscriber)(token, execution_id).await {
        Ok(subscription) => subscription,
        Err(error) => return inference_service_error_response(error),
    };
    websocket
        .on_upgrade(move |socket| inference_service_llm_events_socket(socket, subscription))
        .into_response()
}

/// Streams restricted LLM events and fails closed when a consumer falls behind.
/// 推送受限 LLM 事件，并在消费者滞后时失败关闭。
///
/// # Parameters
/// 参数
/// `socket` is the authorized WebSocket and `receiver` is caller-owned.
/// `socket` 是已授权 WebSocket，`receiver` 由调用方所有。
async fn inference_service_llm_events_socket(
    mut socket: WebSocket,
    subscription: InferenceServiceEventSubscription,
) {
    for event in subscription.replay {
        let text = match serde_json::to_string(&event) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("[inference-service] failed to serialize replayed LLM event: {error}");
                return;
            }
        };
        if socket.send(AxumWsMessage::Text(text.into())).await.is_err() {
            return;
        }
    }
    let mut receiver = subscription.receiver;
    loop {
        let event = match receiver.recv().await {
            Ok(event) => event,
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                let failure = LlmInferenceStreamEvent::Failed {
                    category: String::from("backpressure"),
                    message: format!(
                        "Inference event consumer lagged behind by {skipped} buffered events."
                    ),
                    provider_error: None,
                };
                if let Ok(text) = serde_json::to_string(&failure) {
                    let _ = socket.send(AxumWsMessage::Text(text.into())).await;
                }
                let _ = socket.send(AxumWsMessage::Close(None)).await;
                break;
            }
            Err(broadcast::error::RecvError::Closed) => break,
        };
        let text = match serde_json::to_string(&event) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("[inference-service] failed to serialize LLM event: {error}");
                break;
            }
        };
        if socket.send(AxumWsMessage::Text(text.into())).await.is_err() {
            break;
        }
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
            Arc::new(|_access_token, _request| {
                Box::pin(async move { Ok(serde_json::Value::Null) })
            });
        let inference_event_subscriber: InferenceServiceEventSubscriber =
            Arc::new(|_access_token, _execution_id| {
                Box::pin(async move {
                    let (_sender, receiver) = broadcast::channel(8);
                    Ok(InferenceServiceEventSubscription {
                        replay: Vec::new(),
                        receiver,
                    })
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
            inference_discovery: Arc::new(Mutex::new(InferenceServiceDiscoverySnapshot {
                api_version: INFERENCE_SERVICE_API_VERSION,
                status: String::from("ready"),
                base_url: String::from("http://127.0.0.1:17891"),
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
}
