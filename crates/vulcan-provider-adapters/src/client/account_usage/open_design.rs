//! OpenDesign Design Plan windows and independent wallet balance from the approved account-control key.
//! 使用获批账户控制密钥查询 OpenDesign Design Plan 窗口与独立钱包余额。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::{current_unix_milliseconds, decimal::subtract_non_negative_decimal};
use crate::CredentialExecutionSecret;
use serde::{Deserialize, de::DeserializeOwned};
use std::collections::BTreeSet;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Authenticated member scope returned by the wallet endpoint before a workspace query.
/// 工作区查询前由钱包端点返回的已鉴权成员作用域。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WalletScope {
    /// Exact workspace whose member owns this control credential.
    /// 控制凭据所属成员的精确工作区。
    workspace_id: String,
    /// Exact member that the following preflight must also identify.
    /// 后续预检必须同样标识的精确成员。
    workspace_member_id: String,
}

/// Versioned account preflight observed from the official CLI and production service.
/// 通过官方 CLI 与生产服务观测的带版本账户预检。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BillingPreflight {
    /// Wire schema version, currently one.
    /// 线路结构版本，当前为一。
    schema_version: u32,
    /// Workspace selected by the authenticated wallet response.
    /// 已鉴权钱包响应选定的工作区。
    workspace_id: String,
    /// Member selected by the authenticated wallet response.
    /// 已鉴权钱包响应选定的成员。
    workspace_member_id: String,
    /// Account-level reads require the server's null model selection.
    /// 账户级读取要求服务端返回空模型选择。
    model_id: Option<String>,
    /// Wallet USD amount, independent of every plan credit window.
    /// 钱包美元金额，与每个套餐积分窗口彼此独立。
    balance_usd: String,
    /// Simultaneous plan limits belonging to the same authenticated workspace.
    /// 属于同一已鉴权工作区的并行套餐限制。
    coding_plan: CodingPlan,
}

/// Provider-owned membership and quota windows; nullable tiers are explicitly supported upstream.
/// 供应商持有的会员及额度窗口；上游明确支持空套餐标识。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodingPlan {
    /// Workspace whose plan policy produced these windows.
    /// 产生这些窗口的套餐策略所属工作区。
    workspace_id: String,
    /// Whether the current member is eligible for the plan allowance.
    /// 当前成员是否享有套餐额度。
    eligible: bool,
    /// Current plan identity, absent for accounts without a qualifying tier.
    /// 当前套餐身份；没有适用套餐的账户为空。
    tier: Option<PlanTier>,
    /// Independent limits, never summed into a wallet or an unlimited entitlement.
    /// 独立限制，绝不加总为钱包金额或无限权益。
    windows: Vec<PlanWindow>,
}

/// Exact tiers accepted by the upstream billing preflight contract.
/// 上游账单预检契约接受的精确套餐档位。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum PlanTier {
    /// Go membership.
    /// Go 会员。
    Go,
    /// Plus membership.
    /// Plus 会员。
    Plus,
    /// Pro membership.
    /// Pro 会员。
    Pro,
    /// Max membership.
    /// Max 会员。
    Max,
}

/// Exact reset semantics accepted by the upstream preflight parser.
/// 上游预检解析器接受的精确重置语义。
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ResetMode {
    /// Starts after qualifying account activity.
    /// 在符合条件的账户活动后开始。
    ActivityTriggered,
    /// Recurs on the provider's fixed anchor.
    /// 按供应商固定锚点循环。
    AnchoredRecurring,
}

/// One exact provider policy, preserving arbitrary-size integer credit strings.
/// 一个精确供应商策略，保留任意大小的整数积分字符串。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanWindow {
    /// Stable upstream policy identifier.
    /// 稳定上游策略标识。
    policy_id: String,
    /// Provider-defined whole-second duration used by localized presentation.
    /// 供本地化展示使用的供应商定义整秒周期。
    duration_seconds: u64,
    /// Validated reset policy; timestamps remain entirely server-owned.
    /// 已校验的重置策略；时间戳始终由服务端持有。
    #[serde(rename = "resetMode")]
    _reset_mode: ResetMode,
    /// Exact used credits, which can exceed a limit after concurrent requests.
    /// 精确已用积分，并发请求后可能超过上限。
    used_credits: String,
    /// Exact positive allowance.
    /// 精确的正数额度。
    limit_credits: String,
    /// Exact remaining allowance, bounded by the limit.
    /// 精确剩余额度，不得超过上限。
    remaining_credits: String,
    /// Optional start; an activity-triggered unused window can have no timestamp.
    /// 可选开始时间；未使用的活动触发窗口可以没有时间戳。
    window_start: Option<String>,
    /// Optional server-owned reset instant, never synthesized by the host.
    /// 可选服务端重置时刻，绝不由宿主推算。
    resets_at: Option<String>,
}

/// Queries the registered account authority with the dedicated control credential.
/// 使用专用控制凭据查询已注册的账户授权端。
///
/// `secret` must originate from approved OpenDesign Auth. Returns plan windows and a separate USD wallet.
/// `secret` 必须源自获批的 OpenDesign Auth。返回套餐窗口及独立美元钱包。
pub(in crate::client) async fn query_open_design_wallet(
    secret: &CredentialExecutionSecret,
) -> Result<AccountUsageSnapshot, String> {
    // Both reads use only the account-control credential.
    // 两次读取均只使用账户控制凭据。
    let control_key = secret.open_design_control_key()?;
    // The shared client disables redirects before either authenticated request.
    // 共享客户端在任一鉴权请求前禁用重定向。
    let http = crate::open_design::client()?;
    query_at(&http, crate::open_design::CONTROL_ORIGIN, control_key).await
}

/// Reads authenticated member scope, then its plan preflight, from the fixed authority or a test server.
/// 从固定授权端或测试服务读取已鉴权成员作用域及其套餐预检。
///
/// `http` rejects redirects, `origin` is module-owned, and `control_key` is the account credential.
/// `http` 拒绝重定向，`origin` 由模块持有，`control_key` 是账户凭据。
/// Returns one complete account observation or a controlled error without upstream body text.
/// 返回完整账户观测或不含上游正文的受控错误。
async fn query_at(
    http: &reqwest::Client,
    origin: &str,
    control_key: &str,
) -> Result<AccountUsageSnapshot, String> {
    // The authenticated wallet identifies the exact workspace and member to query.
    // 已鉴权钱包标识待查询的精确工作区及成员。
    let scope: WalletScope = account_json(
        http,
        &format!("{origin}/api/v1/wallet/balance"),
        control_key,
    )
    .await?;
    if scope.workspace_id.trim().is_empty() || scope.workspace_member_id.trim().is_empty() {
        return Err(String::from("OpenDesign wallet scope is invalid."));
    }
    // Encode the opaque workspace as one path segment, never as a caller-selected URL.
    // 将不透明工作区编码为单个路径段，绝不将其当作调用方选择的 URL。
    let mut url = reqwest::Url::parse(origin)
        .map_err(|_| String::from("OpenDesign account authority is invalid."))?;
    url.path_segments_mut()
        .map_err(|_| String::from("OpenDesign account authority cannot contain a workspace path."))?
        .extend([
            "api",
            "v1",
            "workspaces",
            &scope.workspace_id,
            "billing",
            "preflight",
        ]);
    // A failed plan read must not become a misleading wallet-only success.
    // 套餐读取失败不得变成具有误导性的仅钱包成功结果。
    let preflight = account_json(http, url.as_str(), control_key).await?;
    normalize_usage(&scope, preflight)
}

/// Normalizes simultaneous plan windows without treating their credits as wallet money.
/// 规范化并行套餐窗口，不将其积分视作钱包金额。
///
/// `scope` is the authenticated wallet identity and `preflight` is its live account response.
/// `scope` 是已鉴权钱包身份，`preflight` 是该身份的实时账户响应。
/// Returns exact quantities and period metadata, or an explicit scope or format error.
/// 返回精确数量及周期元数据，或明确的作用域或格式错误。
fn normalize_usage(
    scope: &WalletScope,
    preflight: BillingPreflight,
) -> Result<AccountUsageSnapshot, String> {
    if preflight.schema_version != 1
        || preflight.workspace_id != scope.workspace_id
        || preflight.workspace_member_id != scope.workspace_member_id
        || preflight.coding_plan.workspace_id != scope.workspace_id
        || preflight.model_id.is_some()
    {
        return Err(String::from(
            "OpenDesign plan usage does not match the authenticated account scope.",
        ));
    }
    // The official preflight permits signed decimals; the wallet remains independent of plan credits.
    // 官方预检允许带符号十进制数；钱包始终独立于套餐积分。
    let amount = preflight
        .balance_usd
        .strip_prefix('-')
        .unwrap_or(&preflight.balance_usd);
    // Each decimal component must contain at least one ASCII digit and no other character.
    // 每个十进制部分必须至少包含一位 ASCII 数字，且不含其他字符。
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    // A signed whole or fractional amount follows the exact upstream decimal contract.
    // 带符号的整数或小数金额遵循精确上游十进制契约。
    let valid_balance = match amount.split_once('.') {
        Some((whole, fraction)) => digits(whole) && digits(fraction),
        None => digits(amount),
    };
    if !valid_balance {
        return Err(String::from(
            "OpenDesign wallet balance contains an invalid decimal.",
        ));
    }
    // Membership comes from live preflight rather than the historical authorization response.
    // 会员信息来自实时预检，而不是历史授权响应。
    let plan = preflight.coding_plan;
    // Product names retain upstream casing while stable identifiers remain lowercase.
    // 产品名称保留上游大小写，稳定标识保持小写。
    let identity = match plan.tier {
        Some(PlanTier::Go) => Some(("go", "Go")),
        Some(PlanTier::Plus) => Some(("plus", "Plus")),
        Some(PlanTier::Pro) => Some(("pro", "Pro")),
        Some(PlanTier::Max) => Some(("max", "Max")),
        None => None,
    };
    // Stable policy identifiers survive changing array order and reset periods.
    // 稳定策略标识不受数组顺序及重置周期变化影响。
    let mut policy_ids = BTreeSet::new();
    // Every policy remains an independent quota window in the resulting observation.
    // 结果观测中的每个策略均保持为独立额度窗口。
    let mut windows = Vec::with_capacity(plan.windows.len());
    for window in plan.windows {
        if window.policy_id.trim().is_empty()
            || !policy_ids.insert(window.policy_id.clone())
            || window.duration_seconds == 0
            || window.duration_seconds > (1_u64 << f64::MANTISSA_DIGITS) - 1
        {
            return Err(String::from(
                "OpenDesign plan window identity or duration is invalid.",
            ));
        }
        // Exact integer strings avoid imposing a floating-point or fixed-width credit limit.
        // 精确整数字符串避免引入浮点数或固定宽度积分上限。
        if !digits(&window.used_credits)
            || !digits(&window.limit_credits)
            || !digits(&window.remaining_credits)
            || window.limit_credits.bytes().all(|byte| byte == b'0')
            || subtract_non_negative_decimal(&window.limit_credits, &window.remaining_credits)
                .is_err()
        {
            return Err(String::from("OpenDesign plan window credits are invalid."));
        }
        for timestamp in [window.window_start.as_deref(), window.resets_at.as_deref()]
            .into_iter()
            .flatten()
        {
            OffsetDateTime::parse(timestamp, &Rfc3339)
                .map_err(|_| String::from("OpenDesign plan window timestamp is invalid."))?;
        }
        windows.push(AccountUsageWindow {
            id: window.policy_id,
            display_name: String::from("Design Plan"),
            bucket_name: None,
            duration_seconds: Some(window.duration_seconds),
            used: Some(window.used_credits),
            remaining: Some(window.remaining_credits),
            total: Some(window.limit_credits),
            remaining_percent: None,
            unit: String::from("credits"),
            starts_at: window.window_start,
            resets_at: window.resets_at,
            unlimited: false,
            available: plan.eligible,
        });
    }
    windows.sort_by_key(|window| window.duration_seconds);
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: identity.map(|(_, name)| name.to_string()),
        plan_id: identity.map(|(id, _)| id.to_string()),
        used_amount: None,
        remaining_amount: None,
        balance_amount: Some(preflight.balance_usd),
        currency: Some(String::from("USD")),
        quota_windows: windows,
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// Reads bounded account JSON with closed status errors and no upstream body disclosure.
/// 读取有界账户 JSON，使用封闭状态错误且不泄漏上游正文。
///
/// `http`, `url`, and `control_key` belong to the registered query. Returns the requested wire type.
/// `http`、`url` 与 `control_key` 属于已注册查询。返回要求的线路类型。
async fn account_json<T: DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    control_key: &str,
) -> Result<T, String> {
    // Both body size and elapsed request time use the common provider metadata boundaries.
    // 正文大小及请求耗时均使用公共供应商元数据边界。
    let response = http
        .get(url)
        .bearer_auth(control_key)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|_| String::from("OpenDesign account usage request failed."))?;
    // Status is inspected before any untrusted response body is read.
    // 读取不可信响应正文前先检查状态。
    let status = response.status();
    if matches!(status.as_u16(), 401 | 403) {
        return Err(String::from(
            "OpenDesign account authorization expired; sign in again.",
        ));
    }
    if !status.is_success() {
        return Err(format!(
            "OpenDesign account usage returned HTTP {}.",
            status.as_u16()
        ));
    }
    // The shared reader enforces the existing account metadata response size bound.
    // 共享读取器实施既有账户元数据响应大小限制。
    let body =
        read_bounded_provider_metadata_body(response, "OpenDesign account usage", url).await?;
    parse_provider_metadata_json(&body, "OpenDesign account usage")
        .map_err(|_| String::from("OpenDesign account usage response was invalid."))
}

#[cfg(test)]
mod tests;
