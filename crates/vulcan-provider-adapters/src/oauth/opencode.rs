//! Workspace-scoped OpenCode Console device authorization and token renewal.
//! 工作区作用域的 OpenCode Console 设备授权与令牌续期。

use super::*;
use crate::opencode::{CLIENT_ID, CONSOLE_URL};

/// Exact public and private device challenge fields returned by Console.
/// Console 返回的精确公开与私有设备挑战字段。
#[derive(Deserialize)]
struct DeviceChallenge {
    /// Private proof retained exclusively by the polling task.
    /// 仅由轮询任务保留的私有证明。
    device_code: String,
    /// Public code shown to the authorizing user.
    /// 展示给授权用户的公开代码。
    user_code: String,
    /// Browser verification URL, relative to Console or absolute on its origin.
    /// 相对于 Console 或位于同一来源的浏览器验证地址。
    verification_uri_complete: String,
    /// Server-owned lifetime in seconds.
    /// 服务端持有的有效期秒数。
    expires_in: u64,
    /// Minimum polling interval in seconds.
    /// 最小轮询间隔秒数。
    interval: u64,
}

/// Successful device exchange or refresh response, including optional organization scope.
/// 成功设备交换或刷新响应，包含可选组织作用域。
#[derive(Deserialize)]
struct Token {
    /// Current bearer token.
    /// 当前 Bearer 令牌。
    access_token: String,
    /// Rotating refresh token.
    /// 轮换刷新令牌。
    refresh_token: String,
    /// Access-token lifetime in seconds.
    /// 访问令牌有效期秒数。
    expires_in: u64,
    /// Workspace selected during authorization; upstream explicitly permits omission.
    /// 授权时选择的工作区；上游明确允许省略。
    org_id: Option<String>,
}

/// Closed union of the upstream success shape and OAuth polling errors.
/// 上游成功结构与 OAuth 轮询错误的封闭联合。
#[derive(Deserialize)]
#[serde(untagged)]
enum TokenResponse {
    /// Complete token response.
    /// 完整令牌响应。
    Success(Token),
    /// OAuth device-flow error code; descriptions are never exposed.
    /// OAuth 设备授权错误码；绝不公开错误描述。
    Error { error: String },
}

/// Authenticated user identity from the Console user endpoint.
/// Console 用户端点返回的已鉴权用户身份。
#[derive(Deserialize)]
struct User {
    /// Stable provider subject.
    /// 稳定供应商主体。
    id: String,
    /// Verified account email.
    /// 已验证账号邮箱。
    email: String,
}

/// Workspace identity from the Console organization listing.
/// Console 组织列表中的工作区身份。
#[derive(Deserialize)]
struct Organization {
    /// Exact billing workspace identifier.
    /// 精确计费工作区标识。
    id: String,
}

impl OAuthAuthorizationRuntime {
    /// Starts a device transaction against the fixed production Console.
    /// 向固定生产 Console 启动设备事务。
    /// `binding` is the frozen manifest scope; returns a secret-free browser challenge.
    /// `binding` 是冻结清单作用域；返回不含秘密的浏览器挑战。
    pub(super) async fn start_opencode(
        &self,
        binding: String,
    ) -> Result<OAuthAuthorizationChallenge, String> {
        self.start_opencode_at(binding, CONSOLE_URL).await
    }

    /// Registers a bounded transaction against production or a module-owned synthetic server.
    /// 向生产环境或模块持有的合成服务注册有界事务。
    /// `binding` identifies the exact profile and `origin` selects the trusted test/production authority.
    /// `binding` 标识精确配置档，`origin` 选择可信测试或生产授权端。
    /// Returns the public URL and code after the cancellable task has been registered.
    /// 在可取消任务注册完成后返回公开地址与代码。
    async fn start_opencode_at(
        &self,
        binding: String,
        origin: &str,
    ) -> Result<OAuthAuthorizationChallenge, String> {
        // http rejects redirects before any private device proof is issued.
        // http 在签发任何私有设备证明前拒绝重定向。
        let http = crate::opencode::client()?;
        // device follows the exact upstream request, including workspace-scope support.
        // device 遵循精确上游请求，包含工作区作用域支持声明。
        let device: DeviceChallenge = response_json(
            http.post(format!("{origin}/auth/device/code"))
                .json(&serde_json::json!({"client_id":CLIENT_ID,"supports_org_scope":true}))
                .send()
                .await
                .map_err(|_| String::from("OpenCode device authorization request failed."))?,
        )
        .await?;
        if device.device_code.trim().is_empty()
            || device.user_code.trim().is_empty()
            || device.interval == 0
            || device.expires_in == 0
            || device.interval >= device.expires_in
        {
            return Err(String::from(
                "OpenCode returned an invalid device challenge.",
            ));
        }
        // authority and verification prohibit a server-supplied navigation to another origin.
        // authority 与 verification 禁止服务端提供跳转到其他来源的导航地址。
        let authority = Url::parse(&format!("{origin}/"))
            .map_err(|_| String::from("OpenCode Console authority is invalid."))?;
        // verification resolves only within the response issuer's Console namespace.
        // verification 只在响应签发方的 Console 命名空间中解析。
        let verification = authority
            .join(&device.verification_uri_complete)
            .map_err(|_| String::from("OpenCode verification URL is invalid."))?;
        if verification.origin() != authority.origin()
            || !verification.path().starts_with(authority.path())
            || !verification.username().is_empty()
            || verification.password().is_some()
        {
            return Err(String::from(
                "OpenCode verification URL is outside the Console authority.",
            ));
        }
        // lifetime uses the shared host transaction cap rather than a second independent limit.
        // lifetime 使用共享宿主事务上限，而非第二个独立限制。
        let lifetime = Duration::from_secs(device.expires_in).min(OAUTH_TRANSACTION_TIMEOUT);
        // interval is checked against the effective lifetime before scheduling the first request.
        // interval 在调度首个请求前与实际有效期进行核对。
        let interval = Duration::from_secs(device.interval);
        if interval >= lifetime {
            return Err(String::from(
                "OpenCode polling interval exceeds the authorization lifetime.",
            ));
        }
        // snapshot exposes only the host transaction identity and public timing information.
        // snapshot 仅公开宿主事务身份与公开时间信息。
        let snapshot = OAuthTransactionSnapshot {
            transaction_id: format!("oauth_{}", Uuid::new_v4().simple()),
            authorization_binding: binding,
            phase: OAuthTransactionPhase::WaitingForUserAuthorization,
            expires_at_unix_ms: unix_millis()?
                .checked_add(lifetime.as_millis() as u64)
                .ok_or_else(|| String::from("OpenCode authorization expiry overflowed."))?,
            message: None,
        };
        // challenge never contains the private device_code used by the background exchange.
        // challenge 绝不包含后台交换使用的私有 device_code。
        let challenge = OAuthAuthorizationChallenge {
            transaction: snapshot.clone(),
            authorization_url: verification.to_string(),
            flow_kind: OAuthAuthorizationFlowKind::DeviceCode,
            user_code: Some(device.user_code),
            poll_interval_ms: Some(interval.as_millis() as u64),
        };
        // Owned inputs preserve the exact authorization authority and transaction throughout polling.
        // 持有的输入在整个轮询期间保留精确授权端与事务。
        let origin = origin.to_string();
        // transactions retains the runtime's shared cancellation and completion state.
        // transactions 保留运行时共享的取消与完成状态。
        let transactions = Arc::clone(&self.transactions);
        // transaction_id pins completion to the exact registered attempt.
        // transaction_id 将完成事件固定到精确的已注册尝试。
        let transaction_id = snapshot.transaction_id.clone();
        // The task cannot finish before registration, matching all existing device-flow primitives.
        // 任务不能在注册前完成，与所有既有设备授权原语保持一致。
        let (sender, receiver) = oneshot::channel();
        // task owns all private polling inputs and is aborted through the shared runtime.
        // task 持有全部私有轮询输入，并通过共享运行时中止。
        let task = tokio::spawn(async move {
            if receiver.await.is_err() {
                return;
            }
            // One deadline covers polling, account discovery, response reads, and sleeps.
            // 同一截止时间覆盖轮询、账号发现、响应读取与休眠。
            let result = timeout(
                lifetime,
                poll(&http, &origin, &device.device_code, interval),
            )
            .await
            .unwrap_or_else(|_| Err(String::from("OpenCode device authorization expired.")));
            finish_transaction(transactions, &transaction_id, result).await;
        });
        self.register_transaction(&snapshot.transaction_id, snapshot.clone(), &task)
            .await?;
        if sender.send(()).is_err() {
            self.transactions
                .lock()
                .await
                .remove(&snapshot.transaction_id);
            return Err(String::from(
                "OpenCode authorization stopped before registration.",
            ));
        }
        Ok(challenge)
    }
}

/// Polls the exact device-token endpoint until authorization succeeds or fails.
/// 轮询精确设备令牌端点，直到授权成功或失败。
/// `http`, `origin`, `device_code`, and `interval` are frozen by the registered transaction.
/// `http`、`origin`、`device_code` 与 `interval` 由已注册事务冻结。
/// Returns complete workspace-bound credential material; never guesses among multiple workspaces.
/// 返回完整的工作区绑定凭据材料；绝不在多个工作区间猜测选择。
async fn poll(
    http: &Client,
    origin: &str,
    device_code: &str,
    mut interval: Duration,
) -> Result<CredentialSecretMaterial, String> {
    loop {
        sleep(interval).await;
        // response is decoded even on OAuth's expected HTTP 400 pending responses.
        // response 即使是 OAuth 预期的 HTTP 400 等待响应也会解码。
        let response = http.post(format!("{origin}/auth/device/token"))
            .json(&serde_json::json!({"grant_type":"urn:ietf:params:oauth:grant-type:device_code","device_code":device_code,"client_id":CLIENT_ID}))
            .send().await.map_err(|_| String::from("OpenCode device polling failed."))?;
        // status is retained so a malformed error cannot masquerade as a successful token exchange.
        // status 被保留，避免畸形错误伪装为成功的令牌交换。
        let status = response.status();
        if status != reqwest::StatusCode::BAD_REQUEST && !status.is_success() {
            return Err(format!(
                "OpenCode device polling returned HTTP {}.",
                status.as_u16()
            ));
        }
        // token follows the source-defined success/error union without alternate field paths.
        // token 遵循源码定义的成功与错误联合，不使用候选字段路径。
        let token: TokenResponse =
            decode_bounded_oauth_json_response(response, "OpenCode device response").await?;
        match token {
            TokenResponse::Success(token) if status.is_success() => {
                return material(http, origin, token).await;
            }
            TokenResponse::Error { error } if error == "authorization_pending" => {}
            TokenResponse::Error { error } if error == "slow_down" => {
                interval = interval
                    .checked_add(Duration::from_secs(5))
                    .ok_or_else(|| String::from("OpenCode polling interval overflowed."))?;
            }
            _ => {
                return Err(String::from(
                    "OpenCode device authorization was denied, expired, or invalid.",
                ));
            }
        }
    }
}

/// Resolves the authenticated user and selected workspace before accepting a device token.
/// 接受设备令牌前解析已鉴权用户与选定工作区。
/// `http` and `origin` own the authority, while `token` carries its successful exchange.
/// `http` 与 `origin` 持有授权端，`token` 携带其成功交换结果。
/// Returns an OAuth credential or an explicit error for ambiguous or inconsistent identity.
/// 返回 OAuth 凭据，或在身份歧义、不一致时返回显式错误。
async fn material(
    http: &Client,
    origin: &str,
    token: Token,
) -> Result<CredentialSecretMaterial, String> {
    // user and organizations come from the same authenticated authority and access token.
    // user 与 organizations 来自同一已鉴权授权端和访问令牌。
    let user: User = get(http, &format!("{origin}/api/user"), &token.access_token).await?;
    // organizations is the exact membership list used to validate the selected billing workspace.
    // organizations 是用于核验选定计费工作区的精确成员列表。
    let organizations: Vec<Organization> =
        get(http, &format!("{origin}/api/orgs"), &token.access_token).await?;
    // organization is authority-selected; a sole workspace is the only unambiguous omitted-scope case.
    // organization 由授权端选择；单一工作区是省略作用域时唯一无歧义的情况。
    let organization = match token.org_id.as_deref() {
        Some(id) => organizations
            .iter()
            .find(|item| item.id == id)
            .ok_or("OpenCode authorized workspace is not a member of this account.")?,
        None if organizations.len() == 1 => &organizations[0],
        None => {
            return Err(String::from(
                "Select one OpenCode workspace during authorization and sign in again.",
            ));
        }
    };
    if user.id.trim().is_empty() || user.email.trim().is_empty() {
        return Err(String::from("OpenCode account identity is invalid."));
    }
    // tokens retain user identity separately from the workspace used for billing.
    // tokens 将用户身份与计费工作区分开保留。
    let tokens = token_set(&token, Some(user.id), Some(user.email))?;
    CredentialSecretEnvelope::new(CredentialSecretMaterial::OpencodeConsoleOAuth {
        tokens,
        organization_id: organization.id.clone(),
    })
    .map(|envelope| envelope.material)
}

/// Refreshes the fixed Console credential while preserving its exact user and workspace.
/// 刷新固定 Console 凭据，同时保留其精确用户与工作区。
/// `tokens` is the current token set and `organization_id` is the persisted billing identity.
/// `tokens` 为当前令牌集合，`organization_id` 为已持久化计费身份。
/// Returns rotated material, or requires reauthorization if the server changes its workspace.
/// 返回轮换材料；若服务端改变工作区则要求重新授权。
pub(super) async fn refresh(
    tokens: &OAuthTokenSet,
    organization_id: &str,
) -> Result<CredentialSecretMaterial, String> {
    // http owns redirect and complete-response deadlines for this private exchange.
    // http 持有此私有交换的重定向与完整响应截止时间策略。
    let http = crate::opencode::client()?;
    refresh_at(&http, CONSOLE_URL, tokens, organization_id).await
}

/// Executes refresh against the fixed authority or a private test server.
/// 向固定授权端或私有测试服务执行刷新。
/// `http`, `origin`, `tokens`, and `organization_id` describe the exact existing grant.
/// `http`、`origin`、`tokens` 与 `organization_id` 描述精确的既有授权。
/// Returns a complete renewed credential without issuing additional discovery requests.
/// 返回完整续期凭据，不额外发起发现请求。
async fn refresh_at(
    http: &Client,
    origin: &str,
    tokens: &OAuthTokenSet,
    organization_id: &str,
) -> Result<CredentialSecretMaterial, String> {
    // token rotates both access and refresh values atomically through the host secret store.
    // token 经宿主秘密仓库原子轮换访问与刷新值。
    let token: Token = response_json(http.post(format!("{origin}/auth/device/token"))
        .json(&serde_json::json!({"grant_type":"refresh_token","refresh_token":tokens.refresh_token,"client_id":CLIENT_ID}))
        .send().await.map_err(|_| String::from("OpenCode token refresh failed."))?).await?;
    if token
        .org_id
        .as_deref()
        .is_some_and(|id| id != organization_id)
    {
        return Err(String::from(
            "OpenCode refresh changed workspace; sign in again.",
        ));
    }
    Ok(CredentialSecretMaterial::OpencodeConsoleOAuth {
        tokens: token_set(&token, tokens.account_subject.clone(), tokens.email.clone())?,
        organization_id: organization_id.to_string(),
    })
}

/// Builds validated token timing from the server's exact integer lifetime.
/// 根据服务端精确整数有效期构建已验证令牌时间。
/// `token` owns the rotated values; `subject` and `email` retain verified identity.
/// `token` 持有轮换值；`subject` 与 `email` 保留已验证身份。
/// Returns a valid token set or an explicit overflow/expiry error.
/// 返回有效令牌集合或显式溢出、有效期错误。
fn token_set(
    token: &Token,
    subject: Option<String>,
    email: Option<String>,
) -> Result<OAuthTokenSet, String> {
    // issued_at and expires_at are checked before storing or converting durations.
    // issued_at 与 expires_at 在存储或转换时间间隔前进行检查。
    let issued_at_unix_ms = unix_millis()?;
    // expires_at_unix_ms rejects an unrepresentable server lifetime before material is accepted.
    // expires_at_unix_ms 在接受材料前拒绝无法表示的服务端有效期。
    let expires_at_unix_ms = token
        .expires_in
        .checked_mul(1000)
        .and_then(|lifetime| issued_at_unix_ms.checked_add(lifetime))
        .ok_or("OpenCode token expiry overflowed.")?;
    // tokens remain strongly typed and are never serialized into public transaction state.
    // tokens 保持强类型，绝不序列化到公开事务状态。
    let tokens = OAuthTokenSet {
        access_token: token.access_token.clone(),
        refresh_token: token.refresh_token.clone(),
        issued_at_unix_ms,
        expires_at_unix_ms,
        scopes: BTreeSet::new(),
        account_subject: subject,
        email,
    };
    tokens.validate()?;
    Ok(tokens)
}

/// Reads authenticated account metadata from a caller-owned fixed Console URL.
/// 从调用方持有的固定 Console 地址读取已鉴权账号元数据。
/// `http`, `url`, and `token` select the exact request; returns its typed JSON response.
/// `http`、`url` 与 `token` 选定精确请求；返回其类型化 JSON 响应。
async fn get<T: DeserializeOwned>(http: &Client, url: &str, token: &str) -> Result<T, String> {
    response_json(
        http.get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| String::from("OpenCode account discovery failed."))?,
    )
    .await
}

/// Checks HTTP status and applies the shared byte, depth, and node bounds to JSON.
/// 检查 HTTP 状态，并对 JSON 应用共享字节、深度及节点边界。
/// `response` owns the body; returns the requested type without exposing remote error text.
/// `response` 持有正文；返回请求类型，不公开远端错误文本。
async fn response_json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, String> {
    if !response.status().is_success() {
        return Err(format!(
            "OpenCode Console returned HTTP {}.",
            response.status().as_u16()
        ));
    }
    decode_bounded_oauth_json_response(response, "OpenCode Console response").await
}

#[cfg(test)]
mod tests;
