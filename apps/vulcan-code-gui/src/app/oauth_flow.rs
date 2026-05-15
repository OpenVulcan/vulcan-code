//! Shared host-owned OAuth browser-launch and callback-capture helpers reused by GUI authorization flows.
//! 供 GUI 授权流程复用的宿主自有 OAuth 浏览器拉起与回调捕获辅助模块。

use std::collections::BTreeMap;
use std::time::Duration;

/// Parsed OAuth callback payload extracted from one redirect URL, callback path, or raw query string.
/// 从跳转 URL、回调路径或原始查询字符串中提取出的 OAuth 回调载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct HostedOAuthCallbackPayload {
    /// Authorization code returned by the OAuth provider.
    /// OAuth 提供方返回的授权码。
    pub authorization_code: String,
    /// OAuth state echoed back by the authorization server.
    /// 授权服务器回传的 OAuth state。
    pub oauth_state: String,
}

/// Parses one browser callback payload or raw query string into a structured OAuth callback payload.
/// 将单份浏览器回调载荷或原始查询字符串解析为结构化 OAuth 回调载荷。
pub(super) fn parse_hosted_oauth_callback_input(
    value: &str,
) -> Result<HostedOAuthCallbackPayload, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(String::from("请先填写浏览器回调 URL、路径或查询字符串。"));
    }

    let parsed_url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        reqwest::Url::parse(trimmed).map_err(|error| format!("浏览器回调 URL 解析失败：{error}"))?
    } else if trimmed.starts_with('/') {
        reqwest::Url::parse(&format!("http://127.0.0.1{trimmed}"))
            .map_err(|error| format!("浏览器回调路径解析失败：{error}"))?
    } else {
        reqwest::Url::parse(&format!(
            "http://127.0.0.1/callback?{}",
            trimmed.trim_start_matches('?')
        ))
        .map_err(|error| format!("浏览器回调查询字符串解析失败：{error}"))?
    };
    let query = parsed_url.query_pairs().collect::<BTreeMap<_, _>>();

    if let Some(error) = query.get("error") {
        let error_description = query
            .get("error_description")
            .map(|value| format!("：{value}"))
            .unwrap_or_default();
        return Err(format!("OAuth 授权失败：{error}{error_description}"));
    }

    let authorization_code = query
        .get("code")
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| String::from("浏览器回调中缺少 `code` 参数。"))?;
    let oauth_state = query
        .get("state")
        .map(|value| value.to_string())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| String::from("浏览器回调中缺少 `state` 参数。"))?;

    Ok(HostedOAuthCallbackPayload {
        authorization_code,
        oauth_state,
    })
}

/// Extracts the loopback callback port from one configured redirect URI when the host can safely spin up a local listener.
/// 当宿主可以安全启动本地监听器时，从已配置回调 URI 中提取环回端口。
pub(super) fn resolve_loopback_oauth_redirect_port(redirect_uri: &str) -> Option<u16> {
    let parsed = reqwest::Url::parse(redirect_uri).ok()?;
    let host = parsed.host_str()?;
    let is_loopback = matches!(host, "127.0.0.1" | "localhost");
    if !is_loopback {
        return None;
    }
    parsed.port_or_known_default()
}

/// Opens one external URL with the system default browser on the current operating system.
/// 使用当前操作系统的默认浏览器打开单个外部 URL。
pub(super) async fn open_external_url_in_browser(url: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", &url])
                .spawn()
                .map_err(|error| format!("打开默认浏览器失败：{error}"))?;
            return Ok(());
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(&url)
                .spawn()
                .map_err(|error| format!("打开默认浏览器失败：{error}"))?;
            return Ok(());
        }

        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            std::process::Command::new("xdg-open")
                .arg(&url)
                .spawn()
                .map_err(|error| format!("打开默认浏览器失败：{error}"))?;
            Ok(())
        }
    })
    .await
    .map_err(|error| format!("浏览器启动任务加入失败：{error}"))?
}

/// Waits for one localhost OAuth callback on the specified port and returns the captured callback URL for later parsing.
/// 在指定端口等待一次 localhost OAuth 回调，并返回后续可继续解析的捕获回调 URL。
pub(super) async fn capture_loopback_oauth_callback(
    port: u16,
    expected_state: String,
    timeout: Duration,
) -> Result<String, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|error| format!("本地 OAuth 回调监听启动失败：{error}"))?;

    let accept_result = tokio::select! {
        result = listener.accept() => result.map_err(|error| format!("本地 OAuth 回调接收失败：{error}")),
        _ = tokio::time::sleep(timeout) => Err(format!("等待本地 OAuth 回调超时（{} 秒）。", timeout.as_secs())),
    }?;
    let (mut stream, _) = accept_result;
    let mut buffer = vec![0u8; 8192];
    let read_len = stream
        .read(&mut buffer)
        .await
        .map_err(|error| format!("读取本地 OAuth 回调失败：{error}"))?;
    let request_text = String::from_utf8_lossy(&buffer[..read_len]);
    let request_line = request_text
        .lines()
        .next()
        .ok_or_else(|| String::from("本地 OAuth 回调请求行为空。"))?;
    let request_path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| String::from("本地 OAuth 回调请求行缺少路径。"))?;
    let callback_url = format!("http://127.0.0.1:{port}{request_path}");

    match parse_hosted_oauth_callback_input(&callback_url) {
        Ok(callback_payload) if callback_payload.oauth_state == expected_state => {
            let body = "<html><body><h2>OAuth 已完成</h2><p>可以返回 Vulcan Code 继续操作。</p></body></html>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            Ok(callback_url)
        }
        Ok(_) => {
            let body = "<html><body><h2>OAuth 状态校验失败</h2><p>请返回 Vulcan Code 重新发起授权。</p></body></html>";
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            Err(String::from(
                "本地 OAuth 回调中的 `state` 与宿主等待值不一致。",
            ))
        }
        Err(error) => {
            let body = format!(
                "<html><body><h2>OAuth 回调无效</h2><p>{}</p></body></html>",
                error
            );
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    //! Host-owned OAuth flow regression tests covering callback parsing and loopback redirect detection.
    //! 覆盖回调解析与环回地址识别的宿主自有 OAuth 流程回归测试。

    use super::{parse_hosted_oauth_callback_input, resolve_loopback_oauth_redirect_port};

    /// Accepts both full redirect URLs and raw query strings when parsing OAuth callback payloads.
    /// 在解析 OAuth 回调载荷时同时接受完整跳转 URL 与原始查询字符串。
    #[test]
    fn parse_hosted_oauth_callback_input_accepts_url_and_query_string() {
        let from_url = parse_hosted_oauth_callback_input(
            "http://127.0.0.1:9527/callback?code=demo-code&state=demo-state",
        )
        .expect("full url callback should parse");
        let from_query = parse_hosted_oauth_callback_input("code=demo-code&state=demo-state")
            .expect("raw query callback should parse");

        assert_eq!(from_url.authorization_code, "demo-code");
        assert_eq!(from_url.oauth_state, "demo-state");
        assert_eq!(from_query.authorization_code, "demo-code");
        assert_eq!(from_query.oauth_state, "demo-state");
    }

    /// Only loopback redirect URIs should expose a local listener port to the host OAuth coordinator.
    /// 只有环回回调 URI 才应向宿主 OAuth 协调器暴露本地监听端口。
    #[test]
    fn resolve_loopback_oauth_redirect_port_accepts_only_loopback_hosts() {
        assert_eq!(
            resolve_loopback_oauth_redirect_port("http://127.0.0.1:9527/callback"),
            Some(9527)
        );
        assert_eq!(
            resolve_loopback_oauth_redirect_port("http://localhost:3000/auth/callback"),
            Some(3000)
        );
        assert_eq!(
            resolve_loopback_oauth_redirect_port("https://example.com/callback"),
            None
        );
    }
}
