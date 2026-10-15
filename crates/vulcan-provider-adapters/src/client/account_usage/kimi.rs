//! Kimi Coding Plan membership and exact usage-window normalization.
//! Kimi Coding Plan 会员及精确用量窗口归一化。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, controlled_error_summary, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::current_unix_milliseconds;
use super::decimal::{normalized_unsigned_decimal_digits, subtract_non_negative_decimal};
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Queries Kimi Coding Plan membership and rolling usage with the verified CLI identity.
/// 使用已验证 CLI 身份查询 Kimi Coding Plan 会员与滚动用量。
pub(in crate::client) async fn query_kimi_coding_plan(
    http: &reqwest::Client,
    secret: &str,
    device_id: Option<&str>,
) -> Result<AccountUsageSnapshot, String> {
    // url is the verified fixed Kimi Coding Plan usage endpoint.
    // url 是已验证的固定 Kimi Coding Plan 用量入口。
    let url = "https://api.kimi.com/coding/v1/usages";
    let response = http
        .get(url)
        .bearer_auth(secret)
        .header("user-agent", "KimiCLI/1.10.6")
        .header("x-msh-platform", "kimi_cli")
        .header("x-msh-version", "1.10.6")
        .header("x-msh-device-name", kimi_device_name())
        .header(
            "x-msh-device-model",
            format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        )
        .header(
            "x-msh-device-id",
            device_id.unwrap_or("cli-proxy-api-device"),
        )
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("Kimi Coding Plan usage request failed: {error}"))?;
    let status = response.status();
    let body = read_bounded_provider_metadata_body(response, "Kimi Coding Plan usage", url).await?;
    if !status.is_success() {
        return Err(format!(
            "Kimi Coding Plan usage returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(body.as_str(), secret)
        ));
    }
    decode_kimi_coding_plan_usage(body.as_str())
}

/// Decodes one exact Kimi Coding Plan usage response into provider-neutral windows.
/// 将一份精确 Kimi Coding Plan 用量响应解码为供应商无关窗口。
fn decode_kimi_coding_plan_usage(body: &str) -> Result<AccountUsageSnapshot, String> {
    let response: KimiUsageResponse =
        parse_provider_metadata_json(body, "Kimi Coding Plan usage JSON")?;
    let plan_name = kimi_membership_plan_name(response.user.membership.level.as_deref())?;
    let mut windows =
        Vec::with_capacity(response.limits.len() + usize::from(response.usage.is_some()));
    if let Some(usage) = response.usage
        && let Some(window) = kimi_usage_window("weekly_usage", "weekly_usage", usage, None)?
    {
        windows.push(window);
    }
    for (index, mut item) in response.limits.into_iter().enumerate() {
        let provider_name = [
            item.name.as_deref(),
            item.title.as_deref(),
            item.scope.as_deref(),
            item.detail
                .as_ref()
                .and_then(|detail| detail.name.as_deref()),
            item.detail
                .as_ref()
                .and_then(|detail| detail.title.as_deref()),
        ]
        .into_iter()
        .flatten()
        .find(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string());
        let inline_duration = item.inline.duration.take();
        let inline_time_unit = item
            .inline
            .time_unit
            .take()
            .filter(|value| !value.trim().is_empty());
        let mut detail = item.detail.unwrap_or(item.inline);
        let window = item.window.or_else(|| {
            inline_duration
                .or_else(|| detail.duration.take())
                .map(|duration| KimiUsageWindow {
                    duration: Some(duration),
                    time_unit: inline_time_unit.or_else(|| {
                        detail
                            .time_unit
                            .take()
                            .filter(|value| !value.trim().is_empty())
                    }),
                })
        });
        let metric = provider_name
            .as_deref()
            .map(kimi_metric_identifier)
            .unwrap_or_else(|| kimi_unnamed_limit_metric(window.as_ref(), index));
        let display_name = provider_name.unwrap_or_else(|| metric.clone());
        if let Some(window) =
            kimi_usage_window(metric.as_str(), display_name.as_str(), detail, window)?
        {
            windows.push(window);
        }
    }
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(plan_name),
        plan_id: None,
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows: windows,
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// Resolves Kimi's exact provider membership enum to the public plan name used by ModelCore.
/// 将 Kimi 的精确供应商会员枚举解析为 ModelCore 使用的公开套餐名称。
fn kimi_membership_plan_name(provider_level: Option<&str>) -> Result<String, String> {
    match provider_level
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        None => Ok(String::from("Kimi Coding Plan")),
        Some("LEVEL_TRIAL") => Ok(String::from("Andante")),
        Some("LEVEL_BASIC") => Ok(String::from("Moderato")),
        Some("LEVEL_INTERMEDIATE") => Ok(String::from("Allegretto")),
        Some("LEVEL_ADVANCED") => Ok(String::from("Allegro")),
        Some(value) => Err(format!(
            "Kimi Coding Plan usage returned unknown membership level `{value}`."
        )),
    }
}

/// Converts one Kimi counter into a neutral window without inventing absent limits.
/// 将一个 Kimi 计数转换为中立窗口且不臆造缺失上限。
fn kimi_usage_window(
    id: &str,
    display_name: &str,
    detail: KimiUsageDetail,
    window: Option<KimiUsageWindow>,
) -> Result<Option<AccountUsageWindow>, String> {
    let used = detail.used.map(KimiDecimal::exact).transpose()?;
    let total = detail.limit.map(KimiDecimal::exact).transpose()?;
    let remaining = match detail.remaining.map(KimiDecimal::exact).transpose()? {
        Some(value) => Some(value),
        None => match (total.as_deref(), used.as_deref()) {
            (Some(total), Some(used)) => Some(subtract_non_negative_decimal(total, used)?),
            _ => None,
        },
    };
    if used.is_none() && total.is_none() && remaining.is_none() {
        return Ok(None);
    }
    let reset = [
        detail.reset_at,
        detail.reset_at_snake,
        detail.reset_time,
        detail.reset_time_snake,
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.trim().is_empty());
    let bucket_name = window.map(|window| {
        format!(
            "{} {}",
            window
                .duration
                .map_or_else(String::new, KimiDecimal::lossless),
            window.time_unit.unwrap_or_default()
        )
        .trim()
        .to_string()
    });
    Ok(Some(AccountUsageWindow {
        id: id.to_string(),
        display_name: display_name.to_string(),
        bucket_name: bucket_name.filter(|value| !value.is_empty()),
        duration_seconds: None,
        used,
        remaining,
        total,
        remaining_percent: None,
        unit: String::from("provider_defined"),
        starts_at: None,
        resets_at: reset,
        unlimited: false,
        available: true,
    }))
}

/// Normalizes one provider-owned Kimi metric with ModelCore's exact ASCII token rules.
/// 使用 ModelCore 的精确 ASCII 分词规则规范化一个供应商拥有的 Kimi 指标。
fn kimi_metric_identifier(value: &str) -> String {
    let parts = value
        .trim()
        .to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        String::from("provider_limit")
    } else {
        parts.join("_")
    }
}

/// Identifies an unnamed Kimi limit from its provider window using ModelCore's metric rules.
/// 使用 ModelCore 的指标规则根据供应商窗口标识一个未命名的 Kimi 限制。
fn kimi_unnamed_limit_metric(window: Option<&KimiUsageWindow>, index: usize) -> String {
    let fallback = || format!("limit_{}", index + 1);
    let Some(window) = window else {
        return fallback();
    };
    let Some(duration) = window.duration.as_ref().and_then(KimiDecimal::as_f64) else {
        return fallback();
    };
    let multiplier = match window
        .time_unit
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase()
        .as_str()
    {
        "" | "SECONDS" => 1.0,
        "MINUTES" | "TIME_UNIT_MINUTE" => 60.0,
        "HOURS" => 60.0 * 60.0,
        "DAYS" => 24.0 * 60.0 * 60.0,
        _ => return fallback(),
    };
    if duration * multiplier == 5.0 * 60.0 * 60.0 {
        String::from("five_hour_usage")
    } else {
        fallback()
    }
}

/// Resolves the exact current device name used in Kimi metadata headers.
/// 解析 Kimi 元数据请求头使用的精确当前设备名称。
fn kimi_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| String::from("unknown"))
}

/// Accepts Kimi's exact number-or-string decimal representation.
/// 接受 Kimi 精确的数字或字符串十进制表示。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum KimiDecimal {
    /// JSON number representation.
    /// JSON 数字表示。
    Number(serde_json::Number),
    /// JSON string representation.
    /// JSON 字符串表示。
    String(String),
}

impl KimiDecimal {
    /// Converts one provider decimal to a finite floating-point value for duration matching.
    /// 将一个供应商十进制数转换为用于时长匹配的有限浮点值。
    fn as_f64(&self) -> Option<f64> {
        let value = match self {
            Self::Number(value) => value.as_f64(),
            Self::String(value) => value.parse::<f64>().ok(),
        }?;
        value.is_finite().then_some(value)
    }

    /// Returns one validated non-negative exact decimal string.
    /// 返回一个经过校验的非负精确十进制字符串。
    fn exact(self) -> Result<String, String> {
        let value = self.lossless();
        normalized_unsigned_decimal_digits(value.as_str())
            .map(|_| value)
            .map_err(|_| String::from("Kimi Coding Plan usage contains an invalid decimal value."))
    }

    /// Preserves the provider's lossless decimal spelling.
    /// 保留供应商无损十进制拼写。
    fn lossless(self) -> String {
        match self {
            Self::Number(value) => value.to_string(),
            Self::String(value) => value,
        }
    }
}

/// Mirrors one exact Kimi usage-counter subset.
/// 镜像一组精确的 Kimi 用量计数字段子集。
#[derive(Debug, Default, Deserialize)]
struct KimiUsageDetail {
    /// Consumed provider quantity.
    /// 已消费供应商数量。
    #[serde(default)]
    used: Option<KimiDecimal>,
    /// Maximum provider quantity.
    /// 供应商数量上限。
    #[serde(default)]
    limit: Option<KimiDecimal>,
    /// Provider-reported available quantity.
    /// 供应商报告的可用数量。
    #[serde(default)]
    remaining: Option<KimiDecimal>,
    /// Optional metric name.
    /// 可选指标名称。
    #[serde(default)]
    name: Option<String>,
    /// Optional display title.
    /// 可选显示标题。
    #[serde(default)]
    title: Option<String>,
    /// Camel-case absolute reset time.
    /// 驼峰形式绝对重置时间。
    #[serde(default, rename = "resetAt")]
    reset_at: Option<String>,
    /// Snake-case absolute reset time.
    /// 下划线形式绝对重置时间。
    #[serde(default, rename = "reset_at")]
    reset_at_snake: Option<String>,
    /// Alternate camel-case reset time.
    /// 备用驼峰形式重置时间。
    #[serde(default, rename = "resetTime")]
    reset_time: Option<String>,
    /// Alternate snake-case reset time.
    /// 备用下划线形式重置时间。
    #[serde(default, rename = "reset_time")]
    reset_time_snake: Option<String>,
    /// Alternate inline rolling-window duration.
    /// 备用内联滚动窗口时长。
    #[serde(default)]
    duration: Option<KimiDecimal>,
    /// Alternate inline rolling-window unit.
    /// 备用内联滚动窗口单位。
    #[serde(default, rename = "timeUnit", alias = "time_unit")]
    time_unit: Option<String>,
}

/// Mirrors one Kimi rolling-window descriptor.
/// 镜像一个 Kimi 滚动窗口描述。
#[derive(Debug, Deserialize)]
struct KimiUsageWindow {
    /// Positive duration magnitude.
    /// 正数时长数值。
    #[serde(default)]
    duration: Option<KimiDecimal>,
    /// Provider duration unit.
    /// 供应商时长单位。
    #[serde(default, rename = "timeUnit", alias = "time_unit")]
    time_unit: Option<String>,
}

/// Mirrors one named Kimi usage limit.
/// 镜像一个具名 Kimi 用量限制。
#[derive(Debug, Deserialize)]
struct KimiUsageLimit {
    /// Inline legacy counter fields.
    /// 内联旧版计数字段。
    #[serde(flatten)]
    inline: KimiUsageDetail,
    /// Optional fallback scope name.
    /// 可选备用 Scope 名称。
    #[serde(default)]
    scope: Option<String>,
    /// Optional nested counter fields.
    /// 可选嵌套计数字段。
    #[serde(default)]
    detail: Option<KimiUsageDetail>,
    /// Optional explicit rolling-window descriptor.
    /// 可选显式滚动窗口描述。
    #[serde(default)]
    window: Option<KimiUsageWindow>,
    /// Optional inline name copied before nested-detail selection.
    /// 在嵌套 Detail 选择前复制的可选内联名称。
    #[serde(default)]
    name: Option<String>,
    /// Optional inline title copied before nested-detail selection.
    /// 在嵌套 Detail 选择前复制的可选内联标题。
    #[serde(default)]
    title: Option<String>,
}

/// Mirrors the exact Kimi account usage response.
/// 镜像精确的 Kimi 账号用量响应。
#[derive(Debug, Deserialize)]
struct KimiUsageResponse {
    /// Optional account-summary counter.
    /// 可选账号汇总计数。
    #[serde(default)]
    usage: Option<KimiUsageDetail>,
    /// Detailed usage windows.
    /// 详细用量窗口。
    #[serde(default)]
    limits: Vec<KimiUsageLimit>,
    /// Account membership observation.
    /// 账号会员观测。
    #[serde(default)]
    user: KimiUsageUser,
}

/// Mirrors the exact Kimi account user subset.
/// 镜像精确的 Kimi 账号用户字段子集。
#[derive(Debug, Default, Deserialize)]
struct KimiUsageUser {
    /// Current membership facts.
    /// 当前会员事实。
    #[serde(default)]
    membership: KimiUsageMembership,
}

/// Mirrors the exact Kimi membership subset.
/// 镜像精确的 Kimi 会员字段子集。
#[derive(Debug, Default, Deserialize)]
struct KimiUsageMembership {
    /// Provider-owned membership level.
    /// 供应商拥有的会员等级。
    #[serde(default)]
    level: Option<String>,
}

#[cfg(test)]
mod tests;
