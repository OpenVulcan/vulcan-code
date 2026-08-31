//! CommandCode account identity, credit pools, and rolling-window normalization.
//! CommandCode 账号身份、额度池与滚动窗口归一化。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, controlled_error_summary, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::current_unix_milliseconds;
use super::decimal::{
    add_non_negative_decimals, normalized_unsigned_decimal_digits, subtract_non_negative_decimal,
};
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Fixed production origin registered for CommandCode account usage.
/// 为 CommandCode 账号用量注册的固定生产 Origin。
const COMMAND_CODE_API_ORIGIN: &str = "https://api.commandcode.ai";

/// Queries the CommandCode CLI account-usage chain with the inference credential.
/// 使用推理凭据查询 CommandCode CLI 的账号用量链路。
///
/// # Parameters
/// 参数
/// `http` is the shared bounded metadata client and `secret` is the exact CommandCode user key.
/// `http` 是共享的有界元数据客户端，`secret` 是精确的 CommandCode 用户密钥。
///
/// # Returns
/// 返回值
/// A provider-neutral snapshot derived from identity, credits, subscription, and summary data.
/// 返回由身份、额度、订阅与汇总数据派生的供应商无关快照。
pub(in crate::client) async fn query_command_code_usage(
    http: &reqwest::Client,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    let whoami_url = command_code_usage_url("/alpha/whoami", None, None)?;
    let whoami_body = send_command_code_usage_get(http, secret, whoami_url, "identity").await?;
    let whoami: CommandCodeWhoamiResponse =
        parse_provider_metadata_json(whoami_body.as_str(), "CommandCode identity JSON")?;
    if !whoami.success || whoami.user.id.trim().is_empty() {
        return Err(String::from(
            "CommandCode identity response did not confirm an authenticated user.",
        ));
    }
    let org_id = whoami
        .org
        .as_ref()
        .map(|org| org.id.trim())
        .filter(|id| !id.is_empty());
    let credits_url = command_code_usage_url("/alpha/billing/credits", org_id, None)?;
    let subscription_url = command_code_usage_url("/alpha/billing/subscriptions", org_id, None)?;
    let (credits_result, subscription_result) = tokio::join!(
        send_command_code_usage_get(http, secret, credits_url, "credits"),
        send_command_code_usage_get(http, secret, subscription_url, "subscription"),
    );
    let credits_body = credits_result?;
    let subscription_body = subscription_result?;
    let subscription: CommandCodeSubscriptionResponse =
        parse_provider_metadata_json(subscription_body.as_str(), "CommandCode subscription JSON")?;
    if !subscription.success {
        return Err(String::from(
            "CommandCode subscription response reported an unsuccessful result.",
        ));
    }
    let since = subscription
        .data
        .as_ref()
        .map(|data| data.current_period_start.trim())
        .filter(|value| !value.is_empty());
    let summary_url = command_code_usage_url("/alpha/usage/summary", org_id, since)?;
    let summary_body =
        send_command_code_usage_get(http, secret, summary_url, "usage summary").await?;
    decode_command_code_usage(credits_body.as_str(), subscription, summary_body.as_str())
}

/// Builds one fixed-origin CommandCode usage URL with encoded optional query values.
/// 构建一条固定 Origin 的 CommandCode 用量 URL，并编码可选查询值。
///
/// # Parameters
/// 参数
/// `path` is a code-owned endpoint; `org_id` and `since` come from prior authenticated responses.
/// `path` 是代码持有的端点；`org_id` 与 `since` 来自前序已认证响应。
///
/// # Returns
/// 返回值
/// A parsed HTTPS URL restricted to the registered CommandCode production origin.
/// 返回限制在已注册 CommandCode 生产 Origin 的已解析 HTTPS URL。
fn command_code_usage_url(
    path: &str,
    org_id: Option<&str>,
    since: Option<&str>,
) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(COMMAND_CODE_API_ORIGIN)
        .map_err(|_| String::from("Registered CommandCode API origin is invalid."))?;
    url.set_path(path);
    {
        let mut query = url.query_pairs_mut();
        if let Some(org_id) = org_id {
            query.append_pair("orgId", org_id);
        }
        if let Some(since) = since {
            query.append_pair("since", since);
        }
    }
    Ok(url)
}

/// Sends one authenticated and bounded CommandCode usage request.
/// 发送一条已认证且有界的 CommandCode 用量请求。
///
/// # Parameters
/// 参数
/// `http` is the shared client, `secret` is the user key, `url` is fixed-origin, and `label`
/// is a code-owned diagnostic name.
/// `http` 是共享客户端，`secret` 是用户密钥，`url` 是固定 Origin 地址，`label`
/// 是代码持有的诊断名称。
///
/// # Returns
/// 返回值
/// The bounded successful JSON body or a controlled provider error.
/// 返回有界成功 JSON 正文或受控供应商错误。
async fn send_command_code_usage_get(
    http: &reqwest::Client,
    secret: &str,
    url: reqwest::Url,
    label: &str,
) -> Result<String, String> {
    let response = http
        .get(url.clone())
        .bearer_auth(secret)
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("CommandCode {label} request failed: {error}"))?;
    let status = response.status();
    let response_label = format!("CommandCode {label}");
    let body = read_bounded_provider_metadata_body(response, response_label.as_str(), url.as_str())
        .await?;
    if !status.is_success() {
        return Err(format!(
            "CommandCode {label} returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    Ok(body)
}

/// Decodes CommandCode credits, subscription, and summary responses without decimal loss.
/// 在不丢失十进制精度的情况下解码 CommandCode 额度、订阅与汇总响应。
///
/// # Parameters
/// 参数
/// `credits_body` and `summary_body` are successful JSON documents; `subscription` is the
/// already validated subscription observation used to select the billing-period window.
/// `credits_body` 与 `summary_body` 是成功 JSON 文档；`subscription` 是已经验证的订阅观测，
/// 用于选择计费周期窗口。
///
/// # Returns
/// 返回值
/// A strict provider-neutral credit and request-window snapshot.
/// 返回严格的供应商无关额度与请求窗口快照。
fn decode_command_code_usage(
    credits_body: &str,
    subscription: CommandCodeSubscriptionResponse,
    summary_body: &str,
) -> Result<AccountUsageSnapshot, String> {
    let credits: CommandCodeCreditsResponse =
        parse_provider_metadata_json(credits_body, "CommandCode credits JSON")?;
    let summary: CommandCodeUsageSummary =
        parse_provider_metadata_json(summary_body, "CommandCode usage summary JSON")?;
    let monthly_remaining =
        command_code_decimal(&credits.credits.monthly_credits, "credits.monthlyCredits")?;
    let purchased_remaining = command_code_decimal(
        &credits.credits.purchased_credits,
        "credits.purchasedCredits",
    )?;
    let free_remaining =
        command_code_decimal(&credits.credits.free_credits, "credits.freeCredits")?;
    let total_remaining = add_non_negative_decimals(&[
        monthly_remaining.as_str(),
        purchased_remaining.as_str(),
        free_remaining.as_str(),
    ])
    .map_err(|_| String::from("CommandCode credits contain an invalid decimal value."))?;
    let total_cost = command_code_decimal(&summary.total_cost, "summary.totalCost")?;
    let monthly_used = command_code_decimal(
        &summary.total_monthly_credits,
        "summary.totalMonthlyCredits",
    )?;
    let purchased_used = command_code_decimal(
        &summary.total_purchased_credits,
        "summary.totalPurchasedCredits",
    )?;
    let free_used = command_code_decimal(&summary.total_free_credits, "summary.totalFreeCredits")?;
    let period_start = subscription
        .data
        .as_ref()
        .map(|data| data.current_period_start.clone())
        .filter(|value| !value.trim().is_empty());
    let period_end = subscription
        .data
        .as_ref()
        .map(|data| data.current_period_end.clone())
        .filter(|value| !value.trim().is_empty());
    let mut quota_windows = vec![
        command_code_credit_window(
            "monthly-credits",
            "Monthly credits",
            monthly_used,
            monthly_remaining,
            period_start,
            period_end,
        )?,
        command_code_credit_window(
            "purchased-credits",
            "Purchased credits",
            purchased_used,
            purchased_remaining,
            None,
            None,
        )?,
        command_code_credit_window(
            "free-credits",
            "Free credits",
            free_used,
            free_remaining,
            None,
            None,
        )?,
    ];
    if credits.window_limits.limited {
        if let Some(window) = credits.window_limits.five_hour.as_ref() {
            quota_windows.push(command_code_request_window(
                "five-hour-requests",
                "5-hour requests",
                window,
                "windowLimits.fiveHour",
            )?);
        }
        if let Some(window) = credits.window_limits.weekly.as_ref() {
            quota_windows.push(command_code_request_window(
                "weekly-requests",
                "Weekly requests",
                window,
                "windowLimits.weekly",
            )?);
        }
    }
    let plan_id = subscription
        .data
        .as_ref()
        .filter(|data| matches!(data.status.as_str(), "active" | "trialing" | "past_due"))
        .map(|data| data.plan_id.trim().to_string())
        .filter(|value| !value.is_empty());
    let plan_name = plan_id
        .as_deref()
        .map(command_code_plan_name)
        .unwrap_or_else(|| String::from("CommandCode"));
    let refresh_message = if credits.credits.below_threshold {
        Some(String::from(
            "CommandCode reports that the remaining credit balance is below its configured threshold.",
        ))
    } else if credits
        .window_limits
        .five_hour
        .as_ref()
        .is_some_and(|window| window.exceeded)
        || credits
            .window_limits
            .weekly
            .as_ref()
            .is_some_and(|window| window.exceeded)
    {
        Some(String::from(
            "CommandCode reports that one or more request windows are exhausted.",
        ))
    } else if subscription
        .data
        .as_ref()
        .is_some_and(|data| !matches!(data.status.as_str(), "active" | "trialing"))
    {
        Some(String::from(
            "CommandCode reports that the subscription is not active.",
        ))
    } else {
        None
    };
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(plan_name),
        plan_id,
        used_amount: Some(total_cost),
        remaining_amount: Some(total_remaining.clone()),
        currency: Some(String::from("USD")),
        balance_amount: Some(total_remaining),
        quota_windows,
        observed_at: current_unix_milliseconds(),
        refresh_message,
    })
}

/// Validates one CommandCode decimal and preserves its exact JSON spelling.
/// 验证一个 CommandCode 十进制值并保留其精确 JSON 拼写。
///
/// # Parameters
/// 参数
/// `value` is the provider number and `field` is a code-owned diagnostic path.
/// `value` 是供应商数字，`field` 是代码持有的诊断路径。
///
/// # Returns
/// 返回值
/// A lossless non-negative decimal string.
/// 返回无损的非负十进制字符串。
fn command_code_decimal(value: &serde_json::Number, field: &str) -> Result<String, String> {
    let value = value.to_string();
    normalized_unsigned_decimal_digits(value.as_str())
        .map(|_| value)
        .map_err(|_| format!("CommandCode usage field `{field}` is not a non-negative decimal."))
}

/// Builds one CommandCode credit pool window from exact consumed and remaining values.
/// 根据精确已用量与剩余量构建一个 CommandCode 额度池窗口。
///
/// # Parameters
/// 参数
/// Identifiers label the row; `used` and `remaining` are exact USD values; timestamps are optional.
/// 标识用于命名行；`used` 与 `remaining` 是精确美元值；时间戳可选。
///
/// # Returns
/// 返回值
/// A credit window whose total is the exact sum of consumed and remaining credits.
/// 返回总量为已用与剩余额度精确之和的额度窗口。
fn command_code_credit_window(
    id: &str,
    display_name: &str,
    used: String,
    remaining: String,
    starts_at: Option<String>,
    resets_at: Option<String>,
) -> Result<AccountUsageWindow, String> {
    let total = add_non_negative_decimals(&[used.as_str(), remaining.as_str()])
        .map_err(|_| String::from("CommandCode credit pool contains an invalid decimal value."))?;
    Ok(AccountUsageWindow {
        id: id.to_string(),
        display_name: display_name.to_string(),
        bucket_name: Some(String::from("Account credits")),
        used: Some(used),
        remaining: Some(remaining),
        total: Some(total),
        remaining_percent: None,
        unit: String::from("USD"),
        starts_at,
        resets_at,
        unlimited: false,
        available: true,
    })
}

/// Builds one CommandCode rolling request window from the provider's exact counters.
/// 根据供应商精确计数构建一个 CommandCode 滚动请求窗口。
///
/// # Parameters
/// 参数
/// `id` and `display_name` label the row; `window` contains used, cap, exceeded, and reset data;
/// `field` identifies the provider-owned object in diagnostics.
/// `id` 与 `display_name` 用于命名行；`window` 包含已用、上限、超限与重置数据；`field`
/// 在诊断中标识供应商持有的对象。
///
/// # Returns
/// 返回值
/// A request-count quota window that preserves exact observed consumption without underflow.
/// 返回保留实际消费精度且不会下溢的请求次数额度窗口。
fn command_code_request_window(
    id: &str,
    display_name: &str,
    window: &CommandCodeRequestWindow,
    field: &str,
) -> Result<AccountUsageWindow, String> {
    let used = command_code_decimal(&window.used, format!("{field}.used").as_str())?;
    let total = window.cap.to_string();
    let remaining = match subtract_non_negative_decimal(total.as_str(), used.as_str()) {
        Ok(value) => value,
        Err(_) if window.exceeded => String::from("0"),
        Err(_) => {
            return Err(format!(
                "CommandCode usage field `{field}.used` exceeds `{field}.cap` without an exhausted-window declaration."
            ));
        }
    };
    Ok(AccountUsageWindow {
        id: id.to_string(),
        display_name: display_name.to_string(),
        bucket_name: Some(String::from("Rolling request limit")),
        used: Some(used),
        remaining: Some(remaining),
        total: Some(total),
        remaining_percent: None,
        unit: String::from("requests"),
        starts_at: None,
        resets_at: (window.reset_at > 0).then(|| window.reset_at.to_string()),
        unlimited: false,
        available: true,
    })
}

/// Maps one provider plan identifier to the display name used by the CommandCode CLI.
/// 将一个供应商套餐标识映射为 CommandCode CLI 使用的显示名称。
///
/// # Parameters
/// 参数
/// `plan_id` is the provider-owned subscription identifier.
/// `plan_id` 是供应商持有的订阅标识。
///
/// # Returns
/// 返回值
/// A known public plan label, or the exact provider identifier when the plan is new.
/// 返回已知公共套餐标签；若套餐为新增项则返回精确供应商标识。
fn command_code_plan_name(plan_id: &str) -> String {
    let normalized = plan_id.trim().to_ascii_lowercase().replace('_', "-");
    [
        ("individual-provider", "Provider"),
        ("individual-pro-v1", "Pro"),
        ("individual-ultra", "Ultra"),
        ("individual-goat", "GOAT"),
        ("individual-max", "Max"),
        ("individual-pro", "Pro"),
        ("individual-go", "Go"),
        ("teams-pro", "Teams Pro"),
    ]
    .into_iter()
    .find(|(prefix, _)| normalized.starts_with(prefix))
    .map(|(_, name)| name.to_string())
    .unwrap_or_else(|| plan_id.trim().to_string())
}

/// CommandCode authenticated identity response used to resolve organization scope.
/// 用于解析组织作用域的 CommandCode 已认证身份响应。
#[derive(Debug, Deserialize)]
struct CommandCodeWhoamiResponse {
    /// Provider success flag.
    /// 供应商成功标志。
    success: bool,
    /// Authenticated user identity subset.
    /// 已认证用户身份子集。
    user: CommandCodeUserIdentity,
    /// Optional owning organization.
    /// 可选所属组织。
    org: Option<CommandCodeOrganizationIdentity>,
}

/// Minimal authenticated CommandCode user identity.
/// 最小的已认证 CommandCode 用户身份。
#[derive(Debug, Deserialize)]
struct CommandCodeUserIdentity {
    /// Stable provider user identifier.
    /// 稳定供应商用户标识。
    id: String,
}

/// Minimal CommandCode organization identity used as a query scope.
/// 作为查询作用域使用的最小 CommandCode 组织身份。
#[derive(Debug, Deserialize)]
struct CommandCodeOrganizationIdentity {
    /// Stable provider organization identifier.
    /// 稳定供应商组织标识。
    id: String,
}

/// CommandCode remaining-credit and rolling-window response.
/// CommandCode 剩余额度与滚动窗口响应。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandCodeCreditsResponse {
    /// Remaining account credit pools.
    /// 账号剩余额度池。
    credits: CommandCodeCreditPools,
    /// Rolling request limits.
    /// 滚动请求限制。
    window_limits: CommandCodeWindowLimits,
}

/// CommandCode remaining account credit pools.
/// CommandCode 账号剩余额度池。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandCodeCreditPools {
    /// Whether the provider threshold warning is active.
    /// 供应商阈值警告是否生效。
    below_threshold: bool,
    /// Remaining monthly-plan credits.
    /// 月度套餐剩余额度。
    monthly_credits: serde_json::Number,
    /// Remaining purchased credits.
    /// 已购剩余额度。
    purchased_credits: serde_json::Number,
    /// Remaining free credits.
    /// 免费剩余额度。
    free_credits: serde_json::Number,
}

/// CommandCode rolling request-limit collection.
/// CommandCode 滚动请求限制集合。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandCodeWindowLimits {
    /// Whether rolling request limits apply to the account.
    /// 账号是否应用滚动请求限制。
    limited: bool,
    /// Five-hour request window.
    /// 五小时请求窗口。
    five_hour: Option<CommandCodeRequestWindow>,
    /// Weekly request window.
    /// 每周请求窗口。
    weekly: Option<CommandCodeRequestWindow>,
}

/// One exact CommandCode rolling request window.
/// 一个精确 CommandCode 滚动请求窗口。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandCodeRequestWindow {
    /// Exact consumption reported for the window; live responses may contain fractional usage.
    /// 窗口的实际已用量；实时响应可能包含小数消费值。
    used: serde_json::Number,
    /// Request cap for the window.
    /// 窗口请求上限。
    cap: u64,
    /// Whether the provider reports the window exhausted.
    /// 供应商是否报告窗口已耗尽。
    exceeded: bool,
    /// Provider Unix-millisecond reset timestamp; zero means absent.
    /// 供应商 Unix 毫秒重置时间戳；零表示缺失。
    reset_at: u64,
}

/// CommandCode subscription response.
/// CommandCode 订阅响应。
#[derive(Debug, Deserialize)]
struct CommandCodeSubscriptionResponse {
    /// Provider success flag.
    /// 供应商成功标志。
    success: bool,
    /// Optional active or historical subscription.
    /// 可选活动或历史订阅。
    data: Option<CommandCodeSubscription>,
}

/// CommandCode subscription fields required by usage normalization.
/// CommandCode 用量归一化所需的订阅字段。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandCodeSubscription {
    /// Provider-owned lifecycle status.
    /// 供应商持有的生命周期状态。
    status: String,
    /// Provider-owned plan identifier.
    /// 供应商持有的套餐标识。
    plan_id: String,
    /// Current billing-period start.
    /// 当前计费周期开始时间。
    current_period_start: String,
    /// Current billing-period end.
    /// 当前计费周期结束时间。
    current_period_end: String,
}

/// CommandCode billing-period usage summary subset.
/// CommandCode 计费周期用量汇总子集。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandCodeUsageSummary {
    /// Total monetary cost in the selected period.
    /// 所选周期内总金额成本。
    total_cost: serde_json::Number,
    /// Monthly-plan credits consumed in the selected period.
    /// 所选周期内已消费月度套餐额度。
    total_monthly_credits: serde_json::Number,
    /// Purchased credits consumed in the selected period.
    /// 所选周期内已消费购买额度。
    total_purchased_credits: serde_json::Number,
    /// Free credits consumed in the selected period.
    /// 所选周期内已消费免费额度。
    total_free_credits: serde_json::Number,
}

#[cfg(test)]
mod tests;
