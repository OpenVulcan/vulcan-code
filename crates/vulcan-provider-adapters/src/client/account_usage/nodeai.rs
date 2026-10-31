//! NODEAI wallet observations from the endpoint used by its public Key Usage page.
//! 从 NODEAI 公开 Key 用量页使用的端点读取钱包观测。

use super::super::http::{
    PROVIDER_METADATA_REQUEST_TIMEOUT, controlled_error_summary, parse_provider_metadata_json,
    read_bounded_provider_metadata_body,
};
use super::current_unix_milliseconds;
use crate::ServiceEntry;
use serde::Deserialize;
use vulcan_ai_contract::{AccountUsageAvailability, AccountUsageSnapshot};

/// Queries the live NODEAI wallet endpoint with the connection's inference key.
/// 使用连接的推理密钥查询 NODEAI 当前钱包端点。
///
/// `http` is the shared metadata client, `entry` supplies the versioned API base,
/// and `secret` is the API key. Returns a wallet snapshot or a bounded diagnostic.
/// `http` 为共享元数据客户端，`entry` 提供带版本的 API 基址，`secret` 为 API Key。
/// 返回钱包快照或有界诊断。
pub(in crate::client) async fn query_nodeai_balance(
    http: &reqwest::Client,
    entry: &ServiceEntry,
    secret: &str,
) -> Result<AccountUsageSnapshot, String> {
    // The live Key Usage page uses /v1/usage; the three legacy billing URLs return 404.
    // 当前 Key 用量页使用 /v1/usage；旧文档中的三个余额地址返回 404。
    let url = format!("{}/usage", entry.base_url.trim_end_matches('/'));
    // Billing always uses Bearer authentication, including Anthropic inference connections.
    // 余额查询始终使用 Bearer 鉴权，Anthropic 推理连接也不例外。
    let response = http
        .get(url.as_str())
        .bearer_auth(secret)
        .timeout(PROVIDER_METADATA_REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|error| format!("NODEAI account balance request failed: {error}"))?;
    // status is retained before the bounded reader consumes the HTTP response.
    // status 在有界读取器消费 HTTP 响应之前保留状态码。
    let status = response.status();
    // body shares the existing metadata size and UTF-8 limits.
    // body 复用现有元数据大小和 UTF-8 限制。
    let body =
        read_bounded_provider_metadata_body(response, "NODEAI account balance", &url).await?;
    if !status.is_success() {
        return Err(format!(
            "NODEAI account balance returned HTTP {}: {}",
            status.as_u16(),
            controlled_error_summary(&body, secret)
        ));
    }
    // Even a successful HTTP response can contain malformed values that echo the credential.
    // 即使 HTTP 成功，响应中的错误字段也可能回显凭据。
    decode_nodeai_balance(&body).map_err(|error| {
        // diagnostic uses the shared registered message field so validation details survive redaction.
        // diagnostic 使用共享已登记消息字段，确保校验详情在脱敏后仍被保留。
        let diagnostic = serde_json::json!({ "message": error }).to_string();
        controlled_error_summary(&diagnostic, secret)
    })
}

/// Projects a current wallet response without converting key quotas into account balances.
/// 投影当前钱包响应，不将密钥限额转换为账号余额。
///
/// `body` is a successful HTTP response. Returns the reported balance and unit,
/// or an explicit failure when a restricted key or subscription omits the wallet.
/// `body` 为成功 HTTP 响应。返回上游报告的余额和单位；受限密钥或订阅未公开钱包时明确失败。
fn decode_nodeai_balance(body: &str) -> Result<AccountUsageSnapshot, String> {
    // response contains only fields whose ownership is verified by the live endpoint and page.
    // response 只包含已通过真实端点和页面确认归属的字段。
    let response: NodeAiBalanceResponse =
        parse_provider_metadata_json(body, "NODEAI account balance JSON")?;
    if !response.is_valid {
        return Err(String::from("NODEAI reports that the API key is invalid."));
    }
    if response.mode != "unrestricted" {
        return Err(String::from(
            "NODEAI account balance requires unrestricted wallet mode; key quotas are not wallet balances.",
        ));
    }
    // balance is absent for quota-only or subscription observations in the official Key Usage page.
    // 官方 Key 用量页中的纯限额或订阅观测不包含 balance。
    let balance = response.balance.ok_or_else(|| {
        String::from("NODEAI usage response does not expose a wallet balance for this API key.")
    })?;
    if response.unit.trim().is_empty() {
        return Err(String::from(
            "NODEAI account balance omitted its currency unit.",
        ));
    }
    // amount keeps zero and negative balances; the API's unit wins over the site's hard-coded symbol.
    // amount 保留零余额和负余额；单位以上游字段为准，不使用站点写死的货币符号。
    let amount = balance.to_string();
    Ok(AccountUsageSnapshot {
        credential_id: String::new(),
        availability: AccountUsageAvailability::Supported,
        plan_name: None,
        plan_id: None,
        used_amount: None,
        remaining_amount: Some(amount.clone()),
        currency: Some(response.unit),
        balance_amount: Some(amount),
        quota_windows: Vec::new(),
        observed_at: current_unix_milliseconds(),
        refresh_message: None,
    })
}

/// Minimal wallet shape verified against NODEAI's live /v1/usage response and Key Usage page.
/// 已通过 NODEAI 真实 /v1/usage 响应与 Key 用量页验证的最小钱包结构。
#[derive(Debug, Deserialize)]
struct NodeAiBalanceResponse {
    /// Whether the provider considers this key valid.
    /// 供应商是否认为该密钥有效。
    #[serde(rename = "isValid")]
    is_valid: bool,
    /// The provider distinguishes unrestricted wallet and quota-limited key modes.
    /// 供应商区分无限额钱包模式与密钥限额模式。
    mode: String,
    /// Wallet money is absent when the endpoint exposes only a subscription or key quota.
    /// 端点仅公开订阅或密钥限额时不包含钱包金额。
    balance: Option<serde_json::Number>,
    /// The API-owned unit must not be inferred from legacy USD field names or UI symbols.
    /// API 所属单位不得根据旧版 USD 字段名或界面符号推断。
    unit: String,
}

#[cfg(test)]
mod tests;
