//! OpenRouter account-credit queries with exact decimal balances.
//! 保留精确十进制余额的 OpenRouter 账号额度查询。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, controlled_error_summary, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::current_unix_milliseconds;
use super::decimal::subtract_non_negative_decimal;
use crate::ServiceEntry;
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot};

/// Queries the documented OpenRouter credits endpoint with a management key.
/// 使用管理密钥查询 OpenRouter 官方文档定义的余额端点。
///
/// # Parameters
/// 参数
/// `http` is the shared metadata client, `entry` owns the exact service endpoint,
/// and `secret` is the management credential used by the credits API.
/// `http` 是共享元数据客户端，`entry` 持有精确服务端点，`secret` 是余额 API 使用的管理凭据。
///
/// # Returns
/// 返回值
/// A provider-neutral account balance snapshot that retains decimal precision.
/// 返回保留十进制精度的供应商无关账号余额快照。
pub(in crate::client) async fn query_openrouter_credits(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    let url = format!("{}/credits", entry.base_url.trim_end_matches('/'));
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("OpenRouter account usage request failed: {error}"))?;
    let status = response.status();
    let body =
        read_bounded_provider_metadata_body(response, "OpenRouter account usage", url.as_str())
            .await?;
    if !status.is_success() {
        return Err(format!(
            "OpenRouter account usage returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    let response: OpenRouterCreditsResponse =
        parse_provider_metadata_json(body.as_str(), "OpenRouter account usage JSON")?;
    let total_credits = response.data.total_credits.to_string();
    let total_usage = response.data.total_usage.to_string();
    let remaining = subtract_non_negative_decimal(total_credits.as_str(), total_usage.as_str())?;
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(String::from("OpenRouter Credits")),
        plan_id: None,
        used_amount: Some(total_usage),
        remaining_amount: Some(remaining.clone()),
        currency: Some(String::from("USD")),
        balance_amount: Some(remaining),
        quota_windows: Vec::new(),
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// OpenRouter credits response.
/// OpenRouter 余额响应。
#[derive(Debug, Deserialize)]
struct OpenRouterCreditsResponse {
    /// Account credit metadata.
    /// 账号余额元数据。
    data: OpenRouterCreditsData,
}

/// OpenRouter account credit metadata used by the public usage contract.
/// 公共用量契约使用的 OpenRouter 账号余额元数据。
#[derive(Debug, Deserialize)]
struct OpenRouterCreditsData {
    /// Total purchased credits.
    /// 累计购买额度。
    total_credits: serde_json::Number,
    /// Total consumed credits.
    /// 累计消耗额度。
    total_usage: serde_json::Number,
}
