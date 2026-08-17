//! Provider-neutral account and plan usage contracts.
//! 供应商无关的账号与套餐用量契约。

use serde::{Deserialize, Serialize};

/// Carries one provider-reported quota window without losing integer precision.
/// 承载一个供应商报告的额度窗口且不丢失整数精度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountUsageWindow {
    /// Stable provider-scoped window identifier.
    /// 稳定的供应商作用域窗口标识。
    pub id: String,
    /// User-visible window label.
    /// 用户可见窗口标签。
    pub display_name: String,
    /// Optional quota-bucket or model name.
    /// 可选额度桶或模型名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bucket_name: Option<String>,
    /// Provider-reported used quantity as an exact decimal string.
    /// 供应商报告的精确十进制字符串已用量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used: Option<String>,
    /// Provider-reported remaining quantity as an exact decimal string.
    /// 供应商报告的精确十进制字符串剩余量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining: Option<String>,
    /// Provider-reported total quantity as an exact decimal string.
    /// 供应商报告的精确十进制字符串总量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<String>,
    /// Provider-reported remaining percentage as an exact decimal string.
    /// 供应商报告的精确十进制字符串剩余百分比。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_percent: Option<String>,
    /// Provider-owned quantity unit.
    /// 供应商拥有的计量单位。
    pub unit: String,
    /// Optional window start timestamp.
    /// 可选窗口开始时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    /// Optional window reset or end timestamp.
    /// 可选窗口重置或结束时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    /// Whether the provider declares this window unlimited.
    /// 供应商是否声明该窗口无限制。
    #[serde(default)]
    pub unlimited: bool,
    /// Whether this quota bucket is included in the current account plan.
    /// 当前账号套餐是否包含该额度桶。
    #[serde(default = "account_usage_window_available")]
    pub available: bool,
}

/// Supplies the backward-safe default for provider quota-window availability.
/// 为供应商额度窗口可用性提供向后安全的默认值。
///
/// # Returns
/// 返回值
/// Existing quota windows are available unless the provider explicitly marks them unavailable.
/// 除非供应商明确标记不可用，否则既有额度窗口均视为可用。
fn account_usage_window_available() -> bool {
    true
}

/// Identifies account-usage availability.
/// 标识账号用量可用状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountUsageAvailability {
    /// Provider does not support account usage lookup.
    /// 供应商不支持账号用量查询。
    Unsupported,
    /// Account usage lookup is supported.
    /// 支持账号用量查询。
    Supported,
    /// Supported lookup is temporarily unavailable.
    /// 受支持的查询暂时不可用。
    TemporarilyUnavailable,
}

/// Carries one credential-scoped account usage observation.
/// 承载一个凭据作用域的账号用量观测。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountUsageSnapshot {
    /// Credential identifier owning this observation.
    /// 此观测所属的凭据标识。
    pub credential_id: String,
    /// Availability state.
    /// 可用状态。
    pub availability: AccountUsageAvailability,
    /// Optional plan name.
    /// 可选套餐名称。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_name: Option<String>,
    /// Optional provider-owned stable plan identifier.
    /// 可选的供应商持有稳定套餐标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    /// Optional decimal used amount.
    /// 可选十进制已用金额。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_amount: Option<String>,
    /// Optional decimal remaining amount.
    /// 可选十进制剩余金额。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_amount: Option<String>,
    /// Optional currency code.
    /// 可选币种代码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Optional exact account balance.
    /// 可选精确账号余额。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub balance_amount: Option<String>,
    /// Provider-reported quota windows.
    /// 供应商报告的额度窗口。
    #[serde(default)]
    pub quota_windows: Vec<AccountUsageWindow>,
    /// Optional observation timestamp.
    /// 可选观测时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<String>,
    /// Optional controlled refresh message.
    /// 可选的受控刷新消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{AccountUsageAvailability, AccountUsageSnapshot};
    use serde_json::json;

    /// Verifies that empty quota windows remain explicit at the WebUI contract boundary.
    /// 验证空额度窗口在 WebUI 契约边界仍保持显式数组。
    #[test]
    fn serializes_empty_quota_windows_as_an_array() {
        let snapshot = AccountUsageSnapshot {
            credential_id: "credential-openrouter".to_string(),
            availability: AccountUsageAvailability::Supported,
            plan_name: None,
            plan_id: None,
            used_amount: None,
            remaining_amount: None,
            currency: None,
            balance_amount: Some("12.50".to_string()),
            quota_windows: Vec::new(),
            observed_at: None,
            refresh_message: None,
        };

        let encoded = serde_json::to_value(snapshot).expect("account usage should serialize");

        assert_eq!(encoded["quota_windows"], json!([]));
    }
}
