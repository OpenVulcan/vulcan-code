//! Provider authorization-code and PKCE runtime.
//! 供应商授权码与 PKCE 运行时。

use crate::{CredentialSecretEnvelope, CredentialSecretMaterial, MiniMaxRegion, OAuthTokenSet};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{
    Client, Url,
    header::{HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, oneshot},
    task::AbortHandle,
    time::{sleep, timeout},
};
use uuid::Uuid;

const OPENAI_CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_CODEX_AUTHORIZATION_URL: &str = "https://auth.openai.com/oauth/authorize";
const OPENAI_CODEX_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const OPENAI_CODEX_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const CLAUDE_CODE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDE_CODE_AUTHORIZATION_URL: &str = "https://claude.ai/oauth/authorize";
const CLAUDE_CODE_TOKEN_URL: &str = "https://api.anthropic.com/v1/oauth/token";
const CLAUDE_CODE_REDIRECT_URI: &str = "http://localhost:54545/callback";
const GOOGLE_AUTHORIZATION_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_CODE_ASSIST_LOAD_URL: &str =
    "https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist";
const OAUTH_TRANSACTION_TIMEOUT: Duration = Duration::from_secs(600);
/// Maximum wall-clock time for one OAuth HTTP exchange including its complete response body.
/// 单次 OAuth HTTP 交换（含完整响应体）的最长墙钟时间。
const OAUTH_HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Maximum OAuth response bytes retained before closed JSON decoding.
/// 封闭 JSON 解码前允许保留的 OAuth 响应最大字节数。
const OAUTH_RESPONSE_MAX_BYTES: usize = 1024 * 1024;
const OAUTH_CALLBACK_MAX_BYTES: usize = 16 * 1024;
const OAUTH_MAX_LIVE_TRANSACTIONS: usize = 64;
const MINIMAX_OAUTH_CLIENT_ID: &str = "659cf4c1-615c-45f6-a5f6-4bf15eb476e5";
const MINIMAX_GLOBAL_ACCOUNT_URL: &str = "https://account.minimax.io";
const MINIMAX_CHINA_ACCOUNT_URL: &str = "https://account.minimaxi.com";
const MINIMAX_GLOBAL_PLATFORM_URL: &str = "https://platform.minimax.io";
const MINIMAX_CHINA_PLATFORM_URL: &str = "https://platform.minimaxi.com";
const MINIMAX_GLOBAL_API_URL: &str = "https://api.minimax.io";
const MINIMAX_CHINA_API_URL: &str = "https://api.minimaxi.com";
const MINIMAX_OAUTH_MAX_REQUEST_ATTEMPTS: usize = 3;
const KIMI_CODING_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const KIMI_DEVICE_AUTHORIZATION_URL: &str = "https://auth.kimi.com/api/oauth/device_authorization";
const KIMI_TOKEN_URL: &str = "https://auth.kimi.com/api/oauth/token";
const KIMI_DEVICE_FLOW_MAX_LIFETIME: Duration = Duration::from_secs(15 * 60);
const KIMI_DEVICE_FLOW_DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Identifies one exact renewable provider authorization flow.
/// 标识一个精确的可续期供应商授权流程。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthProviderKind {
    /// OpenAI Codex subscription authorization.
    /// OpenAI Codex 订阅授权。
    OpenAiCodex,
    /// Claude Code subscription authorization.
    /// Claude Code 订阅授权。
    ClaudeCode,
    /// Google Code Assist authorization using a user-owned desktop OAuth client.
    /// 使用用户自有桌面 OAuth 客户端的 Google Code Assist 授权。
    GoogleCodeAssist,
    /// Public Gemini API authorization using a user-owned desktop OAuth client and project.
    /// 使用用户自有桌面 OAuth 客户端与项目的公共 Gemini API 授权。
    GoogleGemini,
    /// MiniMax international Token Plan Device Authorization Grant.
    /// MiniMax 国际版 Token Plan 设备授权流程。
    MiniMaxGlobalTokenPlan,
    /// MiniMax China Token Plan Device Authorization Grant.
    /// MiniMax 中国版 Token Plan 设备授权流程。
    MiniMaxChinaTokenPlan,
    /// Kimi Coding Plan Device Authorization Grant.
    /// Kimi Coding Plan 设备授权流程。
    KimiCodingPlan,
}

/// Carries user-owned OAuth client configuration without exposing it in snapshots.
/// 承载用户自有 OAuth 客户端配置且不在快照中暴露。
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OAuthClientConfiguration {
    /// Optional OAuth client identifier required by user-configured flows.
    /// 用户配置流程所需的可选 OAuth 客户端标识。
    pub client_id: Option<String>,
    /// Optional OAuth client secret retained only in the secure credential envelope.
    /// 仅保留在安全凭据封装中的可选 OAuth 客户端密钥。
    pub client_secret: Option<String>,
    /// Optional Google Cloud project required by the public Gemini OAuth flow.
    /// 公共 Gemini OAuth 流程所需的可选 Google Cloud 项目。
    pub project_id: Option<String>,
}

/// Identifies the observable phase of one authorization transaction.
/// 标识一个授权事务的可观察阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthTransactionPhase {
    /// The user must approve a Device Flow challenge in the provider browser.
    /// 用户必须在供应商浏览器中批准 Device Flow 挑战。
    WaitingForUserAuthorization,
    /// A loopback listener is waiting for the provider callback.
    /// 回环监听器正在等待供应商回调。
    WaitingForCallback,
    /// The callback passed state validation and token exchange is running.
    /// 回调已通过 State 校验且正在交换 Token。
    ExchangingToken,
    /// Validated credential material is ready for secure persistence.
    /// 已验证凭据材料已准备好安全持久化。
    Succeeded,
    /// The transaction failed without retaining credential material.
    /// 事务失败且未保留凭据材料。
    Failed,
    /// The user cancelled the transaction.
    /// 用户已取消事务。
    Cancelled,
}

/// Identifies how the user completes one OAuth authorization challenge.
/// 标识用户完成 OAuth 授权挑战的方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthAuthorizationFlowKind {
    /// A browser redirects to a process-local callback listener.
    /// 浏览器重定向到进程内回环监听器。
    LoopbackCallback,
    /// A browser displays a code while the runtime polls the token endpoint.
    /// 浏览器显示代码且运行时轮询 Token 端点。
    DeviceCode,
}

/// Browser-safe snapshot for one in-memory authorization transaction.
/// 单个内存授权事务的浏览器安全快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthTransactionSnapshot {
    /// Opaque transaction identifier.
    /// 不透明事务标识。
    pub transaction_id: String,
    /// Exact provider authorization kind.
    /// 精确供应商授权类别。
    pub provider_kind: OAuthProviderKind,
    /// Current observable phase.
    /// 当前可观察阶段。
    pub phase: OAuthTransactionPhase,
    /// Absolute transaction expiration time in Unix milliseconds.
    /// 事务绝对过期时间，使用 Unix 毫秒。
    pub expires_at_unix_ms: u64,
    /// Controlled failure text that never contains token material.
    /// 不包含 Token 材料的受控失败文本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Challenge returned when the host must open an external browser.
/// 宿主必须打开外部浏览器时返回的挑战。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthAuthorizationChallenge {
    /// Browser-safe transaction snapshot.
    /// 浏览器安全事务快照。
    pub transaction: OAuthTransactionSnapshot,
    /// Exact external authorization URL.
    /// 精确外部授权 URL。
    pub authorization_url: String,
    /// User interaction flow represented by this challenge.
    /// 当前挑战表示的用户交互流程。
    pub flow_kind: OAuthAuthorizationFlowKind,
    /// Provider-issued user code for Device Flow.
    /// Device Flow 中供应商签发的用户代码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_code: Option<String>,
    /// Provider-issued polling interval in milliseconds for Device Flow.
    /// Device Flow 中供应商签发的轮询间隔毫秒数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll_interval_ms: Option<u64>,
}

/// Owns process-local OAuth transactions and their provider exchanges.
/// 持有进程内 OAuth 事务及其供应商交换过程。
pub struct OAuthAuthorizationRuntime {
    client: Client,
    transactions: Arc<Mutex<BTreeMap<String, OAuthTransaction>>>,
}

struct OAuthTransaction {
    snapshot: OAuthTransactionSnapshot,
    material: Option<CredentialSecretMaterial>,
    abort_handle: AbortHandle,
}

struct OAuthFlow {
    provider_kind: OAuthProviderKind,
    client_id: String,
    client_secret: Option<String>,
    /// Optional validated project supplied by a user-owned Google OAuth client.
    /// 用户自有 Google OAuth 客户端提供的可选已校验项目。
    project_id: Option<String>,
    authorization_url: Url,
    token_url: String,
    redirect_uri: String,
    state: String,
    verifier: String,
    scopes: BTreeSet<String>,
    token_body_kind: OAuthTokenBodyKind,
}

#[derive(Clone, Copy)]
enum OAuthTokenBodyKind {
    Form,
    JsonWithState,
}

#[derive(Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
    expires_in: u64,
    #[serde(default)]
    scope: Option<String>,
}

/// Decodes MiniMax's optional business-status envelope.
/// 解码 MiniMax 可选的业务状态封装。
#[derive(Deserialize)]
struct MiniMaxBaseResponse {
    /// MiniMax business status code where zero means success.
    /// MiniMax 业务状态码，零表示成功。
    status_code: i64,
}

/// Decodes the exact MiniMax Device Flow challenge.
/// 解码精确的 MiniMax Device Flow 挑战。
#[derive(Deserialize)]
struct MiniMaxDeviceCodeResponse {
    /// Optional MiniMax business-status envelope.
    /// 可选的 MiniMax 业务状态封装。
    #[serde(default)]
    base_resp: Option<MiniMaxBaseResponse>,
    /// Short code displayed to the user.
    /// 向用户显示的短代码。
    user_code: String,
    /// Browser URL where the user approves the request.
    /// 用户批准请求的浏览器 URL。
    verification_uri: String,
    /// Absolute Unix expiration time in milliseconds.
    /// Unix 毫秒形式的绝对过期时间。
    expired_in: u64,
    /// Provider-requested polling interval in milliseconds.
    /// 供应商要求的轮询间隔毫秒数。
    interval: u64,
    /// State echoed from the challenge request.
    /// 从挑战请求回显的 State。
    state: String,
}

/// Decodes the exact MiniMax Device Flow and refresh response.
/// 解码精确的 MiniMax Device Flow 与刷新响应。
#[derive(Deserialize)]
struct MiniMaxTokenResponse {
    /// Optional MiniMax business-status envelope.
    /// 可选的 MiniMax 业务状态封装。
    #[serde(default)]
    base_resp: Option<MiniMaxBaseResponse>,
    /// Provider flow state such as pending or success.
    /// 供应商流程状态，例如 pending 或 success。
    status: String,
    /// Access token present on successful completion.
    /// 成功完成时提供的 Access Token。
    #[serde(default)]
    access_token: Option<String>,
    /// Refresh token present on successful completion or rotation.
    /// 成功完成或轮换时提供的 Refresh Token。
    #[serde(default)]
    refresh_token: Option<String>,
    /// Absolute Unix expiration time in milliseconds.
    /// Unix 毫秒形式的绝对过期时间。
    #[serde(default)]
    expired_in: Option<u64>,
    /// Trusted regional API resource URL.
    /// 可信的区域 API 资源 URL。
    #[serde(default)]
    resource_url: Option<String>,
}

/// Retains the non-secret state required while polling MiniMax Device Flow.
/// 保留轮询 MiniMax Device Flow 所需的非秘密状态。
struct MiniMaxDeviceFlow {
    /// Exact MiniMax region selected before authorization.
    /// 授权前选择的精确 MiniMax 区域。
    region: MiniMaxRegion,
    /// Account authority base URL.
    /// 账号授权端基础 URL。
    account_url: &'static str,
    /// User code issued by the authority.
    /// 授权端签发的用户代码。
    user_code: String,
    /// PKCE verifier retained only in process memory.
    /// 仅保留在进程内存中的 PKCE Verifier。
    verifier: String,
    /// Absolute challenge expiration in Unix milliseconds.
    /// 挑战的绝对 Unix 毫秒过期时间。
    expires_at_unix_ms: u64,
    /// Provider-requested polling interval.
    /// 供应商要求的轮询间隔。
    poll_interval_ms: u64,
}

/// Decodes one exact Kimi RFC 8628 device authorization response.
/// 解码一份精确的 Kimi RFC 8628 设备授权响应。
#[derive(Deserialize)]
struct KimiDeviceCodeResponse {
    /// Secret device code retained only by the process-local transaction.
    /// 仅由进程内事务保留的秘密设备码。
    device_code: String,
    /// Short code displayed to the user.
    /// 向用户显示的短代码。
    user_code: String,
    /// Browser verification page.
    /// 浏览器验证页面。
    verification_uri: String,
    /// Optional browser page with the code prefilled.
    /// 可选的预填代码浏览器页面。
    #[serde(default)]
    verification_uri_complete: Option<String>,
    /// Provider-reported lifetime in seconds.
    /// 供应商报告的有效秒数。
    expires_in: u64,
    /// Provider-reported minimum polling interval in seconds.
    /// 供应商报告的最小轮询间隔秒数。
    interval: u64,
}

/// Decodes Kimi device exchange and refresh responses.
/// 解码 Kimi 设备交换与刷新响应。
#[derive(Deserialize)]
struct KimiTokenResponse {
    /// Stable OAuth error identifier.
    /// 稳定 OAuth 错误标识。
    #[serde(default)]
    error: String,
    /// Controlled OAuth error description.
    /// 受控 OAuth 错误描述。
    #[serde(default)]
    error_description: String,
    /// Access token returned on success.
    /// 成功时返回的 Access Token。
    #[serde(default)]
    access_token: String,
    /// Refresh token returned or rotated on success.
    /// 成功时返回或轮换的 Refresh Token。
    #[serde(default)]
    refresh_token: String,
    /// Provider lifetime in seconds.
    /// 供应商有效秒数。
    #[serde(default)]
    expires_in: f64,
    /// Exact granted OAuth scope string.
    /// 精确授予的 OAuth Scope 字符串。
    #[serde(default)]
    scope: String,
}

/// Retains the secret and device identity needed while polling Kimi.
/// 保留轮询 Kimi 所需的秘密与设备身份。
struct KimiDeviceFlow {
    /// Provider-issued device secret.
    /// 供应商签发的设备秘密。
    device_code: String,
    /// Stable provider-visible device identifier.
    /// 稳定的供应商可见设备标识。
    device_id: String,
    /// Absolute challenge expiration in Unix milliseconds.
    /// 挑战的 Unix 毫秒绝对过期时间。
    expires_at_unix_ms: u64,
    /// Provider-safe polling interval in milliseconds.
    /// 供应商安全的轮询间隔毫秒数。
    poll_interval_ms: u64,
}

/// Parses a provider OAuth error without retaining its potentially sensitive description.
/// 解析供应商 OAuth 错误且不保留其可能包含敏感信息的描述。
#[derive(Deserialize)]
struct OAuthErrorResponse {
    /// Stable OAuth error identifier such as `invalid_grant`.
    /// 稳定的 OAuth 错误标识，例如 `invalid_grant`。
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiCodexIdentityClaims {
    #[serde(default)]
    email: Option<String>,
    #[serde(rename = "https://api.openai.com/auth")]
    authentication: OpenAiCodexAuthenticationClaims,
}

#[derive(Deserialize)]
struct OpenAiCodexAuthenticationClaims {
    chatgpt_account_id: String,
}

/// Decodes the documented Code Assist project variants observed at the control-plane boundary.
/// 解码在控制平面边界观测到的文档化 Code Assist 项目变体。
#[derive(Deserialize)]
#[serde(untagged)]
enum GoogleCodeAssistProjectReference {
    /// Direct project identifier.
    /// 直接项目标识。
    Id(String),
    /// Object-wrapped project identifier.
    /// 对象包装的项目标识。
    Object {
        /// Exact wrapped identifier.
        /// 精确包装标识。
        id: String,
    },
}

/// Decodes the exact project fields emitted by Code Assist account loading.
/// 解码 Code Assist 账号加载返回的精确项目字段。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleCodeAssistLoadResponse {
    /// Primary Cloud AI Companion project.
    /// 主要 Cloud AI Companion 项目。
    #[serde(default)]
    cloudaicompanion_project: Option<GoogleCodeAssistProjectReference>,
    /// Alternate project identifier emitted by another control-plane revision.
    /// 另一控制平面修订版返回的备用项目标识。
    #[serde(default)]
    project_id: Option<GoogleCodeAssistProjectReference>,
    /// Legacy project field emitted by the same reviewed endpoint.
    /// 同一已审核端点返回的旧项目字段。
    #[serde(default)]
    project: Option<GoogleCodeAssistProjectReference>,
}

impl OAuthAuthorizationRuntime {
    /// Creates one empty process-local authorization runtime.
    /// 创建一个空的进程内授权运行时。
    ///
    /// # Returns
    /// 返回值
    /// A runtime sharing the supplied HTTP client connection pool.
    /// 返回共享所提供 HTTP 客户端连接池的运行时。
    pub fn new(client: Client) -> Self {
        Self {
            client,
            transactions: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Starts one provider authorization transaction and loopback listener.
    /// 启动一个供应商授权事务与回环监听器。
    ///
    /// # Parameters
    /// 参数
    /// `provider_kind` selects an exact reviewed flow and `client` supplies user-owned settings when required.
    /// `provider_kind` 选择精确已审核流程，`client` 在需要时提供用户自有设置。
    ///
    /// # Returns
    /// 返回值
    /// A browser challenge after the loopback port has been reserved successfully.
    /// 成功保留回环端口后返回浏览器挑战。
    pub async fn start(
        &self,
        provider_kind: OAuthProviderKind,
        client: OAuthClientConfiguration,
    ) -> Result<OAuthAuthorizationChallenge, String> {
        if matches!(
            provider_kind,
            OAuthProviderKind::MiniMaxGlobalTokenPlan | OAuthProviderKind::MiniMaxChinaTokenPlan
        ) {
            return self.start_minimax_device_flow(provider_kind).await;
        }
        if provider_kind == OAuthProviderKind::KimiCodingPlan {
            return self.start_kimi_device_flow().await;
        }
        let transaction_id = format!("oauth_{}", Uuid::new_v4().simple());
        let state = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let verifier = build_pkce_verifier();
        let listener = bind_callback_listener(provider_kind).await?;
        let redirect_uri = callback_redirect_uri(provider_kind, &listener)?;
        let flow = build_oauth_flow(provider_kind, client, redirect_uri, state, verifier)?;
        let expires_at_unix_ms = unix_millis()?
            .checked_add(OAUTH_TRANSACTION_TIMEOUT.as_millis() as u64)
            .ok_or_else(|| String::from("OAuth transaction expiration overflowed."))?;
        let snapshot = OAuthTransactionSnapshot {
            transaction_id: transaction_id.clone(),
            provider_kind,
            phase: OAuthTransactionPhase::WaitingForCallback,
            expires_at_unix_ms,
            message: None,
        };
        let authorization_url = flow.authorization_url.to_string();
        let transactions = Arc::clone(&self.transactions);
        let transactions_for_task = Arc::clone(&transactions);
        let transaction_id_for_task = transaction_id.clone();
        let http = self.client.clone();
        let (start_sender, start_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            if start_receiver.await.is_err() {
                return;
            }
            let result = receive_and_exchange_callback(
                listener,
                &flow,
                &http,
                &transactions_for_task,
                transaction_id_for_task.as_str(),
            )
            .await;
            finish_transaction(transactions, transaction_id_for_task.as_str(), result).await;
        });
        let mut registered_transactions = self.transactions.lock().await;
        let now_unix_ms = unix_millis()?;
        registered_transactions
            .retain(|_, transaction| transaction.snapshot.expires_at_unix_ms > now_unix_ms);
        if registered_transactions.len() >= OAUTH_MAX_LIVE_TRANSACTIONS {
            task.abort();
            return Err(String::from(
                "Too many OAuth authorization transactions are active.",
            ));
        }
        registered_transactions.insert(
            transaction_id.clone(),
            OAuthTransaction {
                snapshot: snapshot.clone(),
                material: None,
                abort_handle: task.abort_handle(),
            },
        );
        drop(registered_transactions);
        if start_sender.send(()).is_err() {
            self.transactions
                .lock()
                .await
                .remove(transaction_id.as_str());
            return Err(String::from(
                "OAuth callback task stopped before registration.",
            ));
        }
        Ok(OAuthAuthorizationChallenge {
            transaction: snapshot,
            authorization_url,
            flow_kind: OAuthAuthorizationFlowKind::LoopbackCallback,
            user_code: None,
            poll_interval_ms: None,
        })
    }

    /// Starts one MiniMax OAuth 2.0 Device Authorization Grant with PKCE.
    /// 启动一个带 PKCE 的 MiniMax OAuth 2.0 设备授权流程。
    ///
    /// # Parameters
    /// 参数
    /// `provider_kind` selects the exact reviewed MiniMax regional authority.
    /// `provider_kind` 选择精确且已审核的 MiniMax 区域授权端。
    ///
    /// # Returns
    /// 返回值
    /// A browser-safe Device Flow challenge after the authority accepts the request.
    /// 授权端接受请求后返回浏览器安全的 Device Flow 挑战。
    async fn start_minimax_device_flow(
        &self,
        provider_kind: OAuthProviderKind,
    ) -> Result<OAuthAuthorizationChallenge, String> {
        let (region, account_url) = minimax_region_authority(provider_kind)?;
        let transaction_id = format!("oauth_{}", Uuid::new_v4().simple());
        let state = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let verifier = build_pkce_verifier();
        let challenge = build_pkce_challenge(verifier.as_str());
        let response = self
            .client
            .post(format!("{account_url}/oauth2/device/code"))
            .form(&[
                ("client_id", MINIMAX_OAUTH_CLIENT_ID),
                ("scope", "openid profile coding_plan"),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
                ("state", state.as_str()),
            ])
            .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| format!("MiniMax Device Flow request failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "MiniMax Device Flow request returned HTTP {}.",
                status.as_u16()
            ));
        }
        let device = decode_bounded_oauth_json_response::<MiniMaxDeviceCodeResponse>(
            response,
            "MiniMax Device Flow response",
        )
        .await?;
        validate_minimax_business_status(device.base_resp.as_ref(), "Device Flow")?;
        if device.state != state {
            return Err(String::from("MiniMax OAuth state validation failed."));
        }
        if device.user_code.trim().is_empty() || device.verification_uri.trim().is_empty() {
            return Err(String::from(
                "MiniMax Device Flow response omitted its user challenge.",
            ));
        }
        let now_unix_ms = unix_millis()?;
        if device.expired_in <= now_unix_ms {
            return Err(String::from(
                "MiniMax Device Flow challenge was already expired.",
            ));
        }
        let poll_interval_ms = if device.interval == 0 {
            3_000
        } else {
            device.interval
        };
        let snapshot = OAuthTransactionSnapshot {
            transaction_id: transaction_id.clone(),
            provider_kind,
            phase: OAuthTransactionPhase::WaitingForUserAuthorization,
            expires_at_unix_ms: device.expired_in,
            message: None,
        };
        let authorization_url =
            validate_minimax_verification_uri(region, device.verification_uri.as_str())?;
        let flow = MiniMaxDeviceFlow {
            region,
            account_url,
            user_code: device.user_code.clone(),
            verifier,
            expires_at_unix_ms: device.expired_in,
            poll_interval_ms,
        };
        let transactions = Arc::clone(&self.transactions);
        let transactions_for_task = Arc::clone(&transactions);
        let transaction_id_for_task = transaction_id.clone();
        let http = self.client.clone();
        let (start_sender, start_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            if start_receiver.await.is_err() {
                return;
            }
            let result = poll_minimax_device_flow(
                &http,
                &flow,
                &transactions_for_task,
                transaction_id_for_task.as_str(),
            )
            .await;
            finish_transaction(transactions, transaction_id_for_task.as_str(), result).await;
        });
        self.register_transaction(transaction_id.as_str(), snapshot.clone(), &task)
            .await?;
        if start_sender.send(()).is_err() {
            self.transactions
                .lock()
                .await
                .remove(transaction_id.as_str());
            return Err(String::from(
                "MiniMax Device Flow polling task stopped before registration.",
            ));
        }
        Ok(OAuthAuthorizationChallenge {
            transaction: snapshot,
            authorization_url,
            flow_kind: OAuthAuthorizationFlowKind::DeviceCode,
            user_code: Some(device.user_code),
            poll_interval_ms: Some(poll_interval_ms),
        })
    }

    /// Starts one Kimi Coding Plan RFC 8628 Device Authorization Grant.
    /// 启动一个 Kimi Coding Plan RFC 8628 设备授权流程。
    ///
    /// # Returns
    /// 返回值
    /// A browser-safe challenge while the process polls with the retained device identity.
    /// 返回浏览器安全挑战，同时进程使用保留的设备身份进行轮询。
    async fn start_kimi_device_flow(&self) -> Result<OAuthAuthorizationChallenge, String> {
        let transaction_id = format!("oauth_{}", Uuid::new_v4().simple());
        let device_id = Uuid::new_v4().hyphenated().to_string();
        let response = self
            .client
            .post(KIMI_DEVICE_AUTHORIZATION_URL)
            .form(&[("client_id", KIMI_CODING_CLIENT_ID)])
            .headers(kimi_device_headers(device_id.as_str())?)
            .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| format!("Kimi Device Flow request failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Kimi Device Flow request returned HTTP {}.",
                status.as_u16()
            ));
        }
        let device = decode_bounded_oauth_json_response::<KimiDeviceCodeResponse>(
            response,
            "Kimi Device Flow response",
        )
        .await?;
        if device.device_code.trim().is_empty()
            || device.user_code.trim().is_empty()
            || device.verification_uri.trim().is_empty()
        {
            return Err(String::from(
                "Kimi Device Flow response omitted its user challenge.",
            ));
        }
        let authorization_url = validate_kimi_verification_uri(
            device
                .verification_uri_complete
                .as_deref()
                .unwrap_or(device.verification_uri.as_str()),
        )?;
        let now_unix_ms = unix_millis()?;
        let provider_lifetime_ms = device.expires_in.saturating_mul(1_000);
        let bounded_lifetime_ms = provider_lifetime_ms.min(
            KIMI_DEVICE_FLOW_MAX_LIFETIME
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
        );
        if bounded_lifetime_ms == 0 {
            return Err(String::from(
                "Kimi Device Flow response returned an invalid expiration.",
            ));
        }
        let expires_at_unix_ms = now_unix_ms
            .checked_add(bounded_lifetime_ms)
            .ok_or_else(|| String::from("Kimi Device Flow expiration overflowed."))?;
        let poll_interval_ms = if device.interval == 0 {
            KIMI_DEVICE_FLOW_DEFAULT_POLL_INTERVAL.as_millis() as u64
        } else {
            device.interval.saturating_mul(1_000)
        };
        let snapshot = OAuthTransactionSnapshot {
            transaction_id: transaction_id.clone(),
            provider_kind: OAuthProviderKind::KimiCodingPlan,
            phase: OAuthTransactionPhase::WaitingForUserAuthorization,
            expires_at_unix_ms,
            message: None,
        };
        let flow = KimiDeviceFlow {
            device_code: device.device_code,
            device_id,
            expires_at_unix_ms,
            poll_interval_ms,
        };
        let transactions = Arc::clone(&self.transactions);
        let transactions_for_task = Arc::clone(&transactions);
        let transaction_id_for_task = transaction_id.clone();
        let http = self.client.clone();
        let (start_sender, start_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            if start_receiver.await.is_err() {
                return;
            }
            let result = poll_kimi_device_flow(
                &http,
                &flow,
                &transactions_for_task,
                transaction_id_for_task.as_str(),
            )
            .await;
            finish_transaction(transactions, transaction_id_for_task.as_str(), result).await;
        });
        self.register_transaction(transaction_id.as_str(), snapshot.clone(), &task)
            .await?;
        if start_sender.send(()).is_err() {
            self.transactions
                .lock()
                .await
                .remove(transaction_id.as_str());
            return Err(String::from(
                "Kimi Device Flow polling task stopped before registration.",
            ));
        }
        Ok(OAuthAuthorizationChallenge {
            transaction: snapshot,
            authorization_url,
            flow_kind: OAuthAuthorizationFlowKind::DeviceCode,
            user_code: Some(device.user_code),
            poll_interval_ms: Some(poll_interval_ms),
        })
    }

    /// Registers one bounded OAuth transaction before allowing its task to run.
    /// 在允许任务运行前注册一个有界 OAuth 事务。
    ///
    /// # Parameters
    /// 参数
    /// The arguments carry exact identity, browser snapshot, and cancellable task.
    /// 参数承载精确标识、浏览器快照和可取消任务。
    ///
    /// # Returns
    /// 返回值
    /// Success after atomic registration or an explicit capacity failure.
    /// 原子注册成功或返回明确的容量失败。
    async fn register_transaction(
        &self,
        transaction_id: &str,
        snapshot: OAuthTransactionSnapshot,
        task: &tokio::task::JoinHandle<()>,
    ) -> Result<(), String> {
        let mut registered_transactions = self.transactions.lock().await;
        let now_unix_ms = unix_millis()?;
        registered_transactions
            .retain(|_, transaction| transaction.snapshot.expires_at_unix_ms > now_unix_ms);
        if registered_transactions.len() >= OAUTH_MAX_LIVE_TRANSACTIONS {
            task.abort();
            return Err(String::from(
                "Too many OAuth authorization transactions are active.",
            ));
        }
        registered_transactions.insert(
            transaction_id.to_string(),
            OAuthTransaction {
                snapshot,
                material: None,
                abort_handle: task.abort_handle(),
            },
        );
        Ok(())
    }

    /// Returns one browser-safe transaction snapshot.
    /// 返回一个浏览器安全事务快照。
    ///
    /// # Parameters
    /// 参数
    /// `transaction_id` is the exact opaque identifier returned by `start`.
    /// `transaction_id` 是 `start` 返回的精确不透明标识。
    ///
    /// # Returns
    /// 返回值
    /// The current snapshot or an explicit unknown-transaction error.
    /// 返回当前快照或明确的未知事务错误。
    pub async fn status(&self, transaction_id: &str) -> Result<OAuthTransactionSnapshot, String> {
        self.transactions
            .lock()
            .await
            .get(transaction_id)
            .map(|transaction| transaction.snapshot.clone())
            .ok_or_else(|| format!("OAuth transaction `{transaction_id}` does not exist."))
    }

    /// Cancels one active transaction without retaining credentials.
    /// 取消一个活动事务且不保留凭据。
    ///
    /// # Parameters
    /// 参数
    /// `transaction_id` is the exact transaction to cancel.
    /// `transaction_id` 是要取消的精确事务。
    ///
    /// # Returns
    /// 返回值
    /// The terminal cancelled snapshot.
    /// 返回终态取消快照。
    pub async fn cancel(&self, transaction_id: &str) -> Result<OAuthTransactionSnapshot, String> {
        let mut transactions = self.transactions.lock().await;
        let transaction = transactions
            .get_mut(transaction_id)
            .ok_or_else(|| format!("OAuth transaction `{transaction_id}` does not exist."))?;
        transaction.abort_handle.abort();
        transaction.material = None;
        transaction.snapshot.phase = OAuthTransactionPhase::Cancelled;
        transaction.snapshot.message = Some(String::from("Authorization was cancelled."));
        Ok(transaction.snapshot.clone())
    }

    /// Consumes successful secret material and removes the completed transaction.
    /// 消费成功的秘密材料并移除已完成事务。
    ///
    /// # Parameters
    /// 参数
    /// `transaction_id` is the successful transaction being committed by the service.
    /// `transaction_id` 是服务正在提交的成功事务。
    ///
    /// # Returns
    /// 返回值
    /// The exact validated provider credential material.
    /// 返回精确且已验证的供应商凭据材料。
    pub async fn take_succeeded(
        &self,
        transaction_id: &str,
    ) -> Result<CredentialSecretMaterial, String> {
        let mut transactions = self.transactions.lock().await;
        let phase = transactions
            .get(transaction_id)
            .map(|transaction| transaction.snapshot.phase)
            .ok_or_else(|| format!("OAuth transaction `{transaction_id}` does not exist."))?;
        if phase != OAuthTransactionPhase::Succeeded {
            return Err(format!(
                "OAuth transaction `{transaction_id}` is not successful."
            ));
        }
        let transaction = transactions
            .remove(transaction_id)
            .expect("a successful OAuth transaction must remain present while locked");
        transaction.material.ok_or_else(|| {
            format!("OAuth transaction `{transaction_id}` has no credential material.")
        })
    }

    /// Refreshes renewable credential material without changing its account identity.
    /// 刷新可续期凭据材料且不改变其账号身份。
    ///
    /// # Parameters
    /// 参数
    /// `material` is the exact validated secret material currently stored for one credential.
    /// `material` 是当前为一个凭据保存的精确已验证秘密材料。
    ///
    /// # Returns
    /// 返回值
    /// A complete replacement material value, or a controlled reauthorization error.
    /// 返回完整替换材料，或受控的重新授权错误。
    pub async fn refresh(
        &self,
        material: &CredentialSecretMaterial,
    ) -> Result<CredentialSecretMaterial, String> {
        match material {
            CredentialSecretMaterial::ApiKey { .. }
            | CredentialSecretMaterial::StaticBearer { .. } => Ok(material.clone()),
            CredentialSecretMaterial::OpenAiCodexOAuth {
                tokens,
                id_token,
                account_id,
            } => {
                let token = refresh_oauth_token(
                    &self.client,
                    OPENAI_CODEX_TOKEN_URL,
                    OAuthTokenBodyKind::Form,
                    OPENAI_CODEX_CLIENT_ID,
                    None,
                    tokens,
                )
                .await?;
                let refreshed_id_token = token.id_token.clone().or_else(|| id_token.clone());
                let refreshed_identity = refreshed_id_token
                    .as_deref()
                    .map(parse_openai_codex_identity)
                    .transpose()?;
                let refreshed_account_id = refreshed_identity
                    .as_ref()
                    .map(|claims| claims.authentication.chatgpt_account_id.clone())
                    .or_else(|| account_id.clone());
                if refreshed_account_id.is_none() {
                    return Err(String::from(
                        "OpenAI Codex refresh did not preserve a verified account id; reauthorization is required.",
                    ));
                }
                Ok(CredentialSecretMaterial::OpenAiCodexOAuth {
                    tokens: merge_refreshed_token_set(tokens, &token)?,
                    id_token: refreshed_id_token,
                    account_id: refreshed_account_id,
                })
            }
            CredentialSecretMaterial::ClaudeCodeOAuth { tokens } => {
                let token = refresh_oauth_token(
                    &self.client,
                    CLAUDE_CODE_TOKEN_URL,
                    OAuthTokenBodyKind::JsonWithState,
                    CLAUDE_CODE_CLIENT_ID,
                    None,
                    tokens,
                )
                .await?;
                Ok(CredentialSecretMaterial::ClaudeCodeOAuth {
                    tokens: merge_refreshed_token_set(tokens, &token)?,
                })
            }
            CredentialSecretMaterial::GoogleOAuth {
                tokens,
                client_id,
                client_secret,
                project_id,
                quota_project_id,
            } => {
                let token = refresh_oauth_token(
                    &self.client,
                    GOOGLE_TOKEN_URL,
                    OAuthTokenBodyKind::Form,
                    client_id,
                    client_secret.as_deref(),
                    tokens,
                )
                .await?;
                Ok(CredentialSecretMaterial::GoogleOAuth {
                    tokens: merge_refreshed_token_set(tokens, &token)?,
                    client_id: client_id.clone(),
                    client_secret: client_secret.clone(),
                    project_id: project_id.clone(),
                    quota_project_id: quota_project_id.clone(),
                })
            }
            CredentialSecretMaterial::MiniMaxOAuth {
                tokens,
                region,
                resource_url,
            } => {
                let token = refresh_minimax_oauth_token(&self.client, *region, tokens).await?;
                let refreshed_resource_url = token
                    .resource_url
                    .clone()
                    .unwrap_or_else(|| resource_url.clone());
                validate_minimax_resource_origin(*region, refreshed_resource_url.as_str())?;
                Ok(CredentialSecretMaterial::MiniMaxOAuth {
                    tokens: merge_minimax_refreshed_token_set(tokens, &token)?,
                    region: *region,
                    resource_url: refreshed_resource_url,
                })
            }
            CredentialSecretMaterial::KimiCodingPlanOAuth { tokens, device_id } => {
                let refreshed =
                    refresh_kimi_oauth_token(&self.client, tokens, device_id.as_str()).await?;
                Ok(CredentialSecretMaterial::KimiCodingPlanOAuth {
                    tokens: merge_kimi_token_set(tokens, &refreshed)?,
                    device_id: device_id.clone(),
                })
            }
        }
    }
}

/// Resolves the exact MiniMax region and account authority.
/// 解析精确的 MiniMax 区域与账号授权端。
///
/// # Parameters
/// 参数
/// `provider_kind` must identify one MiniMax Token Plan Device Flow.
/// `provider_kind` 必须标识一个 MiniMax Token Plan Device Flow。
///
/// # Returns
/// 返回值
/// The fixed region and reviewed account authority.
/// 返回固定区域及已审核账号授权端。
fn minimax_region_authority(
    provider_kind: OAuthProviderKind,
) -> Result<(MiniMaxRegion, &'static str), String> {
    match provider_kind {
        OAuthProviderKind::MiniMaxGlobalTokenPlan => {
            Ok((MiniMaxRegion::Global, MINIMAX_GLOBAL_ACCOUNT_URL))
        }
        OAuthProviderKind::MiniMaxChinaTokenPlan => {
            Ok((MiniMaxRegion::China, MINIMAX_CHINA_ACCOUNT_URL))
        }
        _ => Err(String::from(
            "The selected OAuth provider does not use MiniMax Device Flow.",
        )),
    }
}

/// Validates an optional MiniMax business-status envelope.
/// 校验可选的 MiniMax 业务状态封装。
///
/// # Parameters
/// 参数
/// `base_response` is the decoded envelope and `operation` identifies the safe error context.
/// `base_response` 是解码后的封装，`operation` 标识安全错误上下文。
///
/// # Returns
/// 返回值
/// Success when absent or explicitly successful.
/// 缺失或明确成功时返回成功。
fn validate_minimax_business_status(
    base_response: Option<&MiniMaxBaseResponse>,
    operation: &str,
) -> Result<(), String> {
    if let Some(base_response) = base_response
        && base_response.status_code != 0
    {
        return Err(format!(
            "MiniMax {operation} returned business status {}.",
            base_response.status_code
        ));
    }
    Ok(())
}

/// Validates the Device Flow verification URL against the selected regional platform.
/// 根据所选区域平台校验 Device Flow 验证 URL。
///
/// # Parameters
/// 参数
/// `region` selects the reviewed platform origin and `verification_uri` is provider supplied.
/// `region` 选择已审核平台 Origin，`verification_uri` 由供应商提供。
///
/// # Returns
/// 返回值
/// The normalized URI only when scheme, host, and port match exactly and no credentials are embedded.
/// 仅在 Scheme、Host 与端口精确匹配且不嵌入凭据时返回规范化 URI。
fn validate_minimax_verification_uri(
    region: MiniMaxRegion,
    verification_uri: &str,
) -> Result<String, String> {
    let platform_url = match region {
        MiniMaxRegion::Global => MINIMAX_GLOBAL_PLATFORM_URL,
        MiniMaxRegion::China => MINIMAX_CHINA_PLATFORM_URL,
    };
    let expected = Url::parse(platform_url)
        .map_err(|error| format!("Invalid MiniMax platform authority: {error}"))?;
    let actual = Url::parse(verification_uri)
        .map_err(|_| String::from("MiniMax Device Flow returned an invalid verification URL."))?;
    if actual.scheme() != "https"
        || actual.host_str() != expected.host_str()
        || actual.port_or_known_default() != expected.port_or_known_default()
        || !actual.username().is_empty()
        || actual.password().is_some()
    {
        return Err(String::from(
            "MiniMax Device Flow returned an untrusted verification URL.",
        ));
    }
    Ok(actual.to_string())
}

/// Polls MiniMax until the user approves, the challenge expires, or a terminal failure occurs.
/// 轮询 MiniMax，直到用户批准、挑战过期或发生终态失败。
///
/// # Parameters
/// 参数
/// The arguments carry the HTTP pool, in-memory challenge, transaction map, and exact transaction id.
/// 参数承载 HTTP 连接池、内存挑战、事务映射及精确事务标识。
///
/// # Returns
/// 返回值
/// Validated renewable MiniMax credential material.
/// 返回已校验的可续期 MiniMax 凭据材料。
async fn poll_minimax_device_flow(
    client: &Client,
    flow: &MiniMaxDeviceFlow,
    transactions: &Arc<Mutex<BTreeMap<String, OAuthTransaction>>>,
    transaction_id: &str,
) -> Result<CredentialSecretMaterial, String> {
    while unix_millis()? < flow.expires_at_unix_ms {
        sleep(Duration::from_millis(flow.poll_interval_ms)).await;
        set_transaction_phase(
            transactions,
            transaction_id,
            OAuthTransactionPhase::WaitingForUserAuthorization,
        )
        .await;
        let response = client
            .post(format!("{}/oauth2/token", flow.account_url))
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", MINIMAX_OAUTH_CLIENT_ID),
                ("user_code", flow.user_code.as_str()),
                ("code_verifier", flow.verifier.as_str()),
            ])
            .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| format!("MiniMax Device Flow polling failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "MiniMax Device Flow polling returned HTTP {}.",
                status.as_u16()
            ));
        }
        let token = decode_bounded_oauth_json_response::<MiniMaxTokenResponse>(
            response,
            "MiniMax Device Flow token response",
        )
        .await?;
        validate_minimax_business_status(token.base_resp.as_ref(), "Device Flow token exchange")?;
        if token.status == "pending" {
            continue;
        }
        if token.status != "success" {
            return Err(format!(
                "MiniMax Device Flow ended with status `{}`.",
                token.status
            ));
        }
        set_transaction_phase(
            transactions,
            transaction_id,
            OAuthTransactionPhase::ExchangingToken,
        )
        .await;
        return build_minimax_credential_material(flow.region, token);
    }
    Err(String::from("MiniMax Device Flow authorization timed out."))
}

/// Polls the Kimi token endpoint until authorization completes or expires.
/// 轮询 Kimi Token 端点，直到授权完成或过期。
///
/// # Parameters
/// 参数
/// `client` owns HTTP execution, `flow` owns the provider secret and device identity,
/// and the transaction arguments expose progress without exposing credentials.
/// `client` 持有 HTTP 执行，`flow` 持有供应商秘密与设备身份，
/// 事务参数用于公开进度而不公开凭据。
///
/// # Returns
/// 返回值
/// Complete renewable Kimi Coding Plan credential material.
/// 返回完整可续期 Kimi Coding Plan 凭据材料。
async fn poll_kimi_device_flow(
    client: &Client,
    flow: &KimiDeviceFlow,
    transactions: &Arc<Mutex<BTreeMap<String, OAuthTransaction>>>,
    transaction_id: &str,
) -> Result<CredentialSecretMaterial, String> {
    while unix_millis()? < flow.expires_at_unix_ms {
        sleep(Duration::from_millis(flow.poll_interval_ms)).await;
        set_transaction_phase(
            transactions,
            transaction_id,
            OAuthTransactionPhase::WaitingForUserAuthorization,
        )
        .await;
        let response = client
            .post(KIMI_TOKEN_URL)
            .form(&[
                ("client_id", KIMI_CODING_CLIENT_ID),
                ("device_code", flow.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .headers(kimi_device_headers(flow.device_id.as_str())?)
            .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| format!("Kimi Device Flow polling failed: {error}"))?;
        let status = response.status();
        let token = decode_bounded_oauth_json_response::<KimiTokenResponse>(
            response,
            "Kimi Device Flow token response",
        )
        .await?;
        match token.error.as_str() {
            "authorization_pending" | "slow_down" => continue,
            "expired_token" => {
                return Err(String::from("Kimi Device Flow authorization expired."));
            }
            "access_denied" => {
                return Err(String::from("Kimi Device Flow authorization was denied."));
            }
            "" => {}
            _ => {
                return Err(format!(
                    "Kimi Device Flow token exchange failed with `{}`: {}",
                    token.error,
                    controlled_oauth_error(token.error_description.as_str())
                ));
            }
        }
        if !status.is_success() {
            return Err(format!(
                "Kimi Device Flow token exchange returned HTTP {}.",
                status.as_u16()
            ));
        }
        set_transaction_phase(
            transactions,
            transaction_id,
            OAuthTransactionPhase::ExchangingToken,
        )
        .await;
        return build_kimi_credential_material(token, flow.device_id.as_str());
    }
    Err(String::from("Kimi Device Flow authorization timed out."))
}

/// Builds validated Kimi Coding Plan secret material from one successful exchange.
/// 从一次成功交换构建已校验的 Kimi Coding Plan 秘密材料。
///
/// # Parameters
/// 参数
/// `token` is the provider response and `device_id` is the identity bound to this flow.
/// `token` 是供应商响应，`device_id` 是绑定到本次流程的身份。
///
/// # Returns
/// 返回值
/// Complete material retaining the device identity required by subsequent calls.
/// 返回保留后续调用所需设备身份的完整材料。
fn build_kimi_credential_material(
    token: KimiTokenResponse,
    device_id: &str,
) -> Result<CredentialSecretMaterial, String> {
    if token.access_token.trim().is_empty() || token.refresh_token.trim().is_empty() {
        return Err(String::from(
            "Kimi OAuth response omitted renewable token material.",
        ));
    }
    let issued_at_unix_ms = unix_millis()?;
    let expires_at_unix_ms = kimi_token_expiration(issued_at_unix_ms, token.expires_in)?;
    let tokens = OAuthTokenSet {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        issued_at_unix_ms,
        expires_at_unix_ms,
        scopes: token
            .scope
            .split_ascii_whitespace()
            .map(str::to_string)
            .collect(),
        account_subject: None,
        email: None,
    };
    tokens.validate()?;
    let material = CredentialSecretMaterial::KimiCodingPlanOAuth {
        tokens,
        device_id: device_id.to_string(),
    };
    CredentialSecretEnvelope::new(material.clone())?;
    Ok(material)
}

/// Refreshes one Kimi Coding Plan token with its original device identity.
/// 使用原始设备身份刷新一个 Kimi Coding Plan Token。
async fn refresh_kimi_oauth_token(
    client: &Client,
    tokens: &OAuthTokenSet,
    device_id: &str,
) -> Result<KimiTokenResponse, String> {
    let response = client
        .post(KIMI_TOKEN_URL)
        .form(&[
            ("client_id", KIMI_CODING_CLIENT_ID),
            ("refresh_token", tokens.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .headers(kimi_device_headers(device_id)?)
        .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("Kimi OAuth refresh failed: {error}"))?;
    let status = response.status();
    let token = decode_bounded_oauth_json_response::<KimiTokenResponse>(
        response,
        "Kimi OAuth refresh response",
    )
    .await?;
    if !status.is_success() || !token.error.is_empty() || token.access_token.trim().is_empty() {
        return Err(String::from(
            "Kimi OAuth session cannot be refreshed; reauthorization is required.",
        ));
    }
    Ok(token)
}

/// Merges a Kimi refresh response while preserving an unrotated refresh token and account metadata.
/// 合并 Kimi 刷新响应，同时保留未轮换的 Refresh Token 与账号元数据。
fn merge_kimi_token_set(
    current: &OAuthTokenSet,
    refreshed: &KimiTokenResponse,
) -> Result<OAuthTokenSet, String> {
    let issued_at_unix_ms = unix_millis()?;
    let merged = OAuthTokenSet {
        access_token: refreshed.access_token.clone(),
        refresh_token: if refreshed.refresh_token.trim().is_empty() {
            current.refresh_token.clone()
        } else {
            refreshed.refresh_token.clone()
        },
        issued_at_unix_ms,
        expires_at_unix_ms: kimi_token_expiration(issued_at_unix_ms, refreshed.expires_in)?,
        scopes: if refreshed.scope.trim().is_empty() {
            current.scopes.clone()
        } else {
            refreshed
                .scope
                .split_ascii_whitespace()
                .map(str::to_string)
                .collect()
        },
        account_subject: current.account_subject.clone(),
        email: current.email.clone(),
    };
    merged.validate()?;
    Ok(merged)
}

/// Converts Kimi's positive fractional lifetime to a checked Unix-millisecond deadline.
/// 将 Kimi 的正小数有效期转换为经过检查的 Unix 毫秒截止时间。
fn kimi_token_expiration(issued_at_unix_ms: u64, expires_in: f64) -> Result<u64, String> {
    if !expires_in.is_finite() || expires_in <= 0.0 {
        return Err(String::from("Kimi OAuth token expiration is invalid."));
    }
    let lifetime_ms = (expires_in * 1_000.0).trunc();
    if lifetime_ms > u64::MAX as f64 {
        return Err(String::from("Kimi OAuth token expiration overflowed."));
    }
    issued_at_unix_ms
        .checked_add(lifetime_ms as u64)
        .ok_or_else(|| String::from("Kimi OAuth token expiration overflowed."))
}

/// Builds Kimi's verified device identity headers.
/// 构建 Kimi 已验证的设备身份请求头。
fn kimi_device_headers(device_id: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| String::from("unknown"));
    let device_model = kimi_device_model();
    for (name, value) in [
        ("x-msh-platform", "CLIProxyAPI"),
        ("x-msh-version", "dev"),
        ("x-msh-device-name", hostname.as_str()),
        ("x-msh-device-model", device_model.as_str()),
        ("x-msh-device-id", device_id),
    ] {
        headers.insert(
            reqwest::header::HeaderName::from_static(name),
            HeaderValue::from_str(value)
                .map_err(|_| String::from("Kimi OAuth device identity is invalid."))?,
        );
    }
    Ok(headers)
}

/// Preserves the operating-system labels accepted by Kimi's device flow.
/// 保留 Kimi 设备授权流程接受的操作系统标签。
fn kimi_device_model() -> String {
    let operating_system = match std::env::consts::OS {
        "darwin" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    };
    format!("{operating_system} {}", std::env::consts::ARCH)
}

/// Accepts credential-free standard-port HTTPS URLs on the Kimi main domain or its subdomains.
/// 接受 kimi.com 主域或其子域上不含凭据且使用标准端口的 HTTPS URL。
fn validate_kimi_verification_uri(value: &str) -> Result<String, String> {
    let url = Url::parse(value)
        .map_err(|_| String::from("Kimi Device Flow returned an invalid verification URL."))?;
    let is_kimi_domain = match url.host_str() {
        Some(host) => {
            let normalized_host = host.to_ascii_lowercase();
            normalized_host == "kimi.com" || normalized_host.ends_with(".kimi.com")
        }
        None => false,
    };
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || !is_kimi_domain
    {
        return Err(String::from(
            "Kimi Device Flow returned an untrusted verification URL.",
        ));
    }
    Ok(url.to_string())
}

/// Bounds one provider OAuth error detail before displaying it.
/// 在显示供应商 OAuth 错误详情前限制其长度。
fn controlled_oauth_error(value: &str) -> String {
    value.chars().take(240).collect()
}

/// Builds validated MiniMax secret material from a successful token response.
/// 从成功的 Token 响应构建已校验 MiniMax 秘密材料。
///
/// # Parameters
/// 参数
/// `region` is the fixed authority region and `token` is the successful response.
/// `region` 是固定授权区域，`token` 是成功响应。
///
/// # Returns
/// 返回值
/// Complete renewable material with an exact trusted resource URL.
/// 返回包含精确可信资源 URL 的完整可续期材料。
fn build_minimax_credential_material(
    region: MiniMaxRegion,
    token: MiniMaxTokenResponse,
) -> Result<CredentialSecretMaterial, String> {
    let issued_at_unix_ms = unix_millis()?;
    let expires_at_unix_ms = token
        .expired_in
        .ok_or_else(|| String::from("MiniMax OAuth response omitted token expiration."))?;
    let access_token = token
        .access_token
        .ok_or_else(|| String::from("MiniMax OAuth response omitted its access token."))?;
    let refresh_token = token
        .refresh_token
        .ok_or_else(|| String::from("MiniMax OAuth response omitted its refresh token."))?;
    let resource_url = token
        .resource_url
        .unwrap_or_else(|| minimax_api_url(region).to_string());
    validate_minimax_resource_origin(region, resource_url.as_str())?;
    let tokens = OAuthTokenSet {
        access_token,
        refresh_token,
        issued_at_unix_ms,
        expires_at_unix_ms,
        scopes: BTreeSet::from([
            String::from("openid"),
            String::from("profile"),
            String::from("coding_plan"),
        ]),
        account_subject: None,
        email: None,
    };
    tokens.validate()?;
    Ok(CredentialSecretMaterial::MiniMaxOAuth {
        tokens,
        region,
        resource_url,
    })
}

/// Returns the reviewed MiniMax API origin for one fixed region.
/// 返回一个固定区域对应的已审核 MiniMax API Origin。
fn minimax_api_url(region: MiniMaxRegion) -> &'static str {
    match region {
        MiniMaxRegion::Global => MINIMAX_GLOBAL_API_URL,
        MiniMaxRegion::China => MINIMAX_CHINA_API_URL,
    }
}

/// Validates a provider-returned MiniMax resource URL as an exact regional API origin.
/// 将供应商返回的 MiniMax 资源 URL 校验为精确区域 API Origin。
fn validate_minimax_resource_origin(
    region: MiniMaxRegion,
    resource_url: &str,
) -> Result<(), String> {
    if resource_url.trim_end_matches('/') != minimax_api_url(region) {
        return Err(String::from(
            "MiniMax OAuth returned an untrusted regional resource URL.",
        ));
    }
    Ok(())
}

/// Refreshes MiniMax OAuth material with bounded transport-only retries.
/// 使用有界且仅限传输错误的重试刷新 MiniMax OAuth 材料。
///
/// # Parameters
/// 参数
/// `client` owns the HTTP pool, `region` fixes the authority, and `tokens` supplies the refresh token.
/// `client` 持有 HTTP 连接池，`region` 固定授权端，`tokens` 提供 Refresh Token。
///
/// # Returns
/// 返回值
/// The exact successful MiniMax token response.
/// 返回精确的成功 MiniMax Token 响应。
async fn refresh_minimax_oauth_token(
    client: &Client,
    region: MiniMaxRegion,
    tokens: &OAuthTokenSet,
) -> Result<MiniMaxTokenResponse, String> {
    let account_url = match region {
        MiniMaxRegion::Global => MINIMAX_GLOBAL_ACCOUNT_URL,
        MiniMaxRegion::China => MINIMAX_CHINA_ACCOUNT_URL,
    };
    let mut last_failure = String::from("MiniMax OAuth refresh did not start.");
    for attempt in 0..MINIMAX_OAUTH_MAX_REQUEST_ATTEMPTS {
        if attempt > 0 {
            sleep(Duration::from_millis(500 * attempt as u64)).await;
        }
        let response = client
            .post(format!("{account_url}/oauth2/token"))
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", tokens.refresh_token.as_str()),
                ("client_id", MINIMAX_OAUTH_CLIENT_ID),
            ])
            .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                last_failure = format!("MiniMax OAuth refresh transport failed: {error}");
                continue;
            }
        };
        let status = response.status();
        if status.is_client_error() {
            return Err(String::from(
                "MiniMax OAuth session cannot be refreshed; reauthorization is required.",
            ));
        }
        if !status.is_success() {
            last_failure = format!("MiniMax OAuth refresh returned HTTP {}.", status.as_u16());
            continue;
        }
        let token = decode_bounded_oauth_json_response::<MiniMaxTokenResponse>(
            response,
            "MiniMax OAuth refresh response",
        )
        .await?;
        validate_minimax_business_status(token.base_resp.as_ref(), "OAuth refresh")?;
        if token.status != "success" || token.access_token.is_none() {
            return Err(String::from(
                "MiniMax OAuth session cannot be refreshed; reauthorization is required.",
            ));
        }
        return Ok(token);
    }
    Err(format!(
        "MiniMax OAuth refresh failed after {MINIMAX_OAUTH_MAX_REQUEST_ATTEMPTS} attempts: {last_failure}"
    ))
}

/// Merges a successful MiniMax refresh response into the existing token identity.
/// 将成功的 MiniMax 刷新响应合并到现有 Token 身份中。
///
/// # Parameters
/// 参数
/// `current` is the stored token set and `refreshed` is the provider response.
/// `current` 是已存 Token 集合，`refreshed` 是供应商响应。
///
/// # Returns
/// 返回值
/// A complete replacement token set preserving account metadata and unrotated refresh tokens.
/// 返回保留账号元数据及未轮换 Refresh Token 的完整替换 Token 集合。
fn merge_minimax_refreshed_token_set(
    current: &OAuthTokenSet,
    refreshed: &MiniMaxTokenResponse,
) -> Result<OAuthTokenSet, String> {
    let issued_at_unix_ms = unix_millis()?;
    let tokens = OAuthTokenSet {
        access_token: refreshed
            .access_token
            .clone()
            .ok_or_else(|| String::from("MiniMax OAuth refresh omitted its access token."))?,
        refresh_token: refreshed
            .refresh_token
            .clone()
            .unwrap_or_else(|| current.refresh_token.clone()),
        issued_at_unix_ms,
        expires_at_unix_ms: refreshed
            .expired_in
            .ok_or_else(|| String::from("MiniMax OAuth refresh omitted token expiration."))?,
        scopes: current.scopes.clone(),
        account_subject: current.account_subject.clone(),
        email: current.email.clone(),
    };
    tokens.validate()?;
    Ok(tokens)
}

async fn bind_callback_listener(provider_kind: OAuthProviderKind) -> Result<TcpListener, String> {
    let address = match provider_kind {
        OAuthProviderKind::OpenAiCodex => "127.0.0.1:1455",
        OAuthProviderKind::ClaudeCode => "127.0.0.1:54545",
        OAuthProviderKind::GoogleCodeAssist | OAuthProviderKind::GoogleGemini => "127.0.0.1:0",
        OAuthProviderKind::MiniMaxGlobalTokenPlan | OAuthProviderKind::MiniMaxChinaTokenPlan => {
            return Err(String::from(
                "MiniMax Device Flow does not use a loopback listener.",
            ));
        }
        OAuthProviderKind::KimiCodingPlan => {
            return Err(String::from(
                "Kimi Device Flow does not use a loopback listener.",
            ));
        }
    };
    TcpListener::bind(address)
        .await
        .map_err(|error| format!("Failed to bind OAuth loopback listener `{address}`: {error}"))
}

fn callback_redirect_uri(
    provider_kind: OAuthProviderKind,
    listener: &TcpListener,
) -> Result<String, String> {
    match provider_kind {
        OAuthProviderKind::OpenAiCodex => Ok(String::from(OPENAI_CODEX_REDIRECT_URI)),
        OAuthProviderKind::ClaudeCode => Ok(String::from(CLAUDE_CODE_REDIRECT_URI)),
        OAuthProviderKind::GoogleCodeAssist | OAuthProviderKind::GoogleGemini => {
            let port = listener
                .local_addr()
                .map_err(|error| format!("Failed to inspect OAuth loopback listener: {error}"))?
                .port();
            Ok(format!("http://127.0.0.1:{port}/oauth/callback"))
        }
        OAuthProviderKind::MiniMaxGlobalTokenPlan | OAuthProviderKind::MiniMaxChinaTokenPlan => {
            Err(String::from(
                "MiniMax Device Flow does not use a callback redirect URI.",
            ))
        }
        OAuthProviderKind::KimiCodingPlan => Err(String::from(
            "Kimi Device Flow does not use a callback redirect URI.",
        )),
    }
}

fn build_oauth_flow(
    provider_kind: OAuthProviderKind,
    client: OAuthClientConfiguration,
    redirect_uri: String,
    state: String,
    verifier: String,
) -> Result<OAuthFlow, String> {
    let challenge = build_pkce_challenge(verifier.as_str());
    let (
        client_id,
        client_secret,
        project_id,
        authorization_endpoint,
        token_url,
        scopes,
        body_kind,
    ) = match provider_kind {
        OAuthProviderKind::OpenAiCodex => (
            String::from(OPENAI_CODEX_CLIENT_ID),
            None,
            None,
            OPENAI_CODEX_AUTHORIZATION_URL,
            OPENAI_CODEX_TOKEN_URL,
            BTreeSet::from([
                String::from("openid"),
                String::from("email"),
                String::from("profile"),
                String::from("offline_access"),
            ]),
            OAuthTokenBodyKind::Form,
        ),
        OAuthProviderKind::ClaudeCode => (
            String::from(CLAUDE_CODE_CLIENT_ID),
            None,
            None,
            CLAUDE_CODE_AUTHORIZATION_URL,
            CLAUDE_CODE_TOKEN_URL,
            BTreeSet::from([
                String::from("user:profile"),
                String::from("user:inference"),
                String::from("user:sessions:claude_code"),
                String::from("user:mcp_servers"),
                String::from("user:file_upload"),
            ]),
            OAuthTokenBodyKind::JsonWithState,
        ),
        OAuthProviderKind::GoogleCodeAssist => (
            required_client_id(client.client_id)?,
            normalized_optional_secret(client.client_secret)?,
            None,
            GOOGLE_AUTHORIZATION_URL,
            GOOGLE_TOKEN_URL,
            BTreeSet::from([
                String::from("openid"),
                String::from("email"),
                String::from("https://www.googleapis.com/auth/cloud-platform"),
            ]),
            OAuthTokenBodyKind::Form,
        ),
        OAuthProviderKind::GoogleGemini => (
            required_client_id(client.client_id)?,
            normalized_optional_secret(client.client_secret)?,
            Some(required_google_project_id(client.project_id)?),
            GOOGLE_AUTHORIZATION_URL,
            GOOGLE_TOKEN_URL,
            BTreeSet::from([
                String::from("openid"),
                String::from("email"),
                String::from("https://www.googleapis.com/auth/cloud-platform"),
                String::from("https://www.googleapis.com/auth/generative-language.retriever"),
            ]),
            OAuthTokenBodyKind::Form,
        ),
        OAuthProviderKind::MiniMaxGlobalTokenPlan | OAuthProviderKind::MiniMaxChinaTokenPlan => {
            return Err(String::from(
                "MiniMax Device Flow must be constructed by its dedicated runtime.",
            ));
        }
        OAuthProviderKind::KimiCodingPlan => {
            return Err(String::from(
                "Kimi Device Flow must be constructed by its dedicated runtime.",
            ));
        }
    };
    let mut authorization_url = Url::parse(authorization_endpoint)
        .map_err(|error| format!("Invalid OAuth authorization endpoint: {error}"))?;
    {
        let mut query = authorization_url.query_pairs_mut();
        query
            .append_pair("client_id", client_id.as_str())
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", redirect_uri.as_str())
            .append_pair(
                "scope",
                scopes
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .as_str(),
            )
            .append_pair("state", state.as_str())
            .append_pair("code_challenge", challenge.as_str())
            .append_pair("code_challenge_method", "S256");
        match provider_kind {
            OAuthProviderKind::OpenAiCodex => {
                query
                    .append_pair("prompt", "login")
                    .append_pair("id_token_add_organizations", "true")
                    .append_pair("codex_cli_simplified_flow", "true");
            }
            OAuthProviderKind::GoogleCodeAssist | OAuthProviderKind::GoogleGemini => {
                query
                    .append_pair("access_type", "offline")
                    .append_pair("prompt", "consent");
            }
            OAuthProviderKind::ClaudeCode => {}
            OAuthProviderKind::MiniMaxGlobalTokenPlan
            | OAuthProviderKind::MiniMaxChinaTokenPlan => {
                return Err(String::from(
                    "MiniMax Device Flow cannot use an authorization-code URL.",
                ));
            }
            OAuthProviderKind::KimiCodingPlan => {
                return Err(String::from(
                    "Kimi Device Flow cannot use an authorization-code URL.",
                ));
            }
        }
    }
    Ok(OAuthFlow {
        provider_kind,
        client_id,
        client_secret,
        project_id,
        authorization_url,
        token_url: token_url.to_string(),
        redirect_uri,
        state,
        verifier,
        scopes,
        token_body_kind: body_kind,
    })
}

fn required_client_id(value: Option<String>) -> Result<String, String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("Google OAuth requires a desktop client id."))
}

fn normalized_optional_secret(value: Option<String>) -> Result<Option<String>, String> {
    match value {
        Some(value) if value.trim().is_empty() => Err(String::from(
            "Google OAuth client secret must be absent or non-empty.",
        )),
        Some(value) => Ok(Some(value)),
        None => Ok(None),
    }
}

/// Requires one non-empty Google Cloud project for public Gemini OAuth billing and quota.
/// 要求一个非空 Google Cloud 项目用于公共 Gemini OAuth 计费与配额。
///
/// # Parameters
/// 参数
/// `value` is the user-supplied project identifier from the desktop authorization form.
/// `value` 是桌面授权表单中用户提供的项目标识。
///
/// # Returns
/// 返回值
/// The normalized project identifier or a controlled configuration failure.
/// 返回规范化项目标识或受控配置错误。
fn required_google_project_id(value: Option<String>) -> Result<String, String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("Public Gemini OAuth requires a Google Cloud project id."))
}

fn build_pkce_verifier() -> String {
    format!(
        "{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn build_pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

async fn receive_and_exchange_callback(
    listener: TcpListener,
    flow: &OAuthFlow,
    client: &Client,
    transactions: &Arc<Mutex<BTreeMap<String, OAuthTransaction>>>,
    transaction_id: &str,
) -> Result<CredentialSecretMaterial, String> {
    let (mut stream, _) = timeout(OAUTH_TRANSACTION_TIMEOUT, listener.accept())
        .await
        .map_err(|_| String::from("OAuth login timed out."))?
        .map_err(|error| format!("OAuth callback listener failed: {error}"))?;
    let callback = read_callback(&mut stream)
        .await
        .and_then(|(code, state)| validate_callback_state(code, state, flow.state.as_str()));
    let response = callback_html(callback.is_ok());
    let _ = write_callback_response(&mut stream, response).await;
    let (code, _) = callback?;
    set_transaction_phase(
        transactions,
        transaction_id,
        OAuthTransactionPhase::ExchangingToken,
    )
    .await;
    exchange_authorization_code(client, flow, code.as_str()).await
}

/// Validates callback State before the browser receives a success response.
/// 在浏览器收到成功响应前校验回调 State。
///
/// # Parameters
/// 参数
/// `code` and `returned_state` came from the callback; `expected_state` belongs to the
/// exact in-memory transaction.
/// `code` 与 `returned_state` 来自回调；`expected_state` 属于精确的内存事务。
///
/// # Returns
/// 返回值
/// The unchanged callback values only when State matches exactly.
/// 仅当 State 精确匹配时返回未改变的回调值。
fn validate_callback_state(
    code: String,
    returned_state: String,
    expected_state: &str,
) -> Result<(String, String), String> {
    if returned_state != expected_state {
        return Err(String::from("OAuth callback state did not match."));
    }
    Ok((code, returned_state))
}

async fn read_callback(stream: &mut TcpStream) -> Result<(String, String), String> {
    let mut buffer = vec![0_u8; OAUTH_CALLBACK_MAX_BYTES];
    let mut length = 0_usize;
    loop {
        if length == buffer.len() {
            return Err(String::from("OAuth callback exceeded the size limit."));
        }
        let read = stream
            .read(&mut buffer[length..])
            .await
            .map_err(|error| format!("Failed to read OAuth callback: {error}"))?;
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
    parse_callback_request(&buffer[..length])
}

/// Parses one complete loopback HTTP request without retaining request headers.
/// 解析一个完整回环 HTTP 请求且不保留请求头。
///
/// # Parameters
/// 参数
/// `request_bytes` contains the bounded bytes read through the loopback listener.
/// `request_bytes` 包含通过回环监听器读取的有界字节。
///
/// # Returns
/// 返回值
/// The authorization code and State value, or a controlled validation error.
/// 返回授权码与 State 值，或受控校验错误。
fn parse_callback_request(request_bytes: &[u8]) -> Result<(String, String), String> {
    if request_bytes.is_empty() {
        return Err(String::from("OAuth callback was empty."));
    }
    let request = std::str::from_utf8(request_bytes)
        .map_err(|_| String::from("OAuth callback was not valid UTF-8."))?;
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| String::from("OAuth callback request line was invalid."))?;
    let callback_url = Url::parse(format!("http://127.0.0.1{target}").as_str())
        .map_err(|_| String::from("OAuth callback URL was invalid."))?;
    let query = callback_url.query_pairs().collect::<BTreeMap<_, _>>();
    if let Some(error) = query.get("error") {
        return Err(format!("OAuth provider rejected authorization: {error}."));
    }
    let code = query
        .get("code")
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("OAuth callback did not contain an authorization code."))?;
    let state = query
        .get("state")
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("OAuth callback did not contain state."))?;
    Ok((code, state))
}

fn callback_html(succeeded: bool) -> &'static str {
    if succeeded {
        "<!doctype html><meta charset=\"utf-8\"><title>Vulcan Code</title><p>Authorization received. You can close this window and return to Vulcan Code.</p>"
    } else {
        "<!doctype html><meta charset=\"utf-8\"><title>Vulcan Code</title><p>Authorization failed. Return to Vulcan Code for details.</p>"
    }
}

async fn write_callback_response(stream: &mut TcpStream, body: &str) -> Result<(), String> {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .await
        .map_err(|error| format!("Failed to write OAuth callback response: {error}"))
}

async fn exchange_authorization_code(
    client: &Client,
    flow: &OAuthFlow,
    code: &str,
) -> Result<CredentialSecretMaterial, String> {
    let response = match flow.token_body_kind {
        OAuthTokenBodyKind::Form => {
            let mut form = BTreeMap::from([
                ("grant_type", String::from("authorization_code")),
                ("client_id", flow.client_id.clone()),
                ("code", code.to_string()),
                ("redirect_uri", flow.redirect_uri.clone()),
                ("code_verifier", flow.verifier.clone()),
            ]);
            if let Some(secret) = &flow.client_secret {
                form.insert("client_secret", secret.clone());
            }
            client
                .post(flow.token_url.as_str())
                .form(&form)
                .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
                .send()
                .await
        }
        OAuthTokenBodyKind::JsonWithState => {
            client
                .post(flow.token_url.as_str())
                .json(&serde_json::json!({
                    "grant_type": "authorization_code",
                    "client_id": flow.client_id,
                    "code": code,
                    "redirect_uri": flow.redirect_uri,
                    "code_verifier": flow.verifier,
                    "state": flow.state,
                }))
                .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
                .send()
                .await
        }
    }
    .map_err(|error| format!("OAuth token exchange request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "OAuth token exchange failed with HTTP status {}.",
            status.as_u16()
        ));
    }
    let token =
        decode_bounded_oauth_json_response::<OAuthTokenResponse>(response, "OAuth token response")
            .await?;
    let issued_at_unix_ms = unix_millis()?;
    let expires_at_unix_ms = issued_at_unix_ms
        .checked_add(
            token
                .expires_in
                .checked_mul(1_000)
                .ok_or_else(|| String::from("OAuth token expiration overflowed."))?,
        )
        .ok_or_else(|| String::from("OAuth token expiration overflowed."))?;
    let scopes = token
        .scope
        .as_deref()
        .map(|value| {
            value
                .split_whitespace()
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| flow.scopes.clone());
    let codex_identity = match (flow.provider_kind, token.id_token.as_deref()) {
        (OAuthProviderKind::OpenAiCodex, Some(id_token)) => {
            Some(parse_openai_codex_identity(id_token)?)
        }
        (OAuthProviderKind::OpenAiCodex, None) => {
            return Err(String::from(
                "OpenAI Codex token response did not contain an identity token.",
            ));
        }
        _ => None,
    };
    let refresh_token = token.refresh_token.ok_or_else(|| {
        String::from("OAuth authorization response did not contain a refresh token.")
    })?;
    let tokens = OAuthTokenSet {
        access_token: token.access_token,
        refresh_token,
        issued_at_unix_ms,
        expires_at_unix_ms,
        scopes,
        account_subject: None,
        email: codex_identity
            .as_ref()
            .and_then(|claims| claims.email.clone()),
    };
    tokens.validate()?;
    Ok(match flow.provider_kind {
        OAuthProviderKind::OpenAiCodex => CredentialSecretMaterial::OpenAiCodexOAuth {
            tokens,
            id_token: token.id_token,
            account_id: codex_identity.map(|claims| claims.authentication.chatgpt_account_id),
        },
        OAuthProviderKind::ClaudeCode => CredentialSecretMaterial::ClaudeCodeOAuth { tokens },
        OAuthProviderKind::GoogleCodeAssist => {
            let project_id =
                load_google_code_assist_project(client, tokens.access_token.as_str()).await?;
            CredentialSecretMaterial::GoogleOAuth {
                tokens,
                client_id: flow.client_id.clone(),
                client_secret: flow.client_secret.clone(),
                project_id: Some(project_id),
                quota_project_id: None,
            }
        }
        OAuthProviderKind::GoogleGemini => {
            let project_id = flow.project_id.clone().ok_or_else(|| {
                String::from("Public Gemini OAuth lost its validated Google Cloud project id.")
            })?;
            CredentialSecretMaterial::GoogleOAuth {
                tokens,
                client_id: flow.client_id.clone(),
                client_secret: flow.client_secret.clone(),
                project_id: Some(project_id.clone()),
                quota_project_id: Some(project_id),
            }
        }
        OAuthProviderKind::MiniMaxGlobalTokenPlan | OAuthProviderKind::MiniMaxChinaTokenPlan => {
            return Err(String::from(
                "MiniMax Device Flow cannot use callback token exchange.",
            ));
        }
        OAuthProviderKind::KimiCodingPlan => {
            return Err(String::from(
                "Kimi Device Flow cannot use callback token exchange.",
            ));
        }
    })
}

/// Loads the project bound to one authenticated Google Code Assist account.
/// 加载绑定到一个已认证 Google Code Assist 账号的项目。
///
/// # Parameters
/// 参数
/// `client` owns the HTTP pool and `access_token` is the freshly issued OAuth access token.
/// `client` 持有 HTTP 连接池，`access_token` 是刚签发的 OAuth Access Token。
///
/// # Returns
/// 返回值
/// The exact non-empty project identifier required by Code Assist execution.
/// 返回 Code Assist 执行所需的精确非空项目标识。
async fn load_google_code_assist_project(
    client: &Client,
    access_token: &str,
) -> Result<String, String> {
    let response = client
        .post(GOOGLE_CODE_ASSIST_LOAD_URL)
        .bearer_auth(access_token)
        .header("accept", "*/*")
        .header("user-agent", "vulcan-code/0.1")
        .json(&serde_json::json!({
            "metadata": {
                "ideType": "ANTIGRAVITY"
            }
        }))
        .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("Google Code Assist account validation failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Google Code Assist account validation returned HTTP {}.",
            status.as_u16()
        ));
    }
    let body = decode_bounded_oauth_json_response::<GoogleCodeAssistLoadResponse>(
        response,
        "Google Code Assist account response",
    )
    .await?;
    [
        body.cloudaicompanion_project,
        body.project_id,
        body.project,
    ]
    .into_iter()
    .flatten()
    .map(|project| match project {
        GoogleCodeAssistProjectReference::Id(id)
        | GoogleCodeAssistProjectReference::Object { id } => id,
    })
    .map(|id| id.trim().to_string())
    .find(|id| !id.is_empty())
    .ok_or_else(|| {
        String::from(
            "Google Code Assist account has no project; complete account onboarding before connecting it.",
        )
    })
}

/// Refreshes one exact provider token set using its reviewed request encoding.
/// 使用已审核的请求编码刷新一组精确供应商 Token。
///
/// # Parameters
/// 参数
/// The endpoint, body kind, client identity, and current token set are fixed by the
/// selected credential material; `client_secret` is supplied only by user-owned Google clients.
/// 端点、正文类别、客户端身份与当前 Token 集由所选凭据材料固定；
/// `client_secret` 仅由用户自有 Google 客户端提供。
///
/// # Returns
/// 返回值
/// A decoded provider token response whose missing refresh token is handled by the caller.
/// 返回已解码的供应商 Token 响应；缺失的 Refresh Token 由调用方处理。
async fn refresh_oauth_token(
    client: &Client,
    token_url: &str,
    body_kind: OAuthTokenBodyKind,
    client_id: &str,
    client_secret: Option<&str>,
    tokens: &OAuthTokenSet,
) -> Result<OAuthTokenResponse, String> {
    let response = match body_kind {
        OAuthTokenBodyKind::Form => {
            let mut form = BTreeMap::from([
                ("grant_type", String::from("refresh_token")),
                ("client_id", client_id.to_string()),
                ("refresh_token", tokens.refresh_token.clone()),
            ]);
            if let Some(client_secret) = client_secret {
                form.insert("client_secret", client_secret.to_string());
            }
            client
                .post(token_url)
                .form(&form)
                .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
                .send()
                .await
        }
        OAuthTokenBodyKind::JsonWithState => {
            client
                .post(token_url)
                .json(&serde_json::json!({
                    "grant_type": "refresh_token",
                    "client_id": client_id,
                    "refresh_token": tokens.refresh_token,
                }))
                .timeout(OAUTH_HTTP_REQUEST_TIMEOUT)
                .send()
                .await
        }
    }
    .map_err(|error| format!("OAuth token refresh request failed: {error}"))?;
    let status = response.status();
    let body = read_bounded_oauth_response(response, "OAuth token refresh response").await?;
    if !status.is_success() {
        let error_id = decode_bounded_oauth_json::<OAuthErrorResponse>(
            body.as_slice(),
            "OAuth token refresh error response",
        )
        .ok()
        .and_then(|error| error.error);
        if status.as_u16() == 400
            || status.as_u16() == 401
            || error_id.as_deref() == Some("invalid_grant")
        {
            return Err(String::from(
                "OAuth credential can no longer be refreshed; reauthorization is required.",
            ));
        }
        return Err(format!(
            "OAuth token refresh failed with HTTP status {}.",
            status.as_u16()
        ));
    }
    decode_bounded_oauth_json::<OAuthTokenResponse>(body.as_slice(), "OAuth token refresh response")
}

/// Merges one refresh response with stable identity and refresh-token fields.
/// 将一次刷新响应与稳定身份及 Refresh Token 字段合并。
///
/// # Parameters
/// 参数
/// `current` supplies stable fields and `refreshed` supplies newly issued token data.
/// `current` 提供稳定字段，`refreshed` 提供新签发的 Token 数据。
///
/// # Returns
/// 返回值
/// A validated complete token set with a newly computed expiration.
/// 返回带有新计算过期时间的完整已验证 Token 集。
fn merge_refreshed_token_set(
    current: &OAuthTokenSet,
    refreshed: &OAuthTokenResponse,
) -> Result<OAuthTokenSet, String> {
    let issued_at_unix_ms = unix_millis()?;
    let expires_at_unix_ms = issued_at_unix_ms
        .checked_add(
            refreshed
                .expires_in
                .checked_mul(1_000)
                .ok_or_else(|| String::from("OAuth token expiration overflowed."))?,
        )
        .ok_or_else(|| String::from("OAuth token expiration overflowed."))?;
    let scopes = refreshed
        .scope
        .as_deref()
        .map(|value| {
            value
                .split_whitespace()
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| current.scopes.clone());
    let tokens = OAuthTokenSet {
        access_token: refreshed.access_token.clone(),
        refresh_token: refreshed
            .refresh_token
            .clone()
            .unwrap_or_else(|| current.refresh_token.clone()),
        issued_at_unix_ms,
        expires_at_unix_ms,
        scopes,
        account_subject: current.account_subject.clone(),
        email: current.email.clone(),
    };
    tokens.validate()?;
    Ok(tokens)
}

fn parse_openai_codex_identity(id_token: &str) -> Result<OpenAiCodexIdentityClaims, String> {
    let payload = id_token
        .split('.')
        .nth(1)
        .ok_or_else(|| String::from("OpenAI Codex identity token format was invalid."))?;
    let decoded = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| String::from("OpenAI Codex identity token payload was invalid."))?;
    let claims = serde_json::from_slice::<OpenAiCodexIdentityClaims>(&decoded)
        .map_err(|_| String::from("OpenAI Codex identity claims were invalid."))?;
    if claims.authentication.chatgpt_account_id.trim().is_empty() {
        return Err(String::from(
            "OpenAI Codex identity claims did not contain an account id.",
        ));
    }
    Ok(claims)
}

async fn set_transaction_phase(
    transactions: &Arc<Mutex<BTreeMap<String, OAuthTransaction>>>,
    transaction_id: &str,
    phase: OAuthTransactionPhase,
) {
    let mut transactions = transactions.lock().await;
    if let Some(transaction) = transactions.get_mut(transaction_id)
        && transaction.snapshot.phase != OAuthTransactionPhase::Cancelled
    {
        transaction.snapshot.phase = phase;
        transaction.snapshot.message = None;
    }
}

async fn finish_transaction(
    transactions: Arc<Mutex<BTreeMap<String, OAuthTransaction>>>,
    transaction_id: &str,
    result: Result<CredentialSecretMaterial, String>,
) {
    let mut transactions = transactions.lock().await;
    let Some(transaction) = transactions.get_mut(transaction_id) else {
        return;
    };
    if transaction.snapshot.phase == OAuthTransactionPhase::Cancelled {
        return;
    }
    match result {
        Ok(material) => {
            transaction.material = Some(material);
            transaction.snapshot.phase = OAuthTransactionPhase::Succeeded;
            transaction.snapshot.message = None;
        }
        Err(error) => {
            transaction.material = None;
            transaction.snapshot.phase = OAuthTransactionPhase::Failed;
            transaction.snapshot.message = Some(error);
        }
    }
}

/// Reads one OAuth response body without retaining bytes beyond the shared credential boundary.
/// 在不保留超过共享凭据边界字节的前提下读取一份 OAuth 响应体。
///
/// # Parameters
/// 参数
/// `response` is already covered by the request-level deadline and `label` scopes diagnostics.
/// `response` 已受请求级截止时间约束，`label` 限定诊断范围。
///
/// # Returns
/// 返回值
/// Exact bytes within the hard cap, or a controlled transport or size failure without body data.
/// 返回硬上限内的精确字节，或不包含正文数据的受控传输或大小失败。
async fn read_bounded_oauth_response(
    mut response: reqwest::Response,
    label: &str,
) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > OAUTH_RESPONSE_MAX_BYTES as u64)
    {
        return Err(format!(
            "{label} exceeds the {OAUTH_RESPONSE_MAX_BYTES} byte limit."
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("{label} could not be read: {error}"))?
    {
        if body.len().saturating_add(chunk.len()) > OAUTH_RESPONSE_MAX_BYTES {
            return Err(format!(
                "{label} exceeds the {OAUTH_RESPONSE_MAX_BYTES} byte limit."
            ));
        }
        body.extend_from_slice(chunk.as_ref());
    }
    Ok(body)
}

/// Decodes one byte-bounded OAuth JSON body under the shared depth and node limits.
/// 在共享深度与节点数量限制下解码一份受字节约束的 OAuth JSON 正文。
///
/// # Parameters
/// 参数
/// `body` is exact response data and `label` scopes non-secret diagnostics.
/// `body` 是精确响应数据，`label` 限定不含秘密的诊断范围。
///
/// # Returns
/// 返回值
/// The closed response type or a controlled syntax, shape, depth, or node-count failure.
/// 返回封闭响应类型，或受控的语法、结构、深度或节点数量失败。
fn decode_bounded_oauth_json<Response>(body: &[u8], label: &str) -> Result<Response, String>
where
    Response: DeserializeOwned,
{
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|_| format!("{label} was invalid."))?;
    crate::provider_manifest::validate_runtime_json_bounds(&value, label)?;
    serde_json::from_value(value).map_err(|_| format!("{label} was invalid."))
}

/// Reads and decodes one successful OAuth JSON response through all shared bounds.
/// 通过全部共享边界读取并解码一份成功的 OAuth JSON 响应。
///
/// # Parameters
/// 参数
/// `response` owns the network body and `label` scopes non-secret diagnostics.
/// `response` 持有网络正文，`label` 限定不含秘密的诊断范围。
///
/// # Returns
/// 返回值
/// The closed response type or the first transport, size, JSON, depth, or shape failure.
/// 返回封闭响应类型，或首个传输、大小、JSON、深度或结构失败。
async fn decode_bounded_oauth_json_response<Response>(
    response: reqwest::Response,
    label: &str,
) -> Result<Response, String>
where
    Response: DeserializeOwned,
{
    let body = read_bounded_oauth_response(response, label).await?;
    decode_bounded_oauth_json(body.as_slice(), label)
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
fn unix_millis() -> Result<u64, String> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| String::from("System clock is earlier than the Unix epoch."))?
        .as_millis();
    u64::try_from(value).map_err(|_| String::from("System time exceeds supported OAuth range."))
}

#[cfg(test)]
mod tests {
    use super::{
        KimiTokenResponse, MiniMaxTokenResponse, OAuthAuthorizationRuntime,
        OAuthClientConfiguration, OAuthProviderKind, OAuthTokenResponse, OAuthTransaction,
        OAuthTransactionPhase, OAuthTransactionSnapshot, build_kimi_credential_material,
        build_minimax_credential_material, build_oauth_flow, build_pkce_challenge,
        build_pkce_verifier, kimi_device_model, merge_refreshed_token_set, parse_callback_request,
        unix_millis, validate_callback_state, validate_kimi_verification_uri,
        validate_minimax_verification_uri,
    };
    use crate::{CredentialSecretMaterial, MiniMaxRegion, OAuthTokenSet};
    use reqwest::Client;
    use std::collections::BTreeSet;

    #[test]
    fn pkce_verifier_and_challenge_use_supported_alphabets() {
        let verifier = build_pkce_verifier();
        let challenge = build_pkce_challenge(verifier.as_str());
        assert!((43..=128).contains(&verifier.len()));
        assert!(
            verifier
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        );
        assert_eq!(challenge.len(), 43);
        assert!(
            challenge
                .chars()
                .all(|character| character.is_ascii_alphanumeric()
                    || character == '-'
                    || character == '_')
        );
    }

    /// Verifies fragmented-reader parsing logic has a pure bounded HTTP parser.
    /// 验证分片读取逻辑拥有纯净且有界的 HTTP 解析器。
    #[test]
    fn callback_parser_extracts_code_and_state() {
        let callback = parse_callback_request(
            b"GET /auth/callback?code=code-value&state=state-value HTTP/1.1\r\nHost: localhost\r\n\r\n",
        )
        .expect("callback should parse");
        assert_eq!(callback.0, "code-value");
        assert_eq!(callback.1, "state-value");
    }

    /// Verifies a mismatched State is rejected before callback success is reported.
    /// 验证不匹配的 State 会在报告回调成功前被拒绝。
    #[test]
    fn callback_state_mismatch_is_rejected() {
        let error =
            validate_callback_state(String::from("code"), String::from("returned"), "expected")
                .expect_err("mismatched state must fail");
        assert_eq!(error, "OAuth callback state did not match.");
    }

    /// Verifies public Gemini OAuth retains the user project and requests the official scope.
    /// 验证公共 Gemini OAuth 会保留用户项目并请求官方 Scope。
    #[test]
    fn public_gemini_oauth_requires_project_and_official_scope() {
        let flow = build_oauth_flow(
            OAuthProviderKind::GoogleGemini,
            OAuthClientConfiguration {
                client_id: Some(String::from("desktop-client")),
                client_secret: Some(String::from("desktop-secret")),
                project_id: Some(String::from("cloud-project")),
            },
            String::from("http://127.0.0.1:12345/oauth/callback"),
            String::from("state"),
            build_pkce_verifier(),
        )
        .expect("public Gemini OAuth flow should build");
        assert_eq!(flow.project_id.as_deref(), Some("cloud-project"));
        assert!(
            flow.scopes
                .contains("https://www.googleapis.com/auth/generative-language.retriever")
        );

        let error = build_oauth_flow(
            OAuthProviderKind::GoogleGemini,
            OAuthClientConfiguration {
                client_id: Some(String::from("desktop-client")),
                client_secret: None,
                project_id: None,
            },
            String::from("http://127.0.0.1:12345/oauth/callback"),
            String::from("state"),
            build_pkce_verifier(),
        )
        .err()
        .expect("public Gemini OAuth without project must fail");
        assert!(error.contains("project id"));
    }

    /// Verifies Kimi login retains the exact device identity and rejects foreign browser origins.
    /// 验证 Kimi 登录保留精确设备身份并拒绝外部浏览器 Origin。
    #[test]
    fn kimi_material_retains_device_identity() {
        let material = build_kimi_credential_material(
            KimiTokenResponse {
                error: String::new(),
                error_description: String::new(),
                access_token: String::from("access"),
                refresh_token: String::from("refresh"),
                expires_in: 3600.0,
                scope: String::from("coding"),
            },
            "123e4567-e89b-42d3-a456-426614174000",
        )
        .expect("Kimi material should validate");
        assert!(matches!(
            material,
            CredentialSecretMaterial::KimiCodingPlanOAuth {
                device_id,
                ..
            } if device_id == "123e4567-e89b-42d3-a456-426614174000"
        ));
        assert!(
            validate_kimi_verification_uri("https://kimi.com/device")
                .expect("Kimi authority")
                .starts_with("https://kimi.com/")
        );
        assert!(validate_kimi_verification_uri("https://auth.kimi.com/device").is_ok());
        assert!(
            validate_kimi_verification_uri(
                "https://a.b.kimi.com/device?next=https%3A%2F%2Fevil.example#fragment"
            )
            .is_ok()
        );
        assert!(validate_kimi_verification_uri("https://notkimi.com/device").is_err());
        assert!(validate_kimi_verification_uri("https://kimi.com.evil.example/device").is_err());
        assert!(validate_kimi_verification_uri("http://auth.kimi.com/device").is_err());
        assert!(
            validate_kimi_verification_uri("https://user:secret@auth.kimi.com/device").is_err()
        );
        assert!(validate_kimi_verification_uri("https://auth.kimi.com:444/device").is_err());
    }

    /// Verifies Kimi receives the provider-observed operating-system label.
    /// 验证 Kimi 收到供应商已验证的操作系统标签。
    #[test]
    fn kimi_device_model_uses_provider_operating_system_label() {
        let expected_operating_system = match std::env::consts::OS {
            "darwin" => "macOS",
            "windows" => "Windows",
            "linux" => "Linux",
            other => other,
        };
        assert_eq!(
            kimi_device_model(),
            format!("{expected_operating_system} {}", std::env::consts::ARCH)
        );
    }

    /// Verifies providers may rotate only the access token during refresh.
    /// 验证供应商刷新时可以只轮换 Access Token。
    #[test]
    fn refresh_without_new_refresh_token_preserves_existing_value() {
        let current = OAuthTokenSet {
            access_token: String::from("old-access"),
            refresh_token: String::from("stable-refresh"),
            issued_at_unix_ms: 1,
            expires_at_unix_ms: 2,
            scopes: BTreeSet::from([String::from("scope")]),
            account_subject: Some(String::from("subject")),
            email: Some(String::from("user@example.com")),
        };
        let refreshed = OAuthTokenResponse {
            access_token: String::from("new-access"),
            refresh_token: None,
            id_token: None,
            expires_in: 3600,
            scope: None,
        };
        let merged = merge_refreshed_token_set(&current, &refreshed).expect("refresh should merge");
        assert_eq!(merged.access_token, "new-access");
        assert_eq!(merged.refresh_token, "stable-refresh");
        assert_eq!(merged.scopes, current.scopes);
        assert_eq!(merged.account_subject, current.account_subject);
        assert_eq!(merged.email, current.email);
    }

    /// Verifies MiniMax browser and API URLs cannot cross the selected regional trust boundary.
    /// 验证 MiniMax 浏览器与 API URL 不能跨越所选区域信任边界。
    #[test]
    fn minimax_urls_are_region_bound() {
        assert!(
            validate_minimax_verification_uri(
                MiniMaxRegion::Global,
                "https://platform.minimax.io/oauth-authorize?user_code=TEST"
            )
            .is_ok()
        );
        assert!(
            validate_minimax_verification_uri(
                MiniMaxRegion::China,
                "https://platform.minimaxi.com/oauth-authorize?user_code=TEST"
            )
            .is_ok()
        );
        assert!(
            validate_minimax_verification_uri(
                MiniMaxRegion::Global,
                "https://platform.minimaxi.com/oauth-authorize?user_code=TEST"
            )
            .is_err()
        );
        assert!(
            validate_minimax_verification_uri(
                MiniMaxRegion::China,
                "https://account.minimaxi.com/oauth-authorize?user_code=TEST"
            )
            .is_err()
        );
        assert!(
            validate_minimax_verification_uri(
                MiniMaxRegion::Global,
                "https://user:secret@platform.minimax.io/oauth-authorize?user_code=TEST"
            )
            .is_err()
        );
        assert!(
            validate_minimax_verification_uri(
                MiniMaxRegion::Global,
                "https://platform.minimax.io:444/oauth-authorize?user_code=TEST"
            )
            .is_err()
        );
        let now = unix_millis().expect("clock");
        let material = build_minimax_credential_material(
            MiniMaxRegion::Global,
            MiniMaxTokenResponse {
                base_resp: None,
                status: String::from("success"),
                access_token: Some(String::from("access")),
                refresh_token: Some(String::from("refresh")),
                expired_in: Some(now + 60_000),
                resource_url: Some(String::from("https://api.minimaxi.com")),
            },
        );
        assert!(material.is_err());
    }

    /// Verifies successful MiniMax Device Flow material preserves absolute expiration and region.
    /// 验证成功的 MiniMax Device Flow 材料保留绝对过期时间与区域。
    #[test]
    fn minimax_device_material_preserves_absolute_expiration() {
        let now = unix_millis().expect("clock");
        let expires_at = now + 60_000;
        let material = build_minimax_credential_material(
            MiniMaxRegion::China,
            MiniMaxTokenResponse {
                base_resp: None,
                status: String::from("success"),
                access_token: Some(String::from("access")),
                refresh_token: Some(String::from("refresh")),
                expired_in: Some(expires_at),
                resource_url: Some(String::from("https://api.minimaxi.com")),
            },
        )
        .expect("valid MiniMax material");
        match material {
            CredentialSecretMaterial::MiniMaxOAuth {
                tokens,
                region,
                resource_url,
            } => {
                assert_eq!(tokens.expires_at_unix_ms, expires_at);
                assert_eq!(region, MiniMaxRegion::China);
                assert_eq!(resource_url, "https://api.minimaxi.com");
            }
            _ => panic!("unexpected credential material"),
        }
    }

    /// Verifies an early commit attempt cannot detach a still-running authorization transaction.
    /// 验证提前提交不会使仍在运行的授权事务失联。
    #[tokio::test]
    async fn take_succeeded_retains_non_terminal_transaction() {
        let runtime = OAuthAuthorizationRuntime::new(Client::new());
        let task = tokio::spawn(std::future::pending::<()>());
        let transaction_id = String::from("oauth_waiting");
        runtime.transactions.lock().await.insert(
            transaction_id.clone(),
            OAuthTransaction {
                snapshot: OAuthTransactionSnapshot {
                    transaction_id: transaction_id.clone(),
                    provider_kind: OAuthProviderKind::OpenAiCodex,
                    phase: OAuthTransactionPhase::WaitingForCallback,
                    expires_at_unix_ms: u64::MAX,
                    message: None,
                },
                material: None,
                abort_handle: task.abort_handle(),
            },
        );

        let error = match runtime.take_succeeded(transaction_id.as_str()).await {
            Ok(_) => panic!("a waiting transaction must not be consumed"),
            Err(error) => error,
        };
        assert!(error.contains("not successful"));
        assert_eq!(
            runtime
                .status(transaction_id.as_str())
                .await
                .expect("the waiting transaction must remain registered")
                .phase,
            OAuthTransactionPhase::WaitingForCallback
        );

        runtime
            .cancel(transaction_id.as_str())
            .await
            .expect("test transaction should cancel");
    }
}
