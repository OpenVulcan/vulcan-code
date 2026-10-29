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

/// Longest wait for one answer of the user center's edge routes (model list and access).
/// 用户中心边缘接口（模型清单与访问）一次回答的最长等待时间。
const EDGE_ACCESS_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Largest edge-access answer read; the documented answer is a few hundred bytes.
/// 读取的最大边缘访问回答；协议规定的回答只有数百字节。
const EDGE_ACCESS_MAX_RESPONSE_BYTES: usize = 16 * 1024;

/// Largest edge model list or usage answer read; each entry is under two hundred bytes.
/// 读取的最大边缘模型清单或调用量回答；每个条目不足两百字节。
const EDGE_MODELS_MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// Tells whether `text` is an identifier the edge routes accept: 1–64 characters of
/// `[A-Za-z0-9._:-]`.
/// 判断 `text` 是否为边缘接口接受的标识：1–64 个 `[A-Za-z0-9._:-]` 字符。
pub fn is_openvulcan_edge_model_id(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

/// Answer of `GET /api/v1/vulcancode/edge-models`.
/// `GET /api/v1/vulcancode/edge-models` 的回答。
#[derive(Deserialize)]
struct EdgeModelsAnswer {
    /// Models that currently have an enabled edge endpoint.
    /// 当前有已启用边缘端点的模型。
    models: Vec<EdgeModelEntry>,
}

/// One entry of the edge model list.
/// 边缘模型清单中的一个条目。
#[derive(Deserialize)]
struct EdgeModelEntry {
    /// Full edge model id.
    /// 完整的边缘模型 id。
    model_id: String,
}

/// Parses an edge model list: every id must be a valid edge identifier and appear once. The ids
/// are returned sorted.
/// 解析边缘模型清单：每个 id 都必须是有效的边缘标识且只出现一次。返回排序后的 id。
fn parse_edge_models(body: &[u8]) -> Result<Vec<String>, String> {
    let violates = || String::from("The OpenVulcan edge model list violates its contract.");
    let answer: EdgeModelsAnswer = serde_json::from_slice(body).map_err(|_| violates())?;
    let mut ids = std::collections::BTreeSet::new();
    for entry in answer.models {
        if !is_openvulcan_edge_model_id(&entry.model_id) || !ids.insert(entry.model_id) {
            return Err(violates());
        }
    }
    Ok(ids.into_iter().collect())
}

/// Lists the cloud models the credential's deployment currently serves, sorted by id;
/// an empty list means none is enabled.
/// 列出凭据所属部署当前提供的云端模型，按 id 排序；空清单表示没有启用任何模型。
pub async fn request_openvulcan_edge_models(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
) -> Result<Vec<String>, String> {
    let body = edge_list_get(http, secret, "edge-models", "edge model list").await?;
    parse_edge_models(&body)
}

/// Answer of `GET /api/v1/vulcancode/edge-usage`: the signed-in account's cloud judge
/// calls per model, with every model the deployment enables listed.
/// `GET /api/v1/vulcancode/edge-usage` 的回答：已登录账户按模型的云端判定调用，并列出部署启用的
/// 每个模型。
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct OpenVulcanEdgeUsage {
    /// Window of the totals in hours, ending at the current hour.
    /// 合计的时间窗（小时），截至当前小时。
    pub hours: u32,
    /// One line per model, sorted by id.
    /// 每个模型一行，按 id 排序。
    pub models: Vec<OpenVulcanEdgeModelUsage>,
}

/// One model's line of the edge usage answer.
/// 边缘调用量回答中一个模型的一行。
#[derive(Debug, Deserialize, serde::Serialize)]
pub struct OpenVulcanEdgeModelUsage {
    /// Full edge model id.
    /// 完整的边缘模型 id。
    pub model_id: String,
    /// Whether the deployment still serves the model.
    /// 部署是否仍提供该模型。
    pub enabled: bool,
    /// Calls, judged items and failed calls across all of the account's clients.
    /// 该账户全部客户端的调用次数、判定条数与失败次数。
    pub requests: u64,
    /// Judged items over the window.
    /// 时间窗内的判定条数。
    pub items: u64,
    /// Failed calls over the window.
    /// 时间窗内的失败次数。
    pub errors: u64,
    /// RFC 3339 start of the latest hour with calls; absent when the window has none.
    /// 最近一个有调用的小时的 RFC 3339 起点；时间窗内没有调用时缺省。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_hour: Option<String>,
}

/// Parses an edge usage answer: a positive window, and valid model ids that appear once in
/// ascending order.
/// 解析边缘调用量回答：时间窗为正，模型 id 有效、只出现一次且升序。
fn parse_edge_usage(body: &[u8]) -> Result<OpenVulcanEdgeUsage, String> {
    let violates = || String::from("The OpenVulcan edge usage answer violates its contract.");
    let answer: OpenVulcanEdgeUsage = serde_json::from_slice(body).map_err(|_| violates())?;
    let ordered = answer
        .models
        .windows(2)
        .all(|pair| pair[0].model_id < pair[1].model_id);
    if answer.hours == 0
        || !ordered
        || answer
            .models
            .iter()
            .any(|model| !is_openvulcan_edge_model_id(&model.model_id))
    {
        return Err(violates());
    }
    Ok(answer)
}

/// Reads the signed-in account's cloud judge calls per model.
/// 读取已登录账户按模型的云端判定调用。
pub async fn request_openvulcan_edge_usage(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
) -> Result<OpenVulcanEdgeUsage, String> {
    let body = edge_list_get(http, secret, "edge-usage", "edge usage").await?;
    parse_edge_usage(&body)
}

/// GETs the user-center edge route `route` with the native credential and returns the bounded
/// success body; `subject` names the answer in errors.
/// 以原生凭据 GET 用户中心边缘接口 `route`，返回有界的成功正文；`subject` 在错误中指明该回答。
async fn edge_list_get(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
    route: &str,
    subject: &str,
) -> Result<Vec<u8>, String> {
    let region = secret
        .openvulcan_region()
        .ok_or_else(|| format!("The OpenVulcan {subject} requires a native account."))?;
    let url = format!("{}/api/v1/vulcancode/{route}", region.auth_origin());
    let authorization = crate::openvulcan::authorization_header(secret, &url)?;
    let response = http
        .get(&url)
        .timeout(EDGE_ACCESS_REQUEST_TIMEOUT)
        .header("Authorization", authorization)
        .send()
        .await
        .map_err(|_| format!("The OpenVulcan {subject} request failed."))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|_| format!("The OpenVulcan {subject} could not be read."))?;
    if body.len() > EDGE_MODELS_MAX_RESPONSE_BYTES {
        return Err(format!("The OpenVulcan {subject} is too large."));
    }
    if !status.is_success() {
        let code = serde_json::from_slice::<ErrorEnvelope>(&body)
            .map(|envelope| envelope.error.code)
            .unwrap_or_default();
        return Err(format!(
            "OpenVulcan refused the {subject} with HTTP {} `{code}`.",
            status.as_u16()
        ));
    }
    Ok(body.to_vec())
}

/// Tells whether `text` is a deployment id the edge protocol accepts as a site marker: 1–32
/// characters of `[a-z0-9-]`.
/// 判断 `text` 是否为边缘协议接受作为来源标记的部署 id：1–32 个 `[a-z0-9-]` 字符。
pub fn is_openvulcan_site(text: &str) -> bool {
    (1..=32).contains(&text.len())
        && text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// One edge endpoint the deployment enables for a model.
/// 部署为某个模型启用的一个边缘端点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenVulcanEdgeEndpoint {
    /// Endpoint id registered in the deployment.
    /// 部署中登记的端点 id。
    pub endpoint_id: String,
    /// Endpoint base URL: an https origin, or http on loopback.
    /// 端点基础地址：https 源，或回环地址上的 http。
    pub endpoint: String,
    /// Default order set by the operator; lower comes first.
    /// 运营方设置的默认顺序；数值小者在前。
    pub priority: u32,
    /// Whether the deployment heard from the endpoint recently; advisory only.
    /// 部署最近是否收到该端点的上报；仅供参考。
    pub online: bool,
}

/// Endpoints and short-lived access token for one model. It has no `Debug` so the token cannot
/// reach a log.
/// 一个模型的边缘端点与短期访问令牌。不实现 `Debug`，使令牌无法进入日志。
pub struct OpenVulcanEdgeAccess {
    /// Deployment id that issued the token; sent as the site marker with every judge request.
    /// 签发令牌的部署 id；作为来源标记随每个判定请求发送。
    pub site: String,
    /// Every endpoint enabled for the model, in the deployment's priority order; never empty.
    /// 该模型启用的全部端点，按部署的优先顺序排列；从不为空。
    pub endpoints: Vec<OpenVulcanEdgeEndpoint>,
    /// Bearer token every endpoint of this model accepts for this account and installation.
    /// 该模型每个端点都为该账户与安装接受的 Bearer 令牌。
    pub access_token: String,
    /// Token expiry in Unix milliseconds.
    /// 令牌到期时间（Unix 毫秒）。
    pub expires_at_unix_ms: u64,
}

/// Why edge access was not granted.
/// 未获得边缘访问的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenVulcanEdgeAccessError {
    /// The deployment offers no edge endpoint for the model.
    /// 该部署没有为该模型提供边缘端点。
    ModelUnavailable,
    /// Any other failure, described for the decision log.
    /// 其他任何失败，附供判定记录使用的说明。
    Failed(String),
}

/// Answer of `POST /api/v1/vulcancode/edge-access`.
/// `POST /api/v1/vulcancode/edge-access` 的回答。
#[derive(Deserialize)]
struct EdgeAccessAnswer {
    /// Echo of the requested model.
    /// 请求模型的回显。
    model_id: String,
    /// Issuing deployment id.
    /// 签发部署 id。
    site: String,
    /// Enabled endpoints of the model.
    /// 该模型启用的端点。
    endpoints: Vec<EdgeAccessEndpoint>,
    /// EdDSA JWT for the endpoint.
    /// 端点使用的 EdDSA JWT。
    access_token: String,
    /// RFC 3339 expiry equal to the token's `exp`.
    /// 等于令牌 `exp` 的 RFC 3339 到期时间。
    expires_at: String,
}

/// One endpoint of the edge-access answer.
/// 边缘访问回答中的一个端点。
#[derive(Deserialize)]
struct EdgeAccessEndpoint {
    /// Endpoint id.
    /// 端点 id。
    endpoint_id: String,
    /// Endpoint base URL.
    /// 端点基础地址。
    endpoint: String,
    /// Default order.
    /// 默认顺序。
    priority: u32,
    /// Recent report seen by the deployment.
    /// 部署最近是否收到上报。
    online: bool,
}

/// Error envelope of the user center.
/// 用户中心的错误信封。
#[derive(Deserialize)]
struct ErrorEnvelope {
    /// Stable error.
    /// 稳定错误。
    error: ErrorBody,
}

/// Stable error code of the user center.
/// 用户中心的稳定错误代码。
#[derive(Deserialize)]
struct ErrorBody {
    /// Error code.
    /// 错误代码。
    code: String,
}

/// Returns `endpoint` without a trailing slash when it is an https origin, or an http origin on
/// loopback; anything carrying a path, query, fragment or user information is refused.
/// 当 `endpoint` 是 https 源或回环地址上的 http 源时返回去掉末尾斜杠的值；带路径、查询、片段或用户
/// 信息的地址一律拒绝。
fn edge_endpoint_origin(endpoint: &str) -> Option<String> {
    let url = reqwest::Url::parse(endpoint).ok()?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    let scheme = url.scheme() == "https" || (url.scheme() == "http" && loopback);
    (scheme
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| url.origin().ascii_serialization())
}

/// Asks the credential's deployment for an edge endpoint and access token for `model_id`; the
/// installation's authorization id is the caller id the token carries.
/// 向凭据所属部署请求 `model_id` 的边缘端点与访问令牌；安装授权 id 作为令牌携带的调用方 id。
pub async fn request_openvulcan_edge_access(
    http: &reqwest::Client,
    secret: &CredentialExecutionSecret,
    model_id: &str,
) -> Result<OpenVulcanEdgeAccess, OpenVulcanEdgeAccessError> {
    let failed = |message: &str| OpenVulcanEdgeAccessError::Failed(String::from(message));
    if !is_openvulcan_edge_model_id(model_id) {
        return Err(failed("The cloud model id is malformed."));
    }
    let region = secret
        .openvulcan_region()
        .ok_or_else(|| failed("Edge access requires a native OpenVulcan account."))?;
    let client_id = secret
        .provider_device_id()
        .ok_or_else(|| failed("The OpenVulcan account has no installation authorization."))?;
    let url = format!("{}/api/v1/vulcancode/edge-access", region.auth_origin());
    let authorization = crate::openvulcan::authorization_header(secret, &url)
        .map_err(OpenVulcanEdgeAccessError::Failed)?;
    let response = http
        .post(&url)
        .timeout(EDGE_ACCESS_REQUEST_TIMEOUT)
        .header("Authorization", authorization)
        .json(&serde_json::json!({ "model_id": model_id, "client_id": client_id }))
        .send()
        .await
        .map_err(|_| failed("The OpenVulcan edge access request failed."))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|_| failed("The OpenVulcan edge access answer could not be read."))?;
    if body.len() > EDGE_ACCESS_MAX_RESPONSE_BYTES {
        return Err(failed("The OpenVulcan edge access answer is too large."));
    }
    if !status.is_success() {
        let code = serde_json::from_slice::<ErrorEnvelope>(&body)
            .map(|envelope| envelope.error.code)
            .unwrap_or_default();
        if status == reqwest::StatusCode::NOT_FOUND && code == "model_unavailable" {
            return Err(OpenVulcanEdgeAccessError::ModelUnavailable);
        }
        return Err(OpenVulcanEdgeAccessError::Failed(format!(
            "OpenVulcan refused edge access with HTTP {} `{code}`.",
            status.as_u16()
        )));
    }
    parse_edge_access(&body, model_id).map_err(OpenVulcanEdgeAccessError::Failed)
}

/// Parses an edge-access answer for `model_id`: a valid site marker, at least one endpoint with a
/// distinct valid id and an https origin, a token and an RFC 3339 expiry. Endpoints keep the
/// deployment's order.
/// 解析 `model_id` 的边缘访问回答：有效的来源标记、至少一个 id 有效且不重复并为 https 源的端点、
/// 令牌与 RFC 3339 到期时间。端点保持部署给出的顺序。
fn parse_edge_access(body: &[u8], model_id: &str) -> Result<OpenVulcanEdgeAccess, String> {
    let violates = || String::from("The OpenVulcan edge access answer violates its contract.");
    let answer: EdgeAccessAnswer = serde_json::from_slice(body).map_err(|_| violates())?;
    if answer.model_id != model_id
        || answer.access_token.is_empty()
        || !is_openvulcan_site(&answer.site)
        || answer.endpoints.is_empty()
    {
        return Err(violates());
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut endpoints = Vec::with_capacity(answer.endpoints.len());
    for entry in answer.endpoints {
        if !is_openvulcan_edge_model_id(&entry.endpoint_id)
            || !ids.insert(entry.endpoint_id.clone())
        {
            return Err(violates());
        }
        let endpoint = edge_endpoint_origin(&entry.endpoint).ok_or_else(|| {
            String::from("OpenVulcan returned an edge endpoint that is not an https origin.")
        })?;
        endpoints.push(OpenVulcanEdgeEndpoint {
            endpoint_id: entry.endpoint_id,
            endpoint,
            priority: entry.priority,
            online: entry.online,
        });
    }
    let invalid_expiry =
        || String::from("The OpenVulcan edge access answer has an invalid expiry.");
    let expires_at = time::OffsetDateTime::parse(
        &answer.expires_at,
        &time::format_description::well_known::Rfc3339,
    )
    .map_err(|_| invalid_expiry())?;
    let expires_at_unix_ms = u64::try_from(expires_at.unix_timestamp_nanos() / 1_000_000)
        .map_err(|_| invalid_expiry())?;
    Ok(OpenVulcanEdgeAccess {
        site: answer.site,
        endpoints,
        access_token: answer.access_token,
        expires_at_unix_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Edge model lists keep valid, distinct ids in sorted order and refuse anything else.
    /// 边缘模型清单按排序保留有效且不重复的 id，其余一律拒绝。
    #[test]
    fn edge_model_lists_hold_distinct_valid_ids() {
        let parse = |value: serde_json::Value| parse_edge_models(value.to_string().as_bytes());
        assert_eq!(
            parse(serde_json::json!({"models": [
                {"model_id": "vulcan-decision-shell-v1-preview"},
                {"model_id": "vulcan-decision-shell-v1-mini-preview"}
            ]})),
            Ok(vec![
                String::from("vulcan-decision-shell-v1-mini-preview"),
                String::from("vulcan-decision-shell-v1-preview"),
            ])
        );
        assert_eq!(parse(serde_json::json!({"models": []})), Ok(Vec::new()));
        for refused in [
            serde_json::json!({}),
            serde_json::json!({"models": [{"id": "m"}]}),
            serde_json::json!({"models": [{"model_id": ""}]}),
            serde_json::json!({"models": [{"model_id": "shell judge"}]}),
            serde_json::json!({"models": [{"model_id": "x".repeat(65)}]}),
            serde_json::json!({"models": [{"model_id": "m"}, {"model_id": "m"}]}),
        ] {
            assert!(parse(refused.clone()).is_err(), "{refused}");
        }
    }

    /// Edge usage answers keep ordered, valid model lines with a positive window and refuse
    /// anything else.
    /// 边缘调用量回答保留有序、有效的模型行与正的时间窗，其余一律拒绝。
    #[test]
    fn edge_usage_answers_hold_ordered_valid_models() {
        let parse = |value: serde_json::Value| parse_edge_usage(value.to_string().as_bytes());
        let line = |model_id: &str| {
            serde_json::json!({
                "model_id": model_id, "enabled": true, "requests": 3, "items": 7, "errors": 1,
                "last_hour": "2026-10-24T08:00:00Z"
            })
        };
        let usage = parse(serde_json::json!({"hours": 720, "models": [
            line("vulcan-decision-shell-v1-mini-preview"),
            {"model_id": "vulcan-decision-shell-v1-preview", "enabled": false, "requests": 0, "items": 0, "errors": 0}
        ]}))
        .expect("a valid answer parses");
        assert_eq!(usage.hours, 720);
        assert_eq!(usage.models[0].items, 7);
        assert_eq!(usage.models[1].last_hour, None);
        assert!(parse(serde_json::json!({"hours": 720, "models": []})).is_ok());
        for refused in [
            serde_json::json!({"models": []}),
            serde_json::json!({"hours": 0, "models": []}),
            serde_json::json!({"hours": 720, "models": [line("b"), line("a")]}),
            serde_json::json!({"hours": 720, "models": [line("a"), line("a")]}),
            serde_json::json!({"hours": 720, "models": [line("shell judge")]}),
            serde_json::json!({"hours": 720, "models": [{"model_id": "a", "enabled": true, "requests": -1, "items": 0, "errors": 0}]}),
        ] {
            assert!(parse(refused.clone()).is_err(), "{refused}");
        }
    }

    /// Edge-access answers carry a site marker and distinct https endpoints in the deployment's
    /// order; anything else is refused.
    /// 边缘访问回答带有来源标记与按部署顺序排列、不重复的 https 端点；其余一律拒绝。
    #[test]
    fn edge_access_answers_hold_a_site_and_distinct_endpoints() {
        let endpoint = |id: &str, url: &str, priority: u32| serde_json::json!({"endpoint_id": id, "endpoint": url, "priority": priority, "online": true});
        let answer = |site: &str, endpoints: Vec<serde_json::Value>| {
            serde_json::json!({
                "model_id": "m", "site": site, "endpoints": endpoints,
                "access_token": "t", "expires_at": "2026-10-26T00:10:00Z"
            })
        };
        let parse = |value: serde_json::Value| parse_edge_access(value.to_string().as_bytes(), "m");
        let access = parse(answer(
            "global",
            vec![
                endpoint("us-mini", "https://us-models.example.com/", 10),
                endpoint("cn-mini", "https://cn-models.example.com", 20),
            ],
        ))
        .expect("a valid answer parses");
        assert_eq!(access.site, "global");
        assert_eq!(
            access
                .endpoints
                .iter()
                .map(|endpoint| (endpoint.endpoint_id.as_str(), endpoint.endpoint.as_str()))
                .collect::<Vec<_>>(),
            [
                ("us-mini", "https://us-models.example.com"),
                ("cn-mini", "https://cn-models.example.com")
            ]
        );
        assert_eq!(access.endpoints[1].priority, 20);
        let one = || vec![endpoint("a", "https://a.example.com", 0)];
        let token = |model: &str, token: &str, expiry: &str| {
            serde_json::json!({
                "model_id": model, "site": "global", "endpoints": one(),
                "access_token": token, "expires_at": expiry
            })
        };
        for refused in [
            answer("", one()),
            answer("Global", one()),
            answer(&"s".repeat(33), one()),
            answer("global", Vec::new()),
            answer(
                "global",
                vec![
                    endpoint("a", "https://a.example.com", 0),
                    endpoint("a", "https://b.example.com", 1),
                ],
            ),
            answer("global", vec![endpoint("a b", "https://a.example.com", 0)]),
            answer("global", vec![endpoint("a", "http://a.example.com", 0)]),
            serde_json::json!({"model_id": "m", "endpoint": "https://a.example.com", "access_token": "t", "expires_at": "2026-10-26T00:10:00Z"}),
            token("other", "t", "2026-10-26T00:10:00Z"),
            token("m", "", "2026-10-26T00:10:00Z"),
            token("m", "t", "soon"),
        ] {
            assert!(parse(refused.clone()).is_err(), "{refused}");
        }
    }

    /// Site markers are 1–32 lowercase letters, digits or hyphens.
    /// 来源标记为 1–32 个小写字母、数字或连字符。
    #[test]
    fn site_markers_are_short_lowercase_ids() {
        for site in ["global", "cn", "eu", "us-west-2"] {
            assert!(is_openvulcan_site(site), "{site}");
        }
        for site in ["", "Global", "c n", "cn_1", &"s".repeat(33)] {
            assert!(!is_openvulcan_site(site), "{site}");
        }
    }

    /// Endpoints must be bare https origins; plain http is allowed only on loopback.
    /// 端点必须是纯 https 源；明文 http 只允许在回环地址上使用。
    #[test]
    fn edge_endpoints_are_bare_https_or_loopback_origins() {
        assert_eq!(
            edge_endpoint_origin("https://shell.example.com/").as_deref(),
            Some("https://shell.example.com")
        );
        assert_eq!(
            edge_endpoint_origin("https://shell.example.com").as_deref(),
            Some("https://shell.example.com")
        );
        for local in [
            "http://localhost:8787",
            "http://127.0.0.1:8787",
            "http://[::1]:8787",
        ] {
            assert_eq!(edge_endpoint_origin(local).as_deref(), Some(local));
        }
        for refused in [
            "http://shell.example.com",
            "https://shell.example.com/v1",
            "https://shell.example.com/?x=1",
            "https://user@shell.example.com",
            "ftp://shell.example.com",
            "shell.example.com",
        ] {
            assert_eq!(edge_endpoint_origin(refused), None, "{refused}");
        }
    }

    /// Protects decimal precision past the JavaScript and floating-point safe integer boundary.
    /// 保护超过 JavaScript 和浮点安全整数边界后的十进制精度。
    #[test]
    fn points_preserve_integer_precision() {
        assert_eq!(points(18_014_398_509_481_986), "18014398509.481986");
    }
}
