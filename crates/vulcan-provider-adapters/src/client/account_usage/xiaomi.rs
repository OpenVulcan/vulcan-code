//! Xiaomi MiMo finance API balance and personal Token Plan credit observations.
//! 小米 MiMo 财务接口余额与个人 Token Plan 积分观测。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::current_unix_milliseconds;
use super::decimal::normalized_unsigned_decimal_digits;
use crate::{AccountUsageQueryKind, ServiceEntry, xiaomi::MiMoService};
use serde::Deserialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Tagged response contract for the two MiMo finance endpoints.
/// 两个 MiMo 财务端点的带标签响应契约。
#[derive(Deserialize)]
#[serde(tag = "object", rename_all = "snake_case")]
enum UsageResponse {
    /// Cash, gift, and credit-line balance reported as exact decimal strings.
    /// 以精确十进制字符串报告的现金、赠金与授信余额。
    Balance {
        /// Whether the account can spend from this balance after any freeze.
        /// 扣除冻结金额后账户是否可使用此余额。
        is_available: bool,
        /// MiMo documents one account currency per response.
        /// MiMo 文档规定每份响应包含一种账户币种。
        balance_infos: [BalanceInfo; 1],
    },
    /// Credits and period for a personal Token Plan.
    /// 个人 Token Plan 的积分及周期。
    TokenPlan {
        /// Provider-owned subscription identifier.
        /// 供应商持有的订阅标识。
        plan_code: String,
        /// Exact start and end of the current billing period.
        /// 当前计费周期的精确起止时间。
        period: Period,
        /// Official credit counts for that period.
        /// 此周期的官方积分计数。
        quota: Quota,
    },
}

/// One account balance in the currency returned by MiMo.
/// 以 MiMo 返回的币种计量的账户余额。
#[derive(Deserialize)]
struct BalanceInfo {
    /// ISO currency code for the account.
    /// 账户使用的 ISO 币种代码。
    currency: String,
    /// Provider-reported total available balance.
    /// 供应商报告的总可用余额。
    total_available: String,
}

/// MiMo billing cycle dates expressed in UTC.
/// 以 UTC 表达的 MiMo 计费周期日期。
#[derive(Deserialize)]
struct Period {
    /// Inclusive start time in the provider's UTC format.
    /// 采用供应商 UTC 格式的起始时间。
    start_at: String,
    /// End time in the provider's UTC format.
    /// 采用供应商 UTC 格式的结束时间。
    end_at: String,
}

/// MiMo credit counts retained as integers before projection to exact strings.
/// 投影为精确字符串之前以整数保留的 MiMo 积分计数。
#[derive(Deserialize)]
struct Quota {
    /// Total credits available in the billing cycle including compensation.
    /// 本计费周期内含补偿的总积分。
    total_credits: u64,
    /// Credits already consumed in the billing cycle.
    /// 本计费周期内已消耗的积分。
    used_credits: u64,
    /// Credits left in the billing cycle according to the provider.
    /// 供应商报告的本计费周期剩余积分。
    remaining_credits: u64,
}

/// Queries the finance endpoint pinned to the selected Xiaomi MiMo service.
/// 查询固定在所选小米 MiMo 服务上的财务端点。
/// `http` is the metadata transport; `entry`, `secret` and `kind` fix the request scope.
/// `http` 为元数据传输；`entry`、`secret`、`kind` 固定请求作用域。
/// Returns the provider-neutral balance or credit snapshot.
/// 返回供应商无关的余额或积分观测。
pub(in crate::client) async fn query_xiaomi_usage(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
    kind: AccountUsageQueryKind,
) -> Result<AccountUsageSnapshot, String> {
    // service prohibits sending a credential to an unrecognized host or product.
    // service 禁止把凭据发送到未识别的主机或产品。
    let service = MiMoService::for_base_url(&entry.base_url)?;
    // path follows the finance API contract for the selected product and cluster.
    // path 遵循所选产品和集群的财务接口契约。
    let path = match (service, kind) {
        (MiMoService::PayAsYouGo, AccountUsageQueryKind::XiaomiMimoBalance) => "/platform/balance",
        (MiMoService::PersonalTokenPlan, AccountUsageQueryKind::XiaomiMimoTokenPlan) => {
            "/platform/token_plan"
        }
        _ => {
            return Err(String::from(
                "Xiaomi MiMo usage does not match the selected service.",
            ));
        }
    };
    if !secret
        .strip_prefix(service.key_prefix())
        .is_some_and(|suffix| !suffix.is_empty())
    {
        return Err(String::from(
            "Xiaomi MiMo usage requires the API key for the selected product.",
        ));
    }
    // url retains the selected regional hostname and the official /v1 base path.
    // url 保留所选区域主机名及官方 /v1 基础路径。
    let url = format!("{}{path}", entry.base_url);
    // response is bounded by the shared metadata deadline and response size policy.
    // response 受共享元数据截止时间及响应大小策略约束。
    let response = http
        .get(&url)
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|_| String::from("Xiaomi MiMo account usage request failed."))?;
    // status is safe to expose without echoing any upstream error body.
    // status 可安全显示，无需回显任何上游错误正文。
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Xiaomi MiMo account usage returned HTTP {}.",
            status.as_u16()
        ));
    }
    // body is the bounded successful provider response.
    // body 为受大小限制的供应商成功响应。
    let body =
        read_bounded_provider_metadata_body(response, "Xiaomi MiMo account usage", &url).await?;
    decode_usage(&body, service)
}

/// Decodes one successful MiMo response as a monetary balance or one credit window.
/// 将一份 MiMo 成功响应解码为货币余额或一个积分窗口。
/// `body` is the response JSON and `service` is the route fixed before sending the request.
/// `body` 为响应 JSON，`service` 为发出请求前固定的路由。
fn decode_usage(body: &str, service: MiMoService) -> Result<AccountUsageSnapshot, String> {
    // response validates the documented object discriminator and exact required fields.
    // response 校验文档规定的对象判别字段及精确必填字段。
    let response = parse_provider_metadata_json(body, "Xiaomi MiMo account usage JSON")?;
    // snapshot is credential-neutral until the shared client attaches the credential identifier.
    // snapshot 在共享客户端附加凭据标识前与具体凭据无关。
    let mut snapshot = AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: None,
        plan_id: None,
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows: Vec::new(),
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    };
    match (service, response) {
        (
            MiMoService::PayAsYouGo,
            UsageResponse::Balance {
                is_available,
                balance_infos: [balance],
            },
        ) => {
            // A signed total is valid for a postpaid account in arrears.
            // 后付费账号欠费时允许出现带负号的总余额。
            if !matches!(balance.currency.as_str(), "CNY" | "USD")
                || normalized_unsigned_decimal_digits(
                    balance
                        .total_available
                        .strip_prefix('-')
                        .unwrap_or(&balance.total_available),
                )
                .is_err()
            {
                return Err(String::from("Xiaomi MiMo returned an invalid balance."));
            }
            snapshot.currency = Some(balance.currency);
            snapshot.remaining_amount = Some(balance.total_available.clone());
            snapshot.balance_amount = Some(balance.total_available);
            snapshot.refresh_message = (!is_available).then(|| {
                String::from("Xiaomi MiMo reports that the account balance is unavailable.")
            });
        }
        (
            MiMoService::PersonalTokenPlan,
            UsageResponse::TokenPlan {
                plan_code,
                period,
                quota,
            },
        ) => {
            // starts_at and ends_at validate the provider's explicit UTC billing period.
            // starts_at 与 ends_at 校验供应商明确提供的 UTC 计费周期。
            let starts_at = parse_period_time(&period.start_at)?;
            let ends_at = parse_period_time(&period.end_at)?;
            if plan_code.trim().is_empty()
                || starts_at >= ends_at
                || quota.remaining_credits != quota.total_credits.saturating_sub(quota.used_credits)
            {
                return Err(String::from(
                    "Xiaomi MiMo returned an invalid Token Plan period or quota.",
                ));
            }
            snapshot.plan_id = Some(plan_code.clone());
            snapshot.plan_name = Some(plan_code);
            snapshot.quota_windows.push(AccountUsageWindow {
                id: String::from("billing-period"),
                display_name: String::from("MiMo Token Plan"),
                bucket_name: None,
                duration_seconds: Some((ends_at - starts_at).whole_seconds() as u64),
                used: Some(quota.used_credits.to_string()),
                remaining: Some(quota.remaining_credits.to_string()),
                total: Some(quota.total_credits.to_string()),
                remaining_percent: None,
                unit: String::from("credits"),
                starts_at: Some((starts_at.unix_timestamp() * 1_000).to_string()),
                resets_at: Some((ends_at.unix_timestamp() * 1_000).to_string()),
                unlimited: false,
                available: true,
            });
        }
        _ => {
            return Err(String::from(
                "Xiaomi MiMo returned an unexpected account usage object.",
            ));
        }
    }
    Ok(snapshot)
}

/// Interprets one MiMo period timestamp in the documented UTC format.
/// 按文档规定的 UTC 格式解释一个 MiMo 周期时间戳。
/// `value` is a UTC date and time with a space separator; returns a validated UTC instant.
/// `value` 为以空格分隔的 UTC 日期时间；返回已验证的 UTC 时刻。
fn parse_period_time(value: &str) -> Result<OffsetDateTime, String> {
    // RFC 3339 parsing validates the calendar fields after making the UTC zone explicit.
    // 明确 UTC 时区后，RFC 3339 解析会校验日历字段。
    OffsetDateTime::parse(&format!("{}Z", value.replacen(' ', "T", 1)), &Rfc3339)
        .map_err(|_| String::from("Xiaomi MiMo returned an invalid UTC billing timestamp."))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps an international account's signed, exact balance and currency.
    /// 保留国际账号精确的带符号余额和币种。
    #[test]
    fn xiaomi_balance_preserves_signed_currency_amount() {
        // snapshot projects the official balance response without converting it to a number.
        // snapshot 投影官方余额响应，不将其转换为数值。
        let snapshot = decode_usage(
            r#"{"object":"balance","is_available":false,"balance_infos":[{"currency":"USD","total_available":"-0.25"}]}"#,
            MiMoService::PayAsYouGo,
        ).unwrap();
        assert_eq!(snapshot.balance_amount.as_deref(), Some("-0.25"));
        assert_eq!(snapshot.currency.as_deref(), Some("USD"));
        assert!(snapshot.refresh_message.is_some());
        assert!(snapshot.quota_windows.is_empty());
    }

    /// Preserves the plan's independent credit balance and UTC reset window.
    /// 保留套餐独立的积分余量和 UTC 重置窗口。
    #[test]
    fn xiaomi_plan_keeps_exact_credits_and_reset() {
        // snapshot exercises a remaining credit amount larger than JavaScript's safe integer.
        // snapshot 使用超过 JavaScript 安全整数范围的剩余积分。
        let snapshot = decode_usage(
            r#"{"object":"token_plan","plan_code":"pro","period":{"start_at":"2026-11-01 00:00:00","end_at":"2026-12-01 00:00:00"},"quota":{"total_credits":9007199254740995,"used_credits":2,"remaining_credits":9007199254740993}}"#,
            MiMoService::PersonalTokenPlan,
        ).unwrap();
        // window is the billing-period window for the plan.
        // window 是套餐的计费周期窗口。
        let window = snapshot
            .quota_windows
            .iter()
            .find(|window| window.id == "billing-period")
            .unwrap();
        assert_eq!(window.remaining.as_deref(), Some("9007199254740993"));
        assert_eq!(window.used.as_deref(), Some("2"));
        assert_eq!(window.total.as_deref(), Some("9007199254740995"));
        assert_eq!(window.resets_at.as_deref(), Some("1796083200000"));
        assert!(snapshot.balance_amount.is_none());
    }
}
