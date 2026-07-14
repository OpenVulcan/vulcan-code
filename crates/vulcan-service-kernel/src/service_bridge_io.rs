//! Browser service bridge and RPC adapter helpers.
//! 浏览器服务桥接与 RPC 适配辅助函数。

use super::*;
use reqwest::Url;
use vulcan_service_api::WebuiServiceDiscoverySnapshot;

/// Stable service result returned after an external URL is opened.
/// 外部链接成功打开后返回的稳定服务结果。
const EXTERNAL_URL_OPENED: &str = "opened";

/// Stable service result requesting an explicit trusted-UI user confirmation.
/// 请求可信界面进行明确用户确认时返回的稳定服务结果。
const EXTERNAL_URL_CONFIRMATION_REQUIRED: &str = "confirmation_required";

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
fn external_url_is_safe(url: &str) -> bool {
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
    let allowed_host = matches!(parsed.host_str(), Some("github.com" | "context7.com"));
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
    let object = rpc_params_object(params)?;
    let value = object
        .get(key)
        .ok_or_else(|| format!("missing service RPC parameter: {key}"))?;
    deserialize_rpc_param(key, value)
}

/// Reads an optional typed value from one RPC parameter object.
/// 从单个 RPC 参数对象读取可选类型值。
pub(crate) fn rpc_optional_param<T>(params: &Value, key: &str) -> Result<Option<T>, String>
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
) -> Result<&serde_json::Map<String, serde_json::Value>, String> {
    params
        .as_object()
        .ok_or_else(|| String::from("service RPC params must be a JSON object"))
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
fn deserialize_rpc_param<T>(key: &str, value: &Value) -> Result<T, String>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value.clone())
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
        eprintln!("[webui-service] browser service discovery {status} write failed: {error}");
    }
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

        assert_eq!("missing service RPC parameter: projectId", error);
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

        assert!(required_error.starts_with("invalid service RPC parameter `limit`:"));
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

        assert_eq!("service RPC params must be a JSON object", required_error);
        assert_eq!(required_error, optional_error);
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
}
