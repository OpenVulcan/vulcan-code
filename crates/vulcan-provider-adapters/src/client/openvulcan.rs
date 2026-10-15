//! Account and exact point-balance reads for native OpenVulcan authorization.
//! 原生 OpenVulcan 授权的账户及准确积分余额读取。

use super::http::send_metadata_get_with_authentication;
use crate::{AuthenticationProjection, CredentialExecutionSecret, ServiceEntry};
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot, AccountUsageWindow};

/// Safe public account identity returned to the desktop.
/// 返回桌面端的安全公开账户身份。
#[derive(Clone, Debug, Deserialize, serde::Serialize)]
pub struct OpenVulcanAccountIdentity {
    /// Decimal public user identifier, never converted through floating point.
    /// 十进制公开用户标识，绝不经过浮点转换。
    pub id: String,
    /// Exact user name.
    /// 准确用户名。
    pub username: String,
    /// User-selected display name.
    /// 用户选择的显示名称。
    pub nickname: String,
}

/// One server-owned page of privacy-safe hourly usage; decimal counts remain exact strings.
/// 一页由服务端持有的隐私安全小时用量；十进制计数保留准确字符串。
#[derive(Deserialize, serde::Serialize)]
pub struct OpenVulcanUsagePage {
    /// Inclusive calendar interval owned by the server.
    /// 服务端持有的包含式日历区间。
    pub from_date: String,
    /// Inclusive final date of the server-owned interval.
    /// 服务端区间的包含式结束日期。
    pub to_date: String,
    /// Stable model/hour aggregates, never prompts or provider routing secrets.
    /// 稳定模型／小时聚合，绝不包含提示词或供应商路由秘密。
    pub usage: Vec<OpenVulcanUsageRow>,
    /// Opaque owner-bound continuation, empty at the final page.
    /// 绑定所有者的不透明续页游标，末页为空。
    pub next_cursor: String,
}

/// Exact public aggregation row from the user-center usage contract.
/// 用户中心用量契约中的准确公开聚合行。
#[derive(Deserialize, serde::Serialize)]
pub struct OpenVulcanUsageRow {
    /// Canonical UTC hour and public model identity.
    /// 规范 UTC 小时及公开模型身份。
    pub hour_start: String,
    /// Stable public model identity, independent from upstream routing.
    /// 独立于上游路由的稳定公开模型身份。
    pub model_id: String,
    /// Exact total, success and failure counts.
    /// 准确总次数、成功次数及失败次数。
    pub request_count: String,
    /// Exact count of recognized successful requests.
    /// 准确的已识别成功请求数。
    pub successful_requests: String,
    /// Exact count of recognized failed requests.
    /// 准确的已识别失败请求数。
    pub failed_requests: String,
    /// Exact input token count and charge in micro-points.
    /// 准确输入 Token 数及以微积分表示的扣费。
    pub input_tokens: String,
    /// Exact aggregate charge in micro-points.
    /// 以微积分表示的准确聚合扣费。
    pub charged_micro_points: String,
}

/// Reads a bounded usage page for secret; cursor never selects another owner or deployment.
/// 为 secret 读取有界用量页；cursor 绝不选择其他所有者或部署。
pub async fn query_openvulcan_usage_page(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &CredentialExecutionSecret,
    cursor: &str,
) -> Result<OpenVulcanUsagePage, String> {
    if cursor.len() > 512 {
        return Err(String::from(
            "OpenVulcan usage cursor exceeds its contract limit.",
        ));
    }
    // query encodes the opaque cursor as one value, never as a caller-authored URL.
    // query 将不透明游标编码为一个值，绝不作为调用方编写的 URL。
    let region = secret
        .openvulcan_region()
        .ok_or_else(|| String::from("OpenVulcan usage requires native authorization."))?;
    let mut query = reqwest::Url::parse(region.auth_origin())
        .map_err(|_| String::from("Invalid usage query origin."))?;
    query
        .query_pairs_mut()
        .append_pair("limit", "50")
        .append_pair("cursor", cursor);
    get(
        http,
        entry,
        secret,
        &format!(
            "usage?{}",
            query
                .query()
                .ok_or_else(|| String::from("Usage query is missing."))?
        ),
    )
    .await
}

/// Native account projection with its credential-bound device identity.
/// 携带凭据绑定设备身份的原生账户投影。
#[derive(Deserialize)]
struct Account {
    /// Permanent installation authorization.
    /// 永久安装授权。
    authorization_id: String,
    /// Public user identity.
    /// 公开用户身份。
    user: OpenVulcanAccountIdentity,
    /// Current plan ledgers.
    /// 当前套餐账本。
    quota: Quota,
}
/// Current ledger metadata used to name the active plan.
/// 用于标识当前套餐的账本元数据。
#[derive(Deserialize)]
struct Quota {
    /// Permanently available free tier.
    /// 永久可用的免费等级。
    free: Tier,
    /// Active membership, absent when not subscribed.
    /// 有效会员，未订阅时缺省。
    member: Option<Tier>,
}
/// Exact provider-owned tier identifier.
/// 准确的供应商持有等级标识。
#[derive(Deserialize)]
struct Tier {
    /// Plan tier name.
    /// 套餐等级名称。
    tier: String,
}
/// Effective FEFO balances, including the selected base ledger and independent grants.
/// 生效的先到期先扣余额，包括选中基础账本和独立赠送。
#[derive(Deserialize)]
struct Balances {
    /// Bounded lot inventory.
    /// 有界批次清单。
    balances: Vec<Balance>,
}
/// Exact one-lot balance with server-classified status.
/// 带服务端状态分类的准确单批次余额。
#[derive(Deserialize)]
struct Balance {
    /// Stable source category.
    /// 稳定来源类别。
    source_type: String,
    /// Exact integer quantities in millionths of a point.
    /// 以百万分之一积分为单位的准确整数量。
    granted_point_micro: u64,
    /// Settled consumption from this exact lot.
    /// 此准确批次的已结算消耗。
    consumed_point_micro: u64,
    /// Server-computed remaining allowance of this lot.
    /// 服务端计算的此批次剩余额度。
    remaining_point_micro: u64,
    /// Absolute activation and expiry timestamps.
    /// 绝对生效和到期时间。
    starts_at: String,
    /// Absolute RFC3339 expiry of this allowance.
    /// 本额度的绝对 RFC3339 过期时间。
    expires_at: String,
    /// Server-owned active/pending/expired/exhausted/revoked classification.
    /// 服务端持有的有效、待生效、到期、耗尽、撤销分类。
    status: String,
}

/// Requests path from the credential's fixed authority and decodes the exact documented response.
/// 从凭据固定授权端请求 path，并解码准确已记录响应。
async fn get<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &CredentialExecutionSecret,
    path: &str,
) -> Result<T, String> {
    get_checked(http, entry, secret, path)
        .await
        .map_err(|error| error.message)
}

/// Preserves typed authentication failures so offline observations never masquerade as signed-out accounts.
/// 保留强类型认证失败，避免离线观测被误判为已退出账户。
async fn get_checked<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &CredentialExecutionSecret,
    path: &str,
) -> Result<T, crate::ModelDiscoveryError> {
    // region comes only from the secure credential projection.
    // region 仅来自安全凭据投影。
    // region binds query construction to the same authority that owns the stored credential.
    // region 将查询构造绑定至持有已保存凭据的同一授权端。
    let region = secret.openvulcan_region().ok_or_else(|| {
        crate::ModelDiscoveryError::failed("OpenVulcan account requires native authorization.")
    })?;
    let body = send_metadata_get_with_authentication(
        http,
        entry,
        AuthenticationProjection::OpenVulcanAuth,
        &format!("{}/api/v1/vulcancode/{path}", region.auth_origin()),
        secret,
        "OpenVulcan account",
    )
    .await?;
    serde_json::from_str(&body).map_err(|_| {
        crate::ModelDiscoveryError::invalid_response(
            "OpenVulcan account response violates its contract.",
        )
    })
}

/// Loads the verified public identity for secret, rejecting a mismatched permanent authorization.
/// 加载 secret 的已验证公开身份，并拒绝不匹配的永久授权。
pub async fn query_openvulcan_identity(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &CredentialExecutionSecret,
) -> Result<OpenVulcanAccountIdentity, crate::ModelDiscoveryError> {
    // account ties the identity response to this exact installation.
    // account 将身份响应绑定到此准确安装。
    let account: Account = get_checked(http, entry, secret, "account").await?;
    if Some(account.authorization_id.as_str()) != secret.provider_device_id() {
        return Err(crate::ModelDiscoveryError::invalid_response(
            "OpenVulcan account authorization identity did not match.",
        ));
    }
    Ok(account.user)
}

/// Converts exact micro-points into a decimal point string without floating-point rounding.
/// 将准确微积分转换为十进制积分字符串，不进行浮点舍入。
fn points(value: u64) -> String {
    format!("{}.{:06}", value / 1_000_000, value % 1_000_000)
}

/// Returns separate quota windows for effective lots so base quota and grants are never counted twice.
/// 为生效批次返回独立额度窗口，避免基础额度与赠送重复计数。
pub(super) async fn query_usage(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &CredentialExecutionSecret,
) -> Result<AccountUsageSnapshot, String> {
    // account supplies tier metadata while balances alone owns the effective quota inventory.
    // account 提供等级元数据，balances 独占生效额度清单权威。
    let account: Account = get(http, entry, secret, "account").await?;
    if Some(account.authorization_id.as_str()) != secret.provider_device_id() {
        return Err(String::from(
            "OpenVulcan account authorization identity did not match.",
        ));
    }
    // balances owns effective lots; tier selects the documented membership-or-free plan label.
    // balances 持有生效批次；tier 选择协议规定的会员或免费套餐标签。
    let balances: Balances = get(http, entry, secret, "quota-balances").await?;
    let tier = account.quota.member.unwrap_or(account.quota.free).tier;
    // windows preserves each independent allowance instead of counting the base summary twice.
    // windows 保留各独立额度，避免重复计入基础摘要。
    let mut windows = Vec::new();
    for (index, balance) in balances.balances.into_iter().enumerate() {
        if !["active", "exhausted"].contains(&balance.status.as_str()) {
            continue;
        }
        windows.push(AccountUsageWindow {
            id: format!(
                "{}:{}:{}:{index}",
                balance.source_type, balance.starts_at, balance.expires_at
            ),
            display_name: balance.source_type.clone(),
            bucket_name: Some(balance.source_type),
            duration_seconds: None,
            used: Some(points(balance.consumed_point_micro)),
            remaining: Some(points(balance.remaining_point_micro)),
            total: Some(points(balance.granted_point_micro)),
            remaining_percent: None,
            unit: String::from("points"),
            starts_at: Some(balance.starts_at),
            resets_at: Some(balance.expires_at),
            unlimited: false,
            available: true,
        });
    }
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: Some(tier.clone()),
        plan_id: Some(tier),
        used_amount: None,
        remaining_amount: None,
        currency: None,
        balance_amount: None,
        quota_windows: windows,
        observed_at: None,
        refresh_message: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Protects decimal precision past the JavaScript and floating-point safe integer boundary.
    /// 保护超过 JavaScript 和浮点安全整数边界后的十进制精度。
    #[test]
    fn points_preserve_integer_precision() {
        assert_eq!(points(18_014_398_509_481_986), "18014398509.481986");
    }
}
