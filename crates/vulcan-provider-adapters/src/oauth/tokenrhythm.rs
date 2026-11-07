//! TokenRhythm account login: loopback PKCE, then one managed inference key for this account.
//! TokenRhythm 账户登录：环回 PKCE，随后为该账户签发一把托管推理密钥。

use super::*;
use crate::tokenrhythm::{
    CLIENT_ID, INTEGRATION_APP, MAX_ACCESS_TOKEN_SECONDS, ORIGIN, SCOPES, integration_json,
    integration_url, validate_identity,
};

/// Exact loopback callback path registered for the client.
/// 为该客户端注册的精确环回回调路径。
const CALLBACK_PATH: &str = "/oauth/callback";

/// Token endpoint response for both the code exchange and refresh grants.
/// 授权码交换与刷新两种授权共用的令牌端点响应。
#[derive(Deserialize)]
struct TokenResponse {
    /// Account access token for integration resources.
    /// 访问集成资源的账户访问令牌。
    access_token: String,
    /// Must be `Bearer`.
    /// 必须为 `Bearer`。
    token_type: String,
    /// Access-token lifetime in seconds.
    /// 访问令牌有效期（秒）。
    expires_in: u64,
    /// Rotating refresh token.
    /// 轮换的刷新令牌。
    refresh_token: String,
    /// Space-separated granted scopes.
    /// 以空格分隔的已授予权限范围。
    scope: String,
    /// Account the session belongs to.
    /// 会话所属账户。
    account_id: String,
    /// Login session; refresh must keep it unchanged.
    /// 登录会话；刷新时必须保持不变。
    session_id: String,
    /// Optional echo of the client the tokens were issued to.
    /// 可选回显的令牌签发客户端。
    #[serde(default)]
    client_id: Option<String>,
}

/// Managed inference key issued to the integration application.
/// 签发给集成应用的托管推理密钥。
#[derive(Deserialize)]
struct ManagedKeyCredential {
    /// Inference key, used only as the model API credential.
    /// 推理密钥，只作为模型 API 凭据使用。
    secret: String,
    /// Account that owns the key.
    /// 拥有该密钥的账户。
    account_id: String,
    /// Integration application the key is bound to.
    /// 该密钥绑定的集成应用。
    app_id: String,
    /// Must be `active`.
    /// 必须为 `active`。
    status: String,
}

/// Outcome of one loopback request.
/// 一次环回请求的结果。
enum Callback {
    /// The browser returned an authorization code for this transaction.
    /// 浏览器为本事务返回了授权码。
    Code(String),
    /// The user declined authorization.
    /// 用户拒绝授权。
    Denied,
}

impl OAuthAuthorizationRuntime {
    /// Starts TokenRhythm login for an exact profile binding against the production authority.
    /// 针对生产授权端为精确凭据档绑定启动 TokenRhythm 登录。
    ///
    /// `binding` is the manifest-derived transaction identity. Returns a browser-safe challenge.
    /// `binding` 是清单派生的事务身份。返回浏览器安全挑战。
    pub(super) async fn start_tokenrhythm(
        &self,
        binding: String,
    ) -> Result<OAuthAuthorizationChallenge, String> {
        self.start_tokenrhythm_at(binding, ORIGIN.to_string()).await
    }

    /// Binds the loopback listener, registers the transaction and returns the authorize URL.
    /// 绑定环回监听、登记事务并返回授权 URL。
    ///
    /// `origin` is the production authority, or a local server in tests.
    /// `origin` 是生产授权端，测试中为本地服务。
    async fn start_tokenrhythm_at(
        &self,
        binding: String,
        origin: String,
    ) -> Result<OAuthAuthorizationChallenge, String> {
        // listener is bound before the browser can learn the callback address.
        // listener 在浏览器得知回调地址之前绑定。
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| String::from("Failed to bind the TokenRhythm callback listener."))?;
        let port = listener
            .local_addr()
            .map_err(|_| String::from("Failed to inspect the TokenRhythm callback listener."))?
            .port();
        let redirect_uri = format!("http://127.0.0.1:{port}{CALLBACK_PATH}");
        // state and verifier stay inside this in-memory transaction.
        // state 与 verifier 只存在于本内存事务。
        let state = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let verifier = build_pkce_verifier();
        let mut authorization_url = Url::parse(&format!("{origin}/api/oauth/authorize"))
            .map_err(|_| String::from("TokenRhythm authorization endpoint is invalid."))?;
        authorization_url
            .query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", CLIENT_ID)
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("state", &state)
            .append_pair("code_challenge", &build_pkce_challenge(&verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("scope", &SCOPES.join(" "))
            .append_pair("installation_id", &Uuid::new_v4().to_string());
        let expires_at_unix_ms = unix_millis()?
            .checked_add(OAUTH_TRANSACTION_TIMEOUT.as_millis() as u64)
            .ok_or_else(|| String::from("TokenRhythm authorization expiry overflowed."))?;
        let snapshot = OAuthTransactionSnapshot {
            transaction_id: format!("oauth_{}", Uuid::new_v4().simple()),
            authorization_binding: binding,
            phase: OAuthTransactionPhase::WaitingForCallback,
            expires_at_unix_ms,
            message: None,
        };
        let http = crate::tokenrhythm::client()?;
        let transactions = Arc::clone(&self.transactions);
        let transaction_id = snapshot.transaction_id.clone();
        let (start_sender, start_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            if start_receiver.await.is_err() {
                return;
            }
            // One deadline bounds waiting for the browser, the exchange and key issuance.
            // 同一截止时间约束等待浏览器、令牌交换与密钥签发。
            let result = timeout(
                OAUTH_TRANSACTION_TIMEOUT,
                receive(
                    listener,
                    &http,
                    &origin,
                    &redirect_uri,
                    &state,
                    &verifier,
                    &transactions,
                    &transaction_id,
                ),
            )
            .await
            .unwrap_or_else(|_| Err(String::from("TokenRhythm authorization timed out.")));
            finish_transaction(transactions, &transaction_id, result).await;
        });
        self.register_transaction(&snapshot.transaction_id, snapshot.clone(), &task)
            .await?;
        if start_sender.send(()).is_err() {
            self.transactions
                .lock()
                .await
                .remove(&snapshot.transaction_id);
            return Err(String::from(
                "TokenRhythm authorization task stopped before registration.",
            ));
        }
        Ok(OAuthAuthorizationChallenge {
            transaction: snapshot,
            authorization_url: authorization_url.to_string(),
            flow_kind: OAuthAuthorizationFlowKind::LoopbackCallback,
            user_code: None,
            poll_interval_ms: None,
        })
    }
}

/// Waits for this transaction's callback, then exchanges the code and obtains the managed key.
/// 等待本事务回调，然后交换授权码并获取托管密钥。
///
/// Unrelated loopback requests are answered with 400 and do not consume the transaction.
/// Returns the complete credential material, or the first protocol failure.
/// 无关的环回请求返回 400 且不消费事务。返回完整凭据材料或首个协议失败。
#[allow(clippy::too_many_arguments)]
async fn receive(
    listener: TcpListener,
    http: &Client,
    origin: &str,
    redirect_uri: &str,
    state: &str,
    verifier: &str,
    transactions: &Arc<Mutex<BTreeMap<String, OAuthTransaction>>>,
    transaction_id: &str,
) -> Result<CredentialSecretMaterial, String> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|_| String::from("TokenRhythm callback listener failed."))?;
        // Each connection gets its own read deadline so a stalled socket cannot hold the flow.
        // 每条连接单独限时读取，停滞的连接无法占住流程。
        let callback = match timeout(OAUTH_HTTP_REQUEST_TIMEOUT, read_request(&mut stream)).await {
            Ok(Ok(bytes)) => parse_callback(&bytes, state),
            _ => None,
        };
        let code = match callback {
            Some(Callback::Code(code)) => code,
            Some(Callback::Denied) => {
                let _ = write_callback_response(&mut stream, callback_html(false)).await;
                return Err(String::from("TokenRhythm authorization was declined."));
            }
            None => {
                let _ = write_bad_request(&mut stream).await;
                continue;
            }
        };
        set_transaction_phase(
            transactions,
            transaction_id,
            OAuthTransactionPhase::FinalizingCredential,
        )
        .await;
        let material = complete_login(http, origin, &code, redirect_uri, verifier).await;
        let _ = write_callback_response(&mut stream, callback_html(material.is_ok())).await;
        return material;
    }
}

/// Exchanges the code once and obtains the managed key with the new account session.
/// 恰好交换一次授权码，并以新账户会话获取托管密钥。
///
/// Returns the validated material; an uncertain exchange is never retried.
/// 返回已校验材料；结果未知的交换绝不重试。
async fn complete_login(
    http: &Client,
    origin: &str,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<CredentialSecretMaterial, String> {
    let token = request_token(
        http,
        origin,
        CLIENT_ID,
        &[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("code_verifier", verifier),
        ],
    )
    .await?;
    let (tokens, session_id) = token_set(token, CLIENT_ID)?;
    let account_id = tokens
        .account_subject
        .clone()
        .ok_or_else(|| String::from("TokenRhythm account id is missing."))?;
    let managed_key = obtain_managed_key(http, origin, &tokens.access_token, &account_id).await?;
    let material = CredentialSecretMaterial::TokenRhythmAuth {
        tokens,
        session_id,
        client_id: CLIENT_ID.to_string(),
        managed_key,
    };
    CredentialSecretEnvelope::new(material.clone())?;
    Ok(material)
}

/// Posts one form grant to the token endpoint with this host's device description.
/// 携带本机设备描述向令牌端点提交一次表单授权。
///
/// `client_id` is the registration the tokens belong to. Returns the decoded response; 400 and
/// 401 mean the grant can no longer be used.
/// `client_id` 是令牌所属的注册。返回解码后的响应；400 与 401 表示该授权已不可用。
async fn request_token(
    http: &Client,
    origin: &str,
    client_id: &str,
    grant: &[(&str, &str)],
) -> Result<TokenResponse, String> {
    let mut form = vec![("client_id", client_id)];
    form.extend_from_slice(grant);
    let response = http
        .post(format!("{origin}/api/oauth/token"))
        .header("X-TokenRhythm-Device-Name", "Vulcan Code")
        .header("X-TokenRhythm-Device-OS", std::env::consts::OS)
        .form(&form)
        .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|_| String::from("TokenRhythm token request failed; sign in again."))?;
    let status = response.status();
    if matches!(status.as_u16(), 400 | 401) {
        return Err(String::from(
            "TokenRhythm authorization can no longer be used; sign in again.",
        ));
    }
    if !status.is_success() {
        return Err(format!(
            "TokenRhythm token request returned HTTP {}.",
            status.as_u16()
        ));
    }
    decode_bounded_oauth_json_response(response, "TokenRhythm token response").await
}

/// Validates one token response and converts it into a token set and its session id.
/// 校验一次令牌响应，并转换为令牌集合及其会话标识。
///
/// `client_id` is the registration the grant was sent with. The account id is kept as the
/// token set's account subject.
/// `client_id` 是提交授权时使用的注册。账户标识保存为令牌集合的账户主体。
fn token_set(token: TokenResponse, client_id: &str) -> Result<(OAuthTokenSet, String), String> {
    if token.token_type != "Bearer"
        || !(1..=MAX_ACCESS_TOKEN_SECONDS).contains(&token.expires_in)
        || token
            .client_id
            .as_deref()
            .is_some_and(|value| value != client_id)
    {
        return Err(String::from(
            "TokenRhythm returned an invalid token response.",
        ));
    }
    let scopes = token
        .scope
        .split_whitespace()
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if SCOPES.iter().any(|scope| !scopes.contains(*scope)) {
        return Err(String::from(
            "TokenRhythm did not grant every requested permission.",
        ));
    }
    validate_identity("account id", &token.account_id)?;
    validate_identity("session id", &token.session_id)?;
    let issued_at_unix_ms = unix_millis()?;
    let tokens = OAuthTokenSet {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        issued_at_unix_ms,
        expires_at_unix_ms: issued_at_unix_ms + token.expires_in * 1_000,
        scopes,
        account_subject: Some(token.account_id),
        email: None,
    };
    tokens.validate()?;
    Ok((tokens, token.session_id))
}

/// Obtains the account's managed inference key for the integration application.
/// 为集成应用获取账户的托管推理密钥。
///
/// Returns the key only when it is active and bound to `account_id` and this application.
/// 仅当密钥处于启用状态且绑定 `account_id` 与本应用时返回。
async fn obtain_managed_key(
    http: &Client,
    origin: &str,
    access_token: &str,
    account_id: &str,
) -> Result<String, String> {
    let key: ManagedKeyCredential = integration_json(
        http.post(integration_url(origin, "managed-key/credential"))
            .timeout(OAUTH_HTTP_REQUEST_TIMEOUT),
        access_token,
        "managed key",
    )
    .await?;
    if key.account_id != account_id || key.app_id != INTEGRATION_APP {
        return Err(String::from(
            "TokenRhythm managed key belongs to another account or application.",
        ));
    }
    if key.status != "active" {
        return Err(String::from("TokenRhythm managed key is disabled."));
    }
    if key.secret.trim().is_empty() {
        return Err(String::from("TokenRhythm managed key is empty."));
    }
    Ok(key.secret)
}

/// Rotates the account tokens once and keeps the managed key and session identity.
/// 恰好轮换一次账户令牌，并保留托管密钥与会话身份。
///
/// `material` must be TokenRhythm account material. Returns the refreshed material; a changed
/// account or session requires a new login.
/// `material` 必须是 TokenRhythm 账户材料。返回刷新后的材料；账户或会话变化需要重新登录。
pub(super) async fn refresh(
    material: &CredentialSecretMaterial,
) -> Result<CredentialSecretMaterial, String> {
    refresh_at(material, ORIGIN).await
}

/// Refreshes against `origin`, the production authority or a local test server.
/// 针对 `origin`（生产授权端或本地测试服务）刷新。
async fn refresh_at(
    material: &CredentialSecretMaterial,
    origin: &str,
) -> Result<CredentialSecretMaterial, String> {
    let CredentialSecretMaterial::TokenRhythmAuth {
        tokens,
        session_id,
        client_id,
        managed_key,
    } = material
    else {
        return Err(String::from(
            "TokenRhythm refresh requires TokenRhythm account authorization.",
        ));
    };
    // The stored client is used, so a session issued to an earlier registration fails explicitly.
    // 使用已保存的客户端，早期注册签发的会话会明确失败。
    let token = request_token(
        &crate::tokenrhythm::client()?,
        origin,
        client_id,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &tokens.refresh_token),
        ],
    )
    .await?;
    let (refreshed, refreshed_session) = token_set(token, client_id)?;
    if refreshed.account_subject != tokens.account_subject || refreshed_session != *session_id {
        return Err(String::from(
            "TokenRhythm refresh changed the account session; sign in again.",
        ));
    }
    Ok(CredentialSecretMaterial::TokenRhythmAuth {
        tokens: refreshed,
        session_id: session_id.clone(),
        client_id: client_id.clone(),
        managed_key: managed_key.clone(),
    })
}

/// Reads the bounded request head of one loopback connection.
/// 读取一条环回连接的有界请求头。
async fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0_u8; OAUTH_CALLBACK_MAX_BYTES];
    let mut length = 0;
    loop {
        if length == bytes.len() {
            return Err(String::from(
                "TokenRhythm callback exceeded the size limit.",
            ));
        }
        let count = stream
            .read(&mut bytes[length..])
            .await
            .map_err(|_| String::from("TokenRhythm callback read failed."))?;
        if count == 0 {
            return Err(String::from("TokenRhythm callback was incomplete."));
        }
        length += count;
        if bytes[..length].windows(4).any(|part| part == b"\r\n\r\n") {
            bytes.truncate(length);
            return Ok(bytes);
        }
    }
}

/// Accepts only `GET /oauth/callback` carrying this transaction's state exactly once.
/// 只接受恰好携带一次本事务 state 的 `GET /oauth/callback`。
///
/// Returns the code, a decline, or `None` for any unrelated or malformed request.
/// 返回授权码、拒绝，或对无关及格式错误的请求返回 `None`。
fn parse_callback(bytes: &[u8], state: &str) -> Option<Callback> {
    let request = std::str::from_utf8(bytes).ok()?;
    let mut parts = request.lines().next()?.split_whitespace();
    if parts.next()? != "GET" {
        return None;
    }
    let url = Url::parse(&format!("http://127.0.0.1{}", parts.next()?)).ok()?;
    if url.path() != CALLBACK_PATH || url.fragment().is_some() {
        return None;
    }
    let query = url.query_pairs().collect::<Vec<_>>();
    let values = |name: &str| {
        query
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.to_string())
            .collect::<Vec<_>>()
    };
    if values("state") != [state] {
        return None;
    }
    match (values("code").as_slice(), values("error").as_slice()) {
        ([code], []) if !code.is_empty() => Some(Callback::Code(code.clone())),
        ([], [_]) => Some(Callback::Denied),
        _ => None,
    }
}

/// Answers an unrelated loopback request with an empty 400 response.
/// 以空的 400 响应回复无关的环回请求。
async fn write_bad_request(stream: &mut TcpStream) -> Result<(), String> {
    stream
        .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await
        .map_err(|_| String::from("Failed to answer the TokenRhythm callback."))
}

#[cfg(test)]
mod tests;
