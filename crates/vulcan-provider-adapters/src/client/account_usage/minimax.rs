//! MiniMax Token Plan queries and provider-owned quota buckets.
//! MiniMax Token Plan 查询与供应商持有的配额桶。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, controlled_error_summary, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::current_unix_milliseconds;
use crate::ServiceEntry;
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Queries the documented MiniMax Token Plan quota endpoint.
/// 查询文档化的 MiniMax Token Plan 配额端点。
pub(in crate::client) async fn query_minimax_token_plan(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    let url = format!(
        "{}/v1/token_plan/remains",
        entry.base_url.trim_end_matches('/')
    );
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("MiniMax Token Plan usage request failed: {error}"))?;
    let status = response.status();
    let body =
        read_bounded_provider_metadata_body(response, "MiniMax Token Plan usage", url.as_str())
            .await?;
    if !status.is_success() {
        return Err(format!(
            "MiniMax Token Plan usage returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    let response: MiniMaxQuotaResponse =
        parse_provider_metadata_json(body.as_str(), "MiniMax Token Plan usage JSON")?;
    if response.base_resp.status_code != 0 {
        return Err(format!(
            "MiniMax Token Plan usage returned business status {}: {}",
            response.base_resp.status_code, response.base_resp.status_msg
        ));
    }
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(String::from("Token Plan")),
        plan_id: None,
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows: minimax_quota_windows(response.model_remains)?,
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// Normalizes MiniMax model buckets into explicit current and weekly windows.
/// 把 MiniMax 模型额度桶归一化为明确的当前与每周窗口。
///
/// # Parameters
/// 参数
/// `models` is the strict response collection mirrored from the official MiniMax CLI.
/// `models` 是从 MiniMax 官方 CLI 镜像的严格响应集合。
///
/// # Returns
/// 返回值
/// Exact count windows, or a controlled invariant error.
/// 返回精确次数窗口，或受控不变量错误。
fn minimax_quota_windows(
    models: Vec<MiniMaxQuotaModelRemain>,
) -> Result<Vec<AccountUsageWindow>, String> {
    let mut windows = Vec::with_capacity(models.len().saturating_mul(2));
    for model in models {
        let unavailable = model.current_interval_total_count == 0
            && model.current_weekly_total_count == 0
            && model.current_interval_status == Some(3)
            && model.current_weekly_status == Some(3);
        windows.push(minimax_quota_window(MiniMaxQuotaWindowInput {
            id: format!("{}:current", model.model_name),
            display_name: String::from("Current rolling window"),
            bucket_name: model.model_name.clone(),
            total: model.current_interval_total_count,
            remaining: model.current_interval_usage_count,
            remaining_percent: model.current_interval_remaining_percent,
            starts_at: model.start_time,
            resets_at: model.end_time,
            unlimited: false,
            available: !unavailable,
        })?);
        windows.push(minimax_quota_window(MiniMaxQuotaWindowInput {
            id: format!("{}:weekly", model.model_name),
            display_name: String::from("Weekly window"),
            bucket_name: model.model_name,
            total: model.current_weekly_total_count,
            remaining: model.current_weekly_usage_count,
            remaining_percent: model.current_weekly_remaining_percent,
            starts_at: model.weekly_start_time,
            resets_at: model.weekly_end_time,
            unlimited: !unavailable && model.current_weekly_status == Some(3),
            available: !unavailable,
        })?);
    }
    Ok(windows)
}

/// Carries the exact source fields for one MiniMax quota window.
/// 承载一个 MiniMax 额度窗口的精确来源字段。
struct MiniMaxQuotaWindowInput {
    /// Stable normalized window identifier.
    /// 稳定的归一化窗口标识。
    id: String,
    /// User-visible window name.
    /// 用户可见窗口名称。
    display_name: String,
    /// Provider model bucket name.
    /// 供应商模型额度桶名称。
    bucket_name: String,
    /// Provider-reported total request count.
    /// 供应商报告的请求总次数。
    total: u64,
    /// Provider-reported remaining request count.
    /// 供应商报告的剩余请求次数。
    remaining: u64,
    /// Provider-reported remaining percentage.
    /// 供应商报告的剩余百分比。
    remaining_percent: Option<u64>,
    /// Exact provider-reported window start.
    /// 供应商报告的精确窗口起始时间。
    starts_at: u64,
    /// Exact provider-reported window reset time.
    /// 供应商报告的精确窗口重置时间。
    resets_at: u64,
    /// Whether the provider marks this window as unlimited.
    /// 供应商是否将此窗口标记为无限。
    unlimited: bool,
    /// Whether this bucket is included in the current account plan.
    /// 当前账号套餐是否包含该额度桶。
    available: bool,
}

/// Builds one exact MiniMax count window.
/// 构建一个精确 MiniMax 次数窗口。
///
/// # Parameters
/// 参数
/// `input` contains the exact fields reported for one quota bucket.
/// `input` 包含一个额度桶报告的精确字段。
///
/// # Returns
/// 返回值
/// The normalized account window or a controlled invariant failure.
/// 返回归一化账号窗口或受控不变量错误。
fn minimax_quota_window(input: MiniMaxQuotaWindowInput) -> Result<AccountUsageWindow, String> {
    if !input.unlimited && input.remaining > input.total {
        return Err(format!(
            "MiniMax quota window `{}` reports remaining count above its total.",
            input.id
        ));
    }
    Ok(AccountUsageWindow {
        id: input.id,
        display_name: input.display_name,
        bucket_name: Some(input.bucket_name),
        used: (input.available && !input.unlimited)
            .then(|| input.total.saturating_sub(input.remaining).to_string()),
        remaining: (input.available && !input.unlimited).then(|| input.remaining.to_string()),
        total: (input.available && !input.unlimited).then(|| input.total.to_string()),
        remaining_percent: input
            .available
            .then_some(input.remaining_percent)
            .flatten()
            .map(|value| value.to_string()),
        unit: String::from("requests"),
        starts_at: Some(input.starts_at.to_string()),
        resets_at: Some(input.resets_at.to_string()),
        unlimited: input.unlimited,
        available: input.available,
    })
}

/// MiniMax Token Plan quota response mirrored from the official MiniMax CLI.
/// 从 MiniMax 官方 CLI 镜像的 Token Plan 额度响应。
#[derive(Debug, Deserialize)]
struct MiniMaxQuotaResponse {
    /// Required MiniMax business-status envelope.
    /// 必需的 MiniMax 业务状态封装。
    base_resp: MiniMaxQuotaBaseResponse,
    /// Independently reported model or capability buckets.
    /// 独立报告的模型或能力额度桶。
    model_remains: Vec<MiniMaxQuotaModelRemain>,
}

/// Strict MiniMax Token Plan business status.
/// 严格的 MiniMax Token Plan 业务状态。
#[derive(Debug, Deserialize)]
struct MiniMaxQuotaBaseResponse {
    /// Business status code where zero means success.
    /// 业务状态码，零表示成功。
    status_code: i64,
    /// Controlled provider status message.
    /// 供应商受控状态消息。
    #[serde(default)]
    status_msg: String,
}

/// One strict MiniMax Token Plan quota bucket.
/// 一个严格的 MiniMax Token Plan 额度桶。
#[derive(Debug, Deserialize)]
struct MiniMaxQuotaModelRemain {
    /// Provider-owned bucket name.
    /// 供应商拥有的额度桶名称。
    model_name: String,
    /// Current-window start in provider Unix milliseconds.
    /// 供应商 Unix 毫秒表示的当前窗口开始时间。
    start_time: u64,
    /// Current-window end in provider Unix milliseconds.
    /// 供应商 Unix 毫秒表示的当前窗口结束时间。
    end_time: u64,
    /// Provider-reported current-window total request count.
    /// 供应商报告的当前窗口请求总数。
    current_interval_total_count: u64,
    /// Provider-misnamed current-window remaining request count.
    /// 供应商误命名的当前窗口剩余请求数。
    current_interval_usage_count: u64,
    /// Optional current-window status where three means unlimited.
    /// 可选当前窗口状态，其中三表示无限。
    current_interval_status: Option<u8>,
    /// Current-window remaining percentage when count totals are not exposed.
    /// 未公开次数总量时的当前窗口剩余百分比。
    current_interval_remaining_percent: Option<u64>,
    /// Provider-reported weekly total request count.
    /// 供应商报告的每周请求总数。
    current_weekly_total_count: u64,
    /// Provider-misnamed weekly remaining request count.
    /// 供应商误命名的每周剩余请求数。
    current_weekly_usage_count: u64,
    /// Optional weekly status where three means unlimited.
    /// 可选每周状态，其中三表示无限。
    current_weekly_status: Option<u8>,
    /// Weekly remaining percentage when count totals are not exposed.
    /// 未公开次数总量时的每周剩余百分比。
    current_weekly_remaining_percent: Option<u64>,
    /// Weekly-window start in provider Unix milliseconds.
    /// 供应商 Unix 毫秒表示的每周窗口开始时间。
    weekly_start_time: u64,
    /// Weekly-window end in provider Unix milliseconds.
    /// 供应商 Unix 毫秒表示的每周窗口结束时间。
    weekly_end_time: u64,
}

#[cfg(test)]
mod tests;
