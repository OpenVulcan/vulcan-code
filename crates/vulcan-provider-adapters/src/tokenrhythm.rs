//! Fixed authority and client registration for the TokenRhythm account protocol.
//! TokenRhythm 账户协议的固定授权端与客户端注册。
//!
//! Login, managed-key issuance and wallet reads all derive from these values; the client id and
//! the integration application must change together because TokenRhythm binds them to one
//! registration.
//! 登录、托管密钥签发与钱包读取都从这些值派生；客户端标识与集成应用必须一起修改，
//! 因为 TokenRhythm 将二者绑定在同一注册上。

use serde::{Deserialize, de::DeserializeOwned};

/// Account and inference authority.
/// 账户与推理共用的授权端。
pub(crate) const ORIGIN: &str = "https://tokenrhythm.studio";

/// OAuth client Vulcan Code requests from TokenRhythm. Until TokenRhythm registers it the
/// authorize endpoint answers `invalid_client`; replace both values with the registered ones.
/// Vulcan Code 向 TokenRhythm 申请的 OAuth 客户端。注册完成前授权端返回 `invalid_client`；
/// 注册后将两个值改为实际登记值。
pub(crate) const CLIENT_ID: &str = "vulcan-code";

/// Integration application whose account resources `CLIENT_ID` may read.
/// `CLIENT_ID` 可读取其账户资源的集成应用。
pub(crate) const INTEGRATION_APP: &str = "vulcan-code";

/// Scopes requested at login; the token response must grant every one of them.
/// 登录时请求的权限范围；令牌响应必须全部授予。
pub(crate) const SCOPES: [&str; 6] = [
    "profile.read",
    "wallet.read",
    "managed_key.read",
    "managed_key.obtain",
    "session.revoke",
    "offline_access",
];

/// Longest access-token lifetime the authority may issue, in seconds.
/// 授权端可签发的最长访问令牌有效期（秒）。
pub(crate) const MAX_ACCESS_TOKEN_SECONDS: u64 = 86_400;

/// Longest accepted account or session identifier.
/// 可接受的账户或会话标识最大长度。
pub(crate) const MAX_IDENTITY_LENGTH: usize = 256;

/// Builds a client that never forwards account credentials through redirects.
/// 创建绝不通过重定向转发账户凭据的客户端。
///
/// Returns the protocol client, or a controlled initialization error.
/// 返回协议客户端，或受控的初始化错误。
pub(crate) fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| String::from("Failed to initialize the TokenRhythm account client."))
}

/// Returns the URL of one integration resource under `origin`.
/// 返回 `origin` 下一个集成资源的 URL。
///
/// `resource` is a module-owned relative path such as `wallet`.
/// `resource` 是模块持有的相对路径，例如 `wallet`。
pub(crate) fn integration_url(origin: &str, resource: &str) -> String {
    format!("{origin}/api/v1/integrations/{INTEGRATION_APP}/{resource}")
}

/// Validates one non-empty bounded account or session identifier.
/// 校验一个非空且有界的账户或会话标识。
///
/// `label` names the field in the controlled error. Returns success for a usable identifier.
/// `label` 是受控错误中的字段名。标识可用时返回成功。
pub(crate) fn validate_identity(label: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > MAX_IDENTITY_LENGTH
        || value.chars().any(char::is_control)
    {
        return Err(format!("TokenRhythm {label} is invalid."));
    }
    Ok(())
}

/// Success envelope of every integration resource: `code` zero carries `data`.
/// 每个集成资源的成功封装：`code` 为零时携带 `data`。
#[derive(Deserialize)]
struct Envelope<T> {
    /// Business status; only zero is success.
    /// 业务状态；只有零表示成功。
    code: i64,
    /// Resource payload.
    /// 资源载荷。
    data: Option<T>,
}

/// Sends one authenticated integration request and decodes its success envelope.
/// 发送一次已鉴权的集成请求并解码其成功封装。
///
/// `request` is a prepared request without authorization, `access_token` is the account
/// credential, and `label` names the resource in errors. Returns the payload; HTTP 401 asks the
/// user to sign in again, and upstream bodies never enter diagnostics.
/// `request` 是未带鉴权的已构建请求，`access_token` 是账户凭据，`label` 是错误中的资源名。
/// 返回载荷；HTTP 401 要求重新登录，上游正文绝不进入诊断。
pub(crate) async fn integration_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
    access_token: &str,
    label: &str,
) -> Result<T, String> {
    let response = request
        .bearer_auth(access_token)
        .header(reqwest::header::CACHE_CONTROL, "no-store")
        .send()
        .await
        .map_err(|_| format!("TokenRhythm {label} request failed."))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(String::from(
            "TokenRhythm account authorization expired; sign in again.",
        ));
    }
    if !status.is_success() {
        return Err(format!(
            "TokenRhythm {label} returned HTTP {}.",
            status.as_u16()
        ));
    }
    let envelope: Envelope<T> =
        crate::oauth::decode_bounded_oauth_json_response(response, label).await?;
    match envelope {
        Envelope {
            code: 0,
            data: Some(data),
        } => Ok(data),
        _ => Err(format!("TokenRhythm {label} response was invalid.")),
    }
}
