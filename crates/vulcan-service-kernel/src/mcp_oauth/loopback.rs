//! Redirect planning and the loopback receiver for MCP OAuth callbacks.
//! MCP OAuth 回调的回调地址规划与本机回环接收器。

use reqwest::Url;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Instant, timeout, timeout_at};

/// Upper bound for one callback request, request line and headers included.
/// 单个回调请求（含请求行与请求头）的大小上限。
const CALLBACK_REQUEST_MAX_BYTES: usize = 16 * 1024;

/// Time one accepted connection gets to deliver its request.
/// 单个已接受连接发送请求的时限。
const CALLBACK_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Path used when the host chooses the redirect itself.
/// 由宿主自行选择回调地址时使用的路径。
const DEFAULT_CALLBACK_PATH: &str = "/callback";

/// How the authorization callback reaches the host.
/// 授权回调到达宿主的方式。
pub(super) enum CallbackReceiver {
    /// A loopback listener bound to the redirect's port.
    /// 绑定到回调地址端口的本机回环监听器。
    Loopback {
        /// Bound listener.
        /// 已绑定监听器。
        listener: TcpListener,
        /// Exact path the callback must use.
        /// 回调必须使用的精确路径。
        path: String,
        /// Scheme, host and port used to rebuild the full callback URL.
        /// 用于重建完整回调 URL 的协议、主机与端口。
        origin: String,
    },
    /// The redirect points elsewhere; the user pastes the final URL.
    /// 回调地址指向其他位置；由用户粘贴最终 URL。
    Manual,
}

/// Effective redirect URI with its receiver.
/// 生效的回调 URI 及其接收方式。
pub(super) struct RedirectPlan {
    /// Redirect URI sent to the authorization server.
    /// 发送给授权服务器的回调 URI。
    pub(super) redirect_uri: String,
    /// Receiver for the callback.
    /// 回调接收方式。
    pub(super) receiver: CallbackReceiver,
}

/// Chooses the redirect URI and binds a loopback receiver when the redirect allows one.
/// 选择回调 URI，并在回调地址允许时绑定本机回环接收器。
///
/// # Parameters
/// 参数
/// `configured` is the user's redirect URI; `preregistered` is true when a client id is configured.
/// `configured` 是用户配置的回调 URI；配置了客户端标识时 `preregistered` 为 true。
///
/// # Returns
/// 返回值
/// The plan, or an error when a pre-registered client has no redirect or a loopback port is taken.
/// 返回规划；预注册客户端缺少回调地址或回环端口被占用时返回错误。
pub(super) async fn plan_redirect(
    configured: Option<&str>,
    preregistered: bool,
) -> Result<RedirectPlan, String> {
    let Some(configured) = configured else {
        if preregistered {
            return Err(String::from(
                "mcp_oauth_redirect_required: a pre-registered OAuth client needs the redirect URI registered with it",
            ));
        }
        // Dynamic registration registers whatever redirect the host picks, so an ephemeral port works.
        // 动态注册会登记宿主选定的任意回调地址，因此可以使用临时端口。
        let listener = bind("127.0.0.1:0").await?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("mcp_oauth_loopback_failed: {error}"))?
            .port();
        let origin = format!("http://127.0.0.1:{port}");
        return Ok(RedirectPlan {
            redirect_uri: format!("{origin}{DEFAULT_CALLBACK_PATH}"),
            receiver: CallbackReceiver::Loopback {
                listener,
                path: DEFAULT_CALLBACK_PATH.to_string(),
                origin,
            },
        });
    };
    let url =
        Url::parse(configured).map_err(|error| format!("mcp_oauth_redirect_invalid: {error}"))?;
    let Some(bind_host) = loopback_bind_host(&url) else {
        return Ok(RedirectPlan {
            redirect_uri: configured.to_string(),
            receiver: CallbackReceiver::Manual,
        });
    };
    let port = url.port().unwrap_or(80);
    let listener = bind(&format!("{bind_host}:{port}")).await?;
    let host = url.host_str().unwrap_or(bind_host);
    Ok(RedirectPlan {
        redirect_uri: configured.to_string(),
        receiver: CallbackReceiver::Loopback {
            listener,
            path: url.path().to_string(),
            origin: format!("http://{host}:{port}"),
        },
    })
}

/// Returns the address to bind for a plain-HTTP loopback redirect, or `None` for any other redirect.
/// 对纯 HTTP 本机回环回调地址返回要绑定的地址；其他回调地址返回 `None`。
fn loopback_bind_host(url: &Url) -> Option<&'static str> {
    if url.scheme() != "http" {
        return None;
    }
    match url.host_str()? {
        "localhost" | "127.0.0.1" => Some("127.0.0.1"),
        "[::1]" => Some("[::1]"),
        _ => None,
    }
}

/// Binds one loopback listener.
/// 绑定一个本机回环监听器。
async fn bind(address: &str) -> Result<TcpListener, String> {
    TcpListener::bind(address).await.map_err(|error| {
        format!("mcp_oauth_loopback_failed: cannot listen on `{address}` for the OAuth callback: {error}")
    })
}

/// Waits for the matching callback, ignoring stray requests, until `deadline`.
/// 在 `deadline` 之前等待匹配的回调，并忽略无关请求。
///
/// # Returns
/// 返回值
/// The full callback URL and the open connection so the caller can answer after exchanging the code.
/// 返回完整回调 URL 与仍打开的连接，使调用方在交换授权码后再响应。
pub(super) async fn receive_callback(
    listener: &TcpListener,
    path: &str,
    origin: &str,
    expected_state: &str,
    deadline: Instant,
) -> Result<(String, TcpStream), String> {
    loop {
        let (mut stream, _) = timeout_at(deadline, listener.accept())
            .await
            .map_err(|_| {
                String::from("mcp_oauth_timeout: no authorization callback arrived in time")
            })?
            .map_err(|error| format!("mcp_oauth_loopback_failed: {error}"))?;
        let Ok(Ok(target)) = timeout(CALLBACK_READ_TIMEOUT, read_request_target(&mut stream)).await
        else {
            continue;
        };
        let Ok(callback) = Url::parse(&format!("{origin}{target}")) else {
            let _ = respond(&mut stream, 400, false).await;
            continue;
        };
        if callback.path() != path {
            let _ = respond(&mut stream, 404, false).await;
            continue;
        }
        let state = callback
            .query_pairs()
            .find_map(|(key, value)| (key == "state").then(|| value.into_owned()));
        // Browsers may revisit or prefetch; only the request carrying this flow's state completes it.
        // 浏览器可能重访或预取；只有携带本流程 state 的请求才能完成它。
        if state.as_deref() != Some(expected_state) {
            let _ = respond(&mut stream, 400, false).await;
            continue;
        }
        return Ok((callback.to_string(), stream));
    }
}

/// Reads the request target from one bounded HTTP request head.
/// 从一个有界 HTTP 请求头读取请求目标。
async fn read_request_target(stream: &mut TcpStream) -> Result<String, String> {
    let mut buffer = vec![0_u8; CALLBACK_REQUEST_MAX_BYTES];
    let mut length = 0;
    loop {
        if length == buffer.len() {
            return Err(String::from("callback request too large"));
        }
        let read = stream
            .read(&mut buffer[length..])
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        length += read;
        if buffer[..length]
            .windows(4)
            .any(|window| window == b"\r\n\r\n")
        {
            break;
        }
    }
    let head = std::str::from_utf8(&buffer[..length]).map_err(|error| error.to_string())?;
    let mut request_line = head.lines().next().unwrap_or_default().split_whitespace();
    match (request_line.next(), request_line.next()) {
        (Some("GET"), Some(target)) if target.starts_with('/') => Ok(target.to_string()),
        _ => Err(String::from("not a callback request")),
    }
}

/// Answers the browser with a minimal page saying whether authorization finished.
/// 用最小页面告诉浏览器授权是否完成。
pub(super) async fn respond(
    stream: &mut TcpStream,
    status: u16,
    succeeded: bool,
) -> Result<(), String> {
    let (reason, body) = match (status, succeeded) {
        (200, true) => (
            "OK",
            "<!doctype html><meta charset=\"utf-8\"><title>Vulcan Code</title><p>MCP authorization complete. You can close this window and return to Vulcan Code.</p>",
        ),
        (200, false) => (
            "OK",
            "<!doctype html><meta charset=\"utf-8\"><title>Vulcan Code</title><p>MCP authorization failed. Return to Vulcan Code for details.</p>",
        ),
        (404, _) => ("Not Found", "<!doctype html><title>Not Found</title>"),
        _ => ("Bad Request", "<!doctype html><title>Bad Request</title>"),
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
