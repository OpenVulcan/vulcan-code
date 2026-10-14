//! Lossless OpenDesign wallet queries using the approved account-control key.
//! 使用获批账户控制密钥的无损 OpenDesign 钱包查询。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::current_unix_milliseconds;
use crate::CredentialExecutionSecret;
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot};

/// Exact wallet response consumed by the upstream OpenDesign daemon.
/// 上游 OpenDesign 守护进程消费的精确钱包响应。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WalletBalance {
    /// Non-negative USD decimal; retained without floating-point conversion.
    /// 非负美元十进制数；保留原值，不转换为浮点数。
    balance_usd: String,
}

/// Queries the registered wallet authority with the dedicated control credential.
/// 使用专用控制凭据查询已注册的钱包授权端。
///
/// `secret` must originate from approved OpenDesign Auth. Returns an exact USD balance snapshot.
/// `secret` 必须源自获批的 OpenDesign Auth。返回精确美元余额快照。
pub(in crate::client) async fn query_open_design_wallet(
    secret: &CredentialExecutionSecret,
) -> Result<AccountUsageSnapshot, String> {
    // control_key is selected explicitly; inference credentials never substitute for it.
    // control_key 被显式选择；推理凭据绝不替代它。
    let control_key = secret.open_design_control_key()?;
    let http = crate::open_design::client()?;
    let url = format!(
        "{}/api/v1/wallet/balance",
        crate::open_design::CONTROL_ORIGIN
    );
    query_at(&http, &url, control_key).await
}

/// Executes one bounded wallet request against the fixed production URL or a local test server.
/// 对固定生产 URL 或本地测试服务执行一次有界钱包请求。
///
/// `http` rejects redirects, `url` is module-owned, and `control_key` is the account credential.
/// `http` 拒绝重定向，`url` 由模块持有，`control_key` 是账户凭据。
/// Returns exact balance data or a controlled error without upstream body text.
/// 返回精确余额数据或不含上游正文的受控错误。
async fn query_at(
    http: &reqwest::Client,
    url: &str,
    control_key: &str,
) -> Result<AccountUsageSnapshot, String> {
    // response is bounded by both a request deadline and the shared metadata body cap.
    // response 同时受请求截止时间与共享元数据正文上限约束。
    let response = http
        .get(url)
        .bearer_auth(control_key)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|_| String::from("OpenDesign wallet balance request failed."))?;
    let status = response.status();
    if matches!(status.as_u16(), 401 | 403) {
        return Err(String::from(
            "OpenDesign wallet authorization expired; sign in again.",
        ));
    }
    if !status.is_success() {
        return Err(format!(
            "OpenDesign wallet balance returned HTTP {}.",
            status.as_u16()
        ));
    }
    let body =
        read_bounded_provider_metadata_body(response, "OpenDesign wallet balance", url).await?;
    decode_balance(&body)
}

/// Decodes a successful wallet response while preserving every decimal digit.
/// 解码成功的钱包响应，同时保留每一位十进制数字。
///
/// `body` is bounded JSON. Returns balance only, without inventing subscription quota or usage.
/// `body` 是有界 JSON。仅返回余额，不虚构订阅额度或用量。
fn decode_balance(body: &str) -> Result<AccountUsageSnapshot, String> {
    // wallet follows the source contract's string amount; JSON numbers are rejected.
    // wallet 遵循源码契约中的字符串金额；拒绝 JSON 数字。
    let wallet: WalletBalance = parse_provider_metadata_json(body, "OpenDesign wallet balance")
        .map_err(|_| String::from("OpenDesign wallet balance response was invalid."))?;
    // The upstream contract permits digits with one optional fractional part, never exponents.
    // 上游契约仅允许数字及一个可选小数部分，绝不允许指数。
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    let valid = match wallet.balance_usd.split_once('.') {
        Some((whole, fraction)) => digits(whole) && digits(fraction),
        None => digits(&wallet.balance_usd),
    };
    if !valid {
        return Err(String::from(
            "OpenDesign wallet balance contains an invalid decimal.",
        ));
    }
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: None,
        plan_id: None,
        used_amount: None,
        remaining_amount: Some(wallet.balance_usd.clone()),
        balance_amount: Some(wallet.balance_usd),
        currency: Some(String::from("USD")),
        quota_windows: Vec::new(),
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

#[cfg(test)]
mod tests;
