//! Browser service bridge and RPC adapter helpers.
//! 浏览器服务桥接与 RPC 适配辅助函数。

use super::*;
use reqwest::Url;

/// Opens a trusted external URL with the system default browser.
/// 使用系统默认浏览器打开可信外部链接。
pub(crate) fn service_open_external_url(
    state: SharedServiceBridgeState,
    url: String,
) -> Result<String, String> {
    if !external_url_is_allowed(&url) {
        return Err(String::from("不允许打开未授权的外部链接。"));
    }
    shell_host_from_state(&state)?.open_url(&url)?;
    Ok(String::from("已请求使用系统默认浏览器打开外部链接。"))
}

/// Returns whether one external URL may be opened by the host shell.
/// 返回一个外部链接是否允许由宿主 shell 打开。
fn external_url_is_allowed(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed.host_str() == Some("github.com")
        && parsed.username().is_empty()
        && parsed.password().is_none()
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
pub(crate) fn rpc_json_result<T>(result: Result<T, String>) -> Result<Value, String>
where
    T: Serialize,
{
    result.and_then(|value| {
        serde_json::to_value(value)
            .map_err(|error| format!("failed to serialize service RPC response: {error}"))
    })
}

/// Reads a required typed value from one RPC parameter object.
/// 从单个 RPC 参数对象读取必填类型值。
pub(crate) fn rpc_required_param<T>(params: &Value, key: &str) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let object = params
        .as_object()
        .ok_or_else(|| String::from("service RPC params must be a JSON object"))?;
    let value = object
        .get(key)
        .ok_or_else(|| format!("missing service RPC parameter: {key}"))?;
    serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid service RPC parameter `{key}`: {error}"))
}

/// Reads an optional typed value from one RPC parameter object.
/// 从单个 RPC 参数对象读取可选类型值。
pub(crate) fn rpc_optional_param<T>(params: &Value, key: &str) -> Result<Option<T>, String>
where
    T: DeserializeOwned,
{
    let Some(object) = params.as_object() else {
        return Err(String::from("service RPC params must be a JSON object"));
    };
    let Some(value) = object.get(key) else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|error| format!("invalid service RPC parameter `{key}`: {error}"))
}

/// Spawns the browser-facing local service bridge on the first available loopback port.
/// 在首个可用回环端口启动面向浏览器的本地服务桥接层。
pub fn spawn_webui_service_http_bridge(
    event_emitter: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    event_bus: broadcast::Sender<ServiceEvent>,
) {
    std::thread::Builder::new()
        .name(String::from("vulcan-webui-http-bridge"))
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!(
                        "[webui-service] browser service bridge runtime creation failed: {error}"
                    );
                    return;
                }
            };
            runtime.block_on(run_webui_service_http_bridge(
                event_emitter,
                state,
                event_bus,
            ));
        })
        .expect("failed to spawn Vulcan WebUI HTTP bridge thread");
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
) {
    let port_override = env::var(WEBUI_SERVICE_PORT_ENV).ok();
    let (listener, address) = match bind_webui_service_http_listener(port_override.as_deref()).await
    {
        Ok(bound) => bound,
        Err(error) => {
            eprintln!("[webui-service] browser service bridge failed to bind: {error}");
            return;
        }
    };
    let port = address.port();
    let service_token = SessionId::new_v4().to_string();
    let discovery = webui_service_starting_discovery_snapshot(
        port,
        std::process::id(),
        current_unix_millis_text(),
        service_token.clone(),
    );
    let shared_discovery = Arc::new(Mutex::new(discovery.clone()));
    if let Ok(mut guard) = state.lock() {
        guard.webui_service_discovery = Some(Arc::clone(&shared_discovery));
    }
    if let Err(error) =
        write_webui_service_discovery_snapshot(&discovery, dirs::home_dir(), std::env::temp_dir())
    {
        eprintln!("[webui-service] browser service discovery write failed: {error}");
    }
    let rpc_event_emitter = event_emitter.clone();
    let rpc_state = state.clone();
    let rpc_dispatcher: WebuiServiceRpcDispatcher = Arc::new(move |method, params| {
        let event_emitter = rpc_event_emitter.clone();
        let state = rpc_state.clone();
        Box::pin(async move { dispatch_service_rpc(event_emitter, state, method, params).await })
    });
    let service_config = WebuiServiceHttpConfig {
        discovery: shared_discovery,
        service_token,
        event_bus,
        static_root: webui_static_root(),
        rpc_dispatcher,
    };
    eprintln!("[webui-service] browser service bridge listening on http://{address}");
    if let Err(error) = serve_webui_service_http_listener(listener, service_config).await {
        eprintln!("[webui-service] browser service bridge stopped: {error}");
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
    let shared_discovery = match state.lock() {
        Ok(guard) => guard.webui_service_discovery.clone(),
        Err(error) => error.into_inner().webui_service_discovery.clone(),
    };
    let Some(shared_discovery) = shared_discovery else {
        return;
    };
    let ready_snapshot = {
        let mut snapshot = match shared_discovery.lock() {
            Ok(snapshot) => snapshot,
            Err(error) => error.into_inner(),
        };
        let port = snapshot.port;
        let pid = snapshot.pid;
        let started_at = snapshot.started_at.clone();
        let service_token = snapshot.service_token.clone();
        *snapshot = webui_service_discovery_snapshot_with_status(
            status,
            port,
            pid,
            started_at,
            service_token,
        );
        snapshot.clone()
    };
    if let Err(error) = write_webui_service_discovery_snapshot(
        &ready_snapshot,
        dirs::home_dir(),
        std::env::temp_dir(),
    ) {
        eprintln!("[webui-service] browser service discovery ready write failed: {error}");
    }
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

/// Resolves the WebUI static root used by the browser-facing service.
/// 解析面向浏览器服务使用的 WebUI 静态根目录。
pub(crate) fn webui_static_root() -> Option<PathBuf> {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../vulcan-code-web/dist")
        .components()
        .collect::<PathBuf>();
    let override_path = env::var(WEBUI_STATIC_DIR_ENV).ok();
    webui_static_root_from_candidates(override_path.as_deref(), Some(manifest_path))
}

/// Returns the current startup snapshot.
/// 返回当前启动快照。
pub(crate) fn service_startup_snapshot(
    state: SharedServiceBridgeState,
) -> Result<StartupSnapshot, String> {
    state
        .lock()
        .map(|guard| guard.startup.clone())
        .map_err(|error| format!("failed to lock service startup state: {error}"))
}

/// Returns the current workbench snapshot.
/// 返回当前工作台快照。
pub(crate) fn service_workbench_snapshot(
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    state
        .lock()
        .map(|guard| workbench_with_live_pending_question(&guard))
        .map_err(|error| format!("failed to lock service workbench state: {error}"))
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

    /// Verifies non-GitHub or unsafe URLs remain blocked.
    /// 验证非 GitHub 或不安全链接仍会被阻止。
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
        assert!(!external_url_is_allowed("not a url"));
    }
}
