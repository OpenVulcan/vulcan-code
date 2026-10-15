//! Tavily key/account credit queries and explicit finite or unlimited windows.
//! Tavily Key／账号额度查询及显式有限或无限窗口。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, controlled_error_summary, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::current_unix_milliseconds;
use crate::ServiceEntry;
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Queries the documented Tavily account and API-key credit usage endpoint.
/// 查询 Tavily 官方文档定义的账号与 API Key Credit 用量端点。
///
/// # Parameters
/// 参数
/// `http` is the shared metadata client, `entry` owns the exact service endpoint,
/// and `secret` is the credential being validated.
/// `http` 是共享元数据客户端，`entry` 持有精确服务端点，`secret` 是待验证凭据。
///
/// # Returns
/// 返回值
/// A provider-neutral usage snapshot that preserves Tavily's integer credit accounting.
/// 返回保留 Tavily 整数 Credit 计量语义的供应商无关用量快照。
pub(in crate::client) async fn query_tavily_credits(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    let url = format!("{}/usage", entry.base_url.trim_end_matches('/'));
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("Tavily account usage request failed: {error}"))?;
    let status = response.status();
    let body =
        read_bounded_provider_metadata_body(response, "Tavily account usage", url.as_str()).await?;
    if !status.is_success() {
        return Err(format!(
            "Tavily account usage returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    let response: TavilyUsageResponse =
        parse_provider_metadata_json(body.as_str(), "Tavily account usage JSON")?;
    let account_plan = tavily_usage_window(
        "tavily-account-plan",
        "Account plan",
        response.account.plan_usage,
        response.account.plan_limit,
        false,
    )?;
    let account_paygo = tavily_usage_window(
        "tavily-account-paygo",
        "Pay as you go",
        response.account.paygo_usage,
        response.account.paygo_limit,
        response.account.paygo_limit.is_none(),
    )?;
    let key_total = tavily_usage_window(
        "tavily-api-key",
        "API key",
        response.key.usage,
        response.key.limit,
        false,
    )?;
    let account_remaining = account_plan.remaining.clone();
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(response.account.current_plan),
        plan_id: None,
        used_amount: Some(response.account.plan_usage.to_string()),
        remaining_amount: account_remaining,
        currency: None,
        balance_amount: None,
        quota_windows: vec![
            key_total,
            account_plan,
            account_paygo,
            tavily_usage_window(
                "tavily-key-search",
                "API key search",
                response.key.search_usage,
                None,
                false,
            )?,
            tavily_usage_window(
                "tavily-key-extract",
                "API key extract",
                response.key.extract_usage,
                None,
                false,
            )?,
            tavily_usage_window(
                "tavily-account-search",
                "Account search",
                response.account.search_usage,
                None,
                false,
            )?,
            tavily_usage_window(
                "tavily-account-extract",
                "Account extract",
                response.account.extract_usage,
                None,
                false,
            )?,
        ],
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// Builds one strict Tavily credit window without interpreting a missing limit as zero.
/// 构建一个严格的 Tavily Credit 窗口，且不会把缺失上限解释为零。
///
/// # Parameters
/// 参数
/// The identifiers describe the UI row, `used` and `limit` are provider counters,
/// and `unlimited` is true only for the documented unbounded PAYGO case.
/// 标识描述 UI 行，`used` 与 `limit` 是供应商计数，`unlimited` 仅用于文档明确的无界 PAYGO 情况。
///
/// # Returns
/// 返回值
/// A normalized usage window or an error when usage exceeds a reported finite limit.
/// 返回归一化用量窗口；若用量超过供应商报告的有限上限则返回错误。
fn tavily_usage_window(
    id: &str,
    display_name: &str,
    used: u64,
    limit: Option<u64>,
    unlimited: bool,
) -> Result<AccountUsageWindow, String> {
    let remaining = match limit {
        Some(limit) if used <= limit => Some((limit - used).to_string()),
        Some(limit) => {
            return Err(format!(
                "Tavily usage `{id}` reports used credits {used} above limit {limit}."
            ));
        }
        None => None,
    };
    Ok(AccountUsageWindow {
        id: id.to_string(),
        display_name: display_name.to_string(),
        bucket_name: None,
        duration_seconds: None,
        used: Some(used.to_string()),
        remaining,
        total: limit.map(|value| value.to_string()),
        remaining_percent: None,
        unit: String::from("credits"),
        starts_at: None,
        resets_at: None,
        unlimited,
        available: true,
    })
}

/// Tavily usage response containing API-key and account-level credit counters.
/// Tavily 用量响应，包含 API Key 与账号级 Credit 计数。
#[derive(Debug, Deserialize)]
struct TavilyUsageResponse {
    /// Usage counters scoped to the validated API key.
    /// 绑定到待验证 API Key 的用量计数。
    key: TavilyKeyUsage,
    /// Usage counters and plan metadata scoped to the owning account.
    /// 绑定到所属账号的用量计数与套餐元数据。
    account: TavilyAccountUsage,
}

/// Tavily API-key credit counters required by the public usage card.
/// 公共用量卡片所需的 Tavily API Key Credit 计数。
#[derive(Debug, Deserialize)]
struct TavilyKeyUsage {
    /// Credits consumed by this API key.
    /// 此 API Key 已消耗的 Credit。
    usage: u64,
    /// Credit limit assigned to this API key.
    /// 分配给此 API Key 的 Credit 上限。
    limit: Option<u64>,
    /// Search credits consumed by this API key.
    /// 此 API Key 已消耗的搜索 Credit。
    search_usage: u64,
    /// Extraction credits consumed by this API key.
    /// 此 API Key 已消耗的提取 Credit。
    extract_usage: u64,
}

/// Tavily account plan metadata required by the public usage card.
/// 公共用量卡片所需的 Tavily 账号套餐元数据。
#[derive(Debug, Deserialize)]
struct TavilyAccountUsage {
    /// Current provider plan display name.
    /// 当前供应商套餐显示名称。
    current_plan: String,
    /// Credits consumed from the current plan.
    /// 当前套餐已经消耗的 Credit。
    plan_usage: u64,
    /// Total credits provided by the current plan.
    /// 当前套餐提供的 Credit 总量。
    plan_limit: Option<u64>,
    /// Pay-as-you-go credits consumed by the account.
    /// 账号已消耗的按量付费 Credit。
    paygo_usage: u64,
    /// Optional provider-reported pay-as-you-go limit.
    /// 供应商可选报告的按量付费上限。
    paygo_limit: Option<u64>,
    /// Search credits consumed by the account.
    /// 账号已消耗的搜索 Credit。
    search_usage: u64,
    /// Extraction credits consumed by the account.
    /// 账号已消耗的提取 Credit。
    extract_usage: u64,
}

#[cfg(test)]
mod tests;
