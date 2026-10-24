//! Official direct judge: the signed-in OpenVulcan account obtains an edge endpoint and a
//! short-lived token per model, and each escalated call goes straight to that endpoint.
//! 官方直连判定：已登录的 OpenVulcan 账户按模型获取边缘端点与短期令牌，每次升级调用直接发往该端点。
//!
//! Tokens are cached per model and account and renewed shortly before they expire; an endpoint
//! that rejects a token gets one fresh token and one retry. Every other failure goes to the next
//! layer of the pipeline and never allows the call.
//! 令牌按模型与账户缓存，并在到期前不久续期；端点拒绝令牌时换一次新令牌并重试一次。其他任何失败
//! 都交给管线的下一层，绝不放行调用。

use std::collections::BTreeMap;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use vulcan_permission_judge::protocol::JudgeInput;
use vulcan_permission_judge::{
    EDGE_JUDGE_PATH, EdgeJudgeRequest, EdgeJudgeResponse, EdgeOutcome, cloud_policy, edge_outcomes,
    find_edge_package, render_call,
};
use vulcan_provider_adapters::{
    CredentialExecutionSecret, OpenVulcanEdgeAccessError, request_openvulcan_edge_access,
};

use crate::public_ai_settings::openvulcan_account::signed_in_credential;
use crate::{SharedServiceBridgeState, current_v1_unix_millis};

/// A cached token is renewed once less than this much of its lifetime remains.
/// 缓存令牌剩余有效期少于该值时续期。
const ACCESS_REFRESH_AHEAD_MS: u64 = 2 * 60 * 1_000;

/// Longest wait for one edge judgement request; the call then goes to the next layer.
/// 一次边缘判定请求的最长等待时间；超时后调用交给下一层。
const EDGE_JUDGE_TIMEOUT: Duration = Duration::from_secs(15);

/// Largest edge answer read; ten results take a few kilobytes.
/// 读取的最大边缘回答；十条结果只有数 KB。
const EDGE_MAX_RESPONSE_BYTES: usize = 256 * 1024;

/// Endpoint and token for one model, issued to one account. It has no `Debug` so the token cannot
/// reach a log.
/// 签发给某个账户的单个模型的端点与令牌。不实现 `Debug`，使令牌无法进入日志。
struct CachedAccess {
    /// Account connection the token was issued for.
    /// 令牌签发所针对的账户连接。
    connection_id: String,
    /// Endpoint base URL.
    /// 端点基础地址。
    endpoint: String,
    /// Bearer token.
    /// Bearer 令牌。
    token: String,
    /// Token expiry in Unix milliseconds.
    /// 令牌到期时间（Unix 毫秒）。
    expires_at_ms: u64,
}

/// Edge access per edge model id, shared by every judgement of the process.
/// 按边缘模型 id 保存的边缘访问，由进程内所有判定共享。
#[derive(Default)]
pub(super) struct EdgeAccessCache {
    /// Cached access by edge model id; the lock also keeps concurrent calls to one renewal.
    /// 按边缘模型 id 缓存的访问；该锁同时使并发调用只发起一次续期。
    entries: Mutex<BTreeMap<String, CachedAccess>>,
}

/// What one judge request produced.
/// 一次判定请求的结果。
enum Answer {
    /// The endpoint answered.
    /// 端点已作答。
    Judged(EdgeJudgeResponse),
    /// The endpoint rejected the token.
    /// 端点拒绝了令牌。
    TokenRejected,
}

/// The process HTTP client for edge requests; it never follows redirects, so the token reaches
/// only the endpoint the user center named.
/// 进程内用于边缘请求的 HTTP 客户端；不跟随重定向，使令牌只发往用户中心指定的端点。
fn http() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| String::from("Failed to create the edge judge transport."))?;
    Ok(CLIENT.get_or_init(|| client))
}

impl EdgeAccessCache {
    /// Returns the endpoint and token for `model_id`, requesting a new one when none is cached for
    /// this account, the cached one is about to expire, or `renew` is set.
    /// 返回 `model_id` 的端点与令牌；当前账户没有缓存、缓存即将到期或 `renew` 为真时请求新的。
    async fn access(
        &self,
        connection_id: &str,
        secret: &CredentialExecutionSecret,
        model_id: &str,
        renew: bool,
    ) -> Result<(String, String), String> {
        let mut entries = self.entries.lock().await;
        let now = u64::try_from(current_v1_unix_millis()?).unwrap_or_default();
        if !renew
            && let Some(cached) = entries.get(model_id)
            && cached.connection_id == connection_id
            && cached.expires_at_ms.saturating_sub(now) > ACCESS_REFRESH_AHEAD_MS
        {
            return Ok((cached.endpoint.clone(), cached.token.clone()));
        }
        entries.remove(model_id);
        let access = request_openvulcan_edge_access(http()?, secret, model_id)
            .await
            .map_err(|error| match error {
                OpenVulcanEdgeAccessError::ModelUnavailable => format!(
                    "OpenVulcan does not offer the official direct model `{model_id}` in this deployment."
                ),
                OpenVulcanEdgeAccessError::Failed(reason) => reason,
            })?;
        let pair = (access.endpoint.clone(), access.access_token.clone());
        entries.insert(
            model_id.to_string(),
            CachedAccess {
                connection_id: connection_id.to_string(),
                endpoint: access.endpoint,
                token: access.access_token,
                expires_at_ms: access.expires_at_unix_ms,
            },
        );
        Ok(pair)
    }
}

/// Sends one judge request to `endpoint` with `token`.
/// 用 `token` 向 `endpoint` 发送一次判定请求。
async fn send(endpoint: &str, token: &str, request: &EdgeJudgeRequest) -> Result<Answer, String> {
    let response = http()?
        .post(format!("{endpoint}{EDGE_JUDGE_PATH}"))
        .timeout(EDGE_JUDGE_TIMEOUT)
        .bearer_auth(token)
        .json(request)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                String::from("The edge judge did not answer in time.")
            } else {
                String::from("The edge judge could not be reached.")
            }
        })?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|_| String::from("The edge judge answer could not be read."))?;
    if body.len() > EDGE_MAX_RESPONSE_BYTES {
        return Err(String::from("The edge judge answer is too large."));
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(Answer::TokenRejected);
    }
    if !status.is_success() {
        // The endpoint's error body is `{ "error": <code>, "message": <text> }`.
        // 端点的错误正文为 `{ "error": <代码>, "message": <说明> }`。
        let code = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value.get("error")?.as_str().map(str::to_string))
            .unwrap_or_default();
        return Err(format!(
            "The edge judge refused the call with HTTP {} `{code}`.",
            status.as_u16()
        ));
    }
    serde_json::from_slice(&body)
        .map(Answer::Judged)
        .map_err(|_| String::from("The edge judge answer violates its contract."))
}

/// Sends `request` with the endpoint and token `access` returns; when the endpoint rejects the
/// token, `access` is asked once for a renewed one and the request is sent once more.
/// 用 `access` 返回的端点与令牌发送 `request`；端点拒绝令牌时，向 `access` 请求一次续期的令牌并再发送
/// 一次。
async fn exchange<F, Fut>(
    mut access: F,
    request: &EdgeJudgeRequest,
) -> Result<EdgeJudgeResponse, String>
where
    F: FnMut(bool) -> Fut,
    Fut: std::future::Future<Output = Result<(String, String), String>>,
{
    let (endpoint, token) = access(false).await?;
    if let Answer::Judged(response) = send(&endpoint, &token, request).await? {
        return Ok(response);
    }
    let (endpoint, token) = access(true).await?;
    match send(&endpoint, &token, request).await? {
        Answer::Judged(response) => Ok(response),
        Answer::TokenRejected => Err(String::from(
            "The edge judge rejected a freshly issued access token.",
        )),
    }
}

/// Judges `inputs` with the official model `model_id`; one checked outcome per input, in order.
/// 用官方模型 `model_id` 判定 `inputs`；每条输入按顺序得到一条已检查的结果。
pub(super) async fn judge(
    state: &SharedServiceBridgeState,
    cache: &EdgeAccessCache,
    model_id: &str,
    inputs: Vec<JudgeInput>,
) -> Result<Vec<EdgeOutcome>, String> {
    let package = find_edge_package(model_id)
        .ok_or_else(|| format!("`{model_id}` is not an official direct model of this build."))?;
    let (connection_id, secret) = signed_in_credential(state).await?.ok_or_else(|| {
        String::from("Sign in to OpenVulcan before using the official direct judge.")
    })?;
    let expected = inputs.len();
    let request = EdgeJudgeRequest {
        request_id: format!("permission-judge-{}", uuid::Uuid::new_v4()),
        inputs,
    };
    let response = exchange(
        |renew| cache.access(&connection_id, &secret, model_id, renew),
        &request,
    )
    .await?;
    edge_outcomes(
        package,
        &cloud_policy().version,
        &request.request_id,
        expected,
        response,
    )
}

/// Times one request of `batch_size` ordinary calls; returns the wall time.
/// 为一次含 `batch_size` 条普通调用的请求计时；返回总耗时。
pub(super) async fn time_batch(
    state: &SharedServiceBridgeState,
    cache: &EdgeAccessCache,
    model_id: &str,
    batch_size: u32,
) -> Result<Duration, String> {
    let settings = cloud_policy()
        .resolve_settings(&Default::default())
        .map_err(|error| error.to_string())?
        .to_map();
    let input = JudgeInput {
        settings,
        rendered: render_call(None, "shell_command", r#"{"command":"git status"}"#),
    };
    // Obtaining the token first keeps the user center's answer out of the timed request.
    // 先取得令牌，使用户中心的回答不计入计时请求。
    if let Some((connection_id, secret)) = signed_in_credential(state).await? {
        cache
            .access(&connection_id, &secret, model_id, false)
            .await?;
    }
    let started = Instant::now();
    judge(state, cache, model_id, vec![input; batch_size as usize]).await?;
    Ok(started.elapsed())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex as StdMutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// Starts a loopback endpoint that answers each request by its bearer token: `good` is judged,
    /// `busy` is saturated and anything else is rejected. Returns the base URL and the tokens seen.
    /// 启动回环端点，按 Bearer 令牌回答每个请求：`good` 返回判定，`busy` 返回饱和，其余一律拒绝。
    /// 返回基础地址与收到的令牌。
    async fn endpoint() -> (String, Arc<StdMutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let seen = Arc::new(StdMutex::new(Vec::new()));
        let record = Arc::clone(&seen);
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut buffer = Vec::new();
                let mut chunk = [0_u8; 4096];
                // Read the head, then the body its Content-Length announces.
                // 先读取请求头，再读取 Content-Length 声明的正文。
                let (head_end, length) = loop {
                    let read = stream.read(&mut chunk).await.unwrap();
                    buffer.extend_from_slice(&chunk[..read]);
                    if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buffer[..end]).to_ascii_lowercase();
                        let length = head
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length:"))
                            .map_or(0, |value| value.trim().parse::<usize>().unwrap());
                        break (end + 4, length);
                    }
                };
                while buffer.len() < head_end + length {
                    let read = stream.read(&mut chunk).await.unwrap();
                    buffer.extend_from_slice(&chunk[..read]);
                }
                let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
                let token = head
                    .lines()
                    .find_map(|line| line.strip_prefix("authorization: Bearer "))
                    .unwrap_or_default()
                    .to_string();
                let request: serde_json::Value =
                    serde_json::from_slice(&buffer[head_end..head_end + length]).unwrap();
                record.lock().unwrap().push(token.clone());
                let (status, body) = match token.as_str() {
                    "good" => (
                        "200 OK",
                        serde_json::json!({
                            "request_id": request["request_id"], "region": "hk", "model": "m",
                            "checkpoint_sha256": "a".repeat(64), "policy_version": "p", "results": []
                        }),
                    ),
                    "busy" => (
                        "503 Service Unavailable",
                        serde_json::json!({"error": "region_saturated", "message": "busy"}),
                    ),
                    _ => (
                        "401 Unauthorized",
                        serde_json::json!({"error": "invalid_access_token", "message": "no"}),
                    ),
                };
                let body = body.to_string();
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            }
        });
        (format!("http://{address}"), seen)
    }

    fn request() -> EdgeJudgeRequest {
        EdgeJudgeRequest {
            request_id: String::from("r1"),
            inputs: Vec::new(),
        }
    }

    /// Runs `exchange` with a token per renewal step and returns the result and the renewals asked.
    /// 以每个续期步骤对应的令牌运行 `exchange`，返回结果与请求的续期序列。
    async fn run(
        endpoint: &str,
        first: &str,
        renewed: &str,
    ) -> (Result<EdgeJudgeResponse, String>, Vec<bool>) {
        let asked = StdMutex::new(Vec::new());
        let result = exchange(
            |renew| {
                asked.lock().unwrap().push(renew);
                let token = if renew { renewed } else { first };
                std::future::ready(Ok((endpoint.to_string(), token.to_string())))
            },
            &request(),
        )
        .await;
        (result, asked.into_inner().unwrap())
    }

    // A valid token is used once with no renewal.
    // 有效令牌只使用一次，不续期。
    #[tokio::test]
    async fn a_valid_token_is_not_renewed() {
        let (base, seen) = endpoint().await;
        let (result, asked) = run(&base, "good", "unused").await;
        assert_eq!(result.unwrap().request_id, "r1");
        assert_eq!(asked, [false]);
        assert_eq!(*seen.lock().unwrap(), ["good"]);
    }

    // A rejected token is renewed once and the request is sent once more.
    // 被拒绝的令牌续期一次，请求再发送一次。
    #[tokio::test]
    async fn a_rejected_token_is_renewed_once_and_retried_once() {
        let (base, seen) = endpoint().await;
        let (result, asked) = run(&base, "stale", "good").await;
        assert!(result.is_ok());
        assert_eq!(asked, [false, true]);
        assert_eq!(*seen.lock().unwrap(), ["stale", "good"]);

        let (base, seen) = endpoint().await;
        let (result, asked) = run(&base, "stale", "stale").await;
        assert!(result.is_err());
        assert_eq!(asked, [false, true]);
        assert_eq!(seen.lock().unwrap().len(), 2);
    }

    // Any other refusal is an error without renewal or retry.
    // 其他拒绝直接成为错误，不续期也不重试。
    #[tokio::test]
    async fn other_refusals_fail_without_a_retry() {
        let (base, seen) = endpoint().await;
        let (result, asked) = run(&base, "busy", "good").await;
        assert!(result.unwrap_err().contains("region_saturated"));
        assert_eq!(asked, [false]);
        assert_eq!(seen.lock().unwrap().len(), 1);
    }
}
