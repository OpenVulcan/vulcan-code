//! OpenAI Codex subscription rate-limit windows and credits normalization.
//! OpenAI Codex 订阅限额窗口与 Credits 归一化。

use super::super::http::{parse_provider_metadata_json, send_metadata_get_with_authentication};
use super::current_unix_milliseconds;
use crate::{AuthenticationProjection, CredentialExecutionSecret, ServiceEntry};
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Verified ChatGPT backend usage endpoint shared by every Codex subscription account.
/// 所有 Codex 订阅账号共用的已验证 ChatGPT 后端用量入口。
const OPENAI_CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// Queries Codex subscription usage with the account's ChatGPT bearer and account identity.
/// 使用账号的 ChatGPT Bearer 与账号身份查询 Codex 订阅用量。
pub(in crate::client) async fn query_openai_codex_usage(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &CredentialExecutionSecret,
) -> Result<AccountUsageSnapshot, String> {
    let body = send_metadata_get_with_authentication(
        http,
        entry,
        AuthenticationProjection::OpenAiCodexBearer,
        OPENAI_CODEX_USAGE_URL,
        secret,
        "OpenAI Codex usage",
    )
    .await
    .map_err(|error| error.message)?;
    decode_openai_codex_usage(body.as_str())
}

/// Decodes one exact Codex usage response into provider-neutral percentage windows.
/// 将一份精确 Codex 用量响应解码为供应商无关的百分比窗口。
fn decode_openai_codex_usage(body: &str) -> Result<AccountUsageSnapshot, String> {
    let response: CodexUsageResponse =
        parse_provider_metadata_json(body, "OpenAI Codex usage JSON")?;
    let mut windows = Vec::new();
    if let Some(rate_limit) = response.rate_limit {
        push_rate_limit_windows(&mut windows, "codex", None, rate_limit);
    }
    for additional in response.additional_rate_limits.unwrap_or_default() {
        if let Some(rate_limit) = additional.rate_limit {
            push_rate_limit_windows(
                &mut windows,
                additional.metered_feature.as_str(),
                Some(additional.limit_name),
                rate_limit,
            );
        }
    }
    // Credits are a separately purchased pool; only a finite reported balance is shown.
    // Credits 是单独购买的额度池；只展示有限且已报告的余额。
    let balance_amount = response
        .credits
        .filter(|credits| credits.has_credits && !credits.unlimited)
        .and_then(|credits| credits.balance)
        .filter(|balance| !balance.trim().is_empty());
    let plan_id = response.plan_type.filter(|value| !value.trim().is_empty());
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: plan_id.as_deref().map(codex_plan_name),
        plan_id,
        used_amount: None,
        remaining_amount: None,
        currency: balance_amount.as_ref().map(|_| String::from("credits")),
        balance_amount,
        quota_windows: windows,
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// Appends the primary and secondary windows of one Codex limit bucket in provider order.
/// 按供应商顺序追加一个 Codex 限额桶的主窗口与次窗口。
fn push_rate_limit_windows(
    windows: &mut Vec<AccountUsageWindow>,
    limit_id: &str,
    limit_name: Option<String>,
    rate_limit: CodexRateLimit,
) {
    for (slot, window) in [
        ("primary", rate_limit.primary_window),
        ("secondary", rate_limit.secondary_window),
    ] {
        let Some(window) = window else {
            continue;
        };
        windows.push(codex_usage_window(
            format!("{limit_id}:{slot}"),
            limit_name.clone(),
            window,
        ));
    }
}

/// Converts one Codex window into a remaining-percentage meter with its exact reset time.
/// 将一个 Codex 窗口转换为带精确重置时间的剩余百分比计量。
fn codex_usage_window(
    id: String,
    limit_name: Option<String>,
    window: CodexRateLimitWindow,
) -> AccountUsageWindow {
    // Codex reports consumption only as a percentage; the allowance itself is never exposed.
    // Codex 只以百分比报告消耗；额度本身从不公开。
    let used_percent = window.used_percent.clamp(0, 100);
    AccountUsageWindow {
        id,
        display_name: limit_name.clone().unwrap_or_else(|| String::from("Codex")),
        bucket_name: limit_name,
        duration_seconds: u64::try_from(window.limit_window_seconds)
            .ok()
            .filter(|seconds| *seconds > 0),
        used: None,
        remaining: None,
        total: None,
        remaining_percent: Some((100 - used_percent).to_string()),
        unit: String::from("percent"),
        starts_at: None,
        resets_at: (window.reset_at > 0).then(|| window.reset_at.to_string()),
        unlimited: false,
        available: true,
    }
}

/// Returns the public name of one Codex plan type, preserving unknown provider values.
/// 返回一个 Codex 套餐类型的公开名称，未知供应商取值保持原样。
fn codex_plan_name(plan_type: &str) -> String {
    match plan_type {
        "free" => String::from("Free"),
        "go" => String::from("Go"),
        "plus" => String::from("Plus"),
        "pro" => String::from("Pro"),
        "prolite" => String::from("Pro Lite"),
        "promax" => String::from("Pro Max"),
        "team" => String::from("Team"),
        "business" => String::from("Business"),
        "enterprise" => String::from("Enterprise"),
        "edu" | "education" => String::from("Edu"),
        other => other.to_string(),
    }
}

/// Mirrors the exact Codex rate-limit status payload subset.
/// 镜像精确的 Codex 限额状态响应字段子集。
#[derive(Debug, Deserialize)]
struct CodexUsageResponse {
    /// Provider-owned subscription plan type.
    /// 供应商拥有的订阅套餐类型。
    #[serde(default)]
    plan_type: Option<String>,
    /// Main Codex limit bucket.
    /// Codex 主限额桶。
    #[serde(default)]
    rate_limit: Option<CodexRateLimit>,
    /// Purchased credits pool.
    /// 已购买的 Credits 额度池。
    #[serde(default)]
    credits: Option<CodexCredits>,
    /// Feature- or model-specific limit buckets.
    /// 按功能或模型划分的限额桶。
    #[serde(default)]
    additional_rate_limits: Option<Vec<CodexAdditionalRateLimit>>,
}

/// Mirrors one Codex limit bucket with its two rolling windows.
/// 镜像一个带两个滚动窗口的 Codex 限额桶。
#[derive(Debug, Deserialize)]
struct CodexRateLimit {
    /// Shorter rolling window, typically five hours.
    /// 较短的滚动窗口，通常为五小时。
    #[serde(default)]
    primary_window: Option<CodexRateLimitWindow>,
    /// Longer rolling window, typically one week.
    /// 较长的滚动窗口，通常为一周。
    #[serde(default)]
    secondary_window: Option<CodexRateLimitWindow>,
}

/// Mirrors one Codex rolling-window snapshot.
/// 镜像一个 Codex 滚动窗口快照。
#[derive(Debug, Deserialize)]
struct CodexRateLimitWindow {
    /// Consumed share of the window in whole percent.
    /// 窗口已消耗的整数百分比。
    used_percent: i64,
    /// Window length in seconds.
    /// 窗口时长（秒）。
    limit_window_seconds: i64,
    /// Absolute reset time in Unix seconds.
    /// 以 Unix 秒表示的绝对重置时间。
    reset_at: i64,
}

/// Mirrors the Codex credits pool subset.
/// 镜像 Codex Credits 额度池字段子集。
#[derive(Debug, Deserialize)]
struct CodexCredits {
    /// Whether the account holds purchased credits.
    /// 账号是否持有已购买的 Credits。
    has_credits: bool,
    /// Whether credits are unlimited for this account.
    /// 该账号的 Credits 是否不限量。
    unlimited: bool,
    /// Exact provider decimal balance.
    /// 供应商精确十进制余额。
    #[serde(default)]
    balance: Option<String>,
}

/// Mirrors one named Codex limit bucket outside the main bucket.
/// 镜像一个主限额桶之外的具名 Codex 限额桶。
#[derive(Debug, Deserialize)]
struct CodexAdditionalRateLimit {
    /// Provider-owned display name of the bucket.
    /// 供应商拥有的限额桶显示名称。
    limit_name: String,
    /// Stable provider identity of the metered feature.
    /// 计量功能的稳定供应商身份。
    metered_feature: String,
    /// The bucket's rolling windows.
    /// 该限额桶的滚动窗口。
    #[serde(default)]
    rate_limit: Option<CodexRateLimit>,
}

#[cfg(test)]
mod tests;
