//! OpenDesign's native device authorization, verified against Vela CLI 0.1.4.
//! 对照 Vela CLI 0.1.4 核实的 OpenDesign 原生设备授权。

use super::*;
use crate::open_design::CONTROL_ORIGIN;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

/// Maximum lifetime retained for an upstream device challenge.
/// 上游设备挑战在宿主保留的最长生命周期。
const MAX_LIFETIME: Duration = Duration::from_secs(15 * 60);
/// Maximum accepted provider polling interval; prevents overflow and unresponsive challenges.
/// 接受的供应商轮询间隔上限；防止溢出及无响应挑战。
const MAX_POLL_SECONDS: u64 = 60;
/// Exact collection path used by the official client for challenge creation and token polling.
/// 官方客户端创建挑战及轮询令牌使用的精确集合路径。
const DEVICE_PATH: &str = "/api/v1/cli/device-authorizations";

/// Provider-owned device challenge; secret fields never enter public transaction snapshots.
/// 供应商持有的设备挑战；秘密字段绝不进入公开事务快照。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceChallenge {
    /// Exact opaque identifier used in the token path.
    /// 令牌路径使用的精确不透明标识。
    device_id: String,
    /// Private proof retained only by the polling task.
    /// 仅由轮询任务保留的秘密证明。
    device_secret: String,
    /// Public confirmation code displayed by the authorization page.
    /// 授权页面展示的公开确认代码。
    user_code: String,
    /// Official browser activation URL.
    /// 官方浏览器激活 URL。
    activation_url: String,
    /// Server-owned RFC3339 expiration.
    /// 服务端持有的 RFC3339 过期时间。
    expires_at: String,
    /// Server-owned interval in whole seconds.
    /// 服务端持有的整秒轮询间隔。
    poll_interval_seconds: u64,
    /// Initial state, required to be pending.
    /// 初始状态，必须为 pending。
    status: String,
}

/// Account identity supplied with both approved credentials.
/// 随两份已批准凭据一起提供的账户身份。
#[derive(Deserialize)]
struct AccountIdentity {
    /// Stable account subject.
    /// 稳定账户主体。
    id: String,
    /// Account email returned by the authority.
    /// 授权端返回的账户邮箱。
    email: String,
}

/// Token states and approved fields verified against the production device authorization endpoint.
/// 已通过生产设备授权端点验证的令牌状态及获批字段。
#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum TokenResponse {
    /// Authorization still requires browser approval; no credentials are usable.
    /// 授权仍需浏览器批准；没有可用凭据。
    Pending,
    /// The device challenge can no longer be exchanged.
    /// 设备挑战已无法交换凭据。
    Expired,
    /// Both credential roles were approved together; destinations remain host-owned constants.
    /// 两种凭据角色已共同获批；目标地址始终由宿主常量持有。
    Approved {
        /// Account-control credential, never used for inference.
        /// 账户控制凭据，绝不用于推理。
        #[serde(rename = "controlKey")]
        control_key: String,
        /// Inference credential, never substituted for the control credential.
        /// 推理凭据，绝不替代控制凭据。
        #[serde(rename = "runtimeKey")]
        runtime_key: String,
        /// Account bound to this credential pair.
        /// 与此凭据对绑定的账户。
        user: AccountIdentity,
    },
}

impl OAuthAuthorizationRuntime {
    /// Starts the registered production OpenDesign device flow for an exact profile binding.
    /// 为精确凭据档绑定启动已注册的生产 OpenDesign 设备流程。
    ///
    /// `binding` is the manifest-derived transaction identity. Returns a browser-safe challenge.
    /// `binding` 是清单派生的事务身份。返回浏览器安全挑战。
    pub(super) async fn start_open_design(
        &self,
        binding: String,
    ) -> Result<OAuthAuthorizationChallenge, String> {
        self.start_open_design_at(binding, CONTROL_ORIGIN).await
    }

    /// Creates and registers a device flow using the production authority or a local test server.
    /// 使用生产授权端或本地测试服务创建并注册设备流程。
    ///
    /// `binding` freezes profile identity; `origin` is supplied only by this module.
    /// `binding` 冻结凭据档身份；`origin` 仅由本模块提供。
    /// Returns a bounded, cancellable challenge without exposing its device secret.
    /// 返回有界、可取消的挑战，不暴露设备秘密。
    async fn start_open_design_at(
        &self,
        binding: String,
        origin: &str,
    ) -> Result<OAuthAuthorizationChallenge, String> {
        // http disables redirects before any private device proof can be issued.
        // http 在签发任何秘密设备证明之前禁用重定向。
        let http = crate::open_design::client()?;
        // response follows the exact JSON request observed from the official client.
        // response 遵循从官方客户端观测到的精确 JSON 请求。
        let response = http
            .post(format!("{origin}{DEVICE_PATH}"))
            .json(&serde_json::json!({
                "cliVersion": vulcan_product_info::version(),
                "host": "Vulcan Code",
                "profile": "prod"
            }))
            .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|_| String::from("OpenDesign device authorization request failed."))?;
        // device is decoded only after status and response bounds have been checked.
        // device 仅在检查状态及响应边界后解码。
        let device: DeviceChallenge = response_json(response).await?;
        // lifetime derives from server expiry and is capped by the host transaction boundary.
        // lifetime 由服务端过期时间派生，并受宿主事务边界限制。
        let now_ms = unix_millis()?;
        let lifetime = validate_challenge(&device, now_ms)?;
        // snapshot and interval contain only public challenge metadata.
        // snapshot 与 interval 仅包含公开挑战元数据。
        let interval = Duration::from_secs(device.poll_interval_seconds);
        let snapshot = OAuthTransactionSnapshot {
            transaction_id: format!("oauth_{}", Uuid::new_v4().simple()),
            authorization_binding: binding,
            phase: OAuthTransactionPhase::WaitingForUserAuthorization,
            expires_at_unix_ms: now_ms
                .checked_add(lifetime.as_millis() as u64)
                .ok_or_else(|| String::from("OpenDesign authorization expiry overflowed."))?,
            message: None,
        };
        // challenge excludes the private proof retained by the task below.
        // challenge 排除由下方任务保留的秘密证明。
        let challenge = OAuthAuthorizationChallenge {
            transaction: snapshot.clone(),
            authorization_url: device.activation_url.clone(),
            flow_kind: OAuthAuthorizationFlowKind::DeviceCode,
            user_code: Some(device.user_code.clone()),
            poll_interval_ms: Some(interval.as_millis() as u64),
        };
        // Owned task inputs bind the exchange to this one transaction and control endpoint.
        // 任务持有的输入将交换绑定到此唯一事务及控制端点。
        let token_url = format!("{origin}{DEVICE_PATH}/{}/token", device.device_id);
        let transactions = Arc::clone(&self.transactions);
        let transaction_id = snapshot.transaction_id.clone();
        let (start_sender, start_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            if start_receiver.await.is_err() {
                return;
            }
            // One deadline bounds polling, network reads, and sleeps together.
            // 同一截止时间共同约束轮询、网络读取及休眠。
            let result = timeout(
                lifetime,
                poll(&http, &token_url, &device.device_secret, interval),
            )
            .await
            .unwrap_or_else(|_| Err(String::from("OpenDesign device authorization expired.")));
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
                "OpenDesign authorization task stopped before registration.",
            ));
        }
        Ok(challenge)
    }
}

/// Validates the challenge's exact activation target, identity, cadence, and expiration.
/// 校验挑战的精确激活目标、身份、节奏及过期时间。
///
/// `device` is upstream JSON and `now_ms` is the host clock. Returns the bounded lifetime.
/// `device` 是上游 JSON，`now_ms` 是宿主时钟。返回有界生命周期。
fn validate_challenge(device: &DeviceChallenge, now_ms: u64) -> Result<Duration, String> {
    if device.status != "pending"
        || device.device_id.is_empty()
        || device.device_id.len() > 128
        || !device
            .device_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || device.device_secret.trim().is_empty()
        || device.user_code.trim().is_empty()
        || !(1..=MAX_POLL_SECONDS).contains(&device.poll_interval_seconds)
    {
        return Err(String::from(
            "OpenDesign returned an invalid device challenge.",
        ));
    }
    // URL parsing prevents credentials, alternate ports, or foreign origins from entering the UI.
    // URL 解析防止用户信息、替代端口或外部源进入界面。
    let url = Url::parse(&device.activation_url)
        .map_err(|_| String::from("OpenDesign returned an invalid activation URL."))?;
    let query: Vec<_> = url.query_pairs().collect();
    if url.origin().ascii_serialization() != "https://open-design.ai"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.path() != "/cloud/cli/activate"
        || query.len() != 2
        || !query
            .iter()
            .any(|(key, value)| key == "deviceId" && value == &device.device_id)
        || !query
            .iter()
            .any(|(key, value)| key == "userCode" && value == &device.user_code)
    {
        return Err(String::from(
            "OpenDesign activation URL does not match the device challenge.",
        ));
    }
    // expiration is parsed as a timestamp rather than guessed from a fixed polling count.
    // expiration 按时间戳解析，不根据固定轮询次数猜测。
    let expiration = OffsetDateTime::parse(&device.expires_at, &Rfc3339)
        .map_err(|_| String::from("OpenDesign device expiration is invalid."))?
        .unix_timestamp_nanos()
        / 1_000_000;
    let remaining = u64::try_from(expiration)
        .ok()
        .and_then(|value| value.checked_sub(now_ms))
        .filter(|value| *value > 0)
        .ok_or_else(|| String::from("OpenDesign device authorization expired."))?;
    Ok(Duration::from_millis(remaining).min(MAX_LIFETIME))
}

/// Polls one exact device proof until approved, expired, or explicitly rejected by the protocol.
/// 轮询一份精确设备证明，直到获批、过期或被协议明确拒绝。
///
/// `http`, `url`, `secret`, and `interval` are frozen by the initiating challenge.
/// `http`、`url`、`secret` 与 `interval` 由初始挑战冻结。
/// Returns the validated credential pair; unknown states and transport errors are terminal.
/// 返回已校验凭据对；未知状态及传输错误均为终态。
async fn poll(
    http: &Client,
    url: &str,
    secret: &str,
    interval: Duration,
) -> Result<CredentialSecretMaterial, String> {
    loop {
        // response carries the private proof only to the frozen control endpoint.
        // response 仅向冻结的控制端点传递秘密证明。
        let response = http
            .post(url)
            .json(&serde_json::json!({"deviceSecret": secret}))
            .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|_| String::from("OpenDesign authorization polling failed."))?;
        match response_json::<TokenResponse>(response).await? {
            TokenResponse::Pending => sleep(interval).await,
            TokenResponse::Expired => {
                return Err(String::from("OpenDesign device authorization expired."));
            }
            TokenResponse::Approved {
                control_key,
                runtime_key,
                user,
            } => {
                // The production response contains no URLs; credentials stay bound to the registered endpoints.
                // 生产响应不包含 URL；凭据始终绑定已注册端点。
                // material preserves the distinct roles atomically, with no invented refresh token.
                // material 原子保留不同角色，不伪造刷新令牌。
                let material = CredentialSecretMaterial::OpenDesignAuth {
                    runtime_key,
                    control_key,
                    user_id: user.id,
                    email: user.email,
                };
                CredentialSecretEnvelope::new(material.clone())?;
                return Ok(material);
            }
        }
    }
}

/// Reads a bounded successful JSON response without including provider bodies in diagnostics.
/// 读取有界成功 JSON 响应，诊断中不包含供应商正文。
///
/// `response` belongs to a timed request. Returns the requested closed response type.
/// `response` 属于有超时的请求。返回要求的封闭响应类型。
async fn response_json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T, String> {
    if !response.status().is_success() {
        return Err(format!(
            "OpenDesign authorization returned HTTP {}.",
            response.status().as_u16()
        ));
    }
    decode_bounded_oauth_json_response(response, "OpenDesign authorization response").await
}

#[cfg(test)]
mod tests;
