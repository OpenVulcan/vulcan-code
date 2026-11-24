//! Step Plan credit snapshots for domestic and international connections.
//! 国内与国际 Step Plan 连接的额度快照。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::decimal::subtract_non_negative_decimal;
use crate::ServiceEntry;
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Fetch this entry's usage snapshot with its connection key and return normalized credits.
/// 使用连接密钥查询该入口的用量快照，返回规范化额度。
pub(in crate::client) async fn query_stepfun_plan_usage(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    // Both signed regional entries expose the same path below their own plan base URL.
    // 两个签名地区入口均在各自的套餐基础地址下提供同一查询路径。
    let url = format!("{}/usage", entry.base_url.trim_end_matches('/'));
    // Use the regional account credential for this bounded metadata request.
    // 使用地区账号凭据发起有超时限制的元数据请求。
    let response = http
        .get(&url)
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|_| String::from("StepFun plan usage request failed."))?;
    if !response.status().is_success() {
        return Err(format!(
            "StepFun plan usage returned HTTP {}.",
            response.status().as_u16()
        ));
    }
    // Limit the response size before decoding provider-controlled metadata.
    // 解析供应商元数据前先限制响应大小。
    let body = read_bounded_provider_metadata_body(response, "StepFun plan usage", &url).await?;
    decode_stepfun_usage(&body)
}

/// Decode a v1 response body into independent credit buckets or return a metadata error.
/// 将 v1 响应正文解析为独立的额度桶，失败时返回元数据错误。
fn decode_stepfun_usage(body: &str) -> Result<AccountUsageSnapshot, String> {
    // Read the typed wire contract before projecting the account snapshot.
    // 先按上游字段类型读取，再生成账号快照。
    let response: UsageResponse = parse_provider_metadata_json(body, "StepFun plan usage JSON")?;
    if response.schema_version != 1
        || response.plan.state != "known"
        || response.quota.state != "known"
    {
        return Err(String::from(
            "StepFun returned an unsupported or unknown plan usage snapshot.",
        ));
    }
    // Retain every grant because separate grants can expire at different times.
    // 逐笔保留额度，因为不同授予记录的到期时间可能不同。
    let quota_windows = response
        .quota
        .credit_buckets
        .into_iter()
        .enumerate()
        .map(|(index, bucket)| bucket.into_window(index))
        .collect::<Result<_, _>>()?;
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(response.plan.name),
        plan_id: None,
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows,
        observed_at: Some(response.observed_at_ms.to_string()),
        // A blocked account can still report a valid balance, including zero.
        // 即使账号被限制调用，仍可返回有效余额，包括零。
        refresh_message: if response.quota.allowed {
            None
        } else {
            response.quota.reason
        },
    })
}

/// Fields consumed by the shared parser from the provider's schema-versioned snapshot.
/// 共享解析器从供应商版本化快照中读取的字段。
#[derive(Deserialize)]
struct UsageResponse {
    /// Upstream response contract version.
    /// 上游响应契约版本。
    schema_version: u32,
    /// Time of observation on the provider clock in Unix milliseconds.
    /// 上游观测时间，单位为 Unix 毫秒。
    observed_at_ms: u64,
    /// Subscription identity for this account.
    /// 此账号的订阅信息。
    plan: Plan,
    /// Credit availability at the observed time.
    /// 观测时刻的额度可用情况。
    quota: Quota,
}

/// Subscription fields displayed in the account card.
/// 账号卡片展示的订阅字段。
#[derive(Deserialize)]
struct Plan {
    /// Whether the provider could resolve this plan.
    /// 供应商是否成功识别套餐。
    state: String,
    /// Region-specific plan name as reported.
    /// 上游报告的地区套餐名称。
    name: String,
}

/// Reported quota state and all credit sources on the account.
/// 上游报告的额度状态及账号包含的各项额度来源。
#[derive(Deserialize)]
struct Quota {
    /// Whether the provider knows the quota.
    /// 供应商是否已确定额度。
    state: String,
    /// Whether requests are currently permitted.
    /// 当前是否允许发起请求。
    allowed: bool,
    /// Provider explanation when calls are blocked.
    /// 限制请求时的上游原因。
    reason: Option<String>,
    /// Separate credit grants, each with its own balance and dates.
    /// 各自拥有余额和时间信息的独立额度桶。
    credit_buckets: Vec<CreditBucket>,
}

/// A Step Plan credit grant whose integer amounts are JSON strings.
/// Step Plan 额度桶，整数额度以 JSON 字符串传输。
#[derive(Deserialize)]
struct CreditBucket {
    /// Credit source as named by StepFun.
    /// 阶跃星辰定义的额度来源名称。
    #[serde(rename = "type")]
    kind: String,
    /// Total credits granted.
    /// 已授予的总额度。
    total: String,
    /// Credits still available.
    /// 当前剩余额度。
    remaining: String,
    /// Periodic reset time, absent for grants without a reset.
    /// 周期重置时间，无重置的额度桶不提供此值。
    resets_at_ms: Option<u64>,
    /// Expiry of this grant independent of the reset schedule.
    /// 此额度桶的到期时间，独立于周期重置。
    expires_at_ms: Option<u64>,
}

impl CreditBucket {
    /// Map this grant and its position within the snapshot to an exact quota window.
    /// 将本额度桶及其在快照内的位置映射为精确的额度窗口。
    fn into_window(self, index: usize) -> Result<AccountUsageWindow, String> {
        // Counts can exceed JavaScript's safe integer range. Subtract their decimal digits directly.
        // 额度可能超出 JavaScript 安全整数范围，因此直接对十进制数字作减法。
        if [&self.total, &self.remaining]
            .iter()
            .any(|value| value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(String::from(
                "StepFun credit amounts must be non-negative integer strings.",
            ));
        }
        // The authoritative integer balances determine the exact consumed amount.
        // 使用上游整数额度精确计算已用量。
        let used = subtract_non_negative_decimal(&self.total, &self.remaining).map_err(|_| {
            String::from("StepFun credit bucket contains inconsistent credit amounts.")
        })?;
        Ok(AccountUsageWindow {
            id: format!("{}-{index}", self.kind),
            display_name: self.kind.clone(),
            bucket_name: Some(self.kind),
            duration_seconds: None,
            used: Some(used),
            remaining: Some(self.remaining),
            total: Some(self.total),
            remaining_percent: None,
            unit: String::from("credits"),
            starts_at: None,
            resets_at: self.resets_at_ms.map(|ms| ms.to_string()),
            expires_at: self.expires_at_ms.map(|ms| ms.to_string()),
            unlimited: false,
            available: true,
        })
    }
}

#[cfg(test)]
mod tests;
