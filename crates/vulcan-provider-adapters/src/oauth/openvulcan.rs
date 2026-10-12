//! PKCE loopback authorization and single-use Auth rotation for OpenVulcan.
//! OpenVulcan 的 PKCE 环回授权及单次 Auth 轮换。

use super::*;
use crate::OpenVulcanRegion;

mod callback_page;

/// Frozen callback binding and localized pages owned by one authorization task.
/// 由一个授权任务持有的冻结回调绑定及本地化页面。
struct CallbackSession {
    /// The sole deployment allowed to exchange this callback's code.
    /// 唯一允许交换此回调授权码的部署。
    region: OpenVulcanRegion,
    /// Secret anti-forgery binding echoed by the browser.
    /// 浏览器原样返回的秘密防伪绑定。
    state: String,
    /// Secret PKCE verifier retained only on the host.
    /// 仅保留于宿主的秘密 PKCE 校验器。
    verifier: String,
    /// Public result pages prepared before opening the browser.
    /// 打开浏览器前准备的公开结果页面。
    pages: callback_page::CallbackPages,
}

/// Server response containing the browser approval transaction.
/// 包含浏览器批准事务的服务端响应。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BeginResponse {
    /// Server-owned request UUID.
    /// 服务端持有的请求 UUID。
    request_id: String,
    /// Exact regional approval URL.
    /// 准确的区域批准 URL。
    approval_url: String,
    /// Absolute RFC3339 approval deadline.
    /// RFC3339 格式的绝对批准截止时间。
    expires_at: String,
}

/// Exact token response, deliberately without a fabricated refresh token.
/// 准确的令牌响应，不虚构刷新令牌。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenResponse {
    /// Secret calling credential.
    /// 调用秘密凭据。
    access_token: String,
    /// Must be the literal Auth scheme.
    /// 必须为准确的 Auth 方案。
    token_type: String,
    /// Permanent device authorization UUID.
    /// 永久设备授权 UUID。
    authorization_id: String,
    /// Server-owned RFC3339 expiry.
    /// 服务端持有的 RFC3339 过期时间。
    expires_at: String,
}

/// Maps kind to its exact native deployment, or None for unrelated registered flows.
/// 将 kind 映射至准确原生部署，其他已注册流程返回 None。
pub(super) fn region_for_kind(kind: OAuthProviderKind) -> Option<OpenVulcanRegion> {
    match kind {
        OAuthProviderKind::OpenVulcanChina => Some(OpenVulcanRegion::China),
        OAuthProviderKind::OpenVulcanGlobal => Some(OpenVulcanRegion::Global),
        _ => None,
    }
}

/// Builds a bounded client that never forwards credentials through redirects.
/// 创建有界客户端，绝不通过重定向转发凭据。
fn native_client() -> Result<Client, String> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
        .build()
        .map_err(|_| String::from("Failed to initialize OpenVulcan authorization transport."))
}

/// Converts a server RFC3339 timestamp into positive Unix milliseconds or rejects it.
/// 将服务端 RFC3339 时间转换为正 Unix 毫秒值，无效时拒绝。
fn timestamp(value: &str) -> Result<u64, String> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .and_then(|time| u64::try_from(time.unix_timestamp_nanos() / 1_000_000).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| String::from("OpenVulcan returned an invalid expiration timestamp."))
}

/// Validates response and region and returns credential material for secure storage.
/// 校验 response 与 region，并返回用于安全存储的凭据材料。
fn token_material(
    response: TokenResponse,
    region: OpenVulcanRegion,
) -> Result<CredentialSecretMaterial, String> {
    if response.token_type != "Auth" {
        return Err(String::from(
            "OpenVulcan returned an unsupported authentication scheme.",
        ));
    }
    // expires_at_unix_ms must come from the authority rather than a locally guessed lifetime.
    // expires_at_unix_ms 必须来自授权端，而非本地猜测的有效期。
    let expires_at_unix_ms = timestamp(&response.expires_at)?;
    if expires_at_unix_ms <= unix_millis()? {
        return Err(String::from("OpenVulcan returned an expired credential."));
    }
    // material binds all later requests to the selected deployment and device.
    // material 将全部后续请求绑定到所选部署及设备。
    let material = CredentialSecretMaterial::OpenVulcanAuth {
        access_token: response.access_token,
        authorization_id: response.authorization_id,
        region,
        expires_at_unix_ms,
        rotation_pending: false,
    };
    CredentialSecretEnvelope::new(material.clone())?;
    Ok(material)
}

/// Decodes one successful response; failures expose only status and never secret-bearing bodies.
/// 解码一次成功响应；失败仅公开状态，绝不公开可能携带秘密的正文。
async fn response_json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, String> {
    if !response.status().is_success() {
        return Err(format!(
            "OpenVulcan authorization returned HTTP {}.",
            response.status().as_u16()
        ));
    }
    decode_bounded_oauth_json_response(response, "OpenVulcan response").await
}

impl OAuthAuthorizationRuntime {
    /// Cancels transaction_id atomically and revokes an already-received native device credential.
    /// 原子取消 transaction_id，并撤销已经收到的原生设备凭据。
    /// Returns success after local cancellation; a remote revocation error remains explicit.
    /// 本地取消后返回成功；远程撤销错误保持显式。
    pub async fn cancel_openvulcan_login(&self, transaction_id: &str) -> Result<(), String> {
        // material is removed under the same lock used by callback completion.
        // material 在回调完成使用的同一锁内移除。
        let material = {
            let mut transactions = self.transactions.lock().await;
            let transaction = transactions
                .remove(transaction_id)
                .ok_or_else(|| String::from("OpenVulcan authorization no longer exists."))?;
            transaction.abort_handle.abort();
            transaction.material
        };
        if let Some(material) = material {
            revoke_openvulcan_authorization(&material).await?;
        }
        Ok(())
    }

    /// Starts a region-bound PKCE transaction with a random loopback port; returns its public challenge.
    /// 使用随机环回端口启动绑定区域的 PKCE 事务，并返回公开挑战。
    /// Parameters freeze the region, declaration binding and optional application locale for this transaction.
    /// 参数为此事务冻结区域、声明绑定及可选应用语言。
    pub(super) async fn start_openvulcan(
        &self,
        region: OpenVulcanRegion,
        authorization_binding: String,
        return_page_locale: Option<String>,
    ) -> Result<OAuthAuthorizationChallenge, String> {
        // pages fail early on an invalid locale or incomplete catalog, before creating an authorization.
        // pages 在语言无效或目录不完整时提前失败，避免先创建授权。
        let pages = callback_page::CallbackPages::new(region, return_page_locale.as_deref())?;
        // listener is exclusively bound before any browser can receive the callback URL.
        // listener 在任何浏览器获得回调 URL 前独占绑定。
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| String::from("Failed to bind OpenVulcan callback listener."))?;
        // callback_uri is the exact user-center allowlisted path and numeric loopback host.
        // callback_uri 使用用户中心白名单中的准确路径和数字环回主机。
        let callback_uri = format!(
            "http://{}/vulcancode/auth/callback",
            listener
                .local_addr()
                .map_err(|_| String::from("Failed to inspect OpenVulcan callback listener."))?
        );
        // state and verifier remain private to this in-memory transaction.
        // state 和 verifier 仅属于本内存事务。
        let state = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let verifier = build_pkce_verifier();
        // http refuses redirects on every authorization operation.
        // http 对每次授权操作均拒绝重定向。
        let http = native_client()?;
        // response freezes device metadata without relying on machine-specific personal information.
        // response 固定设备元数据，不依赖机器特定个人信息。
        let response = http.post(format!("{}/api/v1/vulcancode/authorize", region.auth_origin()))
            .json(&serde_json::json!({"callback_uri": callback_uri, "state": state, "code_challenge": build_pkce_challenge(&verifier), "code_challenge_method": "S256", "device": {"display_name": "Vulcan Code", "operating_system": std::env::consts::OS, "architecture": std::env::consts::ARCH, "client_version": env!("CARGO_PKG_VERSION")}}))
            .send().await.map_err(|_| String::from("OpenVulcan authorization request failed."))?;
        // begin supplies the authoritative deadline and browser destination.
        // begin 提供权威截止时间及浏览器目的地。
        let begin: BeginResponse = response_json(response).await?;
        let approval = Url::parse(&begin.approval_url)
            .map_err(|_| String::from("OpenVulcan returned an invalid approval URL."))?;
        if approval.origin().ascii_serialization() != region.auth_origin()
            || approval.path() != "/authorize"
            || !approval.username().is_empty()
            || approval.password().is_some()
            || approval.fragment().is_some()
        {
            return Err(String::from(
                "OpenVulcan approval URL escaped the selected authority.",
            ));
        }
        // approval_query must contain each transaction selector exactly once.
        // approval_query 必须恰好包含一次各事务选择器。
        let approval_query = approval.query_pairs().collect::<Vec<_>>();
        if approval_query.len() != 2
            || !approval_query
                .iter()
                .any(|(key, value)| key == "request_id" && value == &begin.request_id)
            || !approval_query
                .iter()
                .any(|(key, value)| key == "state" && value == &state)
            || Uuid::parse_str(&begin.request_id).is_err()
        {
            return Err(String::from(
                "OpenVulcan approval transaction did not match.",
            ));
        }
        // expires_at_unix_ms is bounded by both the authority and the host transaction policy.
        // expires_at_unix_ms 同时受授权端和宿主事务策略约束。
        let expires_at_unix_ms = timestamp(&begin.expires_at)?
            .min(unix_millis()?.saturating_add(OAUTH_TRANSACTION_TIMEOUT.as_millis() as u64));
        let lifetime = Duration::from_millis(
            expires_at_unix_ms
                .checked_sub(unix_millis()?)
                .filter(|value| *value > 0)
                .ok_or_else(|| String::from("OpenVulcan approval transaction expired."))?,
        );
        // snapshot contains no state, verifier or calling credential.
        // snapshot 不包含 state、verifier 或调用凭据。
        let snapshot = OAuthTransactionSnapshot {
            transaction_id: format!("oauth_{}", Uuid::new_v4().simple()),
            authorization_binding,
            phase: OAuthTransactionPhase::WaitingForCallback,
            expires_at_unix_ms,
            message: None,
        };
        let transactions = Arc::clone(&self.transactions);
        let transaction_id = snapshot.transaction_id.clone();
        // callback binds the presentation and secrets to this same frozen regional transaction.
        // callback 将展示内容与秘密绑定到同一个冻结区域事务。
        let callback = CallbackSession {
            region,
            state,
            verifier,
            pages,
        };
        // start_receiver fences task execution until its cancellation handle is registered.
        // start_receiver 在取消句柄注册前阻止任务执行。
        let (start_sender, start_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            if start_receiver.await.is_err() {
                return;
            }
            // result has one total deadline, including slow local callback connections.
            // result 使用单一总截止时间，包括缓慢的本地回调连接。
            let result = timeout(
                lifetime,
                receive(listener, &http, &callback, &transactions, &transaction_id),
            )
            .await
            .unwrap_or_else(|_| Err(String::from("OpenVulcan authorization timed out.")));
            finish_transaction(transactions, &transaction_id, result).await;
        });
        self.register_transaction(&snapshot.transaction_id, snapshot.clone(), &task)
            .await?;
        if start_sender.send(()).is_err() {
            self.cancel(&snapshot.transaction_id).await?;
            return Err(String::from("OpenVulcan callback task stopped."));
        }
        Ok(OAuthAuthorizationChallenge {
            transaction: snapshot,
            authorization_url: begin.approval_url,
            flow_kind: OAuthAuthorizationFlowKind::LoopbackCallback,
            user_code: None,
            poll_interval_ms: None,
        })
    }
}

/// Waits for the exact callback then exchanges its code once; unrelated local traffic is rejected.
/// 等待准确回调后仅交换一次授权码；拒绝无关本地流量。
async fn receive(
    listener: TcpListener,
    http: &Client,
    session: &CallbackSession,
    transactions: &Arc<Mutex<BTreeMap<String, OAuthTransaction>>>,
    transaction_id: &str,
) -> Result<CredentialSecretMaterial, String> {
    loop {
        // stream is bounded independently so incomplete local traffic cannot occupy the entire transaction.
        // stream 独立限时，避免不完整本地流量占用整个事务。
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|_| String::from("OpenVulcan callback listener failed."))?;
        // callback rejects unrelated requests without consuming the live PKCE transaction.
        // callback 拒绝无关请求，且不消费进行中的 PKCE 事务。
        let callback = timeout(
            OAUTH_HTTP_REQUEST_TIMEOUT,
            read_native_callback(&mut stream, &session.state),
        )
        .await;
        // code is exchanged exactly once and never appears in the result document.
        // code 恰好交换一次，绝不出现在结果文档中。
        let code = match callback {
            Ok(Ok(code)) => code,
            _ => {
                let _ = callback_page::write(&mut stream, 400, &session.pages.rejected).await;
                continue;
            }
        };
        set_transaction_phase(
            transactions,
            transaction_id,
            OAuthTransactionPhase::FinalizingCredential,
        )
        .await;
        // Capture every exchange error so HTTP, decoding and validation failures all receive a usable page.
        // 捕获全部交换错误，使 HTTP、解码和校验失败都能返回可用页面。
        let material = exchange_callback_code(
            http,
            session.region.auth_origin(),
            &code,
            &session.verifier,
            session.region,
        )
        .await;
        let _ = write_exchange_result(&mut stream, &session.pages, &material).await;
        return material;
    }
}

/// Exchanges code and verifier once at the declared authority; returns validated material or an explicit error.
/// 在声明的授权端恰好交换一次 code 与 verifier；返回已校验材料或明确错误。
/// The authority is fixed by the selected region in production; tests use an isolated local server.
/// 生产环境授权端由所选区域固定；测试使用隔离本地服务。
async fn exchange_callback_code(
    http: &Client,
    authority: &str,
    code: &str,
    verifier: &str,
    region: OpenVulcanRegion,
) -> Result<CredentialSecretMaterial, String> {
    // response consumes the code once; an uncertain exchange is never retried automatically.
    // response 消费授权码一次；结果未知的交换绝不自动重试。
    let response = http
        .post(format!("{authority}/api/v1/vulcancode/token"))
        .json(&serde_json::json!({"code": code, "code_verifier": verifier}))
        .send()
        .await
        .map_err(|_| {
            String::from("OpenVulcan token exchange failed; start a new authorization.")
        })?;
    token_material(response_json(response).await?, region)
}

/// Writes the actual exchange result without exposing its credential or diagnostic to browser HTML.
/// 写入真实交换结果，且不向浏览器 HTML 暴露凭据或诊断。
/// Parameters supply the local stream, public pages and exchange result; returns transport success or failure.
/// 参数提供本地连接、公开页面及交换结果；返回传输成功或失败。
async fn write_exchange_result(
    stream: &mut TcpStream,
    pages: &callback_page::CallbackPages,
    material: &Result<CredentialSecretMaterial, String>,
) -> Result<(), String> {
    if material.is_ok() {
        callback_page::write(stream, 200, &pages.accepted).await
    } else {
        callback_page::write(stream, 502, &pages.failed).await
    }
}

/// Reads bounded HTTP headers from stream and returns only a validated transaction code.
/// 从 stream 读取有界 HTTP 请求头，并仅返回已验证事务授权码。
async fn read_native_callback(stream: &mut TcpStream, state: &str) -> Result<String, String> {
    // bytes never escape this callback and are not included in diagnostic messages.
    // bytes 不离开本回调，也不进入诊断消息。
    let mut bytes = vec![0_u8; OAUTH_CALLBACK_MAX_BYTES];
    let mut length = 0;
    loop {
        if length == bytes.len() {
            return Err(String::from("OpenVulcan callback exceeded the size limit."));
        }
        let count = stream
            .read(&mut bytes[length..])
            .await
            .map_err(|_| String::from("OpenVulcan callback read failed."))?;
        if count == 0 {
            return Err(String::from("OpenVulcan callback was incomplete."));
        }
        length += count;
        if bytes[..length].windows(4).any(|value| value == b"\r\n\r\n") {
            break;
        }
    }
    parse_native_callback(&bytes[..length], state)
}

/// Requires GET, the declared callback path and unique state/code parameters; returns the secret code.
/// 要求 GET、声明的回调路径及唯一 state/code 参数，并返回秘密授权码。
fn parse_native_callback(bytes: &[u8], state: &str) -> Result<String, String> {
    // request and parts are bounded views of the loopback request line.
    // request 和 parts 是环回请求行的有界视图。
    let request = std::str::from_utf8(bytes)
        .map_err(|_| String::from("Invalid OpenVulcan callback encoding."))?;
    let parts = request
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>();
    if parts.len() != 3
        || parts[0] != "GET"
        || parts[2] != "HTTP/1.1"
        || !parts[1].starts_with("/vulcancode/auth/callback?")
    {
        return Err(String::from("Invalid OpenVulcan callback request."));
    }
    let url = Url::parse(&format!("http://127.0.0.1{}", parts[1]))
        .map_err(|_| String::from("Invalid OpenVulcan callback URL."))?;
    let query = url.query_pairs().collect::<Vec<_>>();
    if url.path() != "/vulcancode/auth/callback"
        || url.fragment().is_some()
        || query.len() != 2
        || !query
            .iter()
            .any(|(key, value)| key == "state" && value == state)
    {
        return Err(String::from("OpenVulcan callback state did not match."));
    }
    query
        .iter()
        .find(|(key, value)| key == "code" && !value.is_empty())
        .map(|(_, value)| value.to_string())
        .ok_or_else(|| String::from("OpenVulcan callback omitted its code."))
}

/// Rotates material exactly once and verifies the permanent authorization identity is unchanged.
/// 恰好轮换一次 material，并验证永久授权身份保持不变。
pub(super) async fn refresh(
    material: &CredentialSecretMaterial,
) -> Result<CredentialSecretMaterial, String> {
    let CredentialSecretMaterial::OpenVulcanAuth {
        access_token,
        authorization_id,
        region,
        expires_at_unix_ms,
        rotation_pending,
    } = material
    else {
        return Err(String::from(
            "OpenVulcan refresh requires native authorization.",
        ));
    };
    if *rotation_pending {
        return Err(String::from(
            "OpenVulcan rotation was interrupted; sign in again.",
        ));
    }
    if *expires_at_unix_ms <= unix_millis()? {
        return Err(String::from(
            "OpenVulcan authorization expired; sign in again.",
        ));
    }
    // response is intentionally not retried because the old credential cannot refresh twice.
    // response 刻意不重试，因为旧凭据不能刷新两次。
    let response = native_client()?
        .post(format!(
            "{}/api/v1/vulcancode/token/refresh",
            region.auth_origin()
        ))
        .header("Authorization", format!("Auth {access_token}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|_| String::from("OpenVulcan refresh outcome is unknown; sign in again."))?;
    let token: TokenResponse = response_json(response).await?;
    if token.authorization_id != *authorization_id {
        return Err(String::from(
            "OpenVulcan refresh changed the device authorization identity.",
        ));
    }
    token_material(token, *region)
}

/// Revokes only material's current device; network failures are reported without leaking credentials.
/// 仅撤销 material 的当前设备；报告网络故障时不泄漏凭据。
pub async fn revoke_openvulcan_authorization(
    material: &CredentialSecretMaterial,
) -> Result<(), String> {
    let CredentialSecretMaterial::OpenVulcanAuth {
        access_token,
        region,
        ..
    } = material
    else {
        return Err(String::from(
            "OpenVulcan logout requires native authorization.",
        ));
    };
    // response never includes another user or device selector.
    // response 绝不包含其他用户或设备选择器。
    let response = native_client()?
        .post(format!("{}/api/v1/vulcancode/logout", region.auth_origin()))
        .header("Authorization", format!("Auth {access_token}"))
        .json(&serde_json::json!({}))
        .send()
        .await
        .map_err(|_| String::from("OpenVulcan remote logout failed."))?;
    if response.status() == reqwest::StatusCode::NO_CONTENT
        || response.status() == reqwest::StatusCode::UNAUTHORIZED
    {
        return Ok(());
    }
    Err(format!(
        "OpenVulcan logout returned HTTP {}.",
        response.status().as_u16()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises upstream HTTP, decoding and validation failures through the real HTML response writer.
    /// 通过真实 HTML 响应写入器验证上游 HTTP、解码及校验失败。
    #[tokio::test]
    async fn token_exchange_failures_return_a_complete_failure_page() {
        for (status, body) in [(503, "unavailable"), (200, "not-json"), (200, "{}")] {
            // upstream is an isolated synthetic authority; no real account or credential is used.
            // upstream 是隔离的合成授权端；不使用真实账户或凭据。
            let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let authority = format!("http://{}", upstream.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut stream, _) = upstream.accept().await.unwrap();
                let mut request = [0_u8; 8192];
                assert!(stream.read(&mut request).await.unwrap() > 0);
                stream.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            });
            // material captures the failure before the result page is written, including early decoding errors.
            // material 在写入结果页前捕获失败，包括提前发生的解码错误。
            let http = native_client().unwrap();
            let material = exchange_callback_code(
                &http,
                &authority,
                "private-code",
                "private-verifier",
                OpenVulcanRegion::China,
            )
            .await;
            assert!(material.is_err());
            server.await.unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let pages =
                callback_page::CallbackPages::new(OpenVulcanRegion::China, Some("zh-CN")).unwrap();
            // browser reads the same bytes a real callback tab would receive.
            // browser 读取与真实回调标签页相同的字节。
            let browser = http.get(format!("http://{address}/")).send();
            let writer = async {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0_u8; 8192];
                assert!(stream.read(&mut request).await.unwrap() > 0);
                write_exchange_result(&mut stream, &pages, &material)
                    .await
                    .unwrap();
            };
            let (response, ()) = tokio::join!(browser, writer);
            let response = response.unwrap();
            assert_eq!(response.status().as_u16(), 502);
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert_eq!(response.headers()["referrer-policy"], "no-referrer");
            let html = response.text().await.unwrap();
            assert!(html.contains("data-outcome=\"failed\""));
            assert!(!html.contains("private-code"));
            assert!(!html.contains("private-verifier"));
        }
    }

    /// Verifies callback binding rejects duplicate selectors, wrong paths and cross-transaction state.
    /// 验证回调绑定拒绝重复选择器、错误路径及跨事务 state。
    #[test]
    fn callback_is_bound_to_one_transaction() {
        assert_eq!(
            parse_native_callback(
                b"GET /vulcancode/auth/callback?code=fixture&state=expected HTTP/1.1\r\n\r\n",
                "expected"
            )
            .unwrap(),
            "fixture"
        );
        for request in [
            "GET /other?code=fixture&state=expected HTTP/1.1",
            "POST /vulcancode/auth/callback?code=fixture&state=expected HTTP/1.1",
            "GET /vulcancode/auth/callback?code=fixture&state=wrong HTTP/1.1",
            "GET /vulcancode/auth/callback?code=fixture&state=expected&state=expected HTTP/1.1",
        ] {
            assert!(parse_native_callback(request.as_bytes(), "expected").is_err());
        }
    }
}
