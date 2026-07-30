use super::*;

/// Browser-facing service discovery snapshot shared by health checks and clients.
/// 健康检查与客户端共享的浏览器侧服务发现快照。
#[derive(Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebuiServiceDiscoverySnapshot {
    /// Human-readable service status.
    /// 人类可读的服务状态。
    pub status: String,
    /// Browser-facing base URL.
    /// 面向浏览器的基础 URL。
    pub base_url: String,
    /// Loopback host used by the service bridge.
    /// 服务桥接层使用的回环主机。
    pub host: String,
    /// TCP port used by the service bridge.
    /// 服务桥接层使用的 TCP 端口。
    pub port: u16,
    /// Owning process id.
    /// 拥有该服务的进程标识。
    pub pid: u32,
    /// UTC startup timestamp in Unix milliseconds.
    /// UTC 启动时间戳，单位为 Unix 毫秒。
    pub started_at: String,
    /// Instance access token for loopback browser clients.
    /// 提供给回环浏览器客户端的实例访问令牌。
    pub service_token: String,
}

impl std::fmt::Debug for WebuiServiceDiscoverySnapshot {
    /// Formats browser discovery metadata while redacting the service token.
    /// 格式化浏览器发现元数据，同时脱敏服务令牌。
    ///
    /// # Parameters
    /// 参数
    /// `formatter` receives only non-secret discovery fields and one redaction marker.
    /// `formatter` 仅接收非秘密发现字段与一个脱敏标记。
    ///
    /// # Returns
    /// 返回值
    /// Standard debug formatting completion.
    /// 标准调试格式化完成结果。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebuiServiceDiscoverySnapshot")
            .field("status", &self.status)
            .field("base_url", &self.base_url)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("pid", &self.pid)
            .field("started_at", &self.started_at)
            .field("service_token", &"<redacted>")
            .finish()
    }
}

/// Service discovery status used while the HTTP bridge is bound but startup is still running.
/// HTTP 桥接已绑定但启动流程仍在运行时使用的服务发现状态。
pub const WEBUI_SERVICE_DISCOVERY_STATUS_STARTING: &str = "starting";

/// Service discovery status used after the full service bootstrap has completed.
/// 完整服务引导完成后使用的服务发现状态。
pub const WEBUI_SERVICE_DISCOVERY_STATUS_READY: &str = "ready";

/// Service discovery status used after service bootstrap fails.
/// 服务引导失败后使用的服务发现状态。
pub const WEBUI_SERVICE_DISCOVERY_STATUS_FAILED: &str = "failed";

/// Builds one browser-facing WebUI service discovery snapshot with an explicit status.
/// 使用显式状态构建一个浏览器侧 WebUI 服务发现快照。
///
/// # Parameters
/// 参数
/// `status` is the service readiness marker, `port` is the bound loopback service port, `pid` is the owner process id, `started_at` is the startup timestamp, and `service_token` is the instance access token.
/// `status` 是服务就绪标记，`port` 是已绑定的回环服务端口，`pid` 是拥有进程标识，`started_at` 是启动时间戳，`service_token` 是实例访问令牌。
///
/// # Returns
/// 返回值
/// A discovery snapshot suitable for health responses and discovery files.
/// 返回适用于健康响应和发现文件的发现快照。
pub fn webui_service_discovery_snapshot_with_status(
    status: impl Into<String>,
    port: u16,
    pid: u32,
    started_at: impl Into<String>,
    service_token: impl Into<String>,
) -> WebuiServiceDiscoverySnapshot {
    let base_url = format!("http://{WEBUI_SERVICE_LOOPBACK_HOST}:{port}");
    WebuiServiceDiscoverySnapshot {
        status: status.into(),
        base_url,
        host: String::from(WEBUI_SERVICE_LOOPBACK_HOST),
        port,
        pid,
        started_at: started_at.into(),
        service_token: service_token.into(),
    }
}

/// Builds one starting browser-facing WebUI service discovery snapshot.
/// 构建一个 starting 状态的浏览器侧 WebUI 服务发现快照。
///
/// # Parameters
/// 参数
/// `port` is the bound loopback service port, `pid` is the owner process id, `started_at` is the startup timestamp, and `service_token` is the instance access token.
/// `port` 是已绑定的回环服务端口，`pid` 是拥有进程标识，`started_at` 是启动时间戳，`service_token` 是实例访问令牌。
///
/// # Returns
/// 返回值
/// A discovery snapshot that advertises a reachable but not-yet-ready service.
/// 返回表示服务可访问但尚未就绪的发现快照。
pub fn webui_service_starting_discovery_snapshot(
    port: u16,
    pid: u32,
    started_at: impl Into<String>,
    service_token: impl Into<String>,
) -> WebuiServiceDiscoverySnapshot {
    webui_service_discovery_snapshot_with_status(
        WEBUI_SERVICE_DISCOVERY_STATUS_STARTING,
        port,
        pid,
        started_at,
        service_token,
    )
}

/// Builds one ready browser-facing WebUI service discovery snapshot.
/// 构建一个 ready 状态的浏览器侧 WebUI 服务发现快照。
///
/// # Parameters
/// 参数
/// `port` is the bound loopback service port, `pid` is the owner process id, `started_at` is the startup timestamp, and `service_token` is the instance access token.
/// `port` 是已绑定的回环服务端口，`pid` 是拥有进程标识，`started_at` 是启动时间戳，`service_token` 是实例访问令牌。
///
/// # Returns
/// 返回值
/// A discovery snapshot suitable for health responses and discovery files.
/// 返回适用于健康响应和发现文件的发现快照。
pub fn webui_service_ready_discovery_snapshot(
    port: u16,
    pid: u32,
    started_at: impl Into<String>,
    service_token: impl Into<String>,
) -> WebuiServiceDiscoverySnapshot {
    webui_service_discovery_snapshot_with_status(
        WEBUI_SERVICE_DISCOVERY_STATUS_READY,
        port,
        pid,
        started_at,
        service_token,
    )
}

/// Browser-facing RPC request body shared by HTTP transports.
/// HTTP 传输共享的浏览器侧 RPC 请求体。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebuiServiceRpcHttpRequest {
    /// Logical service method name.
    /// 逻辑服务方法名。
    pub method: String,
    /// Optional JSON parameter object.
    /// 可选 JSON 参数对象。
    pub params: Option<serde_json::Value>,
}

/// Browser-facing RPC response body shared by HTTP transports.
/// HTTP 传输共享的浏览器侧 RPC 响应体。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebuiServiceRpcHttpResponse {
    /// Whether the RPC call succeeded.
    /// RPC 调用是否成功。
    pub ok: bool,
    /// Successful response payload.
    /// 成功响应载荷。
    pub result: Option<serde_json::Value>,
    /// Normalized error payload.
    /// 标准化错误载荷。
    pub error: Option<WebuiServiceRpcHttpError>,
}

/// Browser-facing RPC error payload shared by HTTP transports.
/// HTTP 传输共享的浏览器侧 RPC 错误载荷。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WebuiServiceRpcHttpError {
    /// Stable machine-readable error code.
    /// 稳定的机器可读错误码。
    pub code: String,
    /// Human-readable error message.
    /// 人类可读错误消息。
    pub message: String,
}

/// Builds one successful browser-facing RPC envelope and redacts secrets in place.
/// 构建单个成功的浏览器侧 RPC 信封，并原地脱敏敏感信息。
///
/// # Parameters
/// 参数
/// `result` is the JSON payload produced by the service method.
/// `result` 是服务方法生成的 JSON 载荷。
///
/// # Returns
/// 返回值
/// A normalized successful RPC envelope safe to send to browser clients.
/// 返回可安全发送给浏览器客户端的标准化成功 RPC 信封。
pub fn webui_service_rpc_success_envelope(
    mut result: serde_json::Value,
) -> WebuiServiceRpcHttpResponse {
    redact_webui_service_rpc_secrets(&mut result);
    WebuiServiceRpcHttpResponse {
        ok: true,
        result: Some(result),
        error: None,
    }
}

/// Builds one failed browser-facing RPC envelope.
/// 构建单个失败的浏览器侧 RPC 信封。
///
/// # Parameters
/// 参数
/// `code` is the stable machine-readable error code, and `message` is the human-readable error.
/// `code` 是稳定的机器可读错误码，`message` 是人类可读错误消息。
///
/// # Returns
/// 返回值
/// A normalized failed RPC envelope.
/// 返回标准化失败 RPC 信封。
pub fn webui_service_rpc_error_envelope(
    code: impl Into<String>,
    message: impl Into<String>,
) -> WebuiServiceRpcHttpResponse {
    WebuiServiceRpcHttpResponse {
        ok: false,
        result: None,
        error: Some(WebuiServiceRpcHttpError {
            code: code.into(),
            message: message.into(),
        }),
    }
}

/// Redacts sensitive fields before a value crosses the browser-facing HTTP boundary.
/// 在值跨越浏览器侧 HTTP 边界前脱敏敏感字段。
///
/// # Parameters
/// 参数
/// `value` is the JSON RPC result payload to sanitize in place.
/// `value` 是需要原地脱敏的 JSON RPC 结果载荷。
pub fn redact_webui_service_rpc_secrets(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, nested) in map.iter_mut() {
                if webui_service_rpc_secret_key_should_be_redacted(key) {
                    if nested
                        .as_str()
                        .is_some_and(|secret| !secret.trim().is_empty())
                    {
                        *nested = serde_json::Value::String(String::from(
                            WEBUI_REDACTED_SECRET_PLACEHOLDER,
                        ));
                    }
                    continue;
                }
                redact_webui_service_rpc_secrets(nested);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_webui_service_rpc_secrets(item);
            }
        }
        _ => {}
    }
}

/// Returns whether one JSON key carries a browser-visible secret.
/// 返回一个 JSON 字段名是否携带浏览器可见敏感信息。
///
/// # Parameters
/// 参数
/// `key` is the JSON object key being inspected.
/// `key` 是正在检查的 JSON 对象字段名。
///
/// # Returns
/// 返回值
/// `true` when the key should be redacted for HTTP clients.
/// 当该字段应对 HTTP 客户端脱敏时返回 `true`。
pub fn webui_service_rpc_secret_key_should_be_redacted(key: &str) -> bool {
    matches!(
        key,
        "apiKey"
            | "api_key"
            | "grpcApiKey"
            | "grpc_api_key"
            | "accessToken"
            | "access_token"
            | "refreshToken"
            | "refresh_token"
            | "clientSecret"
            | "client_secret"
            | "secret"
    )
}
