// Locating the running core service.
// 定位正在运行的核心服务。
//
// The service publishes `<root>/code/webui-service.json` (`WebuiServiceDiscoverySnapshot`) where
// `<root>` is `~/.vulcan` for release builds and `~/.vulcan-debug` for debug builds. The file holds
// the service token, so it is only ever parsed here: the token never reaches logs, the UI or a
// `Debug` print.
// 服务发布 `<root>/code/webui-service.json`，其中 `<root>` 在发布构建为 `~/.vulcan`、调试构建为
// `~/.vulcan-debug`。该文件含服务令牌，因此只在此处解析：令牌绝不进入日志、界面或 `Debug` 输出。

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// User root directory names, release first.
/// 用户根目录名称，发布版在前。
const ROOT_NAMES: [&str; 2] = [".vulcan", ".vulcan-debug"];
/// Discovery file below a root.
/// 根目录下的发现文件。
const DISCOVERY_FILE: [&str; 2] = ["code", "webui-service.json"];

/// A reachable service: its base URL and the token every request carries.
/// 一个可访问的服务：基础 URL 与每个请求携带的令牌。
#[derive(Clone)]
pub struct Endpoint {
    /// Loopback service origin parsed from the discovery record.
    /// 从发现记录解析的回环服务来源。
    pub base_url: String,
    /// Transport-only authentication token, excluded from all diagnostics.
    /// 仅用于传输的认证令牌，从全部诊断输出中排除。
    token: String,
}

impl Endpoint {
    /// Create an explicit synthetic viewer endpoint without discovery; returns transport-only credentials.
    /// 创建显式合成查看器端点，不执行发现；返回仅用于传输的凭据。
    #[cfg(test)]
    pub(super) fn viewer_fixture(address: std::net::SocketAddr) -> Self {
        Self {
            base_url: format!("http://{address}"),
            token: "synthetic-fixture".into(),
        }
    }
    /// Creates a loopback endpoint for synthetic transport tests without reading user discovery.
    /// 创建合成传输测试使用的回环端点，不读取用户发现文件。
    #[cfg(test)]
    /// Returns `Self`.
    /// 返回 `Self`。
    pub(super) fn test_endpoint(address: std::net::SocketAddr) -> Self {
        Self {
            base_url: format!("http://{address}"),
            token: "synthetic-test-token".into(),
        }
    }

    /// Borrows the transport-only authentication token; callers must never display or log it.
    /// 借用仅用于传输的认证令牌；调用方绝不可显示或记录该值。
    /// Returns `&str`.
    /// 返回 `&str`。
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The `/events` WebSocket URL, token included; never display it. `client_id` is the UUID of
    /// this TUI process: the service routes client-owned events by it and releases client state
    /// when its last socket closes.
    /// `/events` WebSocket 地址，含令牌；切勿显示。`client_id` 是本 TUI 进程的 UUID：服务据此投递
    /// 客户端专属事件，并在其最后一个套接字关闭时释放客户端状态。
    /// Returns `String`.
    /// 返回 `String`。
    pub fn events_url(&self, client_id: &str) -> String {
        // Authenticated event transport URL.
        // 仅用于认证传输的事件地址。
        let ws = self
            .base_url
            .strip_prefix("http://")
            .map(|rest| format!("ws://{rest}"))
            .unwrap_or_else(|| self.base_url.clone());
        format!(
            "{}/events?token={}&clientId={client_id}",
            ws.trim_end_matches('/'),
            self.token
        )
    }
}

impl fmt::Debug for Endpoint {
    /// Writes the type display or diagnostic representation into the provided formatter.
    /// 向传入格式化器写入类型显示或诊断表示。
    /// `f` is the destination formatter.
    /// `f` 为目标格式化器。
    /// Returns `fmt::Result`.
    /// 返回 `fmt::Result`。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// Why no service could be used; worded for the user, never containing the token.
/// 无法使用服务的原因；面向用户的文字，绝不包含令牌。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryError {
    /// No discovery file in any root.
    /// 所有根目录中都没有发现文件。
    NotFound(Vec<PathBuf>),
    /// Files exist but none reports a ready service.
    /// 文件存在，但没有一个报告服务就绪。
    NotReady,
}

impl fmt::Display for DiscoveryError {
    /// Writes the type display or diagnostic representation into the provided formatter.
    /// 向传入格式化器写入类型显示或诊断表示。
    /// `f` is the destination formatter.
    /// `f` 为目标格式化器。
    /// Returns `fmt::Result`.
    /// 返回 `fmt::Result`。
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscoveryError::NotFound(paths) => {
                // List.
                // 当前列表。
                let list: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
                write!(f, "no Vulcan Code service found ({})", list.join(", "))
            }
            DiscoveryError::NotReady => write!(f, "the Vulcan Code service is not ready"),
        }
    }
}

/// Roots searched for a discovery file: the explicit one, else `~/.vulcan` then `~/.vulcan-debug`.
/// 搜索发现文件的根目录：显式指定者，否则依次为 `~/.vulcan` 与 `~/.vulcan-debug`。
/// Returns `Vec<PathBuf>`.
/// 返回 `Vec<PathBuf>`。
pub fn roots(explicit: Option<&Path>) -> Vec<PathBuf> {
    if let Some(root) = explicit {
        return vec![root.to_path_buf()];
    }
    // Home.
    // 所选用户主目录。
    let home = std::env::home_dir().unwrap_or_else(std::env::temp_dir);
    ROOT_NAMES.iter().map(|name| home.join(name)).collect()
}

/// Reads every root's discovery file and returns the ready endpoints, most recently started first.
/// 读取每个根目录的发现文件，返回就绪的端点，最近启动的在前。
pub fn discover(explicit: Option<&Path>) -> Result<Vec<Endpoint>, DiscoveryError> {
    // Roots.
    // 服务发现或文件系统根集合。
    let roots = roots(explicit);
    // Found.
    // 是否找到有效结果。
    let mut found = false;
    // Ready.
    // 预检就绪目标集合。
    let mut ready: Vec<(u64, Endpoint)> = Vec::new();
    for root in &roots {
        // Exact path.
        // 准确路径。
        let path = DISCOVERY_FILE
            .iter()
            .fold(root.clone(), |p, part| p.join(part));
        // Text.
        // 当前正文。
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        found = true;
        if let Some(entry) = parse(&text) {
            ready.push(entry);
        }
    }
    if ready.is_empty() {
        return Err(if found {
            DiscoveryError::NotReady
        } else {
            DiscoveryError::NotFound(roots)
        });
    }
    ready.sort_by_key(|(started, _)| std::cmp::Reverse(*started));
    Ok(ready.into_iter().map(|(_, endpoint)| endpoint).collect())
}

/// Parses one discovery snapshot; `None` unless it is ready and complete.
/// 解析一份发现快照；仅在就绪且完整时返回。
fn parse(text: &str) -> Option<(u64, Endpoint)> {
    // Value.
    // 当前协议值。
    let value: Value = serde_json::from_str(text).ok()?;
    if value.get("status")?.as_str()? != "ready" {
        return None;
    }
    // Base url.
    // 公开服务基础地址。
    let base_url = value
        .get("baseUrl")?
        .as_str()?
        .trim_end_matches('/')
        .to_string();
    // Transport-only service token.
    // 仅用于认证传输的服务令牌。
    let token = value.get("serviceToken")?.as_str()?.to_string();
    if base_url.is_empty() || token.is_empty() {
        return None;
    }
    // Started.
    // 服务启动时间。
    let started = value
        .get("startedAt")
        .and_then(|v| v.as_str().and_then(|s| s.parse().ok()).or(v.as_u64()))
        .unwrap_or(0);
    Some((started, Endpoint { base_url, token }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Verifies parses ready snapshot and hides token with synthetic data and assertions.
    /// 使用合成数据及断言验证 parses_ready_snapshot_and_hides_token。
    fn parses_ready_snapshot_and_hides_token() {
        // Text.
        // 当前正文。
        let text = r#"{"status":"ready","baseUrl":"http://127.0.0.1:1/","host":"127.0.0.1",
            "port":1,"pid":2,"startedAt":"5","serviceToken":"secret-token"}"#;
        // Started; Endpoint.
        // 服务启动时间；已认证传输端点。
        let (started, endpoint) = parse(text).expect("ready snapshot");
        assert_eq!(started, 5);
        assert_eq!(endpoint.base_url, "http://127.0.0.1:1");
        assert_eq!(
            endpoint.events_url("c1"),
            "ws://127.0.0.1:1/events?token=secret-token&clientId=c1"
        );
        assert!(!format!("{endpoint:?}").contains("secret-token"));
    }

    #[test]
    /// Verifies rejects failed snapshot with synthetic data and assertions.
    /// 使用合成数据及断言验证 rejects_failed_snapshot。
    fn rejects_failed_snapshot() {
        // Text.
        // 当前正文。
        let text = r#"{"status":"failed","baseUrl":"http://127.0.0.1:1","serviceToken":"t"}"#;
        assert!(parse(text).is_none());
    }
}
