//! Browser service bridge and RPC adapter helpers.
//! 浏览器服务桥接与 RPC 适配辅助函数。

use super::*;
use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::{FuturesUnordered, StreamExt};
use reqwest::Url;
use std::collections::HashMap;
use std::sync::OnceLock;
use vulcan_service_api::{WebuiServiceDiscoverySnapshot, WebuiServiceRpcError};

/// Stable service result returned after an external URL is opened.
/// 外部链接成功打开后返回的稳定服务结果。
const EXTERNAL_URL_OPENED: &str = "opened";

/// Stable service result requesting an explicit trusted-UI user confirmation.
/// 请求可信界面进行明确用户确认时返回的稳定服务结果。
const EXTERNAL_URL_CONFIRMATION_REQUIRED: &str = "confirmation_required";

/// Explicit worker-stack capacity reserved for deeply nested agent stream polling on the WebUI bridge runtime.
/// 为 WebUI 桥接运行时中的深层智能体流轮询显式预留的工作线程栈容量。
const WEBUI_SERVICE_WORKER_STACK_BYTES: usize =
    vulcan_agent_runtime::AGENT_STREAM_WORKER_STACK_BYTES;

/// Returns the one process-lifetime generation timestamp shared by discovery and status.
/// 返回发现记录与状态接口共用的进程生命周期代次时间戳。
/// No parameters; the returned timestamp remains stable until this Core process exits.
/// 无参数；返回的时间戳在当前 Core 进程退出前保持不变。
fn service_generation_started_at() -> &'static str {
    // The tray compares discovery with live status; sampling the clock independently on each
    // read makes a healthy Core look like a different generation every time.
    // 托盘会比较发现记录与实时状态；每次读取分别取时钟会让健康 Core 每次都像新代次。
    static STARTED_AT: OnceLock<String> = OnceLock::new();
    STARTED_AT.get_or_init(current_unix_millis_text).as_str()
}

/// Build the host's HTTP execution configuration without starting threads or publishing discovery.
/// 构建宿主 HTTP 执行配置，不启动线程或发布服务发现。
/// No parameters; returns the same builder used by production dispatch and isolated integration tests.
/// 无参数；返回生产派发与隔离集成测试共用的构造器。
pub(crate) fn webui_service_runtime_builder() -> tokio::runtime::Builder {
    // Keep stack ownership at the actual chat entrypoint instead of duplicating it in callers.
    // 将栈配置归属于实际聊天入口，而不在调用方重复定义。
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder
        .thread_name("vulcan-webui-bridge-runtime")
        .thread_stack_size(WEBUI_SERVICE_WORKER_STACK_BYTES)
        .enable_all();
    builder
}

/// Opens a trusted external URL with the system default browser.
/// 使用系统默认浏览器打开可信外部链接。
pub(crate) fn service_open_external_url(
    state: SharedServiceBridgeState,
    url: String,
    user_confirmed: bool,
) -> Result<String, String> {
    if !external_url_is_safe(&url) {
        return Err(String::from("不允许打开不安全的外部链接。"));
    }
    if !external_url_is_allowed(&url) && !user_confirmed {
        return Ok(String::from(EXTERNAL_URL_CONFIRMATION_REQUIRED));
    }
    shell_host_from_state(&state)?.open_url(&url)?;
    Ok(String::from(EXTERNAL_URL_OPENED))
}

/// Returns whether one external URL satisfies non-bypassable transport and credential safety rules.
/// 返回外部链接是否满足不可绕过的传输与凭据安全规则。
///
/// `url` is the exact caller-provided external URL.
/// `url` 是调用方提供的精确外部 URL。
/// Returns true only for HTTPS URLs with a host and without embedded credentials.
/// 仅对包含主机且不内嵌凭据的 HTTPS URL 返回 true。
/// Every host-initiated browser open of a URL from outside the host must pass this rule.
/// 宿主主动在浏览器中打开来自宿主之外的 URL 时，都必须通过此规则。
pub(crate) fn external_url_is_safe(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str().is_some()
        && parsed.username().is_empty()
        && parsed.password().is_none()
}

/// Returns whether one external URL may be opened by the host shell.
/// 返回一个外部链接是否允许由宿主 shell 打开。
fn external_url_is_allowed(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    // allowed_host limits shell navigation to exact source-defined HTTPS domains.
    // allowed_host 将 Shell 导航限制为源码明确声明的精确 HTTPS 域名。
    let allowed_host = matches!(
        parsed.host_str(),
        Some(
            "github.com"
                | "context7.com"
                | "apikey.fan"
                | "console.apito.ai"
                | "s.qiniu.com"
                | "api.fenno.ai"
                | "console.code0.ai"
        )
    );
    external_url_is_safe(url) && allowed_host
}

/// Returns the host shell adapter from shared service state.
/// 从共享服务状态中返回宿主 shell 适配器。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns the shell adapter.
/// `state` 是持有 shell 适配器的共享服务状态。
///
/// # Returns
/// 返回值
/// A cloned shell host adapter reference for use outside the state lock.
/// 返回可在状态锁之外使用的 shell 宿主适配器引用。
pub(crate) fn shell_host_from_state(
    state: &SharedServiceBridgeState,
) -> Result<Arc<dyn ServiceShellHost>, String> {
    state
        .lock()
        .map(|guard| Arc::clone(&guard.shell_host))
        .map_err(|error| format!("failed to lock service shell host: {error}"))
}

/// Dispatches one normalized service RPC request to the concrete local service implementation.
/// 将单个标准化服务 RPC 请求分发到具体本地服务实现。
pub(crate) fn rpc_json_result<T>(result: Result<T, String>) -> Result<Value, WebuiServiceRpcError>
where
    T: Serialize,
{
    result
        .map_err(WebuiServiceRpcError::Service)
        .and_then(|value| {
            serde_json::to_value(value).map_err(|error| {
                WebuiServiceRpcError::Serialization(format!(
                    "failed to serialize service RPC response: {error}"
                ))
            })
        })
}

/// Reads a required typed value from one RPC parameter object.
/// 从单个 RPC 参数对象读取必填类型值。
pub(crate) fn rpc_required_param<T>(params: &Value, key: &str) -> Result<T, WebuiServiceRpcError>
where
    T: DeserializeOwned,
{
    let object = rpc_params_object(params)?;
    let value = object.get(key).ok_or_else(|| {
        WebuiServiceRpcError::InvalidParams(format!("missing service RPC parameter: {key}"))
    })?;
    deserialize_rpc_param(key, value)
}

/// Reads an optional typed value from one RPC parameter object.
/// 从单个 RPC 参数对象读取可选类型值。
pub(crate) fn rpc_optional_param<T>(
    params: &Value,
    key: &str,
) -> Result<Option<T>, WebuiServiceRpcError>
where
    T: DeserializeOwned,
{
    let object = rpc_params_object(params)?;
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    deserialize_rpc_param(key, value).map(Some)
}

/// Borrows the JSON object backing one service RPC params payload.
/// 借用单个服务 RPC 参数载荷背后的 JSON 对象。
///
/// # Parameters
/// 参数
/// `params` is the raw RPC params value supplied by the WebUI bridge.
/// `params` 是 WebUI 桥接层提供的原始 RPC 参数值。
///
/// # Returns
/// 返回值
/// The borrowed params object, or the canonical non-object params error.
/// 返回借用的参数对象，或标准非对象参数错误。
fn rpc_params_object(
    params: &Value,
) -> Result<&serde_json::Map<String, serde_json::Value>, WebuiServiceRpcError> {
    params.as_object().ok_or_else(|| {
        WebuiServiceRpcError::InvalidParams(String::from(
            "service RPC params must be a JSON object",
        ))
    })
}

/// Deserializes one typed service RPC parameter with the canonical error text.
/// 使用标准错误文本反序列化单个类型化服务 RPC 参数。
///
/// # Parameters
/// 参数
/// `key` is the parameter name used in diagnostics.
/// `key` 是诊断中使用的参数名称。
/// `value` is the raw JSON parameter value.
/// `value` 是原始 JSON 参数值。
///
/// # Returns
/// 返回值
/// The typed parameter value, or the canonical invalid-parameter error.
/// 返回类型化参数值，或标准无效参数错误。
fn deserialize_rpc_param<T>(key: &str, value: &Value) -> Result<T, WebuiServiceRpcError>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value.clone()).map_err(|error| {
        WebuiServiceRpcError::InvalidParams(format!(
            "invalid service RPC parameter `{key}`: {error}"
        ))
    })
}

/// Spawns the browser-facing local service bridge on the first available loopback port.
/// 在首个可用回环端口启动面向浏览器的本地服务桥接层。
///
/// # Parameters
/// 参数
/// `manifest_static_root` is the WebUI root declared by the validated runtime layout manifest.
/// `manifest_static_root` 是已校验运行时布局清单声明的 WebUI 根目录。
pub fn spawn_webui_service_http_bridge(
    event_emitter: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    event_bus: broadcast::Sender<ServiceEvent>,
    manifest_static_root: Option<PathBuf>,
) -> Result<WebuiServiceHttpBridgeHandle, String> {
    // The one-shot acknowledgement carries real bind/publication failures to the caller.
    // 一次性确认将实际绑定或发布失败传递给调用方，不以固定休眠推测就绪。
    let (listening, ready) = std::sync::mpsc::channel();
    let failure: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let failure_for_thread = Arc::clone(&failure);
    let failure_state = Arc::clone(&state);
    let failure_emitter = event_emitter.clone();
    let join = std::thread::Builder::new()
        .name(String::from("vulcan-webui-http-bridge"))
        .spawn(move || {
            // Retain a sender for failures occurring before the listener acknowledgement.
            // 为监听确认之前发生的错误保留发送端。
            let errors = listening.clone();
            // A returned listener or a panicked bridge is abnormal while its owner is alive.
            // 所有者仍运行时监听器返回或桥线程恐慌都属于异常。
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                webui_service_runtime_builder()
                    .build()
                    .map_err(|error| error.to_string())
                    .and_then(|runtime| {
                        runtime.block_on(run_webui_service_http_bridge(
                            event_emitter,
                            state,
                            event_bus,
                            listening,
                            manifest_static_root,
                        ))
                    })
            }));
            let error = match outcome {
                Ok(Ok(())) => String::from("HTTP bridge stopped unexpectedly"),
                Ok(Err(error)) => error,
                Err(_) => String::from("HTTP bridge thread panicked"),
            };
            tracing::warn!("[webui-service] HTTP bridge failed: {error}");
            // Publish the fault to the same service state that bootstrap checks before ready.
            // 把故障发布到引导在发布 ready 前检查的同一服务状态。
            if let Ok(mut guard) = failure_state.lock() {
                guard.service_lifecycle_state.http_bridge_failed = true;
            }
            crate::service_bootstrap::update_service_failure(
                &failure_state,
                &failure_emitter,
                crate::service_bootstrap::ServiceBootstrapError::new(
                    "service-process",
                    "Core Service",
                    format!("HTTP bridge failed: {error}"),
                ),
            );
            if let Ok(mut failure) = failure_for_thread.lock() {
                *failure = Some(error.clone());
            }
            let _ = errors.send(Err(error));
        })
        .map_err(|error| error.to_string())?;
    ready
        .recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|error| format!("Service listener acknowledgement failed: {error}"))??;
    Ok(WebuiServiceHttpBridgeHandle {
        _join: Some(join),
        failure,
    })
}

/// Owned join handle for the browser-facing HTTP bridge thread.
/// 浏览器侧 HTTP 桥线程拥有的 join 句柄。
pub struct WebuiServiceHttpBridgeHandle {
    /// Bridge thread retained until the owning service releases it.
    /// 归属服务释放前一直保留的桥线程。
    _join: Option<std::thread::JoinHandle<()>>,
    /// Listener failure reported after the acknowledgement, observed by the supervisor.
    /// 确认后报告的监听失败，由监督器观察。
    failure: Arc<Mutex<Option<String>>>,
}

impl WebuiServiceHttpBridgeHandle {
    /// Returns the post-acknowledgement bridge failure observed by the supervisor.
    /// 返回监督器观察到的确认后桥失败。
    pub fn observed_failure(&self) -> Option<String> {
        self.failure
            .lock()
            .map(|failure| failure.clone())
            .unwrap_or(None)
    }

    /// Borrows the owned bridge failure slot for an external supervisor.
    /// 为外部监督器借出自有桥失败槽。
    pub fn failure_slot(&self) -> Arc<Mutex<Option<String>>> {
        Arc::clone(&self.failure)
    }
}

/// Runs the browser-facing local service bridge inside an owned Tokio runtime.
/// 在自有 Tokio 运行时中运行面向浏览器的本地服务桥接层。
///
/// # Parameters
/// 参数
/// `event_emitter` forwards service events to all frontend sinks.
/// `event_emitter` 将服务事件转发给所有前端接收器。
/// `state` is the shared service state used by RPC dispatch.
/// `state` 是 RPC 分发使用的共享服务状态。
/// `event_bus` broadcasts service events to WebSocket clients.
/// `event_bus` 将服务事件广播给 WebSocket 客户端。
pub(crate) async fn run_webui_service_http_bridge(
    event_emitter: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    event_bus: broadcast::Sender<ServiceEvent>,
    listening: std::sync::mpsc::Sender<Result<(), String>>,
    manifest_static_root: Option<PathBuf>,
) -> Result<(), String> {
    let port_override = env::var(WEBUI_SERVICE_PORT_ENV).ok();
    // Binding and publication finish before bootstrap is admitted.
    // 绑定和发布完成后才允许启动引导，避免就绪状态被初始状态覆盖。
    let (listener, address) = bind_webui_service_http_listener(port_override.as_deref()).await?;
    let port = address.port();
    let service_token = SessionId::new_v4().to_string();
    let discovery = webui_service_starting_discovery_snapshot(
        port,
        std::process::id(),
        service_generation_started_at().to_owned(),
        service_token.clone(),
    );
    let shared_discovery = Arc::new(Mutex::new(discovery.clone()));
    state
        .lock()
        .map_err(|error| error.to_string())?
        .webui_service_discovery = Some(Arc::clone(&shared_discovery));
    write_webui_service_discovery_snapshot(&discovery, dirs::home_dir(), std::env::temp_dir())?;
    let rpc_event_emitter = event_emitter.clone();
    let rpc_state = state.clone();
    let rpc_dispatcher: WebuiServiceRpcDispatcher = Arc::new(move |method, params| {
        let event_emitter = rpc_event_emitter.clone();
        let state = rpc_state.clone();
        Box::pin(dispatch_service_rpc(event_emitter, state, method, params))
    });
    // client_disconnected releases every content subscription tied to the exact event socket.
    // client_disconnected 释放与精确事件套接字绑定的全部内容订阅。
    let client_disconnected = Arc::new(move |client_id: Uuid| {
        cancel_command_session_displays_for_client(client_id);
        cancel_project_terminal_displays_for_client(client_id);
    });
    // session_resource_state preserves the same kernel authority for opaque content resolution.
    // session_resource_state 为不透明内容解析保留同一内核权威。
    let session_resource_state = state.clone();
    let status_state = state.clone();
    let browser_download_state = state.clone();
    let static_root = webui_static_root(manifest_static_root);
    let html_answers = crate::session_html_answers::viewing::create_html_answer_runtime(
        &state,
        &discovery.base_url,
        static_root.clone(),
    )?;
    state
        .lock()
        .map_err(|error| error.to_string())?
        .html_answer_runtime = Some(Arc::downgrade(&html_answers));
    let service_config = WebuiServiceHttpConfig {
        html_answers: Some(html_answers),
        discovery: shared_discovery,
        service_token,
        event_bus,
        static_root,
        rpc_dispatcher,
        session_resource_resolver: Arc::new(move |request| {
            let state = session_resource_state.clone();
            Box::pin(async move {
                crate::conversation_resources_v1::service_session_resource_content_v1(
                    state, request,
                )
                .await
            })
        }),
        browser_download_resolver: Some(Arc::new(move |download_id, owner_session_id, is_head| {
            let state = browser_download_state.clone();
            Box::pin(async move {
                let manager = crate::browser::service_browser_manager(&state)?;
                let (record, bytes) = tokio::task::spawn_blocking(move || {
                    manager.download_content(&download_id, &owner_session_id)
                })
                .await
                .map_err(|error| format!("browser download worker failed: {error}"))??;
                let byte_length = bytes.len();
                Ok(vulcan_service_api::ServiceMediaResponse {
                    status: 200,
                    headers: vec![
                        (
                            String::from("Content-Type"),
                            String::from("application/octet-stream"),
                        ),
                        (
                            String::from("Content-Disposition"),
                            browser_download_content_disposition(&record.file_name),
                        ),
                        (String::from("Content-Length"), byte_length.to_string()),
                        (String::from("Cache-Control"), String::from("no-store")),
                        (
                            String::from("X-Content-Type-Options"),
                            String::from("nosniff"),
                        ),
                    ],
                    body: if is_head { Vec::new() } else { bytes },
                })
            })
        })),
        client_disconnected,
        status_provider: Some(Arc::new(move || {
            service_status_snapshot_from_state(&status_state)
        })),
        // The typed browser routes receive closures over the kernel manager, so the transport layer
        // never links against the kernel and every browser operation keeps one implementation.
        // 类型化浏览器路由接收操作内核管理器的闭包，因此传输层绝不链接内核，且每个浏览器操作只有
        // 一份实现。
        browser_stream: {
            let ticket_state = state.clone();
            let resume_state = state.clone();
            let poll_state = state.clone();
            let release_state = state.clone();
            Some(vulcan_service_host::WebuiServiceBrowserStream {
                issue_ticket: Arc::new(move |session_id, owner_session_id| {
                    let state = ticket_state.clone();
                    Box::pin(async move {
                        let manager = crate::browser::service_browser_manager(&state)?;
                        manager.require_owner(&session_id, &owner_session_id)?;
                        serde_json::to_value(manager.issue_stream_ticket(&session_id)?)
                            .map_err(|error| format!("failed to serialize stream ticket: {error}"))
                    })
                }),
                resume: Arc::new(move |ticket, page_id, subscription_id, cursor| {
                    let state = resume_state.clone();
                    Box::pin(async move {
                        // Reject malformed cursors before binding any observer resources.
                        // 在绑定任何观察资源前拒绝格式错误的游标。
                        let cursor = match cursor {
                            Some(cursor) => {
                                Some(serde_json::from_value(cursor).map_err(|error| {
                                    format!("invalid browser stream cursor: {error}")
                                })?)
                            }
                            None => None,
                        };
                        let manager = crate::browser::service_browser_manager(&state)?;
                        // The ticket is the sole authority for the observed session.
                        // 凭据是所观察会话的唯一权威。
                        let session_id = manager.consume_stream_ticket(&ticket)?;
                        manager.bind_stream_page(&session_id, &page_id, &subscription_id)?;
                        // A fresh connection waits for one real frame while screenshots remain
                        // asynchronous, so subscribing cannot hold the host input-control gate.
                        // 新连接等待一帧真实画面，但截图保持异步，订阅过程不能占住宿主输入控制锁。
                        let capture = async {
                            let deadline =
                                tokio::time::Instant::now() + std::time::Duration::from_secs(2);
                            loop {
                                if manager.retained_frame_count(&session_id, &subscription_id)? > 0
                                {
                                    break Ok(());
                                }
                                tokio::time::timeout_at(
                                    deadline,
                                    crate::browser::service_browser_page_capture_deferred(
                                        state.clone(),
                                        crate::browser::BrowserPageCaptureRequest {
                                            session_id: session_id.clone(),
                                            page_id: page_id.clone(),
                                        },
                                    ),
                                )
                                .await
                                .map_err(|_| {
                                    String::from("browser first frame exceeded two seconds")
                                })??;
                                if manager.retained_frame_count(&session_id, &subscription_id)? > 0
                                {
                                    break Ok(());
                                }
                                if tokio::time::Instant::now() >= deadline {
                                    break Err(String::from(
                                        "browser first frame exceeded two seconds",
                                    ));
                                }
                                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                            }
                        }
                        .await;
                        if let Err(error) = capture {
                            let _ =
                                manager.release_stream_subscription(&session_id, &subscription_id);
                            return Err(error);
                        }
                        let resume = match manager.resume_bound_stream(
                            &session_id,
                            &subscription_id,
                            cursor,
                        ) {
                            Ok(resume) => resume,
                            Err(error) => {
                                let _ = manager
                                    .release_stream_subscription(&session_id, &subscription_id);
                                return Err(error);
                            }
                        };
                        let mut answer = serde_json::to_value(resume).map_err(|error| {
                            let _ =
                                manager.release_stream_subscription(&session_id, &subscription_id);
                            format!("failed to serialize stream resume: {error}")
                        })?;
                        // The authorising session travels with the answer so the continuing stream
                        // never has to guess which session it observes.
                        // 授权会话随答复一起传递，使持续流无需猜测自己观察的是哪个会话。
                        if let Some(object) = answer.as_object_mut() {
                            object.insert(
                                String::from("sessionId"),
                                serde_json::Value::String(session_id),
                            );
                        }
                        Ok(answer)
                    })
                }),
                poll: Arc::new(move |session_id, subscription_id, cursor| {
                    let state = poll_state.clone();
                    Box::pin(async move {
                        let manager = crate::browser::service_browser_manager(&state)?;
                        let cursor: crate::browser::BrowserStreamCursor =
                            serde_json::from_value(cursor).map_err(|error| {
                                format!("invalid browser stream cursor: {error}")
                            })?;
                        let resume =
                            manager.poll_bound_stream(&session_id, &subscription_id, cursor)?;
                        serde_json::to_value(resume)
                            .map_err(|error| format!("failed to serialize stream resume: {error}"))
                    })
                }),
                release: Arc::new(move |session_id, subscription_id| {
                    let state = release_state.clone();
                    Box::pin(async move {
                        let manager = crate::browser::service_browser_manager(&state)?;
                        // Only a connection-owned subscription is released, so a disconnecting
                        // observer cannot clear an explicitly subscribed observer's frames.
                        // 只释放由连接拥有的订阅，因此断开的观察端无法清除显式订阅观察者的帧。
                        let released =
                            manager.release_stream_subscription(&session_id, &subscription_id)?;
                        Ok(serde_json::json!({ "released": released }))
                    })
                }),
            })
        },
    };
    tracing::info!("[webui-service] browser service bridge listening on http://{address}");
    listening.send(Ok(())).map_err(|error| error.to_string())?;
    // Only a test-support binary can simulate a listener dying after startup acknowledgement.
    // 只有测试支持构建可以模拟监听器在启动确认后死亡。
    #[cfg(feature = "test-support")]
    if std::env::var("VULCAN_TEST_HTTP_BRIDGE_FAIL_AFTER_ACK")
        .ok()
        .as_deref()
        == Some("1")
    {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        return Err(String::from(
            "injected post-acknowledgement HTTP bridge failure",
        ));
    }
    // Host maintenance can wait for a stuck host command; it must not delay frames from other sessions.
    // 宿主维护可能等待卡住的宿主命令，不能延迟其他会话的画面。
    tokio::spawn(run_browser_maintenance(state.clone()));
    tokio::spawn(run_browser_frame_producer(state));
    serve_webui_service_http_listener(listener, service_config)
        .await
        .map_err(|error| error.to_string())
}
/// Builds an RFC 5987 attachment header from Core's sanitized download filename.
/// 根据 Core 清理后的下载文件名构建 RFC 5987 附件响应头。
fn browser_download_content_disposition(file_name: &str) -> String {
    let mut encoded = String::new();
    for byte in file_name.as_bytes() {
        if matches!(
            *byte,
            b'0'..=b'9'
                | b'A'..=b'Z'
                | b'a'..=b'z'
                | b'!'
                | b'#'..=b'&'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        ) {
            encoded.push(char::from(*byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    format!("attachment; filename*=UTF-8''{encoded}")
}

/// Releases one completed scheduling attempt without forgetting another attempt for the same page.
/// 释放一项已完成调度，同时保留同一页面的另一项在途调度。
///
/// `in_flight` tracks bounded page attempts and `completed` identifies the finished page.
/// `in_flight` 跟踪页面的有界在途数，`completed` 指定刚结束的页面。
/// `session_in_flight` tracks the same attempt against its owning session.
/// `session_in_flight` 按所属会话跟踪同一项调度。
fn release_browser_frame_attempt(
    in_flight: &mut HashMap<(String, String), usize>,
    session_in_flight: &mut HashMap<String, usize>,
    completed: &(String, String),
) {
    if let Some(count) = in_flight.get_mut(completed) {
        *count -= 1;
        if *count == 0 {
            in_flight.remove(completed);
        }
        if let Some(session_count) = session_in_flight.get_mut(&completed.0) {
            *session_count -= 1;
            if *session_count == 0 {
                session_in_flight.remove(&completed.0);
            }
        }
    }
}

/// Captures each observed Core page once per cadence, independent of observer delivery speed.
/// 按固定节奏对每个被观察的 Core 页面采集一次，不受观察端交付速度影响。
///
/// # Parameters / 参数
/// `state` owns the same BrowserManager as RPC and stream subscriptions.
/// `state` 拥有与 RPC 和流订阅相同的 BrowserManager。
///
/// # Returns / 返回值
/// The task runs until its owning HTTP runtime shuts down.
/// 任务运行直至所属 HTTP 运行时关闭。
async fn run_browser_frame_producer(state: SharedServiceBridgeState) {
    // A 40 ms attempt cadence leaves scheduling headroom for the measured 20 fps projection goal.
    // 40 毫秒尝试周期为实测每秒 20 帧的投影目标保留调度余量。
    let mut cadence = tokio::time::interval(std::time::Duration::from_millis(40));
    // Per-page counts bound queued acknowledgements without serializing the next cadence.
    // 每页计数限制排队确认，同时不把下一轮采集串行化。
    let mut in_flight = HashMap::<(String, String), usize>::new();
    // A single browser with many observed pages may use at most one quarter of frame slots.
    // 单个浏览器即使有许多被观察页面，也最多占用四分之一的采集名额。
    let mut session_in_flight = HashMap::<String, usize>::new();
    // Pending attempts are polled independently so one slow page cannot delay the next cadence.
    // 独立轮询待完成的采集，使慢页面不能拖延下一轮调度。
    let mut pending = FuturesUnordered::<BoxFuture<'static, (String, String)>>::new();
    // Rotating the starting page prevents a stable sorted order from starving later sessions.
    // 轮换起始页面，避免固定排序让后续会话长期得不到采集机会。
    let mut next_page_offset = 0usize;
    loop {
        tokio::select! {
            _ = cadence.tick() => {
                // A completion and timer tick may become ready together. Drain completions
                // first so a fast page can schedule its next frame on this same tick.
                // 完成事件与计时可能同时就绪；先收取完成项，让正常页面在本轮即可安排下一帧。
                while let Some(Some(completed)) = pending.next().now_or_never() {
                    release_browser_frame_attempt(&mut in_flight, &mut session_in_flight, &completed);
                }
                let Ok(manager) = crate::browser::service_browser_manager(&state) else {
                    continue;
                };
                let Ok(mut pages) = manager.observed_page_targets() else {
                    continue;
                };
                if pages.is_empty() {
                    continue;
                }
                let rotation = next_page_offset % pages.len();
                pages.rotate_left(rotation);
                next_page_offset = next_page_offset.wrapping_add(1);
                // Keep frame attempts below the manager's 32-call blocking limit, leaving
                // capacity for control input even when every observed host is slow.
                // 采集请求数低于管理器的 32 项阻塞上限，即使所有宿主变慢仍为控制输入留出容量。
                for (session_id, page_id) in pages {
                    if pending.len() >= 16 {
                        break;
                    }
                    if session_in_flight.get(&session_id).copied().unwrap_or(0) >= 4 {
                        continue;
                    }
                    let count = in_flight.entry((session_id.clone(), page_id.clone())).or_default();
                    if *count >= 2 {
                        continue;
                    }
                    *count += 1;
                    *session_in_flight.entry(session_id.clone()).or_default() += 1;
                    let capture_state = state.clone();
                    pending.push(async move {
                        let _ = crate::browser::service_browser_page_capture_deferred(
                            capture_state,
                            crate::browser::BrowserPageCaptureRequest {
                                session_id: session_id.clone(),
                                page_id: page_id.clone(),
                            },
                        )
                        .await;
                        (session_id, page_id)
                    }.boxed());
                }
            }
            Some(completed) = pending.next(), if !pending.is_empty() => {
                release_browser_frame_attempt(&mut in_flight, &mut session_in_flight, &completed);
            }
        }
    }
}

/// Expires leases and reaps hosts independently from the page frame producer.
/// 独立于页面画面生产者处理租约到期和宿主回收。
///
/// `state` owns the same manager as the browser RPC and frame producer.
/// `state` 与浏览器 RPC、画面生产者共享同一个管理器。
/// The task runs until its owning HTTP runtime shuts down.
/// 任务运行直至所属 HTTP 运行时关闭。
async fn run_browser_maintenance(state: SharedServiceBridgeState) {
    // Maintenance needs timely lease cleanup but never participates in the 40 ms frame cadence.
    // 维护需要及时清理租约，但不参与每 40 毫秒的画面采集节奏。
    let mut cadence = tokio::time::interval(std::time::Duration::from_millis(200));
    loop {
        cadence.tick().await;
        let Ok(manager) = crate::browser::service_browser_manager(&state) else {
            continue;
        };
        let _ = tokio::task::spawn_blocking(move || {
            let _ = manager.expire_control_leases();
            let _ = manager.reap_hosts();
        })
        .await;
    }
}

/// Marks the browser-facing service discovery snapshot with a terminal bootstrap status.
/// 使用终态引导状态标记浏览器侧服务发现快照。
///
/// # Parameters
/// 参数
/// `state` contains the shared discovery snapshot, and `status` is the new discovery status string.
/// `state` 包含共享发现快照，`status` 是新的发现状态字符串。
pub(crate) fn mark_webui_service_discovery_status(
    state: &SharedServiceBridgeState,
    status: &'static str,
) {
    // Keep the state lock through the publication: a bridge-failure publisher takes the same
    // lock before writing `failed`, so a late bootstrap can never overwrite that terminal state.
    // 发布期间保持状态锁：桥故障发布器写入 failed 前也取得同一把锁，因此迟到的引导绝不能覆盖终态。
    let state_guard = match state.lock() {
        Ok(guard) => guard,
        Err(error) => error.into_inner(),
    };
    if status == WEBUI_SERVICE_DISCOVERY_STATUS_READY
        && state_guard.service_lifecycle_state.http_bridge_failed
    {
        return;
    }
    let shared_discovery = state_guard.webui_service_discovery.clone();
    let Some(shared_discovery) = shared_discovery else {
        return;
    };
    let status_snapshot = {
        let mut snapshot = match shared_discovery.lock() {
            Ok(snapshot) => snapshot,
            Err(error) => error.into_inner(),
        };
        *snapshot = webui_service_discovery_snapshot_with_replaced_status(&snapshot, status);
        snapshot.clone()
    };
    if let Err(error) = write_webui_service_discovery_snapshot(
        &status_snapshot,
        dirs::home_dir(),
        std::env::temp_dir(),
    ) {
        tracing::warn!("[webui-service] browser service discovery {status} write failed: {error}");
    }
    drop(state_guard);
}

/// Rebuilds one WebUI service discovery snapshot with a replacement status.
/// 使用替换状态重建一个 WebUI 服务发现快照。
///
/// # Parameters
/// 参数
/// `snapshot` is the current discovery snapshot whose stable fields should be preserved.
/// `snapshot` 是需要保留稳定字段的当前发现快照。
/// `status` is the replacement discovery status.
/// `status` 是替换后的发现状态。
///
/// # Returns
/// 返回值
/// A discovery snapshot with the same port, pid, startup time, and token, but a new status.
/// 返回保留相同端口、进程标识、启动时间和令牌但使用新状态的发现快照。
fn webui_service_discovery_snapshot_with_replaced_status(
    snapshot: &WebuiServiceDiscoverySnapshot,
    status: &'static str,
) -> WebuiServiceDiscoverySnapshot {
    webui_service_discovery_snapshot_with_status(
        status,
        snapshot.port,
        snapshot.pid,
        snapshot.started_at.clone(),
        snapshot.service_token.clone(),
    )
}

/// Marks the browser-facing service discovery snapshot as ready after bootstrap completes.
/// 在引导完成后将浏览器侧服务发现快照标记为 ready。
pub(crate) fn mark_webui_service_discovery_ready(state: &SharedServiceBridgeState) {
    mark_webui_service_discovery_status(state, WEBUI_SERVICE_DISCOVERY_STATUS_READY);
}

/// Marks the browser-facing service discovery snapshot as failed after bootstrap fails.
/// 在引导失败后将浏览器侧服务发现快照标记为 failed。
pub(crate) fn mark_webui_service_discovery_failed(state: &SharedServiceBridgeState) {
    mark_webui_service_discovery_status(state, WEBUI_SERVICE_DISCOVERY_STATUS_FAILED);
}

/// Removes the discovery record only while it still belongs to this Core instance.
/// 仅当发现记录仍属于当前 Core 实例时将其移除。
/// `state` carries the published instance identity; returns whether an owned file was removed.
/// `state` 携带已发布的实例身份；返回是否移除了当前实例的文件。
pub fn remove_owned_webui_service_discovery(
    state: &SharedServiceBridgeState,
) -> Result<bool, String> {
    let state_guard = state
        .lock()
        .map_err(|error| format!("failed to lock service discovery state: {error}"))?;
    let Some(discovery) = state_guard.webui_service_discovery.as_ref() else {
        return Ok(false);
    };
    let expected = discovery
        .lock()
        .map_err(|error| format!("failed to lock service discovery identity: {error}"))?
        .clone();
    let path = vulcan_service_api::webui_service_discovery_snapshot_path(
        dirs::home_dir(),
        std::env::temp_dir(),
    );
    remove_discovery_file_if_owned(&path, &expected)
}

/// Compares the durable discovery identity before deleting one exact file.
/// 删除精确文件前先比较持久发现记录的实例身份。
/// `path` is the authoritative discovery path and `expected` is this Core's live identity.
/// `path` 是权威发现路径，`expected` 是当前 Core 的实时身份。
/// Returns false when another owner replaced the file or no file remains.
/// 其他实例已替换文件或文件不存在时返回 false。
fn remove_discovery_file_if_owned(
    path: &std::path::Path,
    expected: &WebuiServiceDiscoverySnapshot,
) -> Result<bool, String> {
    let body = match std::fs::read_to_string(path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("failed to read service discovery: {error}")),
    };
    let published: WebuiServiceDiscoverySnapshot = serde_json::from_str(&body)
        .map_err(|error| format!("failed to parse service discovery: {error}"))?;
    // PID alone can be reused, and status changes during the lifecycle. Match the startup
    // generation and secret as well, without removing a newly published replacement.
    // PID 可能复用且状态会变化，因此还需核对启动代次与凭据，避免删除新发布的替代记录。
    if published.pid != expected.pid
        || published.started_at != expected.started_at
        || published.service_token != expected.service_token
    {
        return Ok(false);
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("failed to remove service discovery: {error}")),
    }
}

/// Resolves the WebUI static root used by the browser-facing service.
/// 解析面向浏览器服务使用的 WebUI 静态根目录。
///
/// # Parameters
/// 参数
/// `manifest_static_root` is the root declared by the validated runtime layout manifest, when the
/// owning service resolved one. The explicit environment override still wins over it.
/// `manifest_static_root` 是已校验运行时布局清单声明的根目录（当归属服务解析出时）。
/// 显式环境覆盖仍然优先于它。
pub(crate) fn webui_static_root(manifest_static_root: Option<PathBuf>) -> Option<PathBuf> {
    // Debug sources and installed release assets have explicitly different owners.
    // 开发源码和已安装的发布资源具有明确不同的归属路径。
    // The kernel crate lives at crates/vulcan-service-kernel, so the debug bundle is two
    // levels above the workspace apps directory; release assets ship beside the executable.
    // 内核 crate 位于 crates/vulcan-service-kernel，因此开发包相对工作区 apps 目录上溯两级；
    // 发布资源随可执行文件同目录分发。
    let packaged_root = manifest_static_root.or_else(|| {
        if cfg!(debug_assertions) {
            Some(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/vulcan-code-web/dist"))
        } else {
            std::env::current_exe()
                .ok()
                .and_then(|executable| executable.parent().map(|directory| directory.join("webui")))
        }
    });
    let override_path = env::var(WEBUI_STATIC_DIR_ENV).ok();
    webui_static_root_from_candidates(override_path.as_deref(), packaged_root)
}

/// Returns the current startup snapshot.
/// 返回当前启动快照。
pub fn service_startup_snapshot(
    state: SharedServiceBridgeState,
) -> Result<StartupSnapshot, String> {
    state
        .lock()
        .map(|guard| guard.startup.clone())
        .map_err(|error| format!("failed to lock service startup state: {error}"))
}

/// Returns the current workbench snapshot.
/// 返回当前工作台快照。
///
/// # Parameters
/// 参数
/// `state` owns the retained workbench and live question runtime projection.
/// `state` 持有保留工作台与实时问题运行时投影。
///
/// # Returns
/// 返回值
/// Returns the live workbench snapshot, or a service-state or question-projection error.
/// 返回实时工作台快照，失败时返回服务状态或问题投影错误。
pub fn service_workbench_snapshot(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    // guard keeps the retained workbench stable while live pending questions are attached.
    // guard 在附加实时待回答问题期间保持保留工作台稳定。
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service workbench state: {error}"))?;
    workbench_with_live_pending_question(&guard)
}

/// Builds one detailed service status snapshot from shared service state.
/// 从共享服务状态构建一个详细服务状态快照。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that owns startup progress, shutdown admission,
/// and activity counts.
/// `state` 是持有启动进度、关闭准入与活动计数的共享服务状态。
///
/// # Returns
/// 返回值
/// A status snapshot without configuration or credential content.
/// 返回不含配置或凭据内容的状态快照。
pub fn service_status_snapshot_from_state(
    state: &SharedServiceBridgeState,
) -> ServiceStatusSnapshot {
    // gauge_* snapshots never block HTTP dispatch on browser-driven I/O.
    // gauge_* 快照绝不在浏览器驱动的 I/O 上阻塞 HTTP 分发。
    let (
        lifecycle_fallback,
        revision,
        active_stream_tasks,
        vce_jobs,
        registry_draining,
        drain_active,
        last_bootstrap_failure,
        last_recovery_failure,
    ) = {
        // status_guard takes one short observation; counters below never block service I/O.
        // status_guard 只做一次短观察；以下计数绝不阻塞服务 I/O。
        let status_guard = state.lock();
        let failed_guard = status_guard.as_deref();
        let guard = match failed_guard {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.get_ref(),
        };
        (
            service_lifecycle_from_startup_phase(&guard.startup.phase),
            guard.workbench_generation,
            guard.active_stream_tasks.len() as u64,
            crate::vce::vce_retained_job_gauge(guard.vce_runtime.as_ref()),
            guard
                .session_runtime_registry
                .shutdown_admission_is_closed(),
            guard.service_lifecycle_state.shutdown_drain_active,
            guard.service_lifecycle_state.last_bootstrap_failure.clone(),
            guard.service_lifecycle_state.last_recovery_failure.clone(),
        )
    };
    let running_terminals = service_terminal_running_gauge();
    let active_work = ServiceActiveWorkSummary {
        active_stream_tasks,
        vce_jobs,
        running_terminals,
    };
    // lifecycle reports explicit shutdown and recovery states before bootstrap phases so
    // a draining service never advertises itself as ready for new writes or leases.
    // lifecycle 在引导阶段之前报告明确关闭与恢复状态，使正在排空的服务绝不会把自己
    // 宣传为可接受新写入或新租约。
    let (lifecycle, last_failure) = service_lifecycle_with_recovery(
        lifecycle_fallback,
        registry_draining,
        drain_active,
        last_bootstrap_failure,
        last_recovery_failure,
    );
    ServiceStatusSnapshot {
        lifecycle,
        pid: std::process::id(),
        started_at: service_generation_started_at().to_owned(),
        revision,
        last_failure,
        active_work,
    }
}

/// Reads the interface language the user saved, for native surfaces such as the tray.
/// 读取用户保存的界面语言，供托盘等原生界面使用。
///
/// # Parameters
/// 参数
/// `state` is the shared service state that mirrors the persisted `uiLocale`.
/// `state` 是镜像已持久化 `uiLocale` 的共享服务状态。
///
/// # Returns
/// 返回值
/// The saved language code, or `None` when the user never chose one.
/// 返回已保存的语言代码；用户从未选择时返回 `None`。
pub fn service_ui_locale_from_state(state: &SharedServiceBridgeState) -> Option<String> {
    let locale_guard = state.lock();
    let guard = match locale_guard.as_deref() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.get_ref(),
    };
    guard.startup.ui_locale.clone()
}

/// Resolves the protected lifecycle and retained failure from shutdown and recovery facts.
/// 从关闭与恢复事实解析受保护生命周期及保留失败。
fn service_lifecycle_with_recovery(
    lifecycle_fallback: ServiceLifecycle,
    registry_draining: bool,
    drain_active: bool,
    last_bootstrap_failure: Option<ServiceStatusFailure>,
    last_recovery_failure: Option<ServiceStatusFailure>,
) -> (ServiceLifecycle, Option<ServiceStatusFailure>) {
    // An admission registry retained in drain after failed recovery is a terminal service error,
    // not an indefinitely healthy-looking stopping phase.
    // 恢复失败后仍在排空的准入注册表属于服务终态故障，不能无限呈现为看似正常的停止阶段。
    if registry_draining && last_recovery_failure.is_some() {
        return (ServiceLifecycle::RecoveryFailed, last_recovery_failure);
    }
    if last_recovery_failure.is_some()
        && matches!(
            lifecycle_fallback,
            ServiceLifecycle::Failed | ServiceLifecycle::RecoveryFailed
        )
    {
        return (ServiceLifecycle::RecoveryFailed, last_recovery_failure);
    }
    if lifecycle_fallback == ServiceLifecycle::Failed {
        return (
            ServiceLifecycle::Failed,
            last_bootstrap_failure.or(last_recovery_failure),
        );
    }
    if drain_active || registry_draining {
        return (ServiceLifecycle::Stopping, None);
    }
    if last_recovery_failure.is_some() {
        return (ServiceLifecycle::Recovering, last_recovery_failure);
    }
    (lifecycle_fallback, last_bootstrap_failure)
}

/// Counts running project terminals without blocking HTTP dispatch on host I/O.
/// 在 HTTP 分发线程上统计运行中的项目终端，不阻塞宿主 I/O。
fn service_terminal_running_gauge() -> u64 {
    // Terminal gauge uses only non-blocking try_lock probes; a contended supervisor
    // reports zero for this observation rather than delaying the status endpoint.
    // 终端度量仅使用非阻塞 try_lock 探测；监控器竞争时本次观察报告零，
    // 而不延迟状态端点。
    crate::project_terminals::project_terminal_running_gauge()
}

/// Maps one bootstrap startup phase to the detailed service lifecycle.
/// 将一个引导启动阶段映射为详细服务生命周期。
fn service_lifecycle_from_startup_phase(phase: &str) -> ServiceLifecycle {
    match phase {
        "ready" => ServiceLifecycle::Ready,
        "failed" => ServiceLifecycle::Failed,
        _ => ServiceLifecycle::Starting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies GitHub HTTPS URLs are allowed for provenance links.
    /// 验证 GitHub HTTPS 链接允许作为来源链接打开。
    #[test]
    fn external_url_allowlist_accepts_github_https_urls() {
        assert!(external_url_is_allowed(
            "https://github.com/OpenVulcan/vulcan-code"
        ));
        assert!(external_url_is_allowed(
            "https://github.com/msitarzewski/agency-agents/blob/main/engineering/engineering-code-reviewer.md"
        ));
        assert!(external_url_is_allowed(
            "https://github.com/jnMetaCode/agency-agents-zh/blob/main/academic/academic-study-planner.md"
        ));
    }

    /// Verifies every HTTPS path on the exact Context7 domain is allowed.
    /// 验证精确 Context7 域名下的全部 HTTPS 路径均被允许。
    #[test]
    fn external_url_allowlist_accepts_context7_https_urls() {
        assert!(external_url_is_allowed("https://context7.com"));
        assert!(external_url_is_allowed("https://context7.com/dashboard"));
        assert!(external_url_is_allowed(
            "https://context7.com/docs/howto/api-keys?source=vulcan#setup"
        ));
    }

    /// Verifies disclosed provider referral URLs use the exact HTTPS domain allowlist.
    /// 验证已披露的供应商邀请链接使用精确 HTTPS 域名白名单。
    #[test]
    fn external_url_allowlist_accepts_provider_referral_https_urls() {
        for url in [
            "https://apikey.fan/register?aff=LY4DT3DCTM9N",
            "https://console.apito.ai/register?aff=8jvf",
            "https://s.qiniu.com/reuYBn",
            "https://api.fenno.ai/s/v59N",
            "https://console.code0.ai/register?aff=il6p",
        ] {
            assert!(external_url_is_allowed(url), "{url}");
        }
    }

    /// Verifies non-allowlisted or unsafe URLs remain blocked.
    /// 验证非白名单或不安全链接仍会被阻止。
    #[test]
    fn external_url_allowlist_rejects_unsafe_urls() {
        assert!(!external_url_is_allowed(
            "http://github.com/OpenVulcan/vulcan-code"
        ));
        assert!(!external_url_is_allowed(
            "https://example.com/msitarzewski/agency-agents"
        ));
        assert!(!external_url_is_allowed(
            "https://user:token@github.com/msitarzewski/agency-agents"
        ));
        assert!(!external_url_is_allowed("http://context7.com/dashboard"));
        assert!(!external_url_is_allowed("https://docs.context7.com"));
        assert!(!external_url_is_allowed(
            "https://user:token@context7.com/dashboard"
        ));
        assert!(!external_url_is_allowed("http://s.qiniu.com/reuYBn"));
        assert!(!external_url_is_allowed("https://evil.s.qiniu.com/reuYBn"));
        assert!(!external_url_is_allowed(
            "https://user:token@s.qiniu.com/reuYBn"
        ));
        assert!(!external_url_is_allowed("not a url"));
    }

    /// Verifies explicit confirmation may bypass only the domain allowlist, not URL safety rules.
    /// 验证明示确认只能绕过域名白名单，不能绕过 URL 安全规则。
    #[test]
    fn external_url_safety_remains_non_bypassable() {
        assert!(external_url_is_safe("https://example.com/path"));
        assert!(!external_url_is_safe("http://example.com/path"));
        assert!(!external_url_is_safe("https://user:token@example.com/path"));
        assert!(!external_url_is_safe("not a url"));
    }

    /// Verifies required RPC params report missing keys with the canonical message.
    /// 验证必填 RPC 参数会使用标准文案报告缺失键。
    #[test]
    fn rpc_required_param_reports_missing_key() {
        let error = rpc_required_param::<String>(&json!({}), "projectId")
            .expect_err("missing required RPC parameter should return a diagnostic");

        assert_eq!(error.code(), "invalid_params");
        assert_eq!(
            "missing service RPC parameter: projectId",
            error.to_string()
        );
    }

    /// Verifies optional RPC params treat missing and null values as absent.
    /// 验证可选 RPC 参数会把缺失值和 null 值视为不存在。
    #[test]
    fn rpc_optional_param_treats_missing_and_null_as_none() {
        assert_eq!(
            None,
            rpc_optional_param::<String>(&json!({}), "projectId")
                .expect("missing optional RPC parameter should parse as None")
        );
        assert_eq!(
            None,
            rpc_optional_param::<String>(&json!({ "projectId": null }), "projectId")
                .expect("null optional RPC parameter should parse as None")
        );
    }

    /// Verifies required and optional RPC params share the same invalid-value diagnostics.
    /// 验证必填和可选 RPC 参数共享同一套无效值诊断。
    #[test]
    fn rpc_params_share_invalid_value_diagnostics() {
        let required_error = rpc_required_param::<u64>(&json!({ "limit": "many" }), "limit")
            .expect_err("invalid required RPC parameter should return a diagnostic");
        let optional_error = rpc_optional_param::<u64>(&json!({ "limit": "many" }), "limit")
            .expect_err("invalid optional RPC parameter should return a diagnostic");

        assert_eq!(required_error.code(), "invalid_params");
        assert!(
            required_error
                .to_string()
                .starts_with("invalid service RPC parameter `limit`:")
        );
        assert_eq!(required_error, optional_error);
    }

    /// Verifies required and optional RPC params share the same non-object diagnostics.
    /// 验证必填和可选 RPC 参数共享同一套非对象诊断。
    #[test]
    fn rpc_params_share_non_object_diagnostics() {
        let required_error = rpc_required_param::<String>(&json!(true), "projectId")
            .expect_err("non-object required RPC params should return a diagnostic");
        let optional_error = rpc_optional_param::<String>(&json!(true), "projectId")
            .expect_err("non-object optional RPC params should return a diagnostic");

        assert_eq!(required_error.code(), "invalid_params");
        assert_eq!(
            "service RPC params must be a JSON object",
            required_error.to_string()
        );
        assert_eq!(required_error, optional_error);
    }

    /// Distinguishes business diagnostics from serialization failures without text matching.
    /// 区分业务诊断与序列化失败，不做文本匹配。
    #[test]
    fn rpc_result_preserves_service_and_serialization_categories() {
        let service =
            rpc_json_result::<Value>(Err("invalid params, unknown method".into())).unwrap_err();
        assert_eq!(service.code(), "service_rpc_error");
        // Array keys cannot be represented as JSON object keys.
        // 数组键无法表示为 JSON 对象键。
        let serialization = rpc_json_result(Ok(BTreeMap::from([(vec![1_u8], 1_u8)]))).unwrap_err();
        assert_eq!(serialization.code(), "response_serialization_failed");
    }

    /// Verifies discovery status replacement preserves stable service identity fields.
    /// 验证发现状态替换会保留稳定服务身份字段。
    #[test]
    fn discovery_status_replacement_preserves_service_identity() {
        let snapshot = webui_service_starting_discovery_snapshot(48123, 42, "1000", "token-a");

        let replaced = webui_service_discovery_snapshot_with_replaced_status(
            &snapshot,
            WEBUI_SERVICE_DISCOVERY_STATUS_FAILED,
        );

        assert_eq!(replaced.status, WEBUI_SERVICE_DISCOVERY_STATUS_FAILED);
        assert_eq!(replaced.base_url, snapshot.base_url);
        assert_eq!(replaced.host, snapshot.host);
        assert_eq!(replaced.port, snapshot.port);
        assert_eq!(replaced.pid, snapshot.pid);
        assert_eq!(replaced.started_at, snapshot.started_at);
        assert_eq!(replaced.service_token, snapshot.service_token);
    }

    /// Verifies shutdown removes only the discovery identity owned by this instance.
    /// 验证关闭时只移除属于当前实例身份的发现记录。
    #[test]
    fn discovery_cleanup_preserves_replacement_instance() {
        let directory = tempfile::tempdir().expect("temporary discovery directory");
        let path = directory.path().join("webui-service.json");
        let expected = webui_service_starting_discovery_snapshot(48123, 42, "1000", "token-a");
        let replacement = webui_service_starting_discovery_snapshot(48124, 42, "2000", "token-b");
        std::fs::write(
            &path,
            serde_json::to_vec(&replacement).expect("replacement JSON"),
        )
        .expect("publish replacement");
        assert!(!remove_discovery_file_if_owned(&path, &expected).expect("compare identity"));
        assert!(path.exists(), "replacement identity must remain published");
        std::fs::write(&path, serde_json::to_vec(&expected).expect("owned JSON"))
            .expect("publish owned identity");
        assert!(remove_discovery_file_if_owned(&path, &expected).expect("remove owned identity"));
        assert!(
            !path.exists(),
            "owned discovery must be removed on shutdown"
        );
        assert!(!remove_discovery_file_if_owned(&path, &expected).expect("repeat cleanup"));
    }

    /// Verifies browser downloads preserve sanitized UTF-8 names without creating a header injection.
    /// 验证浏览器下载保留清理后的 UTF-8 文件名，且不会形成响应头注入。
    #[test]
    fn browser_download_content_disposition_preserves_utf8_name() {
        assert_eq!(
            browser_download_content_disposition("中文文件.txt"),
            "attachment; filename*=UTF-8''%E4%B8%AD%E6%96%87%E6%96%87%E4%BB%B6.txt"
        );
        assert_eq!(
            browser_download_content_disposition("line\nname.txt"),
            "attachment; filename*=UTF-8''line%0Aname.txt"
        );
    }
}
