//! Exact Go quota meters and independent credit balance from the current Console API.
//! 当前 Console API 的精确 Go 额度窗口与独立余额。

use super::current_unix_milliseconds;
use crate::{CredentialExecutionSecret, CredentialExecutionSecretKind, opencode};
use serde::{Deserialize, de::DeserializeOwned};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Number of provider microcents per USD, as declared by Console's money codec.
/// Console 金额编解码声明的每美元微美分数量。
const MICRO_CENTS_PER_USD: u128 = 100_000_000;

/// Current paid product, excluding unsupported or guessed future plans.
/// 当前已付费产品，不包含未经支持或猜测的未来套餐。
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Product {
    /// Standard Go subscription.
    /// 标准 Go 订阅。
    Go,
    /// Go Plus subscription.
    /// Go Plus 订阅。
    GoPlus,
}

/// Exact nullable subscription returned by GET /api/go/status.
/// GET /api/go/status 返回的精确可空订阅结构。
#[derive(Deserialize)]
struct Subscription {
    /// Paid product owning the windows.
    /// 持有额度窗口的付费产品。
    product: Product,
    /// Null when this subscription no longer owns an active paid period.
    /// 订阅不再持有有效付费周期时为 null。
    #[serde(deserialize_with = "required_nullable")]
    access: Option<Access>,
}

/// Active period containing the source-owned quota meters.
/// 包含来源持有额度窗口的有效周期。
#[derive(Deserialize)]
struct Access {
    /// Three explicitly named windows from the Console schema.
    /// Console Schema 明确命名的三个窗口。
    meters: Meters,
}

/// Five-hour, weekly, and paid-month windows from the upstream contract.
/// 上游契约中的五小时、周与付费月窗口。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meters {
    /// Five-hour meter; start and reset may be null before first use.
    /// 五小时窗口；首次使用前开始与重置时间可能为 null。
    five_hour: FiveHourMeter,
    /// Source-owned current weekly meter.
    /// 来源持有的当前周窗口。
    week: WeeklyMeter,
    /// Current paid-period month meter.
    /// 当前付费周期月窗口。
    month: MonthlyMeter,
}

/// Money fields shared by each explicitly typed upstream window.
/// 各个明确类型化上游窗口共用的金额字段。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meter {
    /// Exact non-negative integer encoded as a JSON string by the BigInt codec.
    /// BigInt 编解码以 JSON 字符串编码的精确非负整数上限。
    limit_micro_cents: String,
    /// Exact non-negative consumed amount encoded as a JSON string.
    /// 以 JSON 字符串编码的精确非负已用金额。
    used_micro_cents: String,
}

/// Five-hour timestamps are explicitly nullable before the first request.
/// 首个请求前五小时窗口的时间戳明确可为 null。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FiveHourMeter {
    /// Source-owned exact amounts.
    /// 来源持有的精确金额。
    #[serde(flatten)]
    amount: Meter,
    /// Required field whose value may be null.
    /// 必填字段，其值可为 null。
    #[serde(deserialize_with = "required_nullable")]
    starts_at: Option<String>,
    /// Required nullable reset field from Console's five-hour schema.
    /// Console 五小时 Schema 的必填可空重置字段。
    #[serde(deserialize_with = "required_nullable")]
    resets_at: Option<String>,
}

/// Weekly timestamps are required non-null strings in the production schema.
/// 生产 Schema 中周时间戳为必填非空值字符串。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WeeklyMeter {
    /// Source-owned exact amounts.
    /// 来源持有的精确金额。
    #[serde(flatten)]
    amount: Meter,
    /// Beginning of the source-owned weekly period.
    /// 来源持有的周周期开始时间。
    starts_at: String,
    /// End of the source-owned weekly period.
    /// 来源持有的周周期结束时间。
    resets_at: String,
}

/// Paid-month schema contains a reset timestamp and no start timestamp field.
/// 付费月 Schema 包含重置时间戳，不含开始时间戳字段。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MonthlyMeter {
    /// Source-owned exact amounts.
    /// 来源持有的精确金额。
    #[serde(flatten)]
    amount: Meter,
    /// End of the source-owned paid period.
    /// 来源持有的付费周期结束时间。
    resets_at: String,
}

/// Requires a field to exist while retaining the explicit null allowed by the Console schema.
/// 要求字段存在，同时保留 Console Schema 允许的显式 null。
/// `deserializer` owns one JSON value; returns its typed nullable value or a decoding error.
/// `deserializer` 持有单个 JSON 值；返回其类型化可空值或解码错误。
fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

/// Billing status is independent of the Go subscription allowance.
/// 计费状态独立于 Go 订阅额度。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Balance {
    /// Signed microcent credit balance as a JSON decimal-integer string.
    /// 以 JSON 十进制整数字符串表示的有符号微美分余额。
    balance_micro_cents: String,
}

/// Queries the Console workspace bound to an API key or renewable account credential.
/// 查询 API Key 或可续期账号凭据绑定的 Console 工作区。
/// `secret` selects the credential; OAuth must carry its validated OpenCode workspace.
/// `secret` 选择凭据；OAuth 必须携带其已验证的 OpenCode 工作区。
/// Returns all available Go windows, plus the independent credit balance for OAuth only.
/// 返回全部可用 Go 窗口，并且仅为 OAuth 返回独立余额。
pub(in crate::client) async fn query_opencode_go(
    secret: &CredentialExecutionSecret,
) -> Result<AccountUsageSnapshot, String> {
    // http refuses redirects so neither bearer tokens nor workspace identities leave the authority.
    // http 拒绝重定向，避免 Bearer 令牌或工作区身份离开授权端。
    let http = opencode::client()?;
    query_at(&http, opencode::CONSOLE_URL, secret).await
}

/// Reads the production API or a private synthetic server without selecting alternate endpoints.
/// 读取生产 API 或私有合成服务，不选择候选端点。
/// `http` and `origin` own transport; `secret` selects the exact authenticated workspace.
/// `http` 与 `origin` 持有传输；`secret` 选择精确已鉴权工作区。
/// Returns the credential's declared data; API keys skip balance and OAuth errors remain explicit.
/// 返回该凭据声明的数据；API Key 跳过余额，OAuth 错误保持显式可见。
async fn query_at(
    http: &reqwest::Client,
    origin: &str,
    secret: &CredentialExecutionSecret,
) -> Result<AccountUsageSnapshot, String> {
    // Console service keys already bind their workspace; OAuth requires its validated identity.
    // Console 服务密钥已绑定工作区；OAuth 必须使用其已验证身份。
    let organization_id = match secret.kind() {
        CredentialExecutionSecretKind::ApiKey => None,
        CredentialExecutionSecretKind::Bearer => Some(
            secret
                .opencode_organization_id()
                .ok_or("OpenCode Go OAuth usage requires a validated Console workspace.")?,
        ),
        CredentialExecutionSecretKind::OpenVulcanAuth => {
            return Err(String::from(
                "OpenCode Go usage requires a Console API key or OAuth credential.",
            ));
        }
    };
    // subscription is readable with either credential; no alternate endpoint is probed on failure.
    // subscription 可由两种凭据读取；失败时不探测候选端点。
    let subscription: Option<Subscription> = get(
        http,
        &format!("{origin}/api/go/status"),
        secret,
        organization_id,
    )
    .await?;
    // Only the OAuth branch has an explicit workspace and requests the independent credit balance.
    // 只有 OAuth 分支携带显式工作区并请求独立余额。
    let balance = if let Some(organization_id) = organization_id {
        Some(
            get::<Balance>(
                http,
                &format!("{origin}/api/billing/status"),
                secret,
                Some(organization_id),
            )
            .await?,
        )
    } else {
        None
    };
    normalize(subscription, balance)
}

/// Normalizes source-owned windows without converting integer money to floating point.
/// 归一化来源持有的窗口，不将整数金额转换为浮点数。
/// `subscription` may explicitly be null; `balance` is absent for API keys and queried for OAuth.
/// `subscription` 可显式为 null；`balance` 在 API Key 模式缺省，在 OAuth 模式查询。
/// Returns supported account data, retaining absent/expired subscriptions without fabricated windows.
/// 返回受支持账号数据，保留缺失或过期订阅状态而不虚构窗口。
fn normalize(
    subscription: Option<Subscription>,
    balance: Option<Balance>,
) -> Result<AccountUsageSnapshot, String> {
    // An unqueried balance remains absent, while queried balances preserve signed integer precision.
    // 未查询的余额保持缺省；已查询余额保留有符号整数精度。
    let balance_amount = if let Some(balance) = balance {
        // amount includes a permitted overdrawn account without using floating-point money.
        // amount 包含允许的账户透支，金额不使用浮点数。
        let amount = balance
            .balance_micro_cents
            .parse::<i128>()
            .map_err(|_| String::from("OpenCode returned an invalid credit balance."))?;
        Some(format!(
            "{}{}",
            if amount < 0 { "-" } else { "" },
            dollars(amount.unsigned_abs())
        ))
    } else {
        None
    };
    // snapshot keeps credit balance separate from subscription allowance.
    // snapshot 将余额与订阅额度分开保存。
    let mut snapshot = AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: None,
        plan_id: None,
        used_amount: None,
        remaining_amount: None,
        currency: Some(String::from("USD")),
        balance_amount,
        quota_windows: Vec::new(),
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    };
    // An absent subscription is an authoritative account state, not a zero-use active plan.
    // 缺失订阅是权威账号状态，不是已用量为零的有效套餐。
    let Some(subscription) = subscription else {
        snapshot.refresh_message = Some(String::from(
            "This workspace has no OpenCode Go subscription.",
        ));
        return Ok(snapshot);
    };
    // identity uses only the closed upstream product discriminator.
    // identity 仅使用封闭的上游产品标识。
    let (id, name) = match subscription.product {
        Product::Go => ("go", "OpenCode Go"),
        Product::GoPlus => ("go-plus", "OpenCode Go Plus"),
    };
    snapshot.plan_id = Some(id.to_string());
    snapshot.plan_name = Some(name.to_string());
    // Expired paid access must not be displayed as a fresh full allowance.
    // 已过期的付费访问不得展示为全新完整额度。
    let Some(access) = subscription.access else {
        snapshot.refresh_message = Some(String::from(
            "The OpenCode Go subscription has no active paid period.",
        ));
        return Ok(snapshot);
    };
    snapshot.quota_windows = vec![
        window(
            "five-hour",
            "5 hours",
            access.meters.five_hour.amount,
            access.meters.five_hour.starts_at,
            access.meters.five_hour.resets_at,
            Some(5 * 60 * 60),
        )?,
        window(
            "week",
            "Weekly",
            access.meters.week.amount,
            Some(access.meters.week.starts_at),
            Some(access.meters.week.resets_at),
            None,
        )?,
        window(
            "month",
            "Monthly",
            access.meters.month.amount,
            None,
            Some(access.meters.month.resets_at),
            None,
        )?,
    ];
    Ok(snapshot)
}

/// Converts one exact money meter to the shared quota-window contract.
/// 将单个精确金额窗口转换为共享额度窗口契约。
/// `id`, `label`, `meter`, source timestamps, and `duration` describe one explicitly selected window.
/// `id`、`label`、`meter`、来源时间戳与 `duration` 描述单个明确选定的窗口。
/// Returns exact USD values and source timestamps, or rejects malformed monetary/time data.
/// 返回精确美元数值与来源时间戳，或拒绝畸形金额、时间数据。
fn window(
    id: &str,
    label: &str,
    meter: Meter,
    starts_at: Option<String>,
    resets_at: Option<String>,
    duration: Option<u64>,
) -> Result<AccountUsageWindow, String> {
    // used and total never pass through JavaScript-style binary floating-point arithmetic.
    // used 与 total 绝不经过 JavaScript 式二进制浮点运算。
    let used = unsigned_integer(&meter.used_micro_cents)?;
    // total is the exact quota for this same source-owned meter.
    // total 是同一来源窗口的精确额度。
    let total = unsigned_integer(&meter.limit_micro_cents)?;
    // Timestamps retain their declared offsets instead of being reconstructed from period labels.
    // 时间戳保留声明的时区偏移，不根据周期标签重建。
    for timestamp in [starts_at.as_deref(), resets_at.as_deref()]
        .into_iter()
        .flatten()
    {
        OffsetDateTime::parse(timestamp, &Rfc3339)
            .map_err(|_| String::from("OpenCode returned an invalid quota timestamp."))?;
    }
    Ok(AccountUsageWindow {
        id: id.to_string(),
        display_name: label.to_string(),
        bucket_name: None,
        duration_seconds: duration,
        used: Some(dollars(used)),
        total: Some(dollars(total)),
        // Concurrent upstream requests can exceed a limit; the remaining allowance then equals zero.
        // 上游并发请求可能超出上限；此时剩余额度为零。
        remaining: Some(dollars(total.saturating_sub(used))),
        remaining_percent: None,
        unit: String::from("USD"),
        starts_at,
        resets_at,
        unlimited: false,
        available: true,
    })
}

/// Parses the upstream non-negative BigInt JSON string without alternate numeric encodings.
/// 解析上游非负 BigInt JSON 字符串，不接受候选数字编码。
/// `value` is the exact wire spelling; returns its integer value or a controlled format error.
/// `value` 为精确线路拼写；返回整数值或受控格式错误。
fn unsigned_integer(value: &str) -> Result<u128, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(String::from("OpenCode returned an invalid quota amount."));
    }
    value
        .parse()
        .map_err(|_| String::from("OpenCode quota amount exceeds the supported integer range."))
}

/// Formats microcents as an exact USD decimal with at most eight fractional places.
/// 将微美分格式化为最多八位小数的精确美元十进制数。
/// `value` is an unsigned integer amount; returns a normalized decimal string.
/// `value` 为无符号整数金额；返回规范化十进制字符串。
fn dollars(value: u128) -> String {
    // whole and fraction share the single authoritative provider unit conversion.
    // whole 与 fraction 共用唯一权威供应商单位换算。
    let whole = value / MICRO_CENTS_PER_USD;
    let fraction = value % MICRO_CENTS_PER_USD;
    if fraction == 0 {
        return whole.to_string();
    }
    format!("{whole}.{fraction:08}")
        .trim_end_matches('0')
        .to_string()
}

/// Performs one bounded account read using the key or token and its declared workspace scope.
/// 使用密钥或令牌及其声明的工作区作用域执行一次有界账号读取。
/// `http`, `url`, and `secret` define the request; `organization_id` is present only for OAuth.
/// `http`、`url` 与 `secret` 定义请求；`organization_id` 仅在 OAuth 模式存在。
/// Returns typed JSON; errors expose HTTP status only, never upstream bodies or credentials.
/// 返回类型化 JSON；错误仅公开 HTTP 状态，绝不公开上游正文或凭据。
async fn get<T: DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    secret: &CredentialExecutionSecret,
    organization_id: Option<&str>,
) -> Result<T, String> {
    // request omits the organization header for workspace-bound service keys.
    // request 对已绑定工作区的服务密钥省略组织请求头。
    let mut request = http.get(url).bearer_auth(secret.expose_value());
    if let Some(organization_id) = organization_id {
        request = request.header(opencode::ORGANIZATION_HEADER, organization_id);
    }
    // response cannot redirect because every caller supplies the closed Console client.
    // response 无法重定向，因为所有调用方都提供封闭的 Console 客户端。
    let response = request
        .send()
        .await
        .map_err(|_| String::from("OpenCode account usage request failed."))?;
    if !response.status().is_success() {
        return Err(format!(
            "OpenCode account usage returned HTTP {}.",
            response.status().as_u16()
        ));
    }
    crate::oauth::decode_bounded_oauth_json_response(response, "OpenCode account usage response")
        .await
}

#[cfg(test)]
mod tests;
