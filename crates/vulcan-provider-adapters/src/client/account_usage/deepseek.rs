//! DeepSeek account-balance queries with lossless per-currency records.
//! 保留各币种无损记录的 DeepSeek 账号余额查询。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, controlled_error_summary, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::current_unix_milliseconds;
use super::decimal::normalized_unsigned_decimal_digits;
use crate::ServiceEntry;
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Queries the official DeepSeek account-balance endpoint.
/// 查询 DeepSeek 官方账号余额端点。
///
/// # Parameters
/// 参数
/// `http` is the shared metadata client, `entry` owns the official origin, and
/// `secret` is the exact API key being observed.
/// `http` 是共享元数据客户端，`entry` 持有官方 Origin，`secret` 是被观测的精确 API Key。
///
/// # Returns
/// 返回值
/// A lossless provider-neutral snapshot retaining every provider-reported currency.
/// 返回保留供应商报告全部币种的无损供应商无关快照。
pub(in crate::client) async fn query_deepseek_balance(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    let url = format!("{}/user/balance", entry.base_url.trim_end_matches('/'));
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("DeepSeek account balance request failed: {error}"))?;
    let status = response.status();
    let body =
        read_bounded_provider_metadata_body(response, "DeepSeek account balance", url.as_str())
            .await?;
    if !status.is_success() {
        return Err(format!(
            "DeepSeek account balance returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    decode_deepseek_balance(body.as_str())
}

/// Decodes one DeepSeek balance response without losing currency or decimal precision.
/// 解码一份 DeepSeek 余额响应且不丢失币种或十进制精度。
///
/// # Parameters
/// 参数
/// `body` is the complete successful provider JSON response.
/// `body` 是完整的供应商成功 JSON 响应。
///
/// # Returns
/// 返回值
/// One provider-neutral balance snapshot.
/// 返回一份供应商无关余额快照。
fn decode_deepseek_balance(body: &str) -> Result<AccountUsageSnapshot, String> {
    let response: DeepSeekBalanceResponse =
        parse_provider_metadata_json(body, "DeepSeek account balance JSON")?;
    if response.balance_infos.is_empty() {
        return Err(String::from(
            "DeepSeek account balance omitted its currency balances.",
        ));
    }
    for balance in &response.balance_infos {
        validate_deepseek_decimal(balance.total_balance.as_str())?;
        validate_deepseek_decimal(balance.granted_balance.as_str())?;
        validate_deepseek_decimal(balance.topped_up_balance.as_str())?;
        if !matches!(balance.currency.as_str(), "CNY" | "USD") {
            return Err(String::from(
                "DeepSeek account balance returned an unsupported currency.",
            ));
        }
    }
    let single_balance = (response.balance_infos.len() == 1).then(|| &response.balance_infos[0]);
    let quota_windows = response
        .balance_infos
        .iter()
        .map(|balance| AccountUsageWindow {
            id: format!("balance_{}", balance.currency.to_ascii_lowercase()),
            display_name: format!("{} balance", balance.currency),
            bucket_name: Some(String::from("Account balance")),
            duration_seconds: None,
            used: None,
            remaining: Some(balance.total_balance.clone()),
            total: None,
            remaining_percent: None,
            unit: balance.currency.clone(),
            starts_at: None,
            resets_at: None,
            unlimited: false,
            available: response.is_available,
        })
        .collect();
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(String::from("DeepSeek Balance")),
        plan_id: None,
        used_amount: None,
        remaining_amount: single_balance.map(|balance| balance.total_balance.clone()),
        currency: single_balance.map(|balance| balance.currency.clone()),
        balance_amount: single_balance.map(|balance| balance.total_balance.clone()),
        quota_windows,
        observed_at: current_unix_milliseconds(),
        refresh_message: (!response.is_available)
            .then(|| String::from("DeepSeek reports that the account balance is unavailable.")),
    })
}

/// Validates one non-negative DeepSeek decimal string without converting it to floating point.
/// 在不转换为浮点数的情况下验证一个非负 DeepSeek 十进制字符串。
///
/// # Parameters
/// 参数
/// `value` is one provider-reported exact balance component.
/// `value` 是供应商报告的一个精确余额分量。
///
/// # Returns
/// 返回值
/// Success for one canonical non-negative decimal string.
/// 对规范非负十进制字符串返回成功。
fn validate_deepseek_decimal(value: &str) -> Result<(), String> {
    normalized_unsigned_decimal_digits(value)
        .map(|_| ())
        .map_err(|_| String::from("DeepSeek account balance contains an invalid decimal value."))
}

/// DeepSeek official account balance response.
/// DeepSeek 官方账号余额响应。
#[derive(Debug, Deserialize)]
struct DeepSeekBalanceResponse {
    /// Whether the provider reports usable account balance.
    /// 供应商是否报告账号余额可用。
    is_available: bool,
    /// Exact balances grouped by provider currency.
    /// 按供应商币种分组的精确余额。
    balance_infos: Vec<DeepSeekBalanceInfo>,
}

/// One DeepSeek currency balance.
/// 一项 DeepSeek 币种余额。
#[derive(Debug, Deserialize)]
struct DeepSeekBalanceInfo {
    /// Provider currency code.
    /// 供应商币种代码。
    currency: String,
    /// Total available balance.
    /// 总可用余额。
    total_balance: String,
    /// Unexpired granted balance.
    /// 未过期赠金余额。
    granted_balance: String,
    /// Topped-up balance.
    /// 充值余额。
    topped_up_balance: String,
}

#[cfg(test)]
mod tests;
